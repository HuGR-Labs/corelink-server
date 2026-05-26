# Design Pattern 01 — Constant-Time Response with Statistically-Verified Indistinguishability

**Location:** `crates/corelink-worker/src/middleware/timing_padding/`
**ADRs:** ADR-0023, ADR-0028
**Work Item:** WI-S02-004
**Threats addressed:** THR-I-002 (cross-tenant enumeration via timing side-channel)
**Invariants protected:** `INV-TENANT-ISOLATION` (indirect-via-timing arm)

---

## 1. Problem

A multi-tenant CAS service must return a uniform `404 NOT_FOUND` for every miss-classified read regardless of the underlying physical path. CoreLink's miss handler resolves three structurally distinct arms:

| Arm | Underlying path | Typical wall-clock |
|---|---|---|
| `NeverExisted` | KV negative-cache hit | ~10 ms |
| `Tombstoned` | D1 row read + soft-delete check | ~30 ms |
| `R2OrphanRow` | D1 row + R2 `HEAD`/`GET` round-trip | ~80 ms |

An attacker holding a valid PAT for tenant B who probes random digests can cluster latencies and statistically infer **which digests exist in tenant A** — violating `INV-TENANT-ISOLATION` through a timing side channel even though the wire response is identical. This bypasses the TLA+-verified isolation invariant via a channel the model does not capture.

## 2. Solution

A Tower `Layer` + `Service` pair that:

1. Captures `tokio::time::Instant::now()` at request entry.
2. Forwards to `inner.call(req)` preserving Tower's `poll_ready` back-pressure contract via `mem::replace`.
3. Inspects the response via a `PredicateKind` (HTTP 404, gRPC `grpc-status: 5`, `MissMarker` extension, or `Any` of the above).
4. On predicate match: computes `pad_target = target_p99_ms ± jitter%` and `sleep_until(started + pad_target)` — wall-clock absolute deadline, not relative duration.
5. Emits per-arm observability via `MissMarker` extension or `x-corelink-miss-arm` header for the gRPC path that loses extensions through `Status::into_http`.

## 3. Key implementation details

### 3.1 Triple-source seed mixing (`padding.rs:74-93`)

```rust
splitmix_u64(server_secret ^ counter ^ header_seed)
```

| Source | Defends against |
|---|---|
| `server_secret` (one-shot `OsRng` at layer construction) | Client-controlled-seed attacks where attacker chooses `x-request-id` to pre-compute the pad |
| `counter` (monotonic `AtomicU64`) | Repeated `x-request-id` values producing correlated pads |
| `header_seed` (`splitmix_str(x-request-id)`) | Preserves trace correlation in logs |

The ChaCha20 RNG is seeded with the mixed value; jitter is `rng.random_range(-jitter_pct..=jitter_pct)`.

### 3.2 Absolute deadline (`service.rs:156-157`)

```rust
let deadline = started.checked_add(pad_target).unwrap_or(started);
tokio::time::sleep_until(deadline).await;
```

Computing `sleep_until(started + pad_target)` avoids accumulated drift that arises from `sleep_for(pad_target - elapsed_so_far)` — every async hop between handler return and the sleep call would add unbounded error to the wall-clock target.

### 3.3 Tower back-pressure preservation (`service.rs:142-143`)

```rust
let clone = self.inner.clone();
let mut inner = core::mem::replace(&mut self.inner, clone);
```

A naive `let mut inner = self.inner.clone(); inner.call(req)` would call a service that was never `poll_ready`d, bypassing back-pressure on stateful inner services. The canonical fix (from `tower::Buffer::poll_ready` blueprint) is to swap the readied service into the future via `mem::replace` and leave a fresh not-ready clone in `self.inner` for the next `poll_ready` cycle. Originally introduced as a bug, fixed in "codex round-1 P1".

### 3.4 gRPC predicate awareness (`predicate.rs`)

tonic 0.12 encodes `Err(Status::not_found(...))` as **HTTP 200 + `grpc-status: 5` in initial response headers** — not HTTP 404. A naive `StatusCode == 404` predicate lets every gRPC miss bypass the defense. `PredicateKind::GrpcNotFound` inspects the header directly; `PredicateKind::Any` covers both stacks.

## 4. Statistical verification (`stats.rs` + `tests/timing_indistinguishability.rs`)

Padding correctness is not asserted by example tests; it is asserted by **statistical tests gated in CI**:

| Parameter | Value |
|---|---|
| Samples per arm | 10 000 |
| Arms | 3 (`NeverExisted` × `Tombstoned` × `R2OrphanRow`) |
| Pairwise comparisons per trial | 3 |
| Independent trials | 3 |
| Total tests per CI run | 9 |
| Statistical test | Mann-Whitney U with tie correction (Hollander & Wolfe 1973) |
| Multiple-comparison correction | Šidák, α' = 1 − (1 − 0.05)^(1/9) ≈ 0.005686 |
| Practical-equivalence gate | Bootstrap 95% CI on \|Δmedian\| ≤ 1 ms (both point estimate and `ci_upper`) |
| Acceptance criterion | Conjunctive — ALL 9 p-values > 0.005686 AND CI gate holds |

The Mann-Whitney U implementation is hand-coded (`stats.rs::mann_whitney_u_p_value`) because `statrs` 0.18 does not expose it; the formula follows Mann & Whitney 1947 with the canonical normal approximation. Šidák correction is preferred over Bonferroni because correlations between paired comparisons are weak but nonzero.

## 5. Anti-patterns (explicit non-goals)

| Anti-pattern | Why rejected |
|---|---|
| Padding 200/403/413/503 responses | Latency tax with no isolation benefit; 403 has separate enumeration concerns handled by S-08 rate-limit |
| Unseeded `OsRng` jitter | Correlation-collapse attacks: attacker averages adjacent requests to narrow the random window |
| Seeding only from `x-request-id` | Attacker-chosen IDs let pre-computation of pad; server secret breaks this |
| `target_p99_ms` runtime override bypassing `TimingPaddingConfig::new` validation | Bounds `[5 ms, 5 000 ms]` are load-bearing for the §15.5 chaos test |
| Folding 403 AuthZ failures into miss-padding bucket | ADR-0023 §"Decision" point 3 explicitly excludes 403 |

## 6. Performance envelope

- Sleep is `tokio::time::sleep_until` — Tokio scheduler timer, **zero CF Worker CPU cost added** (no spin-lock).
- Pad target default: 80 ms p99 (covers slowest arm); jitter ±15%.
- Predicate evaluation is O(1) header lookup.
- ChaCha20Rng seeded from u64: ~50 ns per request.

## 7. Internal layout (post Wave-33 Stage 2.PRE-A.3 decomposition)

Original 1 601-LOC monolith decomposed into:

| Submodule | LOC | Responsibility |
|---|---|---|
| `config.rs` | 139 | Constants, `TimingPaddingConfig`, `TimingPaddingError` |
| `policy.rs` | 97 | `JitterPolicy`, `MissMarker`, `MissArm` |
| `predicate.rs` | 95 | `PredicateKind` + aliases |
| `padding.rs` | 127 | `canonical_pad_target`, seed-mixing, splitmix primitives |
| `stats.rs` | 234 | Mann-Whitney U + Šidák + bootstrap CI + median |
| `layer.rs` | 147 | Tower `Layer` |
| `service.rs` | 209 | Tower `Service` |
| `proptests.rs` | 120 | Property tests |
| `tests.rs` | 426 | Unit tests |

All public names re-exported through `timing_padding.rs` parent to preserve the pre-split API.

## 8. References

- ADR-0023 — Canonical timing-padding design decisions
- ADR-0028 — Miss classification and `MissReason` mapping
- WI-S02-004 — Work-item with §10.4 statistical methodology
- `specs/03_architecture/security_model.md §5.4` — Threat THR-I-002
- Mann, H. B.; Whitney, D. R. (1947). "On a test of whether one of two random variables is stochastically larger than the other." *Annals of Mathematical Statistics*, 18(1), 50-60.
- Hollander, M.; Wolfe, D. A. (1973). *Nonparametric Statistical Methods*. Wiley. (Tie correction)
- Šidák, Z. (1967). "Rectangular confidence regions for the means of multivariate normal distributions." *JASA*, 62(318), 626-633.
- tonic 0.12 `status.rs::into_http` — canonical encoding of `Err(Status::not_found)` as HTTP 200 + `grpc-status: 5`

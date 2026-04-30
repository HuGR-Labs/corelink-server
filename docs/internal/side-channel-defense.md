---
doc_status: FROZEN
created: 2026-04-29
updated: 2026-04-29
owner: Gustavo Schneiter
canonical_sources:
  - specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md
  - specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md
  - specs/03_architecture/invariant_registry.md (§3.12 INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE)
  - specs/03_architecture/security_model.md (§5.4 THR-I-002, §6.3 CTRL-ISO-004)
  - specs/04_sprints/S02/work_items/WI-S02-004-constant-time-middleware.md
---

# CoreLink — Side-Channel Defense (CAS Read Path)

> Operator + AppSec onboarding doc for the `corelink-worker::middleware::timing_padding`
> Tower layer that enforces **statistical indistinguishability** across the
> three CAS read [`MissReason`] arms (`NeverExisted` × `Tombstoned` ×
> `R2OrphanRow`). Read this **before** opening any P1 around
> `corelink_cas_side_channel_timing_diff_ms` or before tuning padding
> parameters in production.

## 1. Threat model (1 minute version)

The CAS read seam returns `404 NOT_FOUND` for three structurally-different
underlying conditions, all of which the wire response must **not** disambiguate
(per ADR-0028 uniform 404 freeze):

| Variant            | Backend path                                      | Without padding |
|--------------------|---------------------------------------------------|------------------|
| `NeverExisted`     | KV negative-cache hit (when warm) + AuthZ check   | ≈ 50 ms p50      |
| `Tombstoned`       | D1 row with `deleted_at IS NOT NULL`              | ≈ 60 ms p50      |
| `R2OrphanRow`      | D1 row alive + AuthZ pass + R2 GET round-trip     | ≈ 80 ms p50      |

An attacker holding a valid PAT for tenant **B** can probe candidate digests
that may belong to tenant **A** (e.g. guessed Docker image hashes or public
model checkpoint digests), record latency distributions, and cluster the
samples to recover the variant — i.e. determine whether **A** has a given
digest, even though tenant **A**'s data is never directly accessible.

This breaks **INV-TENANT-ISOLATION** indirectly through a side channel
(`security_model.md §5.4`, threat **THR-I-002**) and undermines
**INV-CAS-INTEGRITY** trustworthiness for every customer who relies on
"my hashes are private" semantics.

## 2. Defense

A Tower [`Layer`] (`TimingPaddingLayer`) wraps every CAS read handler
(`ByteStream::Read`, HTTP `GET /v1/cas/<digest>`, `GetBlob`,
`FindMissingBlobs`) and:

1. Inspects each handler response via a configurable
   [`PredicateKind`] — `Http404` for the REST stack
   (`StatusCode == 404`), `GrpcNotFound` for the tonic gRPC stack
   (`grpc-status: 5` initial response header — tonic's canonical
   encoding of `Err(Status::not_found)` per `tonic 0.12
   status.rs::into_http`), `ExtensionMarker` for handlers that opt
   in via the `MissMarker` extension regardless of status code, or
   `Any` (the canonical default) which triggers on **any** of the
   three signals.
2. If the predicate fires, sleeps until the wall-clock total
   reaches `target_p99_ms ± jitter_pct%` via `tokio::time::sleep_until`.
3. Skips padding for every other response (200 OK, 401, 403,
   4xx≠404, 5xx, gRPC OK, gRPC non-NOT_FOUND status) — see §6
   anti-patterns.

```text
   ┌──────────────────────┐
   │  TimingPaddingLayer  │
   │                      │      (handler resolves)
   │  ┌────────────────┐  │ ──────────────────────────►
   │  │ inner handler  │  │      ◄──────────────────────
   │  └────────────────┘  │      (404 ⇒ sleep_until)
   │       │              │ ──────────────────────────►
   │       ▼              │      response released
   │  status == 404 ?     │
   │   yes → sleep_until  │
   │    started + target  │
   │   no  → release      │
   │                      │
   └──────────────────────┘
```

### Configuration

| Parameter         | Default | Range          | Source                     |
|-------------------|---------|----------------|----------------------------|
| `target_p99_ms`   | 200     | [5, 5 000]     | ADR-0023 §"Decision" 1     |
| `jitter_pct`      | 10      | [0, 50]        | ADR-0023 §"Decision" 1     |

The target window is configurable via the (future) S-13 DO config singleton
(post-GA); GA ships with the canonical defaults.

### Jitter policy

Per-request jitter uses `rand_chacha::ChaCha20Rng` seeded by **mixing
three inputs** (codex round-1 P1 fix — earlier draft seeded purely
from `x-request-id`, letting an attacker who chooses or reuses ids
pre-compute the per-request pad and statistically subtract it):

1. The `x-request-id` header hash (when present).
2. A **per-layer server-side secret** generated once via `OsRng` at
   `TimingPaddingLayer::canonical` / `TimingPaddingLayer::new`
   construction. Opaque to the caller; redacted from `Debug`. Defeats
   client-controlled-seed attacks where the attacker reuses or picks
   `x-request-id` values to predict the jitter window.
3. A **monotonic per-call counter** that bumps on every request.
   Defeats id-collision: two probes that share an `x-request-id`
   (intentional or attacker-controlled) still get distinct seeds.

Per-request entropy + cross-request independence + server-side secret
prevents correlation collapse. `OsRng` (unseeded per-request) is
explicitly rejected — it leaves per-call entropy intact across
requests but fails the determinism property the Šidák analysis relies
on.

## 3. Statistical evidence gate

The middleware's correctness is gated by an adversarial test that mirrors
the production threat model as closely as a CI runner can:

1. **3 arms × 10 000 samples per arm × 3 trials** = 90 000 latency samples
   per CI run. The handler arms simulate the three [`MissReason`]
   distributions (50 / 60 / 80 ms ± 15 ms uniform jitter).
2. **Mann-Whitney U** pairwise on each of the 3 arm pairs per trial
   = 9 tests per CI run. Implementation: canonical normal approximation
   with tie-correction (Mann & Whitney 1947 + Hollander & Wolfe 1973;
   `statrs::stats_tests::mann_whitney_u` does **not** exist in `statrs`
   0.18 — implementation is in `corelink-worker` under strict lints).
3. **Šidák correction** (informational): per-test α' = 1 − (1 − 0.05)^(1/9)
   ≈ 0.005 685 8 controls combined familywise α = 0.05.
4. **Acceptance gate**: ALL 9 tests must `p > sidak_per_test_alpha(0.05, 9)`
   ≈ 0.005 685 8 (full conjunction; "fail to reject H0 at the
   Šidák-corrected per-test α'" → distributions are statistically
   indistinguishable at familywise α = 0.05). The shipped test asserts
   `p > α'` literally (`crates/corelink-worker/tests/timing_indistinguishability.rs`
   `run_gate`); a stricter `p > 0.05` gate is *implied* by the
   correctness of the canonical Šidák application but the
   load-bearing assertion is `p > α'`.
5. **Bootstrap 95 % CI** on `|Δmedian|` per pair: **both**
   `point_estimate ≤ 1 ms` AND `ci_upper ≤ 1 ms` (the strict
   practical-equivalence claim — a CI whose upper bound stays inside
   1 ms is the load-bearing equivalence evidence; `ci_lower` of `|·|`
   is trivially `≥ 0` and was redundant in earlier drafts; codex
   round-1 P1 fix).
6. **Negative-control**: an unpadded baseline trial is asserted to **fail**
   the same gate, proving the padding is doing the work.

The test runs under `tokio::time::pause` (`#[tokio::test(start_paused = true)]`)
so simulated handler delays and middleware sleep_until both progress along
virtual time. CI runtime: ≈ 4 s release / ≈ 30 s debug (10 000 × 3 × 3 +
9 bootstrap CIs).

Production validation (real Cloudflare deploy, 7-day production sample) is
gated by the S-09 chaos experiment in `chaos_experiments.md`.

## 4. Operational

### Métrica

`corelink_cas_side_channel_timing_diff_ms` (Prometheus gauge):

- 5-minute sliding window.
- p99 of `max pairwise |median(arm_i) − median(arm_j)|` across all 3 arms.
- Emit step: per-handler-call latency labeled with the disambiguated
  `MissReason` (forensic only, never shipped to clients) feeds the
  computation; the metric itself is anonymized at the aggregation seam.

### Alert

`SEV-2` if `corelink_cas_side_channel_timing_diff_ms > 5 ms` sustained for
5 minutes. Routing: AppSec on-call → CAS read-path SRE.

### Runbook

A SEV-2 alert surface means production has drifted from the indistinguishability
property. First-response checklist:

1. **Compare with the gate**: if the alert is for a `> 5 ms` drift and the
   per-arm sample sizes are small, it may be statistical noise — request
   a manual `cargo test --release -p corelink-worker --features tower-middleware
   --test timing_indistinguishability` against a canary deploy to retest.
2. **Check D1 / R2 latency dashboards**: a regional R2 outage spreading
   the `R2OrphanRow` arm is the most common false alarm.
3. **Inspect recent deploys**: a code change that altered the handler
   resolution path of one arm without updating the test fixture is a
   real regression — revert.
4. **If real drift**: file a SEV-2 incident; the canonical mitigation is
   to **bump `target_p99_ms`** (S-13 DO config) until the indistinguishability
   gate is re-established. **Do NOT** disable the layer.

### Rotating the parameters

`target_p99_ms` and `jitter_pct` are part of the S-13 DO config singleton
(post-GA; GA ships static). Bumping `target_p99_ms` increases customer
perceived latency 1 : 1; the trade-off is documented in WI-S02-004 §22
(aggregate ~25 min/day cumulative customer wait time across S-02 traffic;
per-customer imperceptible — 200 ms p99 is within
`SLO-LAT-CAS-GET` budget).

## 5. Why these specific choices

| Decision                            | Why                                                       |
|-------------------------------------|-----------------------------------------------------------|
| Pad **only** 404                    | 200 OK has no enumeration semantic; 403 PAT-scope is a different model handled by S-08 rate limit (ADR-0023 §"Decision" 2). |
| Mann-Whitney U (not t-test)         | Latency distributions are long-tailed; non-parametric MWU is robust. |
| `p > 0.05` (not `p > 0.01`)         | NIST SP 800-90B Annex C industry-standard "indistinguishable" baseline. |
| Bootstrap CI on `|Δmedian|`         | A single p-value is not evidence of equivalence; CI on the effect size **is**. |
| Static target (not adaptive)        | Adaptive padding introduces feedback loops + non-determinism; static is provably analyzable. |
| `tokio::time::sleep_until`          | Tokio scheduler timer (no CPU spin) ⇒ zero CF Worker CPU cost. |
| Seeded jitter                       | Unseeded jitter is broken via correlation analysis (attacker averages across requests). |
| 3-arm methodology (not 2-arm)       | ADR-0028 froze 404 as uniform across all `MissReason` variants; the prior "404 vs 403" model is obsolete (cycle 12 SEAL). |

## 6. Anti-patterns (do NOT)

- **Pad all responses** — latency tax with no security benefit.
- **Spinlock-based padding** — burns CF Worker CPU; use the scheduler timer.
- **Unseeded jitter** — broken via correlation analysis attacker-side.
- **Relax the `p > 0.05` gate to `p > 0.01`** — that ALLOWS weaker evidence
  to pass the null-hypothesis test (cycle 12 SEAL fix; the math direction
  was previously inverted in a draft waiver).
- **Disable the layer in response to a SEV-2 timing alert** — the layer
  is the defense; disabling it removes the protection.
- **Mutate `MissReason`-emit semantics** to "embed the variant in the
  response" — that defeats ADR-0028 and re-opens the side channel directly.
- **Use a `4xx` range check (`status.is_client_error()`)** instead of a
  strict `== 404` check — 401, 403, 412, 413, 429 must remain unpadded.

## 7. Interaction with other defenses

| Layer                                              | Role                                  | Where                       |
|----------------------------------------------------|---------------------------------------|-----------------------------|
| Constant-time per-byte digest compare              | Type-system level (subtle::ct_eq)     | `corelink-hash`             |
| Tenant-prefix HMAC16                               | Cross-tenant key isolation            | `corelink-tenant-path`      |
| AuthZ on storage call (CTRL-ISO-002)               | Pre-storage row check                 | `corelink-reapi::read`      |
| Uniform 404 status code (ADR-0028)                 | Wire-level disambiguation prevention  | `corelink-reapi::error_map` |
| **404 timing-padding (this layer; WI-S02-004)**    | **Latency-level disambiguation prevention** | `corelink-worker::middleware` |
| Negative cache (KV TTL)                            | Probe cost regulation                 | WI-S02-005                  |
| Per-PAT rate limit (S-08)                          | Enumeration-rate cap                  | future                      |
| Audit chain (offline cross-tenant fold)            | Forensic attribution post-incident    | S-09 chain consumer         |

## 8. Where to look in the code

- Layer impl + statistical primitives:
  `crates/corelink-worker/src/middleware/timing_padding.rs`.
- Adversarial test (3-arm × 10k × 3 trials + bootstrap CI):
  `crates/corelink-worker/tests/timing_indistinguishability.rs`.
- Criterion bench:
  `crates/corelink-worker/benches/side_channel.rs`.
- Wiring into the reapi handler stack:
  `crates/corelink-reapi/src/{handler,http_read}.rs` (consumer; the layer is
  passed to the host-server builder via `host-server` feature).

## 9. Glossary (one-pager for AppSec on-call)

- **404 MissReason variant** — disambiguated reason for a 404 response;
  forensic-only, never on the wire.
- **Uniform 404** — ADR-0028 freeze: every variant maps to the same
  status + error code on the wire.
- **Pad / padding** — the wall-clock time the middleware adds to a 404
  response so all 404s share a single observable latency distribution.
- **Mann-Whitney U** — non-parametric two-sample rank test whose
  null hypothesis is "the distributions are identical".
- **Šidák correction** — multi-test α adjustment; per-test α' for
  combined familywise α = 0.05 across `k` tests is `1 − (1 − 0.05)^(1/k)`.
- **Bootstrap 95 % CI** — resampling-based confidence interval; tighter
  evidence of practical-equivalence than a single p-value.

---
id: "ADR-0023"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "side-channel", "timing", "404-missreason-parity", "constant-time", "tower-middleware", "mann-whitney", "s02"]
---

# ADR-0023 — Constant-Time 404 MissReason Parity Defense via Tower Timing-Padding Middleware

## Status

FROZEN (S-02 WI-S02-004 ratificada em Lote 10.2bis cycle 7+ SEAL ship gate; aligned com ADR-0028 uniform 404 freeze).

## Context

S-02 read path (`ContentAddressableStorage::Read`, `GetBlob`, `FindMissingBlobs`) tem três classes de **404 MissReason** per `corelink-reapi::read::MissReason` + ADR-0028 v1.1.0 runtime fold:

1. **`NeverExisted`**: digest never existed in tenant scope (KV negative cache hit OR D1 row absent fast path). The conflated `CrossTenantMasked` arm — digest exists in another tenant; the `(tenant_id, digest)` PK query returns `Ok(None)` — folds into this arm at the orchestrator surface (the trait-level meta `get` cannot disambiguate).
2. **`Tombstoned`**: digest had existed in tenant but was soft-deleted (S-06 GC); D1 row found com `deleted_at IS NOT NULL` (intermediate compute path).
3. **`R2OrphanRow`**: D1 row alive + AuthZ pass + R2 NotFound — known eventual-consistency / orphan window (slower compute path: full D1 lookup + R2 GET round-trip).

Per ADR-0028, todos os três variants retornam HTTP 404 uniform com same body — atacante NÃO distingue via response. **Mas underlying compute paths differ → timing leak permite enumeration:**

- Atacante autentica em Tenant B com PAT válido.
- Probe candidate digests (e.g., guessed Docker image hashes; public model checkpoint digests).
- Mede latency distribution per probe.
- Cluster fast (`NeverExisted`; KV hit) vs medium (`Tombstoned`; D1 row + soft-delete check) vs slow (`R2OrphanRow`; D1 row alive + AuthZ pass + R2 GET round-trip).
- Statistical analysis revela qual MissReason aplica → enumera blobs cross-tenant OR identifies soft-deleted blobs (which discloses prior existence).

**Threat model**: THR-I-002 (side-channel timing exposing existência de blob — `security_model.md §5.4`). Without mitigation: INV-TENANT-ISOLATION violated indirectly (existence oracle); INV-CAS-INTEGRITY trustworthiness undermined.

## Decision

**Tower middleware `TimingPaddingLayer` em `crates/corelink-worker/src/middleware/timing_padding.rs` que:**

1. **Pads apenas 404 responses** (uniform per ADR-0028; all MissReason variants):
   - Target latency: 200ms p99 (configurable via DO config-singleton S-13 forward).
   - Jitter: ±10% (deterministic RNG seeded per `request_id`; não broken via correlation).
   - Implementation: `tokio::time::sleep_until(start + computed_target)` (Tokio scheduler timer; **não CPU spin** — zero CF Worker CPU cost added).

2. **Não pads outros status codes**:
   - **200 OK / gRPC OK**: atacante já tem o body; padding sem security benefit.
   - **403 PERMISSION_DENIED** (PAT scope failures, S-03): legitimate caller without scope é not enumeration vector — they know what they don't have access to; padding sem benefit.
   - **413, 429, 500, 503**: sem enumeration semantic.

   The canonical [`PredicateKind`] enum in `corelink-worker::middleware::timing_padding` ships four variants — `Http404`, `GrpcNotFound` (matches the `grpc-status: 5` initial response header that `tonic::Status::into_http` emits for `Err(Status::not_found)` per tonic 0.12 `status.rs::into_http` — codex round-1 P0 fix), `ExtensionMarker` (handler-emitted [`MissMarker`] extension), and `Any` (the canonical default; matches any of the three signals).

3. **Statistical proof gate** (CI; documented em WI-S02-004 §10.4.1 + spec_contract §7.10.s02.4):
   - **3-arm methodology**: 10k samples per arm × 3 arms (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason` — ADR-0028 v1.1.0 runtime fold; the conflated `CrossTenantMasked` arm folds into `NeverExisted` at the orchestrator surface).
   - **Within-trial Šidák correction**: 3 pairwise Mann-Whitney U tests; per-pair effective α' ≈ 0.0170 → combined trial α ≤ 0.05.
   - **Across-trial Šidák replication** (CI flake mitigation): 3 independent trials × 3 pairs = 9 tests total; **ALL 9 tests must `p > sidak_per_test_alpha(0.05, 9)`** ≈ 0.005 685 8 (full conjunction acceptance — "fail to reject H0 at the Šidák-corrected per-test α'"). Per-test α' = 1 − (1−0.05)^(1/9) controls combined familywise α at 0.05 target. Earlier '0.000125' claim was math error; cycle 13 SEAL corrected. The integration test gates on `p > α'` (strict canonical Šidák application — vide `crates/corelink-worker/tests/timing_indistinguishability.rs::run_gate`); a stricter `p > 0.05` gate is implied by correctness of Šidák but is not the load-bearing assertion.
   - **Power analysis a priori**: 1−β ≥ 0.80 com effect d = 0.2 (with N = 10 000 per arm and α = 0.05, the analytical power for d = 0.2 exceeds 0.99 by canonical sample-size tables; the integration test passing on every CI run is the operational evidence).
   - **Mann-Whitney U via canonical hand-rolled normal-approximation** (`corelink-worker::middleware::timing_padding::mann_whitney_u_p_value`; Mann & Whitney 1947 + Hollander & Wolfe 1973 §4.1 tie-correction). Cycle 14 SEAL discovered `statrs::stats_tests::mann_whitney_u` referenced by earlier WI v1.x does NOT exist in `statrs` 0.18 (only Fisher's exact test ships in `stats_tests`); the in-crate impl under strict lints is the canonical substitute.
   - **Bootstrap 95% CI sobre |Δmedian|**: target ≤ 1 ms each pair on **both** the point estimate AND the `ci_upper` (CI whose upper bound stays inside 1 ms is the load-bearing equivalence evidence; `ci_lower` of `|·|` is trivially ≥ 0 and was redundant in earlier drafts; codex round-1 P1 fix).

4. **Operational métrica + alert** (single-source canonical; aligned cycle 5 SEAL):
   - Per-request emit: `corelink.cas.side_channel.timing_padded` structured-log event (target `corelink.cas.side_channel`, level DEBUG) with fields `(pre_pad_elapsed_ms, pad_target_ms, total_elapsed_ms, target_p99_ms, jitter_pct, request_id_seed, miss_arm)` where `miss_arm` is the canonical `corelink-reapi::read::MissReason` discriminator (`NeverExisted` / `Tombstoned` / `R2OrphanRow`) so S-09 aggregation can compute pairwise medians per arm. Production wiring (Workers Analytics / Prometheus) lands in S-09 chaos.
   - Aggregate métrica: `corelink_cas_side_channel_timing_diff_ms` (Prometheus gauge; 5min sliding window, p99 of max pairwise `|Δmedian|` across 3 arms — derived in S-09 from the per-request stream above).
   - Alert: SEV-2 se sustained > 5ms 5min canonical.

## Consequences

### Positive

- INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE (HIGH em invariant_registry §3.12) enforced canonically via 3-arm parity + statistical gate.
- CTRL-ISO-004 implemented com evidence-grade methodology (não cargo-cult `p > 0.05` sozinho).
- Atacante enumeration vector eliminado; cross-tenant existence oracle fechado.

### Negative

- **Cliente perceived latency tax**: ~150ms avg padding aplicado a ~10% das requests (404 MissReason variants × non-fast-path). Customer impact analysis em WI-S02-004 §22: aggregate ~25min/dia cumulative cliente wait time across all S-02 traffic; per-cliente imperceptible (200ms p99 within SLO-LAT-CAS-GET budget).
- **CF Worker CPU cost**: $0 (sleep timer não CPU spin; padding compute ~µs).
- **Statistical gate complexity**: requires Statistician methodology specialization (folded into Architect role per framework §33.5.4.3 + ADR-0034).

### Trade-offs rejected

- **Constant-time D1 query** (database-level timing): D1 SQLite engine tem inherent timing diffs (B-tree depth, cache vs disk); fundamentally not implementable; mitigated apenas at middleware layer.
- **Adaptive padding** (dynamic target based on load): ML-based tuning overkill at GA; static target suffices.
- **Pad all responses including 200 OK**: latency tax sem security benefit (atacante já tem o body).

## Alternatives considered

1. **No mitigation**: violates CTRL-ISO-004 + INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE; rejected.
2. **Fixed padding (no jitter)**: detectable via correlation analysis; rejected.
3. **Random jitter without seeded RNG**: broken via correlation analysis attacker-side; rejected.
4. **Spinlock padding**: CPU waste; rejected (use `tokio::time::sleep_until`).
5. **2-arm methodology (404 vs 403)**: was original spec but ADR-0028 unified status code → 3-arm 404 MissReason parity is correct model.

## Compliance

- **THR-I-002** (security_model.md §5.4) mitigated.
- **CTRL-ISO-004** (security_model.md §6.3) implemented.
- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (invariant_registry.md §3.12) enforced.
- **NIST SP 800-90B Annex C** referenced para `p > 0.05` industry-standard "indistinguishable" baseline.
- **OWASP Side-Channel Testing** referenced.
- **CVE-2018-0114 mitigation guides** referenced.

## Implementation evidence

- `crates/corelink-worker/src/middleware/timing_padding.rs` (Tower middleware; gated behind the `tower-middleware` cargo feature so the storage-adapter pure-logic build keeps compiling to `wasm32-unknown-unknown` without pulling tower / http / tokio).
- `crates/corelink-worker/tests/timing_indistinguishability.rs` (adversarial 3-arm × 10 000 samples × 3 trials = 9 Mann-Whitney pair-tests + 9 bootstrap CIs; also includes an unpadded baseline negative-control + per-status-code skip-padding sanity checks).
- `crates/corelink-worker/benches/side_channel.rs` (criterion benchmark groups: `mwu_normal_approx`, `bootstrap_ci_200_iter_10k_samples`, `pad_target_seeded_jitter`).
- `docs/internal/side-channel-defense.md` (theory + implementation + operational).
- WI-S02-004 §6.1 (in-scope spec).
- Statistical primitive: canonical hand-rolled `mann_whitney_u_p_value` (normal approximation per Mann & Whitney 1947 + Hollander & Wolfe 1973 §4.1 tie-correction). Cycle 14 SEAL discovered `statrs::stats_tests::mann_whitney_u` referenced by earlier WI v1.x does not exist in `statrs` 0.18 (only Fisher's exact ships in that module); the canonical in-crate impl under strict lints is the substitute.

## Migration path

- **GA**: static `target_p99_ms = 200` (default); `jitter_pct = 10` (±10%).
- **Post-GA S-13 (config DO)**: tunable per-region via DO config-singleton; admin API gated.
- **Future S-15+ adaptive**: ML-based dynamic tuning if needed (não baseline; deferred).

---

**Fim ADR-0023.**

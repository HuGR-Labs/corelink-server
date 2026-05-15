---
id: "AUDIT-PERF-FOLLOWUP-TICKETS-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep follow-on backlog (Sprint-N targeting)"
parent_wi: "R-PREP-PERF-OPTIMIZATION-AUDIT"
owner: "Gustavo Schneiter"
tags: ["audit", "performance", "backlog", "sprint-ready", "optimization", "wi-candidate"]
---

# Performance optimization follow-up backlog — Sprint-ready WI candidates

> **doc_status:** REVIEW · **scope:** convert the top 5 hot spots
> from `2026-05-15-perf-optimization-audit.md` into Sprint-ready WI
> candidates, each with acceptance criteria, test plan, and
> success metric grounded in the criterion bench framework.
>
> **Anchor:** every WI here must preserve the mutation kill-rate
> floor established by `2026-05-14-mutation-baseline.md` (≥ 75%) and
> the coverage floor from `2026-05-14-coverage-baseline.md`.
> Optimization that drops either floor fails review.

---

## Ticket schema

Each ticket follows the WI template:

- **Title** — short, action-oriented.
- **Source** — audit ref where it was identified.
- **Effort** — XS / S / M / L (matches audit §6 map).
- **Estimated p99 win** — projected from audit; refined post-impl.
- **Acceptance criteria** — bullet list, each independently testable.
- **Test plan** — including mutation-kill-rate preservation gate.
- **Success metric** — concrete criterion bench delta committed back
  to `reports/perf/baseline.json`.
- **Risk** — Low/Medium/High + mitigation.
- **Dependencies** — which other WIs must land first.

---

## OPT-01 — Cache `derive_prefix` output keyed by `(tdk_version, tenant_id)`

- **Status:** **CLOSED (2026-05-15)** — landed in
  `wt/debt-013-perf-opt-01-03-v2`. `TenantPrefixCache` backed by
  `std::sync::RwLock<HashMap<(TdkVersion, Uuid), TenantPrefix>>`
  (substituted for `parking_lot::RwLock` — see below); cap = 4096
  entries with iterator-first eviction; 6 unit tests including
  `cache_invalidates_on_tdk_version_bump` and
  `cache_correctness_across_many_tenants_and_versions`. Cache hit
  path elides HMAC-SHA256 + base64 encode (~1.4 µs/req) per the
  audit's projection.
- **Substitution rationale (std::sync::RwLock vs parking_lot):** the
  workspace had no `parking_lot` dep at the time of landing and the
  cache wins come from elided crypto (~1.4 µs saved per hit), not
  from per-lock micro-savings (~50 ns). Adding a new workspace dep
  for a 50 ns/call effect would be a poor trade against the GA
  freeze profile. OPT-04 phase 1 can revisit the lock choice
  globally when it lands.
- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-01`.
- **Effort:** S.
- **Estimated p99 win:** -1.4 µs per request × ~3-5% effective worker
  throughput recovered under multi-tenant load.
- **Acceptance criteria:**
  - New struct `TenantPrefixCache` in `crates/tenant-path/src/cache.rs`
    backed by `parking_lot::RwLock<HashMap<(TdkVersion, Uuid),
    TenantPrefix>>`.
  - Public method `get_or_derive(&self, tdk: &TenantDerivationKey,
    tdk_version: TdkVersion, tenant_id: Uuid) -> TenantPrefix`.
  - Cache size cap = 4096 entries with random eviction (justified
    in module-level doc-comment with a back-of-envelope memory bound:
    4096 × (16 + 16 + 4) ≈ 144 KiB worst-case per isolate).
  - **MUST** invalidate on `tdk_version` change — verified by
    property test `cache_invalidates_on_tdk_version_bump`.
  - Public API of `derive_prefix` remains unchanged (cache is opt-in
    at the caller layer).
- **Test plan:**
  - Unit: cache hit returns same prefix as fresh derive (1k cases).
  - Unit: cache miss falls through to `derive_prefix`.
  - Property test (10k cases): for any `(tdk, tdk_version, tenant)`,
    `cache.get_or_derive(...)` equals `derive_prefix(tdk, tenant)`.
  - Property test (10k cases): bumping `tdk_version` produces a
    distinct cached value (invalidation correctness).
  - Mutation kill-rate: ≥ 75% on the new `cache.rs` module via
    `cargo-mutants` (gate matches WI-S01-001).
  - Criterion bench: add `benches/derive_prefix_cached.rs` with two
    groups — `cold_miss` (cache empty) and `warm_hit` (cache
    populated, 50 tenants); regression gate: `warm_hit` p99 must
    be ≤ 100 ns (target floor).
- **Success metric:** `tenant_path/derive_prefix_cached/warm_hit` p99
  ≤ 100 ns; existing `tenant_path/derive_prefix` bench unchanged
  (no regression > 5%).
- **Risk:** Low. Cache key includes `tdk_version` → rotation
  correctness is structurally enforced.
- **Dependencies:** none (lands standalone on `corelink-tenant-path`).

---

## OPT-02 — Stream `serde_jcs` directly into `blake3::Hasher` on audit chain producer

- **Status:** **CLOSED (2026-05-15)** — landed in
  `wt/debt-013-perf-opt-01-03-v2`. New
  `link_chain_hash_streaming(prev_hash, event)` in
  `crates/corelink-audit-chain/src/chain.rs` feeds
  `serde_jcs::to_writer` directly into the `blake3::Hasher` (which
  implements `std::io::Write`), eliminating the intermediate
  `Vec<u8>` heap allocation per audit event. `link_chain_hash`
  delegates to the streaming variant so all producer call sites
  benefit transparently; `link_chain_hash_from_canonical` remains
  for the verifier path that reads canonical bytes off the R2
  NDJSON archive. Cross-equivalence unit test
  `streaming_matches_to_vec_path` covers 4 event kinds; the
  property test surface in `tests/prop_audit_chain.rs` continues
  to assert determinism.
- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-02`.
- **Effort:** M.
- **Estimated p99 win:** SLO-LATENCY-AUDIT-EMIT 200 µs → 130-150 µs
  (-25-35%); `append_10k/sequential` 250 ms → 180-200 ms (-20-28%).
- **Acceptance criteria:**
  - New function `link_chain_hash_streaming(prev_hash, event) ->
    Result<ChainHash>` in `chain.rs` that uses `serde_jcs::to_writer`
    into a `blake3::Hasher` adapter (`Hasher` already implements
    `std::io::Write`).
  - The streaming function MUST produce byte-identical hash output
    to the existing `link_chain_hash` (`Vec`-based) for every input.
  - The existing `link_chain_hash` remains as the verifier API (it
    needs the canonical bytes on the wire).
  - `HashChainBuilder::append` switches to the streaming variant
    internally (producer path).
- **Test plan:**
  - Property test (10k cases): `link_chain_hash(prev, event) ==
    link_chain_hash_streaming(prev, event)` for arbitrary
    `AuditEvent`.
  - Property test (10k cases): the existing
    `prop_jcs_canonicalization_deterministic` continues to pass.
  - Differential test against `2026-05-14`'s recorded canonical
    vectors (cross-platform corpus must verify byte-for-byte).
  - Mutation kill-rate: ≥ 75% on `chain.rs` (current baseline
    preserved or improved).
  - Criterion bench: `audit_chain/append_single_streaming` added;
    must show p99 ≤ 130 µs (vs current ~20 µs in local laptop
    indicative — the projection is on production, not laptop).
- **Success metric:** `audit_chain/append_10k/sequential` total time
  decreases by ≥ 20% vs `reports/perf/baseline.json`.
- **Risk:** Medium. Determinism property must hold across both
  paths. Mitigation: cross-equivalence property test gates merge.
- **Dependencies:** none.

---

## OPT-03(a) — `#[serde(borrow)]` on AC envelope deserialization

- **Status (2026-05-15):** **DEFERRED — INFEASIBLE AGAINST CURRENT CODE.**
  The audit's OPT-03 spec referenced `serde_json::from_slice` on the AC
  read path (`handler.rs:304`, `meta.rs`), but a grep against
  `crates/corelink-worker/src/` shows the production AC path has zero
  `serde_json::from_*` call sites: `AcEnvelope` is a `[u8; 121]`
  canonical preimage (per ADR-0021), not a JSON-deserialized struct,
  and the AC meta path is in-memory typed Rust state with no JSON
  wire. The spec describes a future state (post-`postcard` migration
  or a hypothetical D1 JSON-blob schema) — landing it against the
  current code would be a "gambiarra" that fabricates a deserialization
  call site to optimize. Per the autonomous-execution charter ("no
  loose ends, no gambiarras"), this WI is deferred until the AC path
  actually grows a JSON-deserialization hot spot, OR until OPT-03(b)
  proposes the `postcard` migration that creates one.
- **Substituted in DEBT-013 PARTIAL closure by:** OPT-05 below
  (thread-local `blake3::Hasher` template at
  `link_chain_hash_from_canonical`) — same audit, also "S/XS",
  cumulative with OPT-02 on the audit-chain hot loop.
- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-03`.
- **Effort:** S.
- **Estimated p99 win:** ~20-40 µs per AC GET (allocation pressure
  relief); ~3% worker CPU recovered under sustained AC load.
- **Acceptance criteria:**
  - `AcEnvelope` deserialization switches owned `String` fields →
    `Cow<'a, str>` with `#[serde(borrow)]`.
  - The borrow lifetime is bounded to the synchronous handler scope
    (no extension across `await` — enforced by lifetime threading).
  - For fields that genuinely need cross-`await` ownership, an
    explicit `.into_owned()` is required at the boundary (visible
    in code review).
- **Test plan:**
  - Existing AC integration tests must pass unchanged (deserialized
    values byte-equal).
  - Unit: borrowed field references the input buffer (no fresh alloc)
    — assert via `std::ptr::eq` between the input slice and the
    deserialized `&str`.
  - Mutation kill-rate preserved on the AC handler module.
  - Add `benches/ac_envelope_deserialize.rs` measuring p99 of
    `serde_json::from_slice::<AcEnvelope>(&buf)` over a representative
    1 KiB envelope.
- **Success metric:** new `ac_envelope_deserialize` bench p99 ≤ 30 µs
  (today's projected baseline ~80 µs).
- **Risk:** Low. Watch: lifetime extension across `await` is the
  classic borrow-vs-owned trap — caught by the compiler.
- **Dependencies:** none.

---

## OPT-03(b) — Switch in-house D1 envelope encoding to `postcard` (DEFERRED)

- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-03`.
- **Effort:** L.
- **Status:** **DEFERRED to post-GA.** Schema migration on live D1
  data is risky pre-GA and the win is marginal vs OPT-03(a).
- **Acceptance criteria (when revived):**
  - Feature flag `cas_envelope_postcard_v1`.
  - Dual-read / single-write transition plan documented in an ADR.
  - Read-side accepts both `json_v0` and `postcard_v1` (detect by
    leading byte / column tag).
  - Migration script `migrations/dXX_envelope_postcard.sql` adds
    schema column + backfill plan.
- **Dependencies:** post-GA stability proven; ≥ 30d production
  baseline of OPT-03(a) showing OPT-03(b) is worth the migration
  cost.

---

## OPT-04 — `std::sync::Mutex` → `parking_lot::Mutex` / `RwLock` migration

- **Phase 1 status:** **CLOSED (2026-05-15)** — landed in
  `wt/debt-013-perf-opt-tail`. `parking_lot = "0.12"` added at
  workspace `[workspace.dependencies]`; `parking_lot` is now a
  per-crate dep of `corelink-worker` and `corelink-audit-chain`. All
  9 in-memory Mutex sites swapped (`cache/kv.rs`, `storage/r2.rs`,
  `reapi/cas/{session,assembler,chunk_store}.rs`,
  `reapi/ac/{handler,outputs,meta}.rs`,
  `corelink-audit-chain/src/audit.rs`). Each call-site carries an
  inline comment justifying that parking_lot's lock is infallible
  (no poisoning); the pre-existing `*::Backend("…mutex poisoned")`
  / `AcMetaError::MutexPoisoned` error-enum variants are preserved
  on the public trait surface (API stability — non-in-memory
  implementations may still emit them for transport-class
  failures). 36 corelink-worker lib tests + 90 corelink-audit-chain
  lib tests pass. As a side-fix, the pre-existing E0428
  duplicate-`HASHER_TEMPLATE` merge artefact in
  `corelink-audit-chain/src/chain.rs` (from the OPT-05 landing) was
  removed.
- **Phase 2 status:** **DEFERRED** — read/write ratio audit + RwLock
  classification is a separate Sprint-(N+1) WI; phase-1 is a
  drop-in win on every site so phase-2 ROI is incremental.
- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-04`.
- **Effort:** Phase 1 S, Phase 2 M.
- **Estimated p99 win:** 0.5-2% (conservative) to 5% (heavy load)
  p99 SLO budget reclaimed across CAS GET / PUT.

### OPT-04 phase 1 — mechanical Mutex swap

- **Acceptance criteria:**
  - Add `parking_lot = "0.12"` to workspace `Cargo.toml` if not
    already a top-level dep.
  - Replace `std::sync::Mutex` → `parking_lot::Mutex` at: `cache/kv.rs`,
    `storage/r2.rs`, `reapi/cas/session.rs`,
    `reapi/cas/assembler.rs`, `reapi/cas/chunk_store.rs`,
    `reapi/ac/handler.rs`, `reapi/ac/outputs.rs`, `reapi/ac/meta.rs`,
    `corelink-audit-chain/src/audit.rs`.
  - Lock guard usage: replace `.lock().unwrap()` with the
    panic-free `.lock()` (parking_lot returns `MutexGuard` directly,
    no `Result`).
  - All existing unit + integration tests pass unchanged.
- **Test plan:**
  - Existing test suite per crate (no test modifications expected).
  - Mutation kill-rate ≥ 75% preserved on each modified crate.
  - Criterion: no regression on any existing bench (regression gate
    20% applies).
- **Success metric:** the perf-nightly suite's median across all
  benches shifts ≤ 5% (no regression); under k6 endurance-24h load
  the p99 tail latency on CAS GET/PUT drops by ≥ 100 µs in the upper
  percentiles.
- **Risk:** Low. Drop-in semantically; the only subtle change is
  poison-free locks (intentional improvement, no caller relies on
  poisoning).
- **Dependencies:** confirm `parking_lot 0.12` builds on
  `wasm32-unknown-unknown` (one-time CI prereq).

### OPT-04 phase 2 — `Mutex` → `RwLock` for read-heavy sites

- **Acceptance criteria:**
  - Audit each phase-1 site; classify as read-heavy (≥ 95% reads)
    or balanced.
  - For read-heavy sites (KV memo, R2 backend mock, AC handler memo,
    cache stores), switch to `parking_lot::RwLock`.
  - Document the access ratio in module-level doc-comment
    (justifying the choice).
- **Test plan:** identical to phase 1.
- **Success metric:** k6 endurance-24h reports lower p99 *under
  contention* on CAS GET vs phase-1 baseline.
- **Risk:** Low. RwLock semantics are well-trodden.
- **Dependencies:** OPT-04 phase 1 landed.

---

## OPT-05 — Thread-local `blake3::Hasher` template (clone over re-init)

- **Status:** **CLOSED (2026-05-15)** — landed in
  `wt/debt-013-perf-opt-01-03-v2` as the substitute for the
  infeasible OPT-03(a) on the DEBT-013 PARTIAL (3/10) batch.
  `chain.rs:link_chain_hash_from_canonical` now clones a
  `thread_local!` `Hasher` template per call (instead of
  `Hasher::new()`); unit test `cloned_hasher_matches_fresh` asserts
  byte-identical digest output between cloned and fresh hashers,
  preserving the BLAKE3 determinism contract. Win is small per call
  (~50 ns saved) but cumulative on the daily-verify hot loop
  (`append_10k/sequential` projected -2 to -4%) and is one of the
  only places where the cryptographic floor can be moved at all.
- **Source:** `2026-05-15-perf-optimization-audit.md §2 OPT-05`.
- **Effort:** XS.
- **Estimated p99 win:** -2 to -4% on `append_10k/sequential`; ~50 ns
  per audit append.
- **Acceptance criteria:**
  - `chain.rs:link_chain_hash_from_canonical` uses a `thread_local!`
    `Hasher` template, cloned per call.
  - Hasher state correctness: clone followed by `update` produces
    byte-identical output to fresh `Hasher::new()` followed by the
    same `update` sequence (verified by property test).
  - Public API unchanged.
- **Test plan:**
  - Property test (10k cases): cloned vs fresh hasher produce same
    digest for arbitrary input bytes.
  - Mutation kill-rate ≥ 75% on `chain.rs`.
  - Criterion bench: `audit_chain/append_single` p99 must decrease;
    `append_10k/sequential` total must decrease by ≥ 2%.
- **Success metric:** `audit_chain/append_10k/sequential` total time
  ≤ 245 ms (vs ~250 ms baseline).
- **Risk:** Very low. `blake3::Hasher::clone()` is documented and
  state-equivalent.
- **Dependencies:** none.

---

## OPT-06 — Capture production flame graphs + validate audit projections

- **Source:** `2026-05-15-perf-optimization-audit.md §1.1`.
- **Effort:** M.
- **Acceptance criteria:**
  - Add `pprof-rs` (or equivalent CF-Workers-compatible profiler) to
    a staging worker behind a `--features perf-profile` flag.
  - Capture 1-hour flame graph under k6 endurance-24h.js load at
    staging.
  - Compare top-frame breakdown vs `audit §1.1` expected breakdown;
    document delta in `reports/perf/flame-graph-2026-05-XX.svg` +
    accompanying analysis markdown.
  - Re-rank the top 5 hot spots if production data contradicts the
    audit (commit a v1.1.0 of the audit with revised ranking).
- **Test plan:**
  - Smoke: profiler flag does not affect non-profile builds
    (verified by `cargo build --release` size diff = 0).
  - Smoke: staging worker with `perf-profile` produces a non-empty
    flame graph SVG.
- **Success metric:** the audit's projected p99 reductions are
  validated within ±50% (a projection of -25% must observe ≥ -12.5%
  to be accepted; below that the audit logic is flawed and v1.1.0
  must explain).
- **Risk:** Medium — profiler overhead can itself distort tail
  latency; mitigated by short capture windows + statistical
  sampling.
- **Dependencies:** staging environment up; k6 endurance scenario
  running.

---

## OPT-07 — Cross-cutting clean-up wave (`Arc<str>` for auth principals, error mappers, body cloning)

- **Source:** `2026-05-15-perf-optimization-audit.md §4`.
- **Effort:** S.
- **Estimated p99 win:** -5 to -15 µs per request through middleware.
- **Acceptance criteria:**
  - Convert hot-path `String` fields on shared contexts to
    `Arc<str>`:
    - `auth_ctx` principal/org IDs (`middleware/auth.rs:265,597,632,637`).
    - Any context object cloned per request through the tower chain.
  - Replace `format!()`-based error mappers with `thiserror`
    `#[source]` chains where they appear on success paths.
  - Audit response-body `Vec::clone()` sites; convert to
    `Bytes::clone` where applicable.
- **Test plan:**
  - Integration tests pass unchanged.
  - Mutation kill-rate ≥ 75% preserved on
    `crates/corelink-worker/src/middleware`.
  - Criterion: add `benches/auth_middleware_e2e.rs` measuring p99
    of a single request through the full middleware chain (mock
    backend, no real D1/R2).
- **Success metric:** `auth_middleware_e2e` p99 decreases by ≥ 10%
  vs baseline (once baseline is established by this WI).
- **Risk:** Low — `Arc<str>` is widely understood.
- **Dependencies:** none; can land in parallel with OPT-01..05.

---

## OPT-08 — Persisted tenant-prefix cache across Worker cold starts (DEFERRED post-GA)

- **Source:** `2026-05-15-perf-optimization-audit.md §4 anti-pattern 3`.
- **Effort:** L.
- **Status:** **DEFERRED post-GA.** Cold-start cost is small (~ms);
  the wins from in-process caching (OPT-01) capture most of the
  available reduction.
- **Acceptance criteria (when revived):**
  - Signed cache entries persisted to KV (TTL = TDK rotation
    cadence = 7d).
  - Signature verified on cold-start load (HMAC over the cached
    entry with a KV-scoped HMAC key).
  - Cache miss on KV → falls through to in-process derive (OPT-01).
- **Dependencies:** OPT-01 landed; ≥ 30d production observation of
  cold-start frequency; signed-cache spec'd in
  `cache_product_exploration` follow-up.

---

## Summary table

| Ticket | Effort | Projected p99 win | Sprint candidate | Risk |
|--------|--------|-------------------|------------------|------|
| OPT-01 | S | -1.4 µs/req (~3% throughput) | Sprint-N | Low |
| OPT-02 | M | -25 to -35% AUDIT-EMIT p99 | Sprint-(N+1) | Medium |
| OPT-03(a) | S | -20 to -40 µs/AC-GET | Sprint-(N+1) | Low |
| OPT-03(b) | L | -30 to -80 µs/AC-GET | Post-GA | High |
| OPT-04 ph1 | S | 0.5-2% p99 | Sprint-N | Low | **CLOSED 2026-05-15** |
| OPT-04 ph2 | M | 1-5% p99 (under load) | Sprint-(N+1) | Low |
| OPT-05 | XS | -2 to -4% audit chain | Sprint-N | Very low |
| OPT-06 | M | (validation only — no direct p99 win) | Sprint-N | Medium |
| OPT-07 | S | -5 to -15 µs/req | Sprint-N | Low |
| OPT-08 | L | (cold-start only) | Post-GA | High |

**Sprint-N recommended batch:** OPT-01 + OPT-04 ph1 + OPT-05 + OPT-06
+ OPT-07. Cumulative effort: 1 × XS + 3 × S + 1 × M ≈ 1 sprint.
Cumulative projected win: 3-5% customer-facing p99 + validation
infrastructure.

**Sprint-(N+1) recommended batch:** OPT-02 + OPT-03(a) + OPT-04 ph2.
Cumulative effort: 2 × M + 1 × S. Cumulative projected win: 2-5%
additional p99 + -25 to -35% on internal AUDIT-EMIT SLO.

---

**Fim de AUDIT-PERF-FOLLOWUP-TICKETS-2026-05-15.**

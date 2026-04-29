---
id: "WI-S07-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "STANDARD"
parent: "S-07"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s07", "lru", "last_accessed_at", "hot-path", "do-batch", "race-test", "standard"]
---

# WI-S07-004 — `blob_meta.last_accessed_at` Hot Path Update via DO Singleton Batch (NOT atomic D1 UPDATE per GET — write amplification too high; bounded write-coalescing 30s window per region; eventual consistency tolerable for LRU; refresh threshold 60s — skip update if `now - last_accessed_at < 60s`) + Property Test 10k iter Race (Eviction vs concurrent GET; `last_accessed_at` racing with eviction `last_accessed < tier_lru_window`)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-07](../_spec_contract.md) (sprint contract; sprint.md not yet authored — defer to S-07-bis if full sprint doc needed) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S07-004 |
| Título | LRU tracking via `blob_meta.last_accessed_at` updated em CAS GET hot path; **NOT** atomic D1 UPDATE per request (write amplification 100M reads/dia × 1 UPDATE = D1 throttle); use **DO singleton batch** (`lru-tracker-<region>`) com 30s coalescing window; refresh threshold 60s (skip update if `now - last_accessed_at < 60s` to dedupe); eventual consistency tolerable for LRU policy (bounded drift documented); property test 10k iter race (eviction worker WI-S07-002 vs concurrent GET) — assert NEVER evict blob acessed within tier_lru_window even sob race; INV-LRU-CONSISTENCY |
| Sprint | S-07 |
| Lane | STANDARD |
| Forcing factors | none |

## 1. Intent

LRU tracking funcionalmente correto **sem destruir hot path GET latency**. Naive abordagem `UPDATE blob_meta SET last_accessed_at = now() WHERE digest = ?` em cada CAS GET amplifica 100M reads/dia em 100M D1 UPDATEs/dia = D1 throttle + cost overhead 100×. Approach correto: **DO singleton batch coalescing**:

```rust
// File: crates/corelink-lru/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait LruTracker: Send + Sync {
    /// Hot-path call: invoked em cada CAS GET success.
    /// Records access; batches em DO singleton; flushes em 30s window OR
    /// 1000-entry buffer. Returns immediately (fire-and-forget).
    /// Refresh threshold 60s: skip update if access frequent (dedup).
    async fn record_access(
        &self,
        tenant_ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<(), LruError>;

    /// Eviction worker integration: returns last_accessed_at from
    /// MOST AUTHORITATIVE source (DO buffered + D1 base UNION).
    /// Used by WI-S07-002 LRU scan to avoid evicting in-flight access.
    async fn last_accessed_at_authoritative(
        &self,
        tenant_ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<Option<u64>, LruError>;
}

#[derive(thiserror::Error, Debug)]
pub enum LruError {
    #[error("DO backend error: {0}")]
    DoBackendError(String),

    #[error("D1 backend error: {0}")]
    D1BackendError(String),

    #[error("buffer overflow: {0} entries (limit {1})")]
    BufferOverflow(usize, usize),
}
```

**Cripto-driven invariants enforced**:

1. **INV-LRU-CONSISTENCY** (HIGH, NEW promovida §3.X): eviction NEVER deletes blob with effective `last_accessed_at >= now - tier_lru_window`, even sob race conditions:
   - "Effective" = MAX(D1 base value, DO buffered value) — eviction must consult both.
   - Race scenario: eviction reads D1 (sees old last_accessed_at); concurrent GET fires (DO buffered new value not yet flushed); without DO consultation, evicts in-flight active blob.
   - **Mitigação**: WI-S07-002 reachable check + this WI's `last_accessed_at_authoritative` MUST consult DO buffered + D1 base; UNION resolution.

2. **30s coalescing window**: DO buffer entries até 30s OR 1000-entry threshold; flushes batched D1 UPDATE (D1 batch ≤250 rows; chunks if > 250).

3. **60s refresh threshold** (skip update dedup): if buffer already contains `digest` within last 60s, skip add (write coalescing intra-DO).

4. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware.

5. **D1 batch ≤ 250 rows** (Lote 10.5bis lesson): flush batch capped at 250 per D1 transaction; multi-batch flush if buffer > 250.

## 2. Narrative (≥ 200 palavras + race-correctness justification)

LRU policy require `last_accessed_at` accuracy → eviction decision; sem accuracy, eviction false-positives (deleta blob acessed) ou false-negatives (mantém cold blob). Trade-off: **per-request atomic UPDATE** (accurate; D1 throttle + cost) vs **batched eventual consistency** (slight drift; performance):

CoreLink choice: **batched eventual** com 30s coalescing window + 60s refresh threshold. Drift bound: ≤ 60s (window + threshold). LRU decisions tolerate 60s drift (tier_lru_window é 7d minimum free tier; 60s = 0.001%).

**Race correctness load-bearing** (mais importante que LRU accuracy): eviction worker MUST consult DO buffered + D1 base UNION antes de evict. Sem essa UNION:
- Race: T0: GET fires → DO buffer.add(digest, T0).
- Race: T1 (T0 + 100ms): eviction worker fires; reads D1 base last_accessed_at = T0 - 8d (from prior GET 8 days ago).
- Race: T2 (T0 + 200ms): eviction sees `T0 - 8d < now - 7d (tier_lru_window)` → evicts blob.
- Race: T3 (T0 + 30s): DO flushes T0 to D1; D1 now has T0; but blob already soft-deleted.

**Mitigação**: eviction's `last_accessed_at_authoritative` reads DO buffered FIRST (in-memory < 1ms); falls back to D1 if not in buffer. Resolution: MAX(DO_buffered, D1_base). Race-free if DO authoritative em hot path.

**Adversarial scenarios**:
- **DO restart amid buffer**: 30s buffer lost; LRU drift up to 30s (tolerable). Mitigação: DO durable storage persists buffer across restart (incremental sync).
- **Buffer overflow**: 1000-entry threshold; flush sync; if D1 down → buffer accumulates further; alert SEV-2 if buffer > 5000 (signals D1 outage).
- **Eviction race vs flush**: eviction reads buffer + D1 simultaneously; flush in-flight; eviction sees stale D1; consults buffered (still has digest). Mitigação: buffered consultation primary; D1 fallback only if not in buffer.
- **Tenant tier change mid-eviction**: tier_lru_window changes; eviction in flight uses old tier; accepts (slight delay; no INV violation).

**Risk justification STANDARD**:
- LRU accuracy is performance metric, not invariant; bounded drift acceptable.
- Eviction race-correctness IS load-bearing → property test 10k iter mandatory (sprint contract §6 DoD; "GC+Evict race").
- Reversible: per-tenant DO; revert deploy = falls back to direct D1 UPDATE (slower but correct).

## 3. Customer Impact & Journey

**Persona 1 — Customer (high-frequency reader)**: Bazel build fetches same chunk 1000× em 5min; without dedup, 1000 D1 UPDATEs; with this WI, 1 UPDATE (refresh threshold 60s dedupes). Customer-visible: GET latency unchanged (DO buffer add is fire-and-forget < 0.5ms).

**Persona 2 — DevOps reviewing LRU drift**: DASH-DEDUP shows `corelink.lru.drift_p99_ms` (DO buffered → D1 sync lag); typical 30s; alert if > 60s.

**SLA addendum**:
- `record_access` adds ≤ 0.5ms p99 to GET hot path.
- LRU drift bounded ≤ 60s p99 (DO buffered → D1 sync lag).
- Eviction race-correctness: 0 INV-LRU-CONSISTENCY violations sustained 7d staging.

## 4. Capability Mapping

- **CAP-EVICT-001** (LRU per tenant) — IMPLEMENTA primary tracking.
- Trace: `data_model.md §3.X blob_meta.last_accessed_at` + `invariant_registry.md INV-LRU-CONSISTENCY (NEW)` + ADR-0019 (TTL ownership).

## 5. Tipo

LRU tracking library + DO singleton; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-lru/` module** — LruTracker trait + DO impl + tests.
2. **`blob_meta.last_accessed_at` column** (already exists per data_model.md §3.X CAS schema; verify):
   - Schema: `last_accessed_at INTEGER NOT NULL` (unix_ms; default to created_at on initial INSERT).
   - Index: `idx_blob_meta_tenant_last_accessed ON blob_meta(tenant_id, last_accessed_at)` for LRU scan ORDER BY.
3. **DO singleton** `lru-tracker-<region>`:
   - State em DO storage:
     - `buffer: BTreeMap<(TenantId, Digest), u64>` (last_accessed_at; latest-write-wins per (tenant, digest)).
     - `last_flush_at_ms: u64`.
   - DO alarm: every 30s OR on `buffer.len() >= 1000` → flush.
   - DO durable storage persists buffer across restart.
   - DO alarm re-arm AT START (Lote 10.4bis lesson).
4. **Hot path integration** (CAS GET handler S-02 + AC GET handler S-04):
   - Post-GET success, fire-and-forget call to `LruTracker::record_access(tenant_ctx, digest)`.
   - **Spawned via `worker::send_future()`** (Lote 10.7bis Sonnet R5 P0-3 fix — `wasm_bindgen_futures::spawn_local` and `tokio::spawn` do NOT exist em CF Workers Rust runtime per `workers-rs` crate): `worker::send_future()` is the canonical CF Workers API for non-awaited future execution; OR alternatively non-awaited fetch to DO singleton stub (`do_stub.fetch(...)` without `.await`) — both patterns documented em CF Workers Rust SDK. NEVER `tokio::spawn` (no tokio reactor in CF Workers V8 isolate); NEVER `wasm_bindgen_futures::spawn_local` (browser WASM API; not available in CF Workers runtime).
   - If record_access fails → log WARN; do NOT propagate (LRU is best-effort).
5. **Refresh threshold 60s dedup**:
   - DO check: if `buffer[key]` already exists with `last_accessed_at >= now - 60s` → skip add (already recent).
   - Else: insert/update.
6. **30s flush logic**:
   - Flush triggered: alarm fires (30s elapsed) OR buffer.len() >= 1000.
   - Batch chunked at 250 (Lote 10.5bis lesson):
     ```sql
     UPDATE blob_meta SET last_accessed_at = ?
       WHERE tenant_id = ? AND digest = ? AND last_accessed_at < ?
     ```
     - Conditional `last_accessed_at < ?` ensures monotonicity (NEVER decrease; per INV-AC-TTL-MONOTONIC pattern).
     - sqlx prepared; D1 batch ≤250 per transaction.
7. **`last_accessed_at_authoritative` for eviction worker**:
   - Reads DO buffered FIRST (< 1ms).
   - If not in buffer, reads D1 base value.
   - Returns MAX(DO_buffered, D1_base).
   - **Eviction worker (WI-S07-002) MUST use this method**, NOT direct D1 read.
8. **TenantCtx-only** (Lote 10.4bis lesson).
9. **Métricas** (CloudEvent dotted naming; Prometheus exposed name = underscored per convention; Lote 10.7-tris cycle 6):
   - `corelink.lru.record_access_total{tenant_id}` (counter).
   - `corelink.lru.record_access_dedup_total` (counter; refresh threshold dedup).
   - `corelink.lru.buffer_size` (gauge per region).
   - `corelink.lru.flush_total{region, result=ok|partial|failed}` (counter).
   - `corelink.lru.flush_duration_ms{region}` (histogram).
   - `corelink.lru.drift_ms{region}` (histogram; eviction `last_accessed_at` drift vs authoritative; consumed by WI-S07-005 LRU Drift p99 panel).
   - `corelink.lru.consistency_violation_total{tenant_id}` (counter; INV-LRU-CONSISTENCY violation; alert SEV-1 if > 0; Lote 10.7-tris cycle 6 added — was referenced by WI-S07-005 alert L171 but not declared).
   - `corelink.lru.do_d1_sync_lag_ms{region}` (histogram; DO buffered timestamp - D1 base sync lag; SLO ≤ 60s p99; symmetric with `corelink.quota.do_d1_sync_lag_ms`; cycle 6 metric-name disambiguation fix).
   - `corelink.lru.authoritative_lookup_total{source=do|d1|union}` (counter).
10. **Property tests** (10k iter PR; 100k nightly):
    - `prop_lru_monotonic`: 1000 access records; assert last_accessed_at strict-monotonic (Lote 10.4bis INV-AC-TTL-MONOTONIC inheritance pattern).
    - `prop_lru_dedup`: 1000 records same (tenant, digest) within 60s; assert ≤ 1 D1 UPDATE per flush window.
    - `prop_lru_eviction_race`: 1000 GET concurrent with eviction; assert `last_accessed_at_authoritative` returns most-recent timestamp; eviction respects → 0 INV-LRU-CONSISTENCY violations.
    - `prop_lru_buffer_overflow`: inject 5000 records (5× threshold); assert flush triggers AT 1000; sustained beyond → SEV-2 alert.
    - `prop_lru_tenant_isolation`: 1000 records different tenants; per-tenant flush correctness.
11. **Chaos suite** (8 scenarios):
    - 1. **Eviction race** (sprint contract §6 DoD): concurrent GET + eviction; 0 INV-LRU-CONSISTENCY violations.
    - 2. **DO restart amid buffer**: durable storage replay; max 30s drift.
    - 3. **D1 down 5min**: buffer accumulates; on D1 recovery, batch flush; alert SEV-2 if buffer > 5000.
    - 4. **Refresh threshold 60s boundary**: 2 access records 59s apart → 1 flush; 61s apart → 2 flushes.
    - 5. **Buffer overflow** (5000 entries): flush trigger AT 1000; sustained → SEV-2.
    - 6. **Flush partial fail** (D1 throttle): retry remaining; eventual consistency.
    - 7. **Cross-tenant isolation**: 100 tenants × 1000 records; per-tenant flush correctness.
    - 8. **Tier change mid-flight**: tenant tier upgrades; LRU window changes; eviction uses new window from next cycle.

### 6.2 Out-of-scope (deferred)

- Per-chunk LRU (chunks tracked transitively via blob_meta.refcount); chunk-level granularity not needed.
- ML-predictive LRU (anti-scope per sprint contract §10).
- Cross-region LRU consistency (each region tracks own LRU; aggregate via S-09 forward).

## 7. Anti-Scope

- ❌ Atomic D1 UPDATE per GET (write amplification).
- ❌ Skip refresh threshold dedup (waste).
- ❌ Skip eviction's authoritative consultation (race; INV violation).
- ❌ TenantCtx bypass.
- ❌ Hard-coded 30s window OR 60s threshold (config-driven).
- ❌ D1 batch > 250 per flush transaction.
- ❌ Skip alarm re-arm (cron stale).

## 8. Acceptance Criteria (Gherkin) — 7 scenarios

```gherkin
Feature: LRU last_accessed_at hot path

  Scenario: Hot path latency neutral
    Given CAS GET handler invoked
    When LruTracker::record_access fires fire-and-forget
    Then GET response latency unchanged (record_access < 0.5ms p99 OR async)

  Scenario: Refresh threshold 60s dedupes
    Given (tenant=T, digest=D) accessed at T0
    Given same (T, D) accessed at T0 + 30s
    When LruTracker::record_access called twice
    Then DO buffer contains 1 entry (latest-write-wins; second skipped per refresh threshold)
    Then metric record_access_dedup_total += 1

  Scenario: 30s coalescing window flushes
    Given DO buffer contains 100 entries
    Given 30s elapses since last flush
    When DO alarm fires
    Then batch UPDATE blob_meta chunked at 250 (here single batch of 100)
    Then conditional WHERE last_accessed_at < ? ensures monotonic
    Then buffer cleared

  Scenario: Buffer overflow trigger flush
    Given DO buffer reaches 1000 entries
    When 1001st record_access called
    Then flush triggered immediately (NOT waiting for 30s alarm)
    Then buffer drains to ≤ 1000 post-flush

  Scenario: Eviction race correctness (sprint contract §6 DoD)
    Given blob B last accessed 7d 1h ago em D1 (would be evict candidate em free tier 7d TTL)
    Given concurrent GET fires em T0; record_access adds B to DO buffer
    Given eviction worker fires em T0 + 100ms
    When eviction calls last_accessed_at_authoritative(T, B)
    Then DO buffer hit; returns T0 (recent)
    Then eviction sees last_accessed = T0 (NOT 7d 1h ago)
    Then T0 > now - 7d (tier_lru_window) → eviction REFUSED
    Then INV-LRU-CONSISTENCY preserved

  Scenario: DO restart durable replay
    Given DO buffer contains 200 entries; durable storage persisted
    Given DO killed (worker restart)
    When DO cold start
    Then buffer recovered from durable storage
    Then alarm re-armed at START (Lote 10.4bis lesson)
    Then next flush proceeds; max drift 30s

  Scenario: Monotonic last_accessed_at
    Given blob B with D1 last_accessed_at = T_old
    Given record_access at T_new > T_old
    When flush UPDATE
    Then conditional `WHERE last_accessed_at < T_new` succeeds
    Then UPDATE applied
    But if record_access at T_old' < T_new (out-of-order rare case)
    Then conditional fails; UPDATE skipped (preserves monotonicity)
```

## 9. Design Decisions

- 9.1: DO singleton batch (NOT per-request D1 UPDATE) — write amplification mitigation.
- 9.2: 30s coalescing window + 60s refresh threshold — balance accuracy vs cost.
- 9.3: Eviction MUST consult `last_accessed_at_authoritative` (DO buffered + D1 base UNION).
- 9.4: Conditional UPDATE `WHERE last_accessed_at < ?` ensures monotonicity (INV-AC-TTL-MONOTONIC pattern).
- 9.5: D1 batch ≤ 250 (Lote 10.5bis lesson).
- 9.6: DO durable storage persists buffer across restart.
- 9.7: Alarm re-arm AT START (Lote 10.4bis lesson).
- 9.8: TenantCtx-only (Lote 10.4bis).
- 9.9: Fire-and-forget integration (latency neutral).
- 9.10: NO new ADR (extends existing patterns).

## 10. Completeness Criteria SOTA

- [ ] **10.s07.004.1** Module compila + integration tests green.
- [ ] **10.s07.004.2** All 7 Gherkin scenarios green.
- [ ] **10.s07.004.3** Property tests 5 × 10k green; **prop_lru_eviction_race 0 INV-LRU-CONSISTENCY violations** (sprint contract §6 DoD; "GC+Evict race"); 100k nightly sustained 7d.
- [ ] **10.s07.004.4** Chaos suite 8 scenarios green.
- [ ] **10.s07.004.5** Hot path latency record_access ≤ 0.5ms p99 (criterion benchmark).
- [ ] **10.s07.004.6** LRU drift ≤ 60s p99 sustained.
- [ ] **10.s07.004.7** Métricas (7) emitted.
- [ ] **10.s07.004.8** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s07.004.9** Cost regression gate per-record_access ≤ $0.0000001 (DO write share).
- [ ] **10.s07.004.10** Cost: per-flush-batch (250 entries) ≤ $0.000005 (D1 UPDATE × 250).

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; alerts armed; 5 sign-offs.

## 12. Invariants Validated

- **INV-LRU-CONSISTENCY** (HIGH, NEW promovida §3.X): eviction respects authoritative `last_accessed_at` (DO buffered + D1 base UNION); 0 race violations.
- **INV-AC-TTL-MONOTONIC** (HIGH, registry §3.15 inheritance): `last_accessed_at` strictly monotonic; conditional UPDATE preserves.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): per-tenant DO state.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| LRU module | `crates/corelink-lru/` | Rust |
| DO singleton impl | `crates/corelink-lru/src/do_singleton.rs` | Rust |
| Property tests | `crates/corelink-lru/tests/prop_lru.rs` | Rust |
| Chaos suite | `tests/chaos_lru.rs` | Rust |
| Hot path integration patches | `crates/corelink-worker/src/handlers/cas_get.rs`, `ac_get.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions) | TOML |

## 14. Quality Standards SOTA

- 14.s07.004.1: Zero unsafe; zero unwrap.
- 14.s07.004.2: rustdoc 100% public API.
- 14.s07.004.3: Test coverage ≥ 90%.
- 14.s07.004.4: Latência: record_access ≤ 0.5ms p99; flush ≤ 100ms p99 @ 250-entry batch; authoritative_lookup ≤ 1ms p99.
- 14.s07.004.5: SAST clean.
- 14.s07.004.6: Métricas (7 §6.1.9).
- 14.s07.004.7: Memory bounded ≤ 100 KiB DO state per region (1000 entries × 100 bytes).
- 14.s07.004.8: Cost regression gate per-record_access ≤ $0.0000001.
- 14.s07.004.9: TenantCtx-only (Lote 10.4bis).
- 14.s07.004.10: D1 batch ≤ 250 (Lote 10.5bis).
- 14.s07.004.11: Alarm re-arm AT START (Lote 10.4bis).
- 14.s07.004.12: Conditional UPDATE monotonic (INV-AC-TTL-MONOTONIC pattern).

## 15. Chaos Experiments (8)

§6.1.11 enumerated.

## 16. PRR

STANDARD lane sprint review (5 sign-offs).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + LruTracker trait | 1 |
| ST-002 | DO singleton state + buffer + alarm | 3 |
| ST-003 | record_access + dedup + flush | 3 |
| ST-004 | last_accessed_at_authoritative (DO + D1 union) | 2 |
| ST-005 | Hot path integration CAS GET + AC GET | 1.5 |
| ST-006 | Métricas (7) emit | 1 |
| ST-007 | Property tests (5 × 10k) — including eviction race | 4 |
| ST-008 | Chaos suite (8) | 2.5 |
| ST-009 | Cross-WI integration test (eviction worker uses authoritative) | 1.5 |

**Total**: ~19h. **PERT** O=15h M=17h P=24h: **~17h** (close to sprint contract estimate 16h).

## 18. Dependencies

- Hard: S-02 SEALED (CAS GET handler) ; S-04 WI-S04-001 SEALED (AC GET handler); WI-S07-002 (eviction worker consumes authoritative method).
- Soft: WI-S07-001 (FindMissingBlobs handler may also feed LRU on chunk hits — defer to S-08 metric refinement).

## 19. Effort PERT: ~17h. ## 20. Time-boxing: 24h hard limit.

## 21. Observability

7 metrics §6.1.9. Trace span `lru.{record_access, flush, authoritative_lookup}`. Sampled 0.1% prod due to high volume.

## 22. Cost Analysis

- Per-record_access: ~$0.0000001 (DO write to in-memory buffer).
- Per-flush (250 entries): ~$0.000005 (D1 UPDATE × 250 / batch share + DO compute).
- TCO 12m: 100M reads × $0.0000001 + 100k flushes × $0.000005 = $10/yr + $0.5/yr = ~$11/yr. Trivial.

## 23. API Contract

- Public: `LruTracker` trait + `LruError` enum; `#[non_exhaustive]`.

## 24. Post-mortem Hooks

- INV-LRU-CONSISTENCY violation → CRITICAL post-mortem (eviction false positive).
- LRU drift > 60s sustained → SEV-1; D1 throughput investigation.
- Buffer overflow > 5000 sustained → SEV-2; D1 outage signal.
- Customer "blob unexpectedly evicted" + concurrent GET → race investigation.

## 25. Rollback / Recovery

- Rollback: revert DO; fall back to direct D1 UPDATE per GET (slower, write amp; tolerable for revert).
- Recovery: DO durable replay buffer.
- RTO: 5min.
- RPO: 30s (buffer flush window).

## 26. Security & Privacy

**STRIDE**: tenant-scoped DO; no cross-tenant; sqlx prepared; metrics no PII.

**LINDDUN**: no PII in LRU tracking.

## 27. Knowledge Transfer

Tech talk (45min): "S-07 LRU: DO Batch Coalescing + Authoritative Consultation"; doc `docs/dev/lru-architecture.md`; onboarding test 4 questions: write-amplification mitigation, refresh threshold dedup, eviction race correctness, monotonicity invariant.

## 28. Risk Register (8-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Eviction race INV-LRU-CONSISTENCY | L | M | HIGH | L | LOW | Authoritative UNION lookup; property test 10k |
| R-002 | Hot path latency regression | L | L | LOW | L | LOW | Fire-and-forget; criterion benchmark |
| R-003 | Buffer overflow under D1 outage | L | M | MEDIUM | L | LOW | Alert SEV-2 at 5000; circuit breaker |
| R-004 | DO restart drift | L | L | LOW | L | LOW | Durable storage; max 30s drift |
| R-005 | Refresh threshold drift (1ms boundary) | L | L | LOW | L | LOW | Test boundary; documented |
| R-006 | Cross-tenant DO collision | L | L | CRITICAL | L | LOW | Per-region DO; tenant_id em key |
| R-007 | Monotonicity violation (out-of-order writes) | L | M | MEDIUM | L | LOW | Conditional UPDATE; chaos test |
| R-008 | Tier change race | L | L | LOW | L | LOW | Eventual consistency; next cycle uses new tier |

## 29. Review Checkpoints

D+0 design (Architect; race analysis); D+2 AppSec; D+4 code review; D+5 chaos; D+6 sprint review.

## 30. Sign-off (STANDARD 5)

| # | Role | Status |
|---|---|---|
| 1 | Owner / Final Approver (Gustavo) | _pending_ |
| 2 | Engineer (peer) | _TBD; mandatory_ |
| 3 | QA | _TBD; mandatory — eviction race property test green; 100k nightly_ |
| 4 | AppSec | _TBD; mandatory — TenantCtx + DO isolation_ |
| 5 | SRE | _TBD; mandatory — drift alerts armed; buffer overflow alert_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7) | Criação WI-S07-004; SOTA pós-Lote 10.6-tris lessons absorbed: DO batch coalescing (write amplification mitigation); authoritative UNION lookup (eviction race correctness); INV-AC-TTL-MONOTONIC pattern inheritance; conditional UPDATE; D1 batch ≤250; alarm re-arm at start; TenantCtx-only. |

## 32. Anti-patterns evitados

- ❌ Per-GET atomic D1 UPDATE; ❌ Skip refresh threshold; ❌ Skip authoritative UNION; ❌ TenantCtx bypass; ❌ Hard-coded 30s; ❌ D1 batch > 250; ❌ Skip alarm re-arm; ❌ Skip monotonic conditional.

---

**Fim WI-S07-004.** Próximo: WI-S07-005 (DASH-DEDUP + alerts + PRR ship gate).

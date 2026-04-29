---
id: "WI-S07-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-28"
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
  - "STORAGE-SEMANTICS"
  - "INVARIANT-REGISTRY"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
tags: ["wi", "s07", "eviction", "lru", "ttl", "quota", "soft-delete-first", "standard"]
---

# WI-S07-002 — Eviction Worker (`worker-evict` cron daily 02:00 UTC + jitter ±10min per region; ad-hoc trigger ≥95% quota; LRU per `blob_meta.last_accessed_at`; TTL per-tier free=7d/solo=30d/team=90d/business=365d/enterprise=365d default with admin override up to 730d max (canonical per ADR-0019; Lote 10.7-tris cycle 3 fix); soft-delete-first reusing S-06 GC grace 72h; cascade prevention; INV-GC-001 inheritance — NEVER deletes reachable blob)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-07](../_spec_contract.md) (sprint contract; sprint.md not yet authored — defer to S-07-bis if full sprint doc needed) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S07-002 |
| Título | Eviction worker (`crates/corelink-evict`); cron daily 02:00 UTC + jitter ±10min per region (PAT-JITTER-001); ad-hoc trigger quando tenant atinge 95% `tenant_quota.max_storage_bytes`; LRU policy via `blob_meta.last_accessed_at`; TTL per-tier (free=7d, solo=30d, team=90d, business=365d, enterprise=365d default with admin override cap 730d — supersedes S-04 default 90d via ADR-0019); soft-delete-first (reuses S-06 GC grace 72h via `deleted_at`); cascade prevention (BLOB-scope per Lote 10.7bis P0-8: NEVER evict blob ref'd by active `ac_meta.blob_refs`; chunk-level reachability owned by S-06 GC via `chunks.refcount`; INV-GC-001 inheritance); INV-GC-001 inheritance NEVER violates (chaos test 30d staging) |
| Sprint | S-07 |
| Lane | STANDARD |
| Forcing factors | none directly (STANDARD per sprint contract); BUT cripto-adjacent invariant inheritance: INV-GC-001 (CRITICAL) MUST hold via S-06 grace + reconcile; degrade to chaos test gate |

## 1. Intent

Eviction worker enforça **storage cost discipline** via 3 mechanisms layered:
1. **TTL per-tier**: AC entry expira após tier-specific window (free=7d ... business=365d ... enterprise=365d default with admin override cap 730d per ADR-0019).
2. **LRU**: blob_meta com `last_accessed_at < now - tier_lru_window` evicted (when quota pressure ≥95%).
3. **Quota trigger**: ad-hoc evict pass quando tenant atinge 95% `max_storage_bytes`.

CRITICAL invariant inheritance: eviction NEVER deletes reachable blob (INV-GC-001) — soft-delete-first via `deleted_at` reaproveita S-06 GC grace period 72h; physical delete delegado para WI-S06-004 cron post-grace.

```rust
// File: crates/corelink-evict/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait EvictionWorker: Send + Sync {
    /// Daily cron entry (02:00 UTC + jitter ±10min per region).
    async fn execute_daily(
        &self,
        tenant_ctx: &TenantCtx,
        region: &Region,
    ) -> Result<EvictionResult, EvictionError>;

    /// Ad-hoc trigger when tenant reaches 95% quota.
    /// Invoked by quota middleware (WI-S07-003) on threshold breach.
    async fn execute_quota_trigger(
        &self,
        tenant_ctx: &TenantCtx,
        target_bytes_to_reclaim: u64,
    ) -> Result<EvictionResult, EvictionError>;
}

pub struct EvictionResult {
    pub candidates_scanned: u64,
    pub ttl_expired_count: u64,
    pub lru_evicted_count: u64,
    pub bytes_reclaimed: u64,                  // sum blob_meta.size_bytes (planned)
    pub cascade_prevented_count: u64,          // chunks still ref'd; NOT evicted
    pub gc_grace_respected: bool,              // INV-GC-001 attestation per run
    pub duration_ms: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum EvictionError {
    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("phase budget exceeded: {duration_ms}ms > {budget_ms}ms")]
    PhaseBudgetExceeded { duration_ms: u64, budget_ms: u64 },

    #[error("INV-GC-001 violation attempted: blob {digest_hex8} still reachable; eviction refused")]
    GcInvariantViolation { digest_hex8: String },

    #[error("audit emission failed; eviction aborted (fail-closed)")]
    AuditEmissionFailed,
}
```

**Cripto-driven invariants enforced**:

1. **INV-GC-001 inheritance** (CRITICAL): eviction NEVER deletes reachable blob:
   - **Scope: blob-only eviction** (Lote 10.7bis P0-8 fix; chunk lifecycle owned by S-06 GC via `chunks.refcount` + sweep). Eviction iterates `blob_meta` (LRU candidates); reachable check counts active references via `ac_meta.blob_refs` (NOT `manifest_chunks` — that conflated blob-eviction with chunk-eviction; original spec used wrong identifier `chunk_digest = blob_digest` which never matches except for unimultipart blobs).
   - **Soft-delete-first**: eviction sets `blob_meta.deleted_at = now()` (NOT physical R2 delete).
   - **Reuse S-06 GC grace 72h**: `physical delete` happens via WI-S06-004 cron AFTER `deleted_at < now - 72h`; reconcile (WI-S06-005) catches drift.
   - **Cascade prevention**: pre-evict, verify `active_refcount = 0` query for the BLOB digest. If reachable via any active `ac_meta.blob_refs`, **DO NOT evict** (returns `Err(GcInvariantViolation)`).
   - **Race-aware reachable check SQL** (Lote 10.7bis P0-6 fix — strict-`<` predicate analogous to S-06 INV-GC-004 protects legitimate-re-ref during eviction phase):
     ```sql
     -- Capture evict_started_at_ms BEFORE scan begins (mirroring INV-GC-MARK-STARTED-AT-ATOMIC pattern from S-06):
     -- UPDATE eviction_run SET evict_started_at_ms = unix_ms_now() WHERE run_id = ?  -- commit-then-scan
     SELECT COUNT(*) AS active_refcount
     FROM ac_meta a, json_each(a.blob_refs) j
     WHERE a.tenant_id = $1
       AND j.value = $2                             -- blob_digest under eviction consideration
       AND a.deleted_at IS NULL
       AND a.created_at < $3                        -- evict_started_at_ms; AC entries with created_at >= evict_started_at_ms are PROTECTED (canonical INV-GC-004 inheritance pattern; equivalent contrapositive SQL delete predicate; protect-if->= per gc_correctness.tla L152-154 — S-06 Lote 10.6 cycle 4/5 canonical)
     ```
     - **Canonical `>=` protects (TLA-aligned)**: AC entries with `ac.created_at >= evict_started_at_ms` are PROTECTED — race-correctness: eviction must NOT delete blob with concurrent UpdateAR re-ref; same boundary semantic as S-06 INV-GC-004 protect-if-equal-or-newer per `gc_correctness.tla` L152-154 (Lote 10.6 cycle 4/5 canonical); equivalent SQL delete predicate `< evict_started_at_ms` (strict less-than for delete is the contrapositive).
     - Uses `json_each(a.blob_refs)` (NOT `LIKE '%digest%'` — Lote 10.6bis P0-1 lesson); index-friendly via existing `idx_ac_meta_tenant_deleted` (canonical pós S-06 Lote 10.6 cycle 1).
     - **Crypto SME EMPHATIC mandatory** for race-correctness derivation (analogous to S-06 INV-GC-004 review).

2. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware; NEVER from request body.

3. **Tenant-scoped strict** (sqlx prepared `WHERE tenant_id = ?`): cross-tenant eviction impossible.

4. **Audit fail-closed** (Lote 10.6bis pattern): D1 batch atomic `UPDATE blob_meta SET deleted_at` + `INSERT audit_outbox`; if audit fails, batch ROLLBACK; no silent eviction.

5. **D1 batch ≤ 250 row constraint** (Lote 10.5bis lesson): eviction batch capped at 250 candidates per D1 transaction.

6. **PAT-JITTER-001** (Lote 9.2 pattern): ±10min jitter em cron schedule prevents thundering herd cross-region.

## 2. Narrative (≥ 250 palavras + cripto-adjacent risk justification)

Eviction policy é **3-layer defense** contra storage cost overrun:
1. **TTL per-tier** (CAP-EVICT-002): free tier 7d aggressive; enterprise 365d default (admin override cap 730d per ADR-0019); reflects pricing semantics.
2. **LRU** (CAP-EVICT-001): cold blobs com `last_accessed_at < now - tier_lru_window` candidatos.
3. **Quota trigger** (CAP-EVICT-003): 95% quota → eviction worker fires ad-hoc; reclaim até voltar abaixo de 90%.

INV-GC-001 inheritance é **the single load-bearing claim**: eviction NEVER directly deletes; eviction soft-deletes (`deleted_at`); S-06 GC mark/sweep + grace + reconcile own the physical delete safety net. Cascade prevention pre-checks `active_refcount > 0` via canonical `json_each` SQL idiom (Lote 10.6bis P0-1 lesson absorbed — NEVER `LIKE` substring match on JSON column).

**Why STANDARD lane** (NOT HIGH_RISK):
- Sprint contract §2 explicit: "não toca invariantes CRITICAL; respeita INV-GC-001 via herança de enforcement do S-06; eviction é reversível dentro do grace period; worst-case é cliente retry-uploading chunk evicted".
- BUT: chaos test 30d sustained IS gating (sprint contract §6 DoD); INV-GC-001 0 violations sustained.

**Adversarial scenarios considered**:
- **Eviction race vs S-06 GC mark phase**: blob marked-as-orphan by S-06 but UpdateActionResult fires AFTER mark_started_at (legitimate re-ref); eviction must respect mark-phase-aware re-ref protection (INV-GC-004 via S-06). Mitigação: pre-evict reachable check uses same `json_each` SQL idiom; chaos test #1 simulates concurrent mark + UpdateAR + eviction.
- **Quota race condition** (FM-059): tenant at 99% quota; concurrent writes; eviction fires; both succeed → over quota. Mitigação: DO atomic counter (delegate WI-S07-003 quota middleware); eviction is async after middleware blocks new writes.
- **Eviction storm cross-region**: jitter ±10min per region distributes cron fire times; thundering herd avoided.
- **TTL config tampering**: enterprise customer attempts TTL > 730d via admin API. Mitigação: hard cap 730d em config validator; TTL > cap rejected.
- **Cascade leak**: bug em reachable check → evict chunk still ref'd → INV-GC-001 violation. Mitigação: chaos test 30d sustained; alert if `cascade_prevented_count` drops abruptly (signals bug in reachable check).

**Risk justification STANDARD**:
- Reversible via re-upload (within 72h grace window).
- Bounded blast radius (per-tenant; NEVER cross-tenant).
- Non-cripto algorithm (no key material); audit trail observability.

## 3. Customer Impact & Journey

**Persona 1 — Customer (free tier)**: AC entry for action_digest D last accessed 8d ago → expired (TTL 7d) → soft-deleted → grace 72h → physical delete; if customer re-runs same Bazel action within 72h, re-upload skip (idempotent CAS); customer-visible "cache miss" mais frequente em free tier (acceptable per pricing).

**Persona 2 — DevOps reviewing eviction**: DASH-DEDUP (forward WI-S07-005) shows `bytes_reclaimed_per_tenant_tier`; alert if anomaly (eviction spike ≥10× WoW = customer abuse OR config bug).

**Persona 3 — Compliance (LGPD)**: TTL per-tier aligns with retention policy; soft-delete-first respects 72h reversibility; physical delete audit-trailed (S-06 audit chain).

**SLA addendum**:
- Daily eviction cron 02:00 UTC + jitter ±10min per region.
- Eviction decision latency p99 ≤ 50ms (sprint contract §10.s07.2).
- Quota trigger latency: middleware-to-evict-decision ≤ 500ms p99.
- INV-GC-001 0 violations sustained 30d (sprint contract §6 DoD).
- Reversibility window: 72h post soft-delete.

## 4. Capability Mapping

- **CAP-EVICT-001** (LRU per tenant) — IMPLEMENTA primary.
- **CAP-EVICT-002** (TTL per-tier) — IMPLEMENTA primary; supersedes S-04 CAP-AC-004 default 90d (per ADR-0019).
- **CAP-EVICT-003** (Soft-pressure 95% trigger) — IMPLEMENTA primary; hard-block 100% delegado a S-08 CAP-QUOTA-001.
- **CAP-EVICT-004** (Soft-delete-first) — IMPLEMENTA primary; reuses S-06 GC grace.
- Trace: `data_model.md §4.2 tenant_quota` + `failure_modes.md FM-059, FM-305` + `invariant_registry.md INV-GC-001/003, INV-CAS-IMMUTABILITY` + ADR-0019 (TTL ownership) + ADR-0020 (quota ownership).

## 5. Tipo

Cron worker + reachable-check + tier-aware policy; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-evict/` module** — EvictionWorker trait + Cron DO impl + tests.
2. **CF Cron DO** `worker-evict-<region>`:
   - Schedule: daily 02:00 UTC.
   - Jitter: ±10min via `rand::Rng::gen_range(-600..=600)` seconds offset (PAT-JITTER-001).
   - Alarm re-arm AT START of handler (Lote 10.4bis lesson; cron-stale-on-panic recovery).
3. **TTL per-tier resolution** at runtime via tenant_quota row (forward S-13 admin override; default tier-mapped):
   ```rust
   pub fn ttl_for_tier(tier: &Tier) -> Duration {
       match tier {
           Tier::Free       => Duration::from_secs(7 * 86400),    // 7d
           Tier::Solo       => Duration::from_secs(30 * 86400),   // 30d
           Tier::Team       => Duration::from_secs(90 * 86400),   // 90d
           Tier::Business   => Duration::from_secs(365 * 86400),  // 365d
           Tier::Enterprise => Duration::from_secs(365 * 86400),  // 365d default (canonical per ADR-0019 §Decision; max 730d via admin override; Lote 10.7bis cycle 2 fix)
       }
   }
   ```
4. **TTL CHECK constraint inline** em tenant_quota (NEW migration; Lote 10.5bis lesson D1 ALTER unsupported):
   - migration `00X_tenant_quota_ttl.sql`:
     ```sql
     -- D1 ALTER TABLE ... ADD COLUMN supported (additive); CHECK constraint inline NOT possible
     -- post-CREATE — use NEW table + INSERT SELECT + DROP + RENAME if CHECK needed retroactively;
     -- here we keep TTL config em separate config-singleton DO (S-13 forward) NOT in tenant_quota row.
     -- Default tier-mapped via app-side ttl_for_tier();
     -- enterprise customer override stored em config-singleton (S-13).
     ```
5. **LRU scan SQL** (canonical idiom; bounded):
   ```sql
   -- Find candidates: blobs not accessed within tier_lru_window AND refcount=0 (reachability check)
   SELECT digest, size_bytes, last_accessed_at, deleted_at
   FROM blob_meta
   WHERE tenant_id = ?
     AND deleted_at IS NULL                       -- not already soft-deleted
     AND last_accessed_at < (? - ?)                  -- now - tier_lru_window
   ORDER BY last_accessed_at ASC                     -- coldest first
   LIMIT 250;                                         -- D1 batch cap (Lote 10.5bis lesson)
   ```
6. **Reachable check pre-evict** (Lote 10.7bis P0-6 + P0-8 fixes — race-aware strict-`<` predicate; blob-only scope; canonical `json_each` idiom from Lote 10.6bis P0-1):
   ```sql
   -- evict_started_at_ms captured BEFORE scan via UPDATE eviction_run; commit-then-scan ordering
   SELECT COUNT(*) AS active_refcount
   FROM ac_meta a, json_each(a.blob_refs) j
   WHERE a.tenant_id = ?
     AND j.value = ?                              -- blob_digest under eviction consideration
     AND a.deleted_at IS NULL
     AND a.created_at < ?;                         -- evict_started_at_ms; race-protection: AC with created_at >= evict_started_at_ms is PROTECTED (canonical INV-GC-004 inheritance per gc_correctness.tla L152-154)
   ```
   - If `active_refcount > 0` → **CASCADE PREVENTED**; `cascade_prevented_count += 1`; skip eviction; emit metric.
   - **Eviction scope is BLOB-only** (Lote 10.7bis P0-8 fix): chunks (refcount-managed via `chunks` table) are S-06 GC's domain (sweep + grace + reconcile). WI-S07-002 NEVER touches chunks directly.
7. **Soft-delete eviction batch** (D1 atomic):
   ```sql
   BEGIN;
   UPDATE blob_meta SET deleted_at = ?
     WHERE tenant_id = ? AND digest = ?
     AND deleted_at IS NULL;                       -- idempotent
   INSERT INTO audit_outbox (...) VALUES (...);
   COMMIT;
   ```
   - If audit_outbox INSERT fails → ROLLBACK (Lote 10.6bis fail-closed pattern).
8. **Quota trigger entry point** (consumed by WI-S07-003 middleware; canonical async fire-and-forget per Lote 10.7bis R5 P0-3):
   - When middleware detects `bytes_used / max_storage_bytes ≥ 0.95`, invokes `worker::send_future(execute_quota_trigger(tenant_ctx, target_bytes_to_reclaim))` — async-spawn does NOT block hot path write (canonical Lote 10.7bis R5 P0-3 fire-and-forget; aligns WI-S07-003 §6 Gherkin L298 + design decision L361; cycle 6 sync→async alignment).
   - `target_bytes_to_reclaim` = `bytes_used - 0.90 * max_storage_bytes` (reclaim until 90% headroom).
   - Async eviction phase budget 500ms p99 from trigger receipt to first soft-delete (observability target; not a sync block).
9. **Cron DO scheduler** (delegate ADR-0019 / ADR-0042 cron pattern):
   - `wrangler.toml` cron entry: `cron = "0 2 * * *"` per region.
   - Sticky DO per region; alarm re-arm at start.
10. **Métricas** (spec uses CloudEvent dotted naming `corelink.evict.*`; Prometheus exposed metric names replace dots with underscores per `prometheus.io/docs/practices/naming/` convention — e.g., `corelink.evict.cron_fired_total` → `corelink_evict_cron_fired_total` em queries; Lote 10.7-tris cycle 6 clarification):
    - `corelink.evict.cron_fired_total{region}` (counter).
    - `corelink.evict.candidates_scanned_total{tenant_id}` (counter).
    - `corelink.evict.ttl_expired_total{tenant_id, tier}` (counter).
    - `corelink.evict.lru_evicted_total{tenant_id}` (counter).
    - `corelink.evict.bytes_reclaimed_total{tenant_id, tier}` (counter; consumed by DASH-DEDUP).
    - `corelink.evict.cascade_prevented_total{tenant_id}` (counter; alert if drops abruptly = bug signal).
    - `corelink.evict.quota_trigger_fired_total{tenant_id}` (counter).
    - `corelink.evict.duration_ms{region}` (histogram; SLO ≤ phase budget).
    - `corelink.evict.gc_invariant_violation_total` (counter; **alert if > 0; SEV-0** — canonical INV-GC-001 inheritance is CRITICAL severity per invariant_registry; aligns WI-S07-005 ship-gate; Lote 10.7-tris cycle 5 fix).
11. **Property tests** (10k iter PR; 100k nightly):
    - `prop_evict_idempotent`: re-run on already soft-deleted = no-op.
    - `prop_evict_tenant_isolation`: 1000 concurrent across tenants; no cross-tenant impact.
    - `prop_evict_cascade_prevention`: blob ref'd by active `ac_meta.blob_refs` (created_at < evict_started_at_ms) → eviction refused; 100% INV-GC-001 preserved (BLOB-scope per Lote 10.7bis P0-8; chunk reachability = S-06 GC scope).
    - `prop_evict_gc_race`: simulate S-06 mark phase concurrent with eviction; 0 INV-GC-001 violations.
    - `prop_evict_quota_trigger`: tenant at 95% quota → trigger fires; reclaims to 90%; 0 false positives.
12. **Chaos suite** (8 scenarios; STANDARD floor 6 + 2 margin):
    - 1. **GC race**: S-06 mark phase fires; eviction fires; INV-GC-001 0 violations (chaos test 30d sustained per sprint contract DoD).
    - 2. **Cascade prevention**: blob ref'd by 5 active AC entries (`ac_meta.blob_refs`); eviction attempts; refused 5/5; cascade_prevented metric 5 (BLOB-scope per Lote 10.7bis P0-8).
    - 3. **Cron stale**: worker panics mid-eviction; alarm re-arm-at-start triggers; no missed cycle.
    - 4. **Quota trigger storm**: 1000 tenants reach 95% simultaneously; per-tenant trigger; no cross-tenant interference.
    - 5. **Audit fail-closed**: audit_outbox INSERT fails; D1 batch ROLLBACK; no silent eviction.
    - 6. **Tier TTL respect**: free tier blob 6d 23h ≤ TTL 7d → NOT evicted; 7d 1ms > TTL → evicted (boundary check).
    - 7. **Enterprise TTL > 730d override attempt**: admin API attempts TTL=800d → rejected by config validator (730d hard cap).
    - 8. **D1 throttle**: backoff retry; eventual consistency; no silent skip.

### 6.2 Out-of-scope (deferred)

- Cross-tenant eviction (impossible by design; not in-scope).
- Hard-block 100% quota (delegate S-08 CAP-QUOTA-001).
- Predictive ML LRU (anti-scope per sprint contract §10).
- Compression-aware eviction (anti-scope; compression deferred pós-GA).
- Customer-facing reclaim metric (delegate WI-S07-005 + S-16).
- Cross-region eviction coordination (per-region cron; no cross-region needed).

## 7. Anti-Scope

- ❌ Hard-delete (skip soft-delete-first).
- ❌ Skip reachable check (cascade leak risk).
- ❌ Skip GC grace 72h (S-06 inheritance).
- ❌ Cross-tenant eviction.
- ❌ Skip audit fail-closed.
- ❌ TTL override > 730d max (enterprise hard cap).
- ❌ TenantCtx bypass.
- ❌ D1 batch > 250 (Lote 10.5bis lesson).
- ❌ Skip jitter (thundering herd risk).
- ❌ `LIKE '%digest%'` em reachable check (Lote 10.6bis P0-1 lesson; use `json_each`).

## 8. Acceptance Criteria (Gherkin) — 9 scenarios

```gherkin
Feature: Eviction worker LRU + TTL + quota trigger

  Scenario: Daily cron TTL eviction (free tier)
    Given tenant T (tier=free) with blob B last_accessed_at = now - 8d
    Given blob B has active_refcount = 0 (no `ac_meta.blob_refs` referencing B; chunk-level lifecycle = S-06 GC scope per Lote 10.7bis P0-8)
    When cron fires at 02:XX UTC (jittered)
    Then SELECT identifies B as candidate (last_accessed < now - 7d)
    Then reachable check returns active_refcount = 0
    Then UPDATE blob_meta SET deleted_at = now() committed
    Then audit emit corelink.evict.executed succeeded
    Then metric ttl_expired_total{tier=free} += 1
    Then metric bytes_reclaimed_total += B.size_bytes

  Scenario: Daily cron TTL boundary (free tier; NOT evicted)
    Given tenant T (tier=free) with blob B last_accessed_at = now - 6d 23h 59m
    When cron fires
    Then last_accessed > now - 7d (TTL boundary; strict <)
    Then B NOT marked candidate
    Then deleted_at remains NULL

  Scenario: Cascade prevention (blob ref'd by active AC)
    Given blob B (digest=B)
    Given ac_meta has row (tenant=T, blob_refs contains B, deleted_at IS NULL, created_at < evict_started_at_ms)
    When eviction attempts B
    Then reachable check returns active_refcount = 1 (ac_meta.blob_refs hit; canonical INV-GC-004 inheritance per Lote 10.7bis P0-8 scope-reduce)
    Then eviction REFUSED; cascade_prevented_count += 1
    Then deleted_at remains NULL
    Then audit emit corelink.evict.cascade_prevented

  Scenario: Quota trigger (95% threshold)
    Given tenant T at 96% storage quota
    Given middleware (WI-S07-003) detects breach
    When middleware invokes execute_quota_trigger(T, target_bytes=10GB)
    Then worker scans tenant T blob_meta for LRU candidates
    Then reachable check + soft-delete batch (D1 ≤250)
    Then bytes_reclaimed_total += 10GB (target met)
    Then bytes_used drops to ~90% of quota

  Scenario: GC race (S-06 mark phase concurrent)
    Given S-06 GC mark phase running for tenant T at T_mark_start
    Given UpdateActionResult fires at T_mark_start + 100ms (legitimate re-ref)
    When eviction worker fires at T_mark_start + 200ms
    Then eviction reachable check uses json_each canonical idiom
    Then eviction observes ac_meta.created_at >= T_mark_start (re-ref protection)
    Then eviction REFUSED for that blob
    Then INV-GC-001 0 violations sustained

  Scenario: TTL > 730d enterprise override rejected
    Given enterprise customer requests TTL = 800d via admin API (S-13 forward)
    When config validator processes
    Then 800d > 730d max (CAP-EVICT-002 hard cap)
    Then admin API returns 422 + COR_S07_TTL_EXCEEDS_MAX
    Then tenant config unchanged

  Scenario: Audit fail-closed
    Given audit_outbox INSERT fails (D1 throttle simulated)
    When eviction batch executes
    Then D1 BEGIN/COMMIT rollbacks
    Then deleted_at NOT updated (atomic)
    Then SEV-1 alert fired
    Then no silent eviction

  Scenario: Idempotent re-run
    Given blob B already soft-deleted (deleted_at = T_old)
    When cron re-runs
    Then SELECT WHERE deleted_at IS NULL omits B
    Then B not re-counted; not double-emitted audit

  Scenario: Daily cron jitter ±10min
    Given cron schedule 02:00 UTC per region
    When 5 regions fire on same day
    Then fire times distributed across [01:50..02:10] UTC
    Then no thundering herd cross-region (each region fires at slightly different time)
```

## 9. Design Decisions

- 9.1: Soft-delete-first reusing S-06 grace 72h; physical delete delegated.
- 9.2: Per-tier TTL hard-coded em `ttl_for_tier()` (config-driven via S-13 admin override default tier-mapped); enterprise hard cap 730d.
- 9.3: Reachable check uses canonical `json_each` SQL idiom (Lote 10.6bis P0-1 lesson absorbed).
- 9.4: D1 batch ≤ 250 per scan (Lote 10.5bis lesson).
- 9.5: PAT-JITTER-001 ±10min cron jitter (thundering herd prevention).
- 9.6: Alarm re-arm AT START (Lote 10.4bis lesson; cron-stale-on-panic recovery).
- 9.7: Audit fail-closed em D1 batch (Lote 10.6bis pattern).
- 9.8: TenantCtx-only enforcement (Lote 10.4bis lesson).
- 9.9: ADR-0019 forward (TTL ownership) + ADR-0020 forward (quota ownership) ratificação confirmation in WI-S07-005 PRR.
- 9.10: NO new ADR needed (ADR-0019 + ADR-0020 already cover scope).

## 10. Completeness Criteria SOTA

- [ ] **10.s07.002.1** Module compila + integration tests green.
- [ ] **10.s07.002.2** All 9 Gherkin scenarios green.
- [ ] **10.s07.002.3** Property tests 5 × 10k green; 100k nightly sustained 7d.
- [ ] **10.s07.002.4** Chaos suite 8 scenarios green; **30d sustained chaos zero INV-GC-001 violations** (sprint contract §6 DoD).
- [ ] **10.s07.002.5** Cron DO live em wrangler.toml; alarm re-arm at start.
- [ ] **10.s07.002.6** Métricas (9) emitted.
- [ ] **10.s07.002.7** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s07.002.8** Cost regression gate per-eviction ≤ $0.000005.
- [ ] **10.s07.002.9** Eviction decision latency ≤ 50ms p99 (sprint contract §10.s07.2).
- [ ] **10.s07.002.10** Quota trigger latency middleware-to-evict ≤ 500ms p99.
- [ ] **10.s07.002.11** RB-FM-305 (tombstone lost) dry-run + RB-FM-059 (DO quota exceeded) dry-run executed (sprint contract §R-S07-9).
- [ ] **10.s07.002.12** TTL boundary chaos test: 6d 23h 59m NOT evicted; 7d 1ms IS evicted.

## 11. DoD

- [ ] Module SEALED; all Gherkin/property/chaos green; integration tests green; alerts armed; runbooks dry-runned; Architect + AppSec + SRE + QA reviews.

## 12. Invariants Validated

- **INV-GC-001** (CRITICAL, registry §3.4 + TLA+): inherited via S-06 GC grace + reconcile; eviction NEVER violates (chaos test 30d sustained).
- **INV-GC-003** (HIGH): refcount consistency preserved via cascade prevention (NEVER evict ref'd blob; BLOB-scope per Lote 10.7bis P0-8 — chunk-level refcount owned by S-06 GC reconcile WI-S06-005).
- **INV-CAS-IMMUTABILITY** (CRITICAL): eviction is metadata `deleted_at` mark; chunk body untouched.
- **INV-DEDUP-CONSISTENCY** (HIGH; WI-S07-001 base): cascade prevention preserves dedup invariant.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): tenant-scoped strict.
- **INV-EVICT-SOFT-DELETE-FIRST** (HIGH, NEW promovida — register em §3.X): eviction sets `deleted_at`, NEVER R2 DELETE direct.
- **INV-EVICT-CASCADE-PREVENTED** (HIGH, NEW): pre-evict reachable check refuses if `active_refcount > 0`.
- **INV-EVICT-TTL-CAP-RESPECTED** (MEDIUM, NEW): enterprise TTL ≤ 730d hard cap.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Eviction module | `crates/corelink-evict/` | Rust |
| Property tests | `crates/corelink-evict/tests/prop_evict.rs` | Rust |
| Chaos suite | `tests/chaos_evict.rs` | Rust |
| Wrangler cron DO binding | `wrangler.toml` (additions) | TOML |
| RB-FM-305 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-305-dry-run-s07.md` | Markdown |
| RB-FM-059 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-059-dry-run-s07.md` | Markdown |

## 14. Quality Standards SOTA

- 14.s07.002.1: Zero unsafe; zero unwrap em production paths.
- 14.s07.002.2: rustdoc 100% public API.
- 14.s07.002.3: Test coverage ≥ 90%.
- 14.s07.002.4: Latência: per-eviction decision ≤ 50ms p99; per-cron-tick ≤ phase budget 30 min @ 100k candidates.
- 14.s07.002.5: SAST clean.
- 14.s07.002.6: Métricas (9 §6.1.10).
- 14.s07.002.7: Memory bounded ≤ 1 MiB stack per cron tick.
- 14.s07.002.8: Cost regression gate per-eviction ≤ $0.000005.
- 14.s07.002.9: D1 batch ≤ 250 (Lote 10.5bis lesson).
- 14.s07.002.10: Reachable check uses `json_each` (Lote 10.6bis P0-1 lesson).
- 14.s07.002.11: Audit fail-closed pattern (Lote 10.6bis pattern).
- 14.s07.002.12: PAT-JITTER-001 ±10min cron jitter.
- 14.s07.002.13: Alarm re-arm AT START (Lote 10.4bis lesson).

## 15. Chaos Experiments (8)

§6.1.12 enumerated.

## 16. PRR

STANDARD lane sprint review (5 sign-offs); no full HIGH_RISK PRR.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + EvictionWorker trait | 1.5 |
| ST-002 | Cron DO + alarm re-arm + jitter | 2.5 |
| ST-003 | LRU scan SQL + tier TTL resolution | 3 |
| ST-004 | Reachable check (json_each canonical) | 2 |
| ST-005 | Soft-delete batch + audit fail-closed | 2.5 |
| ST-006 | Quota trigger entry point | 1.5 |
| ST-007 | Métricas (9) emit | 1.5 |
| ST-008 | Property tests (5 × 10k) | 4 |
| ST-009 | Chaos suite (8) | 4 |
| ST-010 | RB-FM-305 dry-run + report | 2 |
| ST-011 | RB-FM-059 dry-run + report | 2 |

**Total**: ~26h. **PERT** O=22h M=24h P=36h: **~25h** (matches sprint contract estimate 24h).

## 18. Dependencies

- Hard: S-05 SEALED (manifest_chunks); S-06 SEALED (GC grace + reconcile + INV-GC-001/004 enforcement); WI-S01-001 (CAS write handler base).
- Soft: WI-S07-003 (quota middleware invokes trigger); WI-S07-004 (last_accessed_at hot path); S-13 (admin API for TTL override).

## 19. Effort PERT: ~25h. ## 20. Time-boxing: 36h hard limit.

## 21. Observability

9 metrics §6.1.10. Trace span `evict.{cron, ttl_scan, lru_scan, quota_trigger, soft_delete, cascade_check}`. Log structured `corelink.evict.decision{tenant_id, digest_hex8, reason, bytes, latency_ms}` (1% sampled prod).

## 22. Cost Analysis

- Per-eviction: ~$0.000005 (D1 read + UPDATE + audit emit).
- Per-cron-tick (100k candidates worst case): ~$0.50 (100k × $0.000005).
- TCO 12m: 5 regions × 1k tenants × 1 evict/dia × 365 dias × ~10 candidates/tenant/dia × $0.000005 = ~$913/yr.

## 23. API Contract

- Public: `EvictionWorker` trait + `EvictionResult`, `EvictionError` types; `#[non_exhaustive]`.
- Cron: wrangler.toml schedule binding.

## 24. Post-mortem Hooks

- INV-GC-001 violation detected → CRITICAL post-mortem (data loss reachable; sprint contract §18 trigger).
- Cascade prevention drops abruptly (signals reachable check bug) → SEV-1 + 5-Why.
- Eviction storm cross-region (jitter failed) → SEV-2 + jitter validation.
- Customer reports "blob lost" + recent eviction → CRITICAL (FM-300 adjacent).

## 25. Rollback / Recovery

- Rollback: revert eviction worker via wrangler version revert; re-upload via customer (if within 72h grace, soft-delete reversible via re-upload mesma digest).
- Recovery: degrade-mode `evict-pause` global stop via DO config singleton (S-13 forward).
- RTO: 5min (revert) + 72h reversibility for soft-deleted (S-06 grace).
- RPO: 0 (within grace) ; data loss only post-grace (acceptable per pricing).

## 26. Security & Privacy

**STRIDE delta**:
- **T (Tampering)**: TTL config tampering blocked by validator (730d hard cap).
- **R (Repudiation)**: every eviction audit-emitted (S-09 chain).
- **I (Information disclosure)**: no PII in eviction logs (digest hex first 8 chars only).
- **D (DoS)**: jitter prevents thundering herd; phase budget caps per-cron-tick work.

**LINDDUN delta**:
- **L (Linkability)**: per-tenant scope; no cross-tenant correlation possible.
- **U (Unawareness)**: customer-visible "bytes_reclaimed_last_30d" metric (forward S-16).
- **C (Compliance)**: LGPD Art. 16 retention compliance via TTL per-tier; soft-delete reversibility 72h.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-07 Eviction: 3-Layer Defense (TTL + LRU + Quota); INV-GC-001 inheritance; soft-delete-first"; doc `docs/dev/eviction-architecture.md`; onboarding test 6 questions: TTL per-tier rationale, cascade prevention, INV-GC-001 inheritance via S-06 grace, soft-delete-first vs hard-delete, json_each idiom (Lote 10.6bis lesson), PAT-JITTER-001 thundering herd prevention.

## 28. Risk Register (10-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-GC-001 violation (eviction deletes reachable) | L | M | CRITICAL | L | LOW | Cascade prevention pre-evict + S-06 grace + chaos test 30d sustained |
| R-002 | Cascade leak via reachable check bug | L | M | CRITICAL | L | LOW | json_each canonical idiom (Lote 10.6bis); property test 100k iter |
| R-003 | Quota trigger storm cross-region | M | L | LOW | L | LOW | Per-region cron; jitter ±10min; thundering herd avoidance |
| R-004 | TTL config tampering > 730d | L | L | MEDIUM | L | LOW | Hard cap em config validator; rejection 422 |
| R-005 | D1 throttle on reachable check | M | L | MEDIUM | L | LOW | Backoff retry; circuit breaker |
| R-006 | Customer "lost data" complaint (within grace) | L | L | LOW | L | LOW | 72h grace reversibility; re-upload idempotent |
| R-007 | Audit fail-closed false positive (audit fails legit) | L | M | MEDIUM | L | LOW | Retry audit emit before ROLLBACK |
| R-008 | Cron stale on panic | L | L | MEDIUM | L | LOW | Alarm re-arm AT START (Lote 10.4bis) |
| R-009 | Eviction race vs S-06 mark phase | L | M | HIGH | L | LOW | Reachable check uses same json_each + S-06 mark_started_at consistency |
| R-010 | LRU last_accessed_at race (WI-S07-004 dep) | M | L | LOW | L | LOW | Eventual consistency tolerable for LRU; bounded drift documented |

## 29. Review Checkpoints

D+0 design (Architect + Crypto SME advisory for INV-GC-001 inheritance review); D+2 AppSec; D+4 code review; D+6 chaos validation; D+7 sprint review.

## 30. Sign-off (STANDARD 5 + 1 advisory)

| # | Role | Status |
|---|---|---|
| 1 | Owner / Final Approver (Gustavo) | _pending_ |
| 2 | Engineer (peer) | _TBD; mandatory_ |
| 3 | QA | _TBD; mandatory — chaos suite + property test green; 30d sustained zero INV-GC-001 violations_ |
| 4 | AppSec | _TBD; mandatory — TenantCtx + cascade prevention + audit fail-closed verified_ |
| 5 | SRE | _TBD; mandatory — RB-FM-305/059 dry-runs + alerts armed_ |
| _adv_ | Crypto SME | _advisory — INV-GC-001 inheritance review (S-06 grace + reconcile correctness)_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7) | Criação WI-S07-002; SOTA pós-Lote 10.6-tris lessons absorbed: json_each canonical idiom (P0-1 lesson); TenantCtx-only; D1 batch ≤250; CHECK inline; PAT-JITTER-001; alarm re-arm at start; audit fail-closed; soft-delete-first reusing S-06 grace; INV-GC-001 inheritance via cascade prevention; ADR-0019/0020 forward references. |

## 32. Anti-patterns evitados

- ❌ Hard-delete (bypass S-06 grace); ❌ Skip reachable check (cascade leak); ❌ `LIKE '%digest%'` (json_each canonical); ❌ Cross-tenant; ❌ Skip audit; ❌ Skip jitter; ❌ Skip alarm re-arm; ❌ TTL > 730d; ❌ Hard-coded TTL (use ttl_for_tier()); ❌ D1 batch > 250.

---

**Fim WI-S07-002.** Próximo: WI-S07-003 (Quota enforcement middleware + tipado error).

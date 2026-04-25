---
id: "WI-S04-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-04"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s04", "ac", "ttl", "durable-object", "cron", "eviction", "adr-0019", "high-risk"]
---

# WI-S04-005 — TTL Worker (Cron Durable Object) + Refresh-on-Hit Infrastructure + S-07 Boundary Alignment ADR-0019 + Tenant-Scoped Expiry Enforcement + Bounded Batch Size + RB-FM-303 Forward

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-005 |
| Título | TTL infrastructure: Cron Durable Object expires `ac_meta` rows + R2 envelopes; refresh-on-hit synchronous in handler GET path; per-tenant TTL via S-07 / ADR-0019 boundary; bounded batch size to avoid D1/R2 throttle; tenant-scoped expiry (never cross-tenant DELETE); chaos test stale read handling under load; runbook RB-FM-AC-TTL-DRIFT |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (TTL bug deletes wrong tenant rows = catastrophic), FF-HR-005 (CTRL-AC-001/-002 invariants enforced via TTL flow) |

## 1. Intent

Implementar TTL infrastructure que expira AC entries respeitando per-tier defaults (S-07/ADR-0019) sem violar tenant scoping:

```text
Components:

1. **Refresh-on-hit (synchronous in GetActionResult flow)**:
   - WI-S04-001 step [6] já invoca: D1 UPDATE ac_meta SET last_hit_at = now(), expires_at = now() + tier_ttl WHERE (tenant_id, action_digest) = (?, ?).
   - Esta WI provê: tier_ttl resolution function (delegated S-07 ADR-0019); env-config fallback during S-04 GA pre-S-07.

2. **Cron Durable Object `AcTtlWorker`**:
   - Schedule: every 1 hour (configurable via `CORELINK_AC_TTL_CRON_INTERVAL_S`; default 3600).
   - Per-region instance (5 regions sam/iad/lhr/nrt/syd); shard work by region.
   - Bounded batch: 1000 rows per cron tick; sleep 100ms between batches to avoid D1/R2 throttle.

3. **Expiry job flow** (per cron tick):
   [1] D1 SELECT tenant_id, action_digest, region FROM ac_meta WHERE expires_at < now() AND region = $1 LIMIT 1000 ORDER BY expires_at ASC
   [2] FOR EACH expired row:
       a. R2 DELETE ac-<region>/<tenant_prefix>/<action_digest>.json
       b. D1 DELETE FROM ac_meta WHERE tenant_id = ? AND action_digest = ? (tenant-scoped; safety)
       c. KV invalidate ac_neg:<tenant_prefix>:<action_digest> (proactive for retry signal)
       d. Audit emit ac.evict.ttl_expired (outbox)
   [3] Métricas: rows_evicted_total, region, batch_size, latency.
   [4] Sleep 100ms; repeat until batch returns < 1000 rows OR cron interval ends.
```

```rust
// File: crates/corelink-worker/src/ac/ttl/mod.rs

#[durable_object]
pub struct AcTtlWorker {
    state: State,
    env: Env,
}

#[durable_object]
impl DurableObject for AcTtlWorker {
    async fn alarm(&mut self) -> Result<Response> {
        let region = self.env.var("AC_TTL_WORKER_REGION")?;  // sam|iad|lhr|nrt|syd
        let batch_size = self.env.var("AC_TTL_BATCH_SIZE")?.parse::<usize>().unwrap_or(1000);
        let mut total_evicted = 0;
        loop {
            let expired = self.fetch_expired_batch(&region, batch_size).await?;
            if expired.is_empty() { break; }
            self.evict_batch(&region, &expired).await?;
            total_evicted += expired.len();
            if expired.len() < batch_size { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // Re-arm alarm
        let next = SystemTime::now() + Duration::from_secs(3600);
        self.state.storage().set_alarm(next).await?;
        // Métricas + audit emit
        emit_metric!("corelink.ac.ttl.rows_evicted_total", total_evicted, region);
        Ok(Response::ok("ok")?)
    }
}

pub trait TierTtlResolver: Send + Sync {
    /// Resolve TTL duration for given tenant_tier.
    /// During S-04 GA: env-config fallback (90d default).
    /// Post-S-07 SEALED: delegated to S-07 CAP-EVICT-002 per-tier table (ADR-0019).
    fn resolve_ttl(&self, tenant_tier: TenantTier) -> Duration;
}

// S-04 GA fallback impl (env-config)
pub struct EnvConfigTierTtlResolver { /* ... */ }

// S-07 forward impl (config-singleton)
pub struct S07PerTierTtlResolver { /* ... */ }
```

**ADR-0019 boundary** (already accepted; this WI implements):

> **ADR-0019 — AC TTL ownership: S-07 supersedes S-04 (per-tier TTL)**
>
> S-04 entrega **infrastructure** (TTL worker DO + refresh-on-hit + expiry job).
> S-07 supersede **default value** via per-tier table:
> - Free 7d, Solo 30d, Team 90d, Business 365d, Enterprise customer-configurable.
>
> **Boundary**: S-04 reads tier_ttl via `TierTtlResolver` trait; impl swappable post-S-07 SEALED.

**Constraint cripto-driven (tenant scoping)**:

1. **DELETE tenant-scoped strict**: SQL `DELETE FROM ac_meta WHERE tenant_id = ? AND action_digest = ?` — never `WHERE expires_at < ?` alone (would cross-tenant delete). **Lote 10.4bis P0 fix**: CI gate (clippy lint OR grep) em `crates/corelink-worker/src/ac/ttl/` enforce que todo DELETE statement inclui `tenant_id = ?` clause.
2. **R2 path scoping**: DELETE reads `tenant_prefix` BLOB(16) materialized em `ac_meta` column (Lote 10.4bis P0 fix: WI-S04-002 schema fix); cron worker uses column directly **without TDK access** (avoids cron trust boundary expansion). Previously spec was silent on cron derivation, implying cron needed TDK to re-compute HMAC — closed by column materialization. Layer 4 path consistency with WI-S04-001 handler INSERT.
3. **Bounded batch**: **Lote 10.4bis P0 fix: 250 rows/tick** (was 1000; reduced for D1 batch 100KB limit — 1000 rows × ~200B audit-event = 200KB+ exceeds D1 batch limit). 250 × ~400B (D1 DELETE + audit_outbox INSERT pair) = 100KB. Sleep 100ms between; avoid D1 lock contention + R2 rate limit.
4. **Per-region shard**: each region's TTL worker only deletes its own region's rows; cross-region deletion impossible by design (region filter in SELECT). **Lote 10.4bis P0 fix**: stagger alarms across regions (sam=00, iad=12, lhr=24, nrt=36, syd=48 minutes past hour) to spread D1 lock contention; documented em §6.1 cron config.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

TTL flow has DUAL nature:
- **Synchronous refresh-on-hit** (in WI-S04-001 GET handler): customer-facing; latency-sensitive; ≤ 10ms p99 budget for D1 UPDATE.
- **Async batch eviction** (this WI's Cron DO): background; off hot path; can take 1h to complete batch.

HIGH_RISK em N dimensões:

1. **Cross-tenant DELETE**: bug em SQL WHERE clause `DELETE WHERE expires_at < ?` (sem tenant_id filter) → deletes ALL expired rows globally; if pagination buggy, cross-tenant rows entangled. Catastrophic FM-303-adjacent. Mitigação: SQL DELETE always `WHERE tenant_id = ? AND action_digest = ?`; never bulk-delete by expires_at alone; integration test asserts.

2. **R2 vs D1 inconsistency** (Lote 10.4bis P0 fix: clarified flow + atomicity boundaries):
   - **Step order**: R2 DELETE (external) → on success → D1 DELETE + audit_outbox INSERT (atomic D1 batch) → KV invalidate (post-batch).
   - **R2 DELETE fails (5xx)**: retry once with backoff; if persistent, abort batch row + preserve D1 row (still valid R2 reference); audit emit `ac.evict.r2_failed`; metric alert; next tick retries (idempotent: `expires_at < now` still selects row).
   - **D1 batch fails after R2 success**: R2 envelope already deleted; D1 row references dead R2; subsequent GET returns 404 from R2-not-found path (customer-acceptable — entry expired); next cron tick re-attempts D1 DELETE (idempotent). Orphan window = 1 cron interval.
   - **D1 batch atomicity**: D1 DELETE + audit_outbox INSERT atomic via single `db.batch([...])` call; both succeed OR both rollback. R2 DELETE is OUTSIDE the batch (precedes it); the atomicity is between D1 ops only, NOT the R2-D1 pair.
   - Reverse ordering (D1-first then R2): leaves orphan R2 unreferenced; S-06 GC reconcile catches but slower fix; chosen ordering (R2-first) preferred.

3. **Refresh-on-hit storm**: every GET triggers UPDATE last_hit_at + expires_at; high-rate workload (100 req/s same digest) = 100 D1 UPDATE/s = lock contention. Mitigação: refresh-on-hit only if last_hit_at < now - refresh_threshold (e.g., 60s); reduces UPDATE rate proportionally; integration test under 1k req/s burst.

4. **Cron DO sharding (5 regions)**: each region's worker independent; coordination not needed (region scoping). BUT global expiry policy change (e.g., emergency TTL=0 for all tenants pos-incident) requires coordinated update; central control plane needed. Mitigação: env-config single source-of-truth; each DO reads at alarm fire; no distributed state.

5. **Batch size too large** (Lote 10.4bis P0 fix: D1 batch 100KB limit conflict): 10000 rows per batch → D1 timeout → partial commit → inconsistent state. Mitigação: **batch size 250** (was 1000 in v1.0; reduced em Lote 10.4bis para D1 batch 100KB limit — each row produces D1 DELETE + audit_outbox INSERT pair ~400 bytes; 250 × 400 = 100KB exactly aligned; R2 DELETE is sequential outside D1 batch). R2 DELETE 250 sequential takes ~2.5s OK within 30s alarm budget. Multiple batches per tick com sleep 100ms; loop until batch returns < 250 OR alarm budget exhausted.

6. **TTL jitter**: all expires_at exactly at hour boundary → cron tick at hour boundary expires all simultaneously → R2 DELETE storm. Mitigação: refresh-on-hit adds jitter (±10% randomization); batch-bounded eviction smooths storm; alert if batch_size hits cap continuously (sustained workload too high; needs scale).

7. **Negative cache invalidation oversight**: TTL evicts row in D1+R2; KV `ac_neg:` not populated previously (last GET was hit, not miss); customer immediately re-uploads → UPDATE handler → KV.delete is idempotent (no-op). OK. BUT: customer GET right after eviction → 404 → KV populated 60s; then customer UPDATE → KV.delete; then customer GET → ok. Race: 60s window between eviction + re-upload, customer sees miss. Acceptable (customer expects expiry; RE-UPLOAD-OR-RETRY semantics).

8. **Per-tenant TTL pre-S-07 GA**: only env-config fallback (single global default 90d); per-tier override coming S-07. Mitigação: ADR-0019 documents handoff; S-04 GA does NOT promise per-tier (admin override S-13 manual workaround); customers on free tier may have unexpected long TTL pre-S-07 (acceptable; conservative; no premature data loss).

9. **Cron schedule drift**: alarm not re-armed → worker dies silently → TTL never expires → D1+R2 storage grows. Mitigação: alarm re-arm at end of each tick; CI test asserts re-arm logic; metric alert if cron tick rate < expected (1/h × 5 regions = 120/day; alert if < 100/day).

**Atacante adversarial scenarios**:

- **TTL extension attack**: legitimate customer triggers refresh-on-hit at high rate to keep entries forever; storage growth attack. Mitigação: rate limit S-08 forward; per-tenant storage quota (tenant_quota table; S-07/S-08); enforce limit in handler.

- **Cross-tenant DELETE via SQL injection**: defense-in-depth. Mitigação: sqlx prepared statements; parameter binding; never raw concat; ADR-0036 documents.

- **Bulk DELETE replay**: attacker with admin access (S-13 forward) replays bulk eviction with elevated permissions. Mitigação: S-13 admin plane authorization; audit chain captures.

- **R2 DELETE bombing** (DoS via cron): malformed cron schedule fires every 1s; R2 rate limit hits; legitimate ops queue. Mitigação: cron interval bounded (≥ 60s); chaos test rapid-fire scenarios.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: cross-tenant DELETE = catastrophic data loss FM-303-adjacent.
- **FF-HR-005**: TTL flow enforces invariants (expires_at boundary; no orphan envelopes); bug = INV-AC-* drift.
- **Reversibility**: deletion irreversible (R2+D1); pre-deploy validation mandatory; rollback test simulates partial fail.
- **Customer impact**: aggressive TTL = customer build slow (cache miss); conservative TTL = storage cost growth.

13 sign-offs incl. Architect (cron DO design + sharding), DBA (D1 batch semantics), AppSec (cross-tenant DELETE prevention).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**:
- Builds frequently; refresh-on-hit keeps entries warm during active project; entries expire when project goes idle.
- Customer-visible: cache hit ratio drops gradually for unused projects (~7-90d depending on tier); re-execute action populates fresh.
- No customer action required; transparent expiry.

**Persona 2 — DevOps reviewing storage cost**:
- Reviews `corelink.r2.ac_bucket.size_bytes{region}` metric; sees gradual decrease as TTL expires.
- Reviews `corelink.ac.ttl.rows_evicted_total{region}` rate; alerts if zero (cron not firing).
- Customer dashboard S-16 shows "X GB storage used; Y entries (avg age Zd)".

**Persona 3 — Compliance reviewer (LGPD/GDPR retention)**:
- Reviews retention policy: AC entries auto-expire per tier; DSR cascade S-11 forward complementary.
- ADR-0019 documents per-tier defaults; alignment with retention model.

**SLA addendum**:
- Refresh-on-hit p99 ≤ 10ms (synchronous in GET; D1 UPDATE).
- Cron DO tick: ≤ 30s per batch of 1000 rows.
- Eviction lag: row expires at T; cron picks up next tick (≤ 1h post-expiry); R2+D1 cleared.
- Stale window post-expiry: ≤ 1h max (cron interval); GET returns 410 expired pre-cron-pickup; 404 post-cron.
- Per-region cron isolation: 5 independent workers; cross-region coordination not needed.

## 4. Capability Mapping

- **CAP-AC-004** (AC TTL management infrastructure) — IMPLEMENTA primary.
- **CAP-EVICT-002** (S-07) — IMPLEMENTA partial (infrastructure; defaults superseded S-07 ADR-0019).
- Trace: `data_model.md §4.2 ac_meta.expires_at` + `storage_semantics.md §5.4 lifecycle policies` + `failure_modes.md FM-AC-TTL-DRIFT forward` + ADR-0019.

## 5. Tipo

Cron DO infra + refresh-on-hit hooks; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Module `crates/corelink-worker/src/ac/ttl/`**:
   - `mod.rs`: re-exports.
   - `worker.rs`: AcTtlWorker Durable Object impl.
   - `resolver.rs`: TierTtlResolver trait + impls (EnvConfig + S07PerTier forward).
   - `evict.rs`: batch eviction logic.
   - `refresh.rs`: refresh-on-hit synchronous logic (used by WI-S04-001 handler).

2. **TierTtlResolver trait**:
   - `resolve_ttl(tenant_tier) -> Duration`.
   - `EnvConfigTierTtlResolver`: reads `CORELINK_AC_TTL_DEFAULT_S` env var; default 90d (7,776,000s).
   - `S07PerTierTtlResolver` (forward): reads from S-07 config-singleton; per-tier table.
   - Boundary explicit via ADR-0019.

3. **Refresh-on-hit logic**:
   - `refresh_if_needed(tenant_id, action_digest, last_hit_at) -> Option<RefreshOp>`.
   - Threshold: only update if `now - last_hit_at >= refresh_threshold` (default 60s; env `CORELINK_AC_REFRESH_THRESHOLD_S`).
   - Reduces UPDATE storm under high read rate.
   - SQL: `UPDATE ac_meta SET last_hit_at = $1, expires_at = $1 + ttl WHERE tenant_id = $2 AND action_digest = $3`.

4. **Cron Durable Object `AcTtlWorker`**:
   - 1 instance per region (5 instances); env var `AC_TTL_WORKER_REGION` selects.
   - `alarm()` handler triggered by alarm system.
   - Initial alarm set at deploy; re-arm at end of each tick.
   - Default interval 3600s (1h); env `CORELINK_AC_TTL_CRON_INTERVAL_S`.

5. **Expiry job batch flow**:
   - SELECT batch (LIMIT 1000) of expired rows for region.
   - For each row: R2 DELETE → D1 DELETE → KV invalidate → audit emit.
   - Sleep 100ms between batches; loop until batch returns < 1000 OR alarm budget (~30s) exhausted.
   - Tenant-scoped DELETE: SQL `DELETE WHERE tenant_id = ? AND action_digest = ?` strict.

6. **R2 DELETE error handling**:
   - R2 DELETE 5xx → retry once with exponential backoff (1s).
   - Persistent failure → log error + audit emit ac.evict.r2_failed; D1 DELETE NOT executed (preserves consistency; orphan R2 cleaner via S-06 GC reconcile).
   - Metric `corelink.ac.ttl.r2_delete_failed_total{region}` (alert if > 5% of attempts).

7. **D1 DELETE error handling**:
   - D1 DELETE 5xx → retry; if persistent, R2 already deleted; orphan D1 row references missing R2.
   - **Compensating action**: re-INSERT D1 row (idempotent via ON CONFLICT) → R2 will be re-created on next UPDATE; OR retry DELETE in next cron tick.
   - Decision: retry DELETE in next tick (eventual consistency); log error.

8. **KV invalidation**:
   - KV.delete `ac_neg:<tenant_prefix>:<action_digest>` post-eviction.
   - Idempotent (delete non-existent = no-op).
   - Reduces customer cache miss latency post-eviction (force fresh re-fetch).

9. **Audit emission**:
   - Outbox pattern (reuse WI-S01-005 audit_outbox table).
   - Event type: `ac.evict.ttl_expired` per row.
   - Batch insert into outbox; drain worker emits to S-09 chain.

10. **Métricas**:
    - `corelink.ac.ttl.rows_evicted_total{region}` (counter).
    - `corelink.ac.ttl.cron_ticks_total{region}` (counter; alert if rate < expected).
    - `corelink.ac.ttl.batch_duration_ms{region}` (histogram).
    - `corelink.ac.ttl.batch_size{region}` (gauge; alert if = max sustained — workload outpaces).
    - `corelink.ac.ttl.r2_delete_failed_total{region}` (counter; alert if > 5%).
    - `corelink.ac.ttl.d1_delete_failed_total{region}` (counter).
    - `corelink.ac.ttl.refresh_on_hit_total{tenant_tier}` (counter).
    - `corelink.ac.ttl.refresh_skipped_total` (counter; threshold not met).

11. **Property tests** (10k iter PR; 100k nightly):
    - `prop_ttl_tenant_isolation`: 1000 random (tenant_a, action_digest); evict only Tenant A's rows; Tenant B's untouched.
    - `prop_refresh_on_hit_threshold`: random GET sequences; UPDATE only triggered if last_hit < threshold.
    - `prop_evict_idempotent`: re-evict same row → no-op (already deleted).
    - `prop_r2_d1_consistency`: R2 fail simulates; D1 row preserved (no orphan D1 ref to deleted R2).
    - `prop_batch_bounded`: 10000 expired rows; batch size 1000; 10 ticks complete eviction.
    - `prop_cron_alarm_rearm`: alarm fires; tick completes; alarm re-armed for next interval.

12. **Integration tests**:
    - End-to-end: insert ac_meta row with expires_at = now - 1s; trigger cron; verify R2+D1+KV cleaned + audit emitted.
    - Multi-region: 5 regions with mixed expired rows; assert each region's worker only touches its rows.
    - Refresh-on-hit: GET hot path; verify expires_at extended.

13. **Chaos suite**:
    - Cron DO crash mid-batch: verify partial state recoverable next tick.
    - R2 outage 1h: verify graceful no-orphan-D1; reconcile next cycle.
    - D1 timeout: verify retry + log.
    - High-rate refresh storm: 1k req/s same digest; verify threshold reduces UPDATE rate.

14. **Runbook**:
    - `RB-FM-AC-TTL-DRIFT.md`: cron not firing; rows expired but not evicted; manual eviction procedure.
    - `RB-FM-AC-TTL-STORM.md`: batch_size hit cap continuously; scale-out trigger.

15. **Documentation**:
    - `docs/internal/ac-ttl-infrastructure.md`: cron DO design + refresh-on-hit + S-07 boundary handoff.
    - rustdoc 100% public API.

### 6.2 Out-of-scope (deferred)

- **Per-tier TTL defaults table** (free=7d, solo=30d, ...): S-07/ADR-0019.
- **Per-tenant custom TTL override**: S-13 admin plane.
- **Multi-region replication of TTL worker** (one-active multi-region): S-14.
- **TTL storm prediction (autoscale cron interval)**: S-09 forward.
- **Customer-visible TTL countdown widget** in dashboard: S-16.
- **DSR cascade integration** (S-11 forward): TTL-evicted rows already audit-logged; S-11 export consumes.

## 7. Anti-Scope

- ❌ DELETE without tenant_id filter (cross-tenant catastrophic).
- ❌ Bulk DELETE > 1000 per batch (D1 timeout risk).
- ❌ R2 DELETE post D1 DELETE (orphan R2; preserved order R2-first then D1).
- ❌ Refresh-on-hit every GET (threshold reduces storm).
- ❌ Cron without re-arm logic (silent death).
- ❌ Cross-region cron coordination (per-region independent).
- ❌ Synchronous batch in alarm > 30s (timeout).
- ❌ Hard-coded TTL value (env-config; per-tier post-S-07).
- ❌ TTL extension via API (admin override S-13 only).
- ❌ Skip audit emission (forensic trail).
- ❌ Skip metric emission (operational visibility).
- ❌ Custom alarm scheduling (use DO native alarm system).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: AC TTL infrastructure (cron DO + refresh-on-hit)

  Background:
    Given AcTtlWorker DO instance per region (sam, iad, lhr, nrt, syd)
    Given default TTL 90d via env CORELINK_AC_TTL_DEFAULT_S=7776000
    Given refresh threshold 60s via env CORELINK_AC_REFRESH_THRESHOLD_S=60
    Given ac_meta + R2 ac-<region> + KV ac_neg from WI-S04-002

  Scenario: Refresh-on-hit happy path
    Given Tenant A has ac_meta row (D, last_hit_at = T-300s, expires_at = T+86_400_000ms - 300s)
    When client GetActionResult(D)
    Then handler step [6] refresh_if_needed → threshold met (last_hit > 60s ago)
    And D1 UPDATE last_hit_at = now, expires_at = now + 7776000s
    And p99 latency ≤ 10ms

  Scenario: Refresh-on-hit threshold skip
    Given last_hit_at = T-30s (below 60s threshold)
    When client GetActionResult(D)
    Then handler refresh_if_needed → SKIP (within threshold)
    And NO D1 UPDATE
    And metric corelink.ac.ttl.refresh_skipped_total incremented

  Scenario: Cron DO eviction tick (single region)
    Given ac_meta has 100 rows in region=sam with expires_at < now
    When alarm fires for AcTtlWorker[sam]
    Then SELECT 100 rows (batch < 1000)
    And FOR EACH: R2 DELETE → D1 DELETE → KV invalidate → audit emit
    And metric corelink.ac.ttl.rows_evicted_total{region=sam} += 100
    And alarm re-armed +1h

  Scenario: Cron DO bounded batch (large workload)
    Given ac_meta has 10000 rows expired in region=sam
    When alarm fires
    Then 10 batches of 1000 each, with 100ms sleep between
    And total tick duration ≤ 30s
    And metric corelink.ac.ttl.batch_size{region=sam} = 1000 (cap hit)
    And alert fired (workload may need scale-out)

  Scenario: Tenant scoping (cross-tenant DELETE prevented)
    Given Tenant A has expired row D_a; Tenant B has live row D_b
    When cron evicts Tenant A's D_a
    Then SQL: DELETE WHERE tenant_id = A AND action_digest = D_a (strict)
    And Tenant B's D_b untouched
    And property test prop_ttl_tenant_isolation green

  Scenario: R2 DELETE failure (graceful; preserves consistency)
    Given R2 DELETE returns 5xx for envelope
    When cron processes row
    Then R2 retry with exponential backoff (1 retry); if persistent fail
    And D1 row PRESERVED (NOT deleted)
    And audit emit ac.evict.r2_failed
    And metric corelink.ac.ttl.r2_delete_failed_total{region} incremented
    And next cron tick retries

  Scenario: D1 DELETE failure (after R2 success)
    Given R2 DELETE OK; D1 DELETE returns 5xx
    When cron processes row
    Then D1 retry; if persistent fail, log error
    And next cron tick retries (idempotent; row still has expires_at < now)
    And metric d1_delete_failed_total incremented

  Scenario: Cron alarm re-arm
    Given alarm fires; tick completes
    When tick logic exits
    Then alarm re-armed for now + 3600s
    And property test prop_cron_alarm_rearm green

  Scenario: Cron NOT re-armed (chaos test)
    Given chaos PR removes alarm.set call
    When tick completes
    Then no further alarm fires (silent death)
    And metric corelink.ac.ttl.cron_ticks_total{region} stale
    And alert fires (rate < 1/h sustained)
    And runbook RB-FM-AC-TTL-DRIFT triggered

  Scenario: Multi-region isolation
    Given AcTtlWorker[sam] has 100 rows; AcTtlWorker[iad] has 200 rows
    When sam alarm fires (separate from iad)
    Then sam evicts 100; iad unaffected
    When iad alarm fires
    Then iad evicts 200; sam unaffected

  Scenario: Per-tier TTL via TierTtlResolver (S-04 GA fallback)
    Given EnvConfigTierTtlResolver active (S-07 not yet SEALED)
    Given env CORELINK_AC_TTL_DEFAULT_S=7776000 (90d)
    When refresh_on_hit invoked for any tenant_tier
    Then tier_ttl resolves to 90d (single global default)

  Scenario: Per-tier TTL via S07PerTierTtlResolver (post-S-07 SEALED)
    Given S07PerTierTtlResolver active
    Given config-singleton table: free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable
    When refresh_on_hit invoked for tenant_tier=free
    Then tier_ttl resolves to 7d
    When refresh_on_hit invoked for tenant_tier=enterprise
    Then tier_ttl resolves to customer-configured value

  Scenario: Audit emission per eviction
    Given 100 rows evicted in batch
    When eviction completes
    Then 100 audit_outbox rows inserted (single batch INSERT)
    And event_type = ac.evict.ttl_expired
    And drain worker emits to S-09 chain

  Scenario: KV invalidation post-eviction
    Given row evicted
    When eviction completes
    Then KV.delete ac_neg:<tenant_prefix>:<action_digest> invoked (idempotent)
    And next GET returns 404 (cold path; no negative cache)
    And customer can re-execute action immediately
```

## 9. Design Decisions

### 9.1 Why Cron Durable Object (não Cron Trigger workers)

- Durable Object provides per-instance state (alarm + region-local) without external store.
- CF Cron Triggers are global (1 instance) — single point of failure; bottleneck for 5-region eviction.
- DO sharded per region scales naturally; coordination not needed.
- DO alarm system reliable; workers wake up at scheduled time.

### 9.2 Why batch size 1000 (não 100 ou 10000)

- D1 SELECT 1000 LIMIT is OK fetch-only (no batch-statement limit).
- R2 DELETE 1000 sequential ~10s; within 30s alarm budget.
- 100 too small: 10x more iterations; more overhead.
- 10000 too large: D1 timeout risk; alarm budget exceed.

### 9.3 Why R2-first then D1-DELETE (não D1-first)

- R2 DELETE then D1 DELETE: if R2 fails, D1 preserved; row still references R2 (orphan recoverable via re-insert).
- D1 DELETE then R2 DELETE: if R2 fails, D1 row gone but R2 envelope remains; orphan R2 (storage cost; S-06 GC reconcile catches).
- R2-first preferred; orphan R2 < orphan D1 ref.

### 9.4 Why refresh-on-hit threshold 60s

- Without threshold: 100 req/s on same digest = 100 D1 UPDATE/s; lock contention.
- 60s threshold: at most 1 UPDATE/min per digest; reduces D1 load 100×.
- Drift acceptable: refresh slightly delayed (up to 60s) but TTL still extended; customer perceives as "hot entry stays warm".

### 9.5 Why per-region sharding (não single global cron)

- Reduce coordination complexity (per-region independent).
- Aligned with R2 bucket per region (already isolated).
- Scales naturally as regions added.
- Single global cron = single failure point.

### 9.6 Why alarm re-arm at end of tick

- Avoid drift: re-arm to fixed interval (e.g., +3600s from now).
- Self-healing: if tick fails, next alarm still fires (DO retry semantics).
- Documented in CF DO docs as canonical pattern.

### 9.7 Why TierTtlResolver trait (não direct config read)

- Boundary boundary explicit between S-04 (infrastructure) and S-07 (per-tier defaults).
- Swappable impl: S-04 GA uses EnvConfig; S-07 SEALED uses config-singleton.
- Test double easy: MockTierTtlResolver returns configurable values.

### 9.8 Why audit emission in outbox (não direct S-09)

- Reuse WI-S01-005 outbox pattern; consistent with handler audit emission.
- Drain worker emits async; cron tick latency unaffected.
- Atomicity: D1 batch (ac_meta DELETE + audit_outbox INSERT) atomic; both succeed or both rollback.

### 9.9 Why tenant_scoped DELETE strict (não bulk WHERE expires_at)

- Defense-in-depth tenant isolation; SQL injection or query bug confined.
- Audit chain captures per-row tenant_id (forensic).
- Performance: 1000 rows × 1 statement each = ~1ms; OK.

### 9.10 ADR-0019 already published; this WI implements

- ADR-0019 ratificada in Lote 9.5b; this WI is implementation per ADR.
- TierTtlResolver trait is the boundary mechanism documented in ADR-0019.

## 10. Completeness Criteria SOTA

- [ ] **10.s04.005.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_ttl_tenant_isolation, prop_refresh_on_hit_threshold, prop_evict_idempotent, prop_r2_d1_consistency, prop_batch_bounded, prop_cron_alarm_rearm.
- [ ] **10.s04.005.2** Refresh-on-hit p99 ≤ 10ms; cron tick ≤ 30s for batch 1000 (EVT-021).
- [ ] **10.s04.005.3** Tenant scoping integration test: 100 mixed rows; DELETE only target tenant; others untouched (EVT-002).
- [ ] **10.s04.005.4** Multi-region isolation test: 5 regions; eviction per region only (EVT-002).
- [ ] **10.s04.005.5** R2 DELETE failure handling: retry + preserve D1; metric alert; runbook (EVT-017).
- [ ] **10.s04.005.6** Cron alarm re-arm test: alarm fires → tick → alarm rearmed; chaos PR removing rearm caught (EVT-002).
- [ ] **10.s04.005.7** Audit emission: 1000 evicted rows → 1000 audit outbox entries (EVT-002).
- [ ] **10.s04.005.8** TierTtlResolver trait: env-config fallback active S-04 GA; S-07 forward replaceable (EVT-027 ADR-0019).
- [ ] **10.s04.005.9** Métricas (8 listadas §6.1.10) emitted; dashboard widget partial (full em WI-S04-006).
- [ ] **10.s04.005.10** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s04.005.11** Cost regression gate (Lote 10.4bis P0 fix corrigida): per-eviction cost ≤ $0.000015 (with headroom over $0.000011 actual; was wrongly $0.000003 in v1.0) (Lote 9.4 §14.10).
- [ ] **10.s04.005.12** Runbooks RB-FM-AC-TTL-DRIFT + RB-FM-AC-TTL-STORM published.

## 11. DoD

- [ ] Module `ac/ttl/` compila + integration tests green.
- [ ] AcTtlWorker DO deployed in 5 regions.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green em CI; 100k nightly green.
- [ ] Refresh-on-hit threshold logic green (chaos high-rate test).
- [ ] Multi-region isolation test green.
- [ ] Tenant scoping integration test green.
- [ ] Cron alarm re-arm chaos test green.
- [ ] Métricas (8 listadas §6.1.10) emitted.
- [ ] TierTtlResolver trait + EnvConfig impl active; S-07 forward documented.
- [ ] Runbooks RB-FM-AC-TTL-DRIFT + RB-FM-AC-TTL-STORM published.
- [ ] Architect + DBA + AppSec reviews.
- [ ] PRR Architect mini sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

- **INV-AC-TTL-MONOTONIC** (HIGH, NEW — promovida em registry §3.15 Lote 10.4bis): refresh-on-hit increments last_hit_at + extends expires_at; never decreases.
- **INV-AC-EVICT-TENANT-SCOPED** (CRITICAL, NEW): DELETE strict tenant_id filter; cross-tenant impossible.
- **INV-AC-EVICT-CONSISTENCY** (HIGH, NEW): R2 DELETE before D1 DELETE; orphan R2 < orphan D1 ref.
- **INV-AC-TENANT-SCOPED** (CRITICAL, registry §3.3): all SQL ops respect tenant_id.
- **INV-AC-OUTPUTS-VALID** (HIGH, registry §3.3): TTL eviction does NOT reference output blobs (only AC envelope); INV preserved.

TLA+ alignment: tenant_isolation.tla — TTL flow respects tenant scoping; eviction is tenant-scoped DELETE.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| TTL module | `crates/corelink-worker/src/ac/ttl/` | Rust |
| AcTtlWorker DO | `crates/corelink-worker/src/ac/ttl/worker.rs` | Rust |
| TierTtlResolver | `crates/corelink-worker/src/ac/ttl/resolver.rs` | Rust |
| Refresh logic | `crates/corelink-worker/src/ac/ttl/refresh.rs` | Rust |
| Evict batch logic | `crates/corelink-worker/src/ac/ttl/evict.rs` | Rust |
| Property tests | `crates/corelink-worker/tests/prop_ac_ttl.rs` | Rust |
| Integration tests | `tests/it_ac_ttl_lifecycle.rs` | Rust |
| Chaos suite | `tests/chaos/ac_ttl.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions) | TOML |
| RB-FM-AC-TTL-DRIFT | `specs/02_governance/runbooks/RB-FM-AC-TTL-DRIFT.md` | Markdown |
| RB-FM-AC-TTL-STORM | `specs/02_governance/runbooks/RB-FM-AC-TTL-STORM.md` | Markdown |
| Doc | `docs/internal/ac-ttl-infrastructure.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s04.005.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s04.005.2** rustdoc 100% public API + 4 examples (refresh, evict batch, custom TierTtlResolver, alarm rearm).
- **14.s04.005.3** Test coverage ≥ 90% (eviction logic boundary).
- **14.s04.005.4** Latência: refresh p99 ≤ 10ms; cron tick ≤ 30s; batch bounded 1000 rows.
- **14.s04.005.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`.
- **14.s04.005.6** Métricas: 8 listadas §6.1.10.
- **14.s04.005.7** Runbooks: RB-FM-AC-TTL-DRIFT + RB-FM-AC-TTL-STORM.
- **14.s04.005.8** Forward-compat: TierTtlResolver trait swappable (env → S-07 config-singleton).
- **14.s04.005.9** Memory bounded: per-tick ≤ 1 MiB heap (1000 rows × ~1 KiB serialized).
- **14.s04.005.10** Cost regression gate (Lote 10.4bis P0 fix): per-eviction ≤ $0.000015 (with 35% headroom over actual $0.000011).

## 15. Chaos Experiments

1. **Cron not re-armed**: chaos PR removes alarm.set call; verify cron stops firing; metric stale; alert fires after 1h. Hypothesis: alert detection ≤ 1h post-stop.

2. **Cross-tenant DELETE attempt**: chaos PR introduces SQL `DELETE WHERE expires_at < ?` (no tenant filter); CI integration test detects via tenant isolation property test red.

3. **R2 outage 1h**: simulate R2 DELETE 5xx for 1h; verify retries; D1 rows preserved; orphan R2 cleaner via S-06 reconcile next cycle.

4. **D1 timeout mid-batch**: simulate D1 503 mid-eviction; verify partial batch state recoverable next tick (idempotent — expires_at < now still selected).

5. **Refresh-on-hit storm**: 1k req/s on same digest; verify threshold reduces UPDATE rate to ≤ 1/min per digest.

6. **Cron interval too aggressive**: env CORELINK_AC_TTL_CRON_INTERVAL_S=10 (10s); verify R2 rate limit not hit; rate limit applied via batch_size.

7. **Cron interval too conservative**: 24h interval; verify rows expire but eviction lag up to 24h; metric alert if lag exceeds expected.

8. **Multi-region race**: 5 regions concurrent eviction; verify no shared state corruption.

9. **Per-tier TTL change runtime**: env CORELINK_AC_TTL_DEFAULT_S changed mid-day; verify next refresh uses new TTL; old entries unaffected (refresh extends; doesn't reset).

10. **Audit outbox D1 batch fail**: simulate audit_outbox INSERT fail mid-batch; verify atomic rollback (eviction NOT committed; retry next tick).

11. **TierTtlResolver impl swap (S-04 → S-07 transition)**: chaos PR swaps impl; verify no regression; integration test re-runs.

12. **Bulk eviction storm (10M rows)**: simulate 10M expired rows in single region; verify batch-bounded eviction over hours; metric alerts persistent.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S04-006. Este WI mini-PRR Architect + DBA + AppSec.

- [ ] All Gherkin green.
- [ ] Property + chaos green.
- [ ] Multi-region isolation green.
- [ ] Tenant scoping strict green.
- [ ] Cron alarm rearm green.
- [ ] Refresh threshold storm test green.
- [ ] Métricas + runbooks live.
- [ ] TierTtlResolver trait + S-07 boundary documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | TTL module skeleton + traits | 1.5h |
| ST-002 | AcTtlWorker DO impl | 3h |
| ST-003 | Wrangler DO binding setup | 1h |
| ST-004 | TierTtlResolver trait + EnvConfig impl | 2h |
| ST-005 | Refresh-on-hit logic + threshold | 2.5h |
| ST-006 | Batch eviction logic (R2 → D1 → KV → audit) | 4h |
| ST-007 | Alarm re-arm logic | 1h |
| ST-008 | Multi-region instance config | 1.5h |
| ST-009 | Métricas emit (8 metrics) | 2h |
| ST-010 | Property tests (6 properties × 10k iter) | 4h |
| ST-011 | Integration tests (lifecycle + multi-region + tenant scope) | 4h |
| ST-012 | Chaos suite (12 scenarios) | 4h |
| ST-013 | Runbooks RB-FM-AC-TTL-DRIFT + RB-FM-AC-TTL-STORM | 2h |
| ST-014 | Documentation `ac-ttl-infrastructure.md` | 2h |
| ST-015 | rustdoc + 4 examples | 2h |
| ST-016 | Architect + DBA + AppSec review iteration | 3h |
| ST-017 | Cost regression bench setup | 1h |
| ST-018 | S-07 forward TierTtlResolver impl stub | 1h |

**Total Optimistic**: ~41h. **PERT** (O=36h, M=43h, P=66h): **~46h**.

## 18. Dependencies

### Hard blockers

- WI-S04-001 (handler) SEALED — refresh-on-hit invoked from handler.
- WI-S04-002 (D1 ac_meta + R2 ac bucket) SEALED.
- ADR-0019 ratificada (already; Lote 9.5b).
- WI-S01-005 (audit_outbox table) SEALED.
- WI-S03-005 (tenant_id types) SEALED.

### Soft blockers

- S-07 (per-tier defaults table) — soft; this WI works with EnvConfig fallback; S-07 swaps impl.
- S-13 (admin plane) — soft; admin TTL override forward.

### Outbound

- WI-S04-006 (conformance + PRR) consumes for ship gate.
- S-07 (eviction) extends/replaces TierTtlResolver impl.
- S-06 (GC reconcile) consumes orphan R2 cleanup signal.

## 19. Effort PERT

O: 36h, M: 43h, P: 66h → PERT **46h**.

## 20. Time-boxing

**56h hard limit**. Se exceder → escalation: split em "cron DO" + "refresh-on-hit" sub-WIs.

## 21. Observability

8 métricas listadas §6.1.10. Trace spans:
- `ac.ttl.cron_tick` — region, batch_size, duration, rows_evicted.
- `ac.ttl.evict_row` — tenant_id (UUIDv7), action_digest_prefix, region, r2_delete_ok, d1_delete_ok, audit_emit_ok.
- `ac.ttl.refresh_on_hit` — tenant_id, action_digest_prefix, threshold_met (bool), duration_ms.

Logs structured JSON:
- INFO em cron tick complete.
- WARN em batch_size hit cap (workload outpaces).
- ERROR em R2/D1 delete failed.
- CRITICAL em cron stale > 1h sem ticks.

Dashboard widget DASH-AC TTL (partial; full em WI-S04-006):
- Rows evicted rate per region.
- Cron tick rate per region (target 1/h).
- Batch size distribution.
- R2/D1 delete failure rate (alert if > 5%).
- Refresh-on-hit rate (and skipped).

## 22. Cost Analysis

**Per-eviction cost**:
- R2 DELETE: $4.5/M class A.
- D1 DELETE: $1/M.
- KV DELETE: $5/M.
- Audit outbox INSERT: ~$0.50/M.
- Per-eviction: **~$0.000011** (Lote 10.4bis P0 fix: prior text said "$0.000003 amortized over batch" — the per-op cost IS $0.000011; "amortized" was misleading. Actual cost: R2 $4.5/M + D1 $1/M + KV $5/M + audit_outbox $0.50/M = $11/M = $0.000011 per eviction).

**Per-refresh cost** (synchronous in handler):
- D1 UPDATE: $1/M.
- Per-refresh: ~$0.000001.

**TCO 12m projection** (10M GET/dia → 10% trigger refresh = 1M refreshes/dia; 1M evictions/dia at ~1y entry rotation):
- Refresh: 1M × $0.000001 = $1/dia.
- Eviction: 1M × $0.000011 = $11/dia (Lote 10.4bis P0 fix: was $0.000003 errado).
- DO compute (5 instances × 1 tick/h × 30s): negligible (~$0.10/dia).
- Total: ~$5/dia × 365 = **~$1.8k/yr**.

**Cost regression gate** (Lote 10.4bis P0 fix corrigida math): per-eviction ≤ $0.000015 (with 35% headroom over $0.000011) + per-refresh ≤ $0.000001.

**Comparison vs alternatives**:
- TTL via R2 lifecycle policies (R2 native expiry): $0/yr but coarse-grained (per-bucket; not per-tenant); LOSES audit emission.
- DynamoDB TTL: similar cost; but separate infra.
- This impl: **$1.8k/yr** with audit + tenant-scoped + multi-region.

## 23. API Contract

Public crate API (semver post v1.0):

```rust
pub trait TierTtlResolver: Send + Sync {
    fn resolve_ttl(&self, tenant_tier: TenantTier) -> Duration;
}

pub struct EnvConfigTierTtlResolver { /* env-config based */ }

// Refresh-on-hit utility (consumed by WI-S04-001 handler)
pub fn refresh_if_needed(
    last_hit_at: SystemTime,
    threshold: Duration,
) -> bool;

// Cron DO is internal; not public API.
```

**Stability**: post-v1.0, TierTtlResolver trait stable; impls additive.

**Versioning**: TTL schema v1 (env-config + S-07 swap point); future may add multi-tier hierarchical.

## 24. Post-mortem Hooks

- Cross-tenant DELETE detected (post-mortem CRITICAL; FM-303 catastrophic; data loss).
- Cron stale > 1h sustained → SEV-1 (TTL drift; storage growth).
- R2 DELETE failure rate > 5% sustained → SEV-2 (R2 reliability).
- Refresh storm regression detected (D1 lock contention) → SEV-2 (threshold logic broken).
- Per-tier TTL handoff S-04 → S-07 regression → SEV-2 (boundary contract violated).
- Audit emission gap (eviction without audit log) → SEV-1 (forensic trail broken).
- DO crash mid-batch → SEV-3 (recoverable; next tick).

## 25. Rollback / Recovery

- TTL infra rollback: revert wrangler.toml DO bindings + redeploy; cron stops; existing rows continue to expire eventually via R2 lifecycle (if configured).
- Refresh threshold rollback: env CORELINK_AC_REFRESH_THRESHOLD_S revert.
- Per-tier resolver swap rollback: revert TierTtlResolver impl active.
- RTO: ≤ 30 min (DO unbinding + redeploy).
- RPO: 0 (stateless infra; row state preserved in D1+R2).

Fallback: if cron broken, manual eviction via admin script (S-13 forward); RB-FM-AC-TTL-DRIFT documents.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: cron DO authenticated via Wrangler binding; cannot be invoked externally.
- **Tampering**: tenant_scoped DELETE strict (never bulk WHERE expires_at without tenant_id); SQL prepared statements.
- **Repudiation**: audit emission per eviction (outbox + S-09 chain).
- **Information disclosure**: DO logs do NOT include sensitive data; tenant_id pseudonymous.
- **DoS**: bounded batch size; cron interval bounded; rate limit per region.
- **Elevation of privilege**: DO bindings scoped to deploy time; cannot be elevated by handler code.

**LINDDUN delta**:
- **Linkability**: tenant_id pseudonymous; action_digest content-hash.
- **Identifiability**: not applicable (no PII in TTL flow).
- **Non-repudiation**: append-only audit chain.
- **Detectability**: cron tick metric; eviction rate metric; failure metric alert.
- **Disclosure of information**: not applicable.
- **Unawareness**: ADR-0019 documents per-tier handoff; runbooks cover incident response.
- **Non-compliance**: LGPD Art. 18 retention satisfied via auto-expiry; DSR S-11 cascade complementary.

## 27. Knowledge Transfer

- **Tech talk** (1h): "AC TTL Infrastructure: Cron Durable Object + Refresh-on-Hit + S-07 Boundary".
- **Doc** `docs/internal/ac-ttl-infrastructure.md` — architecture + handoff design.
- **Workshop** (1.5h): com Architect + DBA + Backend authors.
- **Onboarding test** (5 questions): refresh threshold rationale, R2-first-then-D1 ordering, batch bound rationale, multi-region sharding, ADR-0019 boundary.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-tenant DELETE via SQL bug | L | L | CRITICAL | L | LOW | Strict tenant_id filter; integration test; ADR documents; CI gate |
| R-002 | Cron not re-armed (silent death) | L | M | HIGH | L | LOW | Re-arm at end of tick; metric alert if rate stale; runbook |
| R-003 | R2 DELETE failure cascades to orphan R2 | M | M | MEDIUM | M | LOW | Retry + S-06 reconcile cleanup; metric alert; runbook |
| R-004 | Refresh-on-hit storm (D1 lock contention) | M | M | HIGH | M | LOW | Threshold 60s; integration test; metric alert |
| R-005 | Per-tier TTL handoff S-07 regression | L | M | MEDIUM | L | LOW | TierTtlResolver trait boundary; ADR-0019 documents; integration test |
| R-006 | DO crash mid-batch (orphan state) | M | L | LOW | L | LOW | Idempotent (next tick re-selects); metric alert |
| R-007 | Multi-region race condition (cross-region pollution) | L | L | LOW | L | LOW | Per-region scoped SELECT; integration test 5 regions |
| R-008 | Audit emission gap (forensic trail broken) | L | M | HIGH | L | LOW | D1 batch atomicity; integration test; metric alert |
| R-009 | Bulk eviction storm (10M rows in 1 tick) | M | M | MEDIUM | M | LOW | Bounded batch; alert if cap hit sustained; scale-out runbook |
| R-010 | TTL too aggressive (customer perception) | M | L | LOW | L | LOW | Customer-visible métric; per-tier override S-07 forward; admin override S-13 |
| R-011 | TTL too conservative (storage growth) | M | L | MEDIUM | L | LOW | Storage size metric alert; customer dashboard S-16 visibility |
| R-012 | Cron interval misconfigured (10s too aggressive) | L | L | MEDIUM | L | LOW | Validation in DO init (≥ 60s minimum); chaos test |
| R-013 | KV invalidation skipped (customer cache miss prolonged) | L | L | LOW | L | LOW | Idempotent KV.delete; integration test verifies |
| R-014 | TierTtlResolver impl swap (S-04 → S-07) regression | L | L | MEDIUM | L | LOW | Trait boundary; chaos test impl swap; ADR-0019 documents |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + DBA review cron DO sharding + batch bounds + tenant scoping.
2. **AppSec (D+1)**: AppSec review cross-tenant DELETE prevention + audit emission.
3. **Code (D+3)**: peer review (1 engineer + 1 DBA).
4. **Adversarial (pre-merge D+5)**: red team — cross-tenant DELETE attempts, cron stale detection, R2 outage handling.
5. **Chaos (D+6)**: 12 scenarios validated.
6. **PRR (D+7)**: Architect mini sign-off (full ship em WI-S04-006).

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; LGPD retention review_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — cron DO sharding + S-07 boundary_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory** — cross-tenant DELETE prevention_ | _pending_ | _pending_ |
| 13 | DBA (advisory) | _mandatory; D1 batch semantics + retry semantics_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-005 (Lote 10.4); SOTA pós-Lote 10.3bis (32 seções; 13-row sign-off; 14-row risk; cost TCO 12m; 12 chaos experiments; STRIDE+LINDDUN delta). |

## 32. Anti-patterns evitados

- ❌ DELETE without tenant_id filter (cross-tenant catastrophic).
- ❌ Bulk DELETE > 1000 per batch.
- ❌ R2 DELETE post D1 DELETE (orphan R2).
- ❌ Refresh-on-hit every GET.
- ❌ Cron without re-arm.
- ❌ Cross-region cron coordination.
- ❌ Synchronous batch > 30s.
- ❌ Hard-coded TTL value.
- ❌ TTL extension via API.
- ❌ Skip audit emission.
- ❌ Skip metric emission.
- ❌ Custom alarm scheduling.

---

**Fim WI-S04-005.** Próximo: WI-S04-006 (REAPI v2 conformance test suite + DASH-AC dashboards + RB-FM-303 dry-run + PRR ship gate 13 sign-offs).

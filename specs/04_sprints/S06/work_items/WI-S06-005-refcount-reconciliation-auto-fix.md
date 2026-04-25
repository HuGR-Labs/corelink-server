---
id: "WI-S06-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005"]
parent: "S-06"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s06", "gc", "reconcile", "refcount", "ctrl-gc-002", "auto-fix", "high-risk"]
---

# WI-S06-005 — Daily Refcount Reconciliation (CTRL-GC-002) + Drift Detection (>0.1% SEV-2; >1% SEV-1) + Auto-Fix Small Drifts (≤5 records) + Manual Review Large Drifts + INV-GC-003 Enforcement

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-005 |
| Título | Daily reconciliation job (cron 03:00 UTC + jitter; ≥30min após mark/sweep latest completion); recompute `expected_refcount = count(ac_meta a, json_each(a.blob_refs) j WHERE j.value = digest AND a.deleted_at_ms IS NULL)` per (tenant_id, digest) (Lote 10.6bis P0-1 fix: canonical `json_each` JSON-aware membership; NOT `LIKE '%digest%'` substring match); compare with `blob_meta.refcount`; drift > 0.1% global = SEV-2; per-tenant > 1% = SEV-1; auto-fix small drifts via percentage-floor + absolute-floor (drift_count ≤5 AND drift_percent ≤0.01%; Lote 10.6bis P0-6 scale-invariant) com audit emit; manual review large drifts pause + alert; CTRL-GC-002 enforcement; INV-GC-003 sustained < 0.1% drift 7d (sprint contract DoD) |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (refcount drift cascades to wrong reachable computation; INV-GC-001 indirect risk), FF-HR-005 (controle integridade; refcount fundação de mark phase) |

## 1. Intent

Reconciliation daily detecta drift entre `blob_meta.refcount` (denormalized counter) e `count(ac_meta with blob_refs containing digest AND deleted_at IS NULL)` (true ref count). Drift indica bug em UpdateActionResult/DeleteActionResult/Sweep não-mantendo refcount; pode cascade para mark phase reachable computation incorreta:

```rust
// File: crates/corelink-gc/src/reconcile.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait ReconcilePhase: Send + Sync {
    async fn execute(
        &self,
        tenant_id: &TenantId,
        region: &Region,
    ) -> Result<ReconcileResult, ReconcileError>;
}

pub struct ReconcileResult {
    pub blobs_scanned: u64,
    pub drifts_detected: u64,
    pub auto_fixed_count: u64,                  // drifts ≤ 5 records auto-corrected
    pub manual_review_count: u64,               // drifts > 5 records flagged
    pub max_drift_per_tenant: f64,              // percentage
    pub global_drift_percent: f64,              // aggregate
    pub duration_ms: u64,
    pub sev_level: SevLevel,                    // None | Sev2 | Sev1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SevLevel {
    None,                                       // drift < 0.1% global; per-tenant < 1%
    Sev2,                                       // global drift 0.1%..1%; auto-fix small + alert
    Sev1,                                       // per-tenant drift > 1%; pause auto-fix; manual review
}

#[derive(thiserror::Error, Debug)]
pub enum ReconcileError {
    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("audit emission failed; reconcile aborted (fail-closed)")]
    AuditEmissionFailed,
}
```

**Cripto-driven invariants**:

1. **expected_refcount SQL aggregate** (per blob digest) — **Lote 10.6bis P0-1 canonical `json_each` idiom** (NOT LIKE substring match; LIKE produces silent false drift signals on schema evolution, substring collisions, AND amplifies into auto-fix corruption):
   ```sql
   SELECT
       blob_meta.tenant_id,
       blob_meta.digest,
       blob_meta.refcount AS stored_refcount,
       (SELECT COUNT(*)
        FROM ac_meta a, json_each(a.blob_refs) j
        WHERE a.tenant_id = blob_meta.tenant_id
          AND j.value = blob_meta.digest
          AND a.deleted_at_ms IS NULL
          AND a.created_at_ms < ?) AS expected_refcount  -- bound to reconcile_started_at_ms snapshot (Lote 10.6bis P1-6 fix)
   FROM blob_meta
   WHERE blob_meta.tenant_id = ?
     AND blob_meta.deleted_at_ms IS NULL;
   ```
   - **Why `json_each`**: D1/SQLite native JSON-aware membership; matches each element of `blob_refs` JSON array exactly via `j.value = digest`; survives schema evolution (e.g., `{refs:[...], metadata:{...}}` envelope) — LIKE silently matches metadata strings forever.
   - **Auto-fix amplification risk eliminated**: substring collisions inflate `expected_refcount` → wrong drift → wrong UPDATE refcount → permanent canonical-state corruption. `json_each` removes this class.
   - **Index requirement**: `idx_ac_meta_tenant_deleted_at` covers tenant + soft-delete pre-filter (verified WI-S04-002); `json_each` per-row extract is O(json_array_size) (small — typical AC has 1-10 outputs).
   - **Snapshot bound**: `a.created_at_ms < reconcile_started_at_ms` mirrors WI-S06-003 mark phase pattern; bounds race window to writes-before-snapshot (Lote 10.6bis P1-6 fix).

2. **Drift threshold**:
   - Global drift % = drifts_detected / blobs_scanned.
   - Per-tenant drift % = max per-tenant drifts / per-tenant blobs.
   - SEV-2: 0.1% < global ≤ 1%.
   - SEV-1: per-tenant > 1%.

3. **Auto-fix discipline (Lote 10.6bis P0-6 — scale-invariant)**:
   - **Auto-fix gate**: `drift_count ≤ 5 AND drift_percent ≤ 0.01%` per tenant. Both conditions required; either fails → manual review.
   - **Boundary**: `=5` auto-fixed (operative bound `≤5`); `=6` manual review.
   - **Why two-floor**: 10-blob tenant with 5 drifts = 50% drift (catastrophic; manual); 1M-blob tenant with 5 drifts = 0.0005% (negligible; auto-fix). Single absolute-floor of 5 mis-fires across scale.
   - **Auto-fix UPDATE**: `UPDATE blob_meta SET refcount = expected_refcount WHERE tenant_id = ? AND digest = ?` (sqlx prepared; D1 batch ≤250).
   - **Auto-fix failure mode**: D1 throttle → exponential backoff retry 3 attempts (100ms, 500ms, 2s) → on persistent fail, downgrade to "drift detected, fix pending" + persist em `gc_drift_pending(tenant_id, digest, expected, observed, attempted_at_ms)` table + emit SEV-2 (NOT SEV-1; auto-fix-failure ≠ refcount-integrity-broken). Next reconcile cron tick re-attempts from `gc_drift_pending`.
   - All auto-fixes audit-emitted (forensic trail) — pre-execution audit BEFORE UPDATE per fail-closed pattern.

4. **Tenant-scoped strict** (Lote 10.4bis lesson).

5. **Audit emit fail-closed** (consistência com WI-S06-003 pattern).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

Refcount é denormalized counter; bug em UpdateActionResult/DeleteActionResult/Sweep deixa refcount stale → mark phase reachable computation usa refcount como sinal → wrong classification → INV-GC-001 indirect violation. HIGH_RISK em N dimensões:

1. **Refcount drift cascade**: stale refcount = blob com refcount=2 mas only 1 ac_meta entry references → mark sees refcount > 0 → blob "reachable" → never deleted (false positive; storage bloat) OR refcount=0 mas 1 ac_meta references → mark sees orphan → sweep deletes → INV-GC-001 violation. Mitigação: reconcile daily + auto-fix.

2. **SQL aggregate query performance**: `LIKE '%' || digest || '%'` é full-scan ac_meta.blob_refs JSON column (D1 SQLite no JSON path index). Mitigação: chunked iteration + bounded concurrency; phase budget 1h p99 @ 1M blobs.

3. **Auto-fix discipline boundary**: 5 records é arbitrary threshold; small drifts likely transient (UpdateAR mid-flight); large drifts indicate systemic bug. Mitigação: configurable threshold via env; audit chain captures auto-fix decisions.

4. **SEV escalation**: SEV-2 alert é warning (no immediate impact); SEV-1 paused é blocker (refcount integrity broken). Mitigação: PagerDuty integration; runbook RB-FM-302 (refcount drift) forward stub.

5. **Race com concurrent writes** (UpdateAR + DeleteAR + Sweep mid-reconcile): captured snapshot via `gc_run.reconcile_started_at_ms`; reconcile compares counts at snapshot; minor drift acceptable (window < 1s).

6. **Cross-tenant isolation strict** (Lote 10.4bis).

7. **Audit emit fail-closed** (lesson WI-S06-003).

8. **Phase budget**: 1h p99 @ 1M blobs (analytics workload; not hot path).

**Atacante adversarial scenarios**:

- **Refcount manipulation attack**: customer with admin scope manipulates blob_meta.refcount (forces wrong reachable). Mitigação: reconcile detects drift; auto-fix corrects; audit chain captures suspicious.
- **Cross-tenant scan injection**: defense-in-depth.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: refcount drift cascades to INV-GC-001 risk.
- **FF-HR-005**: controle integridade; refcount foundational.

13 sign-offs.

## 3. Customer Impact & Journey

**Persona 1 — Customer**: invisible (background); customer-visible only via SLO-CORRECT-GC sustained metric (forward S-09).

**Persona 2 — DevOps/On-call**: SEV-2 alert → review drift; SEV-1 alert → paused state + manual review; runbook RB-FM-302 stub forward.

**SLA addendum**:
- Reconcile daily 03:00 UTC + jitter (after mark/sweep cron).
- Phase budget 1h p99 @ 1M blobs.
- Drift detection < 1s post-reconcile completion.
- Auto-fix latency < 5s per drift (D1 UPDATE single row).
- SEV-2 alert ≤ 5 min post-detection (PagerDuty PD-WARNING).
- SEV-1 alert ≤ 1 min post-detection (PD-CRITICAL); auto-fix paused.

## 4. Capability Mapping

- **CAP-GC-004** (Refcount reconciliation daily) — IMPLEMENTA primary.
- Trace: `failure_modes.md FM-302/303` + `security_model.md CTRL-GC-002` + `invariant_registry.md INV-GC-003`.

## 5. Tipo

Reconciliation worker; HIGH_RISK; FF-HR-011 + FF-HR-005.

## 6. Escopo (compact)

### 6.1 In-scope

1. `crates/corelink-gc/src/reconcile/` module.
2. **Daily cron DO** 03:00 UTC + jitter ±10min; per-region instance.
3. **SQL aggregate query** per (tenant_id, digest); chunked iteration.
4. **Drift detection thresholds**: 0.1% / 1%; configurable env.
5. **Auto-fix small drifts** (≤5 records): atomic D1 UPDATE refcount.
6. **Pause + manual review large drifts** (>5 records): SEV-1 alert; audit emit.
7. **Audit emission outbox** (WI-S01-004): reconcile.{started, drift_detected, auto_fixed, manual_review_required, completed} events.
8. **Métricas**:
   - `corelink.gc.reconcile.cron_fired_total{region}`.
   - `corelink.gc.reconcile.drifts_detected_total{tenant_id, region}` (alert if > 0.1%).
   - `corelink.gc.reconcile.auto_fixed_total{tenant_id, region}`.
   - `corelink.gc.reconcile.manual_review_required_total{tenant_id, region}` (alert > 0).
   - `corelink.gc.reconcile.global_drift_percent` (gauge; alert > 0.1%).
   - `corelink.gc.reconcile.per_tenant_drift_percent_max` (gauge; alert > 1%).
   - `corelink.gc.reconcile.duration_ms{region}` (histogram; SLO ≤ 1h p99).
9. **Property tests** (10k iter PR; 100k nightly):
   - `prop_reconcile_idempotent`: re-run reconcile on consistent state = no drift detected.
   - `prop_auto_fix_bounded`: drifts ≤5 AND ≤0.01% auto-fixed; >5 OR >0.01% paused.
   - `prop_tenant_isolation`: cross-tenant reconcile independence.
   - `prop_drift_threshold_strict`: 0.1% / 1% boundaries respected.
   - `prop_audit_emit_per_action`: every drift detection + auto-fix audit emitted.
   - `prop_reconcile_json_blob_refs_evolution` (Lote 10.6bis P0-1 NEW): blob_refs schema evolution from `["d1","d2"]` → `{"refs":["d1","d2"],"metadata":{"x":"d1"}}`; assert `json_each` correctly counts only refs array members (NOT metadata "d1" substring); LIKE-defect regression test.
   - `prop_auto_fix_scale_invariant` (Lote 10.6bis P0-6 NEW): generate tenants of size 10/1k/100k blobs × drift counts 1-100; assert percentage+absolute floor (`≤5 AND ≤0.01%`) consistently selects auto-fix vs manual review across all scales.
   - `prop_auto_fix_failure_drift_pending` (Lote 10.6bis P0-6 NEW): inject D1 throttle on 3 sequential auto-fix UPDATEs; assert drift persisted to `gc_drift_pending`; next reconcile re-attempts; SEV-2 emitted (NOT SEV-1).
   - `prop_auto_fix_conditional_predicate` (Lote 10.6bis P0-7 #11 NEW): inject concurrent UpdateAR mid-auto-fix; assert `WHERE refcount = stored_refcount` conditional predicate prevents lost UpdateAR increment; no ping-pong.
10. **Chaos suite** (15 scenarios; SOTA bar above HIGH_RISK floor 10; Lote 10.6bis P0-7 expansion):
    - 1. Sustained drift < 0.1% (sprint contract DoD §10.s06.5) → no SEV alert.
    - 2. Drift 0.5% global → SEV-2 alert; auto-fix percentage+absolute floor; metric tracks.
    - 3. Drift 2% per-tenant → SEV-1 alert; auto-fix paused; manual review required.
    - 4. Concurrent UpdateAR mid-reconcile → snapshot via `reconcile_started_at_ms` + `a.created_at_ms < snapshot` predicate bounds race window deterministically.
    - 5. Cross-tenant scan injection → sqlx prepared rejects.
    - 6. D1 throttle on auto-fix UPDATE → exponential backoff retry 3 attempts (100/500/2000ms) → on persistent fail, persist `gc_drift_pending` row; SEV-2 (NOT SEV-1); next cron tick re-fixes.
    - 7. Phase budget exceeded (5M blobs) → SEV-2 alert.
    - 8. Audit emit fail → reconcile ROLLBACK.
    - 9. Refcount manipulation attempt (customer admin S-13 endpoint legitimate UPDATE bypass) → reconcile detects drift; audit chain captures legitimate-UPDATE provenance + suspicious-pattern flag if multiple admin UPDATEs same digest <1h.
    - 10. Worker crash mid-reconcile → resume from checkpoint persisted in `gc_run.reconcile_chunk_offset`.
    - 11. **Auto-fix race with concurrent UpdateAR** (Lote 10.6bis P0-7 NEW): reconcile detects drift D=3; auto-fix UPDATE refcount fires; concurrent UpdateAR fires +1; resolution: `UPDATE blob_meta SET refcount = expected_refcount WHERE refcount = stored_refcount` conditional predicate → if UpdateAR raced first, conditional fails → re-fetch + re-derive expected_refcount for this digest → re-evaluate; no ping-pong (auto-fix idempotent under conditional).
    - 12. **Reconcile vs physical-delete race** (Lote 10.6bis P0-7 NEW): WI-004 physical-delete fires for tenant T digest D mid-reconcile; reconcile snapshot at `reconcile_started_at_ms = T_start`; physical-delete commits at T_start+5min; reconcile aggregate at T_start+10min sees blob_meta row gone; `expected_refcount > 0`, `stored_refcount = NULL` (row deleted); SQL three-valued logic: NULL ≠ 0 (treat as drift); resolution: NULL stored_refcount → drift detection logic emits "deletion-during-reconcile" event (SEV-2 informational), NOT auto-fix UPDATE (no row to update); next reconcile cron tick sees consistent state.
    - 13. **Cross-region drift aggregation** (Lote 10.6bis P0-7 NEW): 5 regions per-region reconcile; per-region `global_drift_percent` published; cross-region rollup is sum-aggregate at S-09 observability layer (NOT in this WI); chaos test asserts each region's drift bounded independently; per-region SEV thresholds independent.
    - 14. **DSR-bypass impact on reconcile** (Lote 10.6bis P0-7 NEW): WI-004 DSR bypass deletes blob mid-reconcile-window; ac_meta still references digest (DSR scope is blob, not AC entries); reconcile sees `expected_refcount = 1, stored_refcount = NULL`; resolution: DSR-flagged digests captured em `dsr_signals_processed` table; reconcile JOIN against this table → exclude DSR-deleted digests from drift detection (else auto-fix would attempt UPDATE NULL row); chaos test asserts DSR-deleted blob produces NO drift signal.
    - 15. **Refcount manipulation threat surface** (Lote 10.6bis P0-7 NEW): threat model — admin endpoint S-13 `POST /v1/admin/blob/refcount` (forward; NOT in S-06 in-scope); attacker with admin scope sets refcount = 999999 to defer eviction OR refcount = 0 to force deletion; mitigation: reconcile-detects-drift → drift > 5 records OR > 0.01% triggers SEV-1 manual review; sqlx prepared blocks SQL injection; admin UPDATE provenance captured em audit chain (S-09); chaos test asserts adversarial UPDATE detected within 1 reconcile cycle (24h max).

### 6.2 Out-of-scope

- TLA+ CI gate (WI-S06-006).
- DASH-GC dashboard (WI-S06-007).
- Customer-facing drift metric (S-16 forward).

## 7. Anti-Scope

- ❌ Auto-fix > 5 records (manual review mandatory).
- ❌ Cross-tenant reconcile.
- ❌ Skip audit emit (fail-closed mandatory).
- ❌ Trust client tenant_id (TenantCtx-only Lote 10.4bis).
- ❌ Variable-time SQL aggregate.
- ❌ Hard-coded thresholds (env-config).

## 8. Acceptance Criteria (compact 8 scenarios)

```gherkin
Feature: Reconciliation daily + drift detection + auto-fix

  Scenario: Reconcile happy path (no drift)
    Given consistent refcount state
    When reconcile fires
    Then drifts_detected = 0; SEV None; metric global_drift_percent = 0

  Scenario: Drift 0.5% → SEV-2 alert
    Given drift 0.5% global
    When reconcile detects
    Then SEV-2 alert fired; auto-fix small (≤5) executed; metric tracks

  Scenario: Drift 2% per-tenant → SEV-1 paused
    Given drift 2% for tenant T
    When reconcile detects
    Then SEV-1 alert; auto-fix PAUSED for tenant T; manual review required
    And audit emit corelink.gc.reconcile.manual_review_required

  Scenario: Auto-fix idempotent
    Given small drift (3 records); auto-fix executes
    When reconcile re-runs
    Then no drift detected (idempotent)

  Scenario: Concurrent UpdateAR mid-reconcile
    Given reconcile_started_at_ms = T
    Given UpdateAR fires at T+500ms
    When reconcile snapshots state at T
    Then bounded drift < 1s window; not flagged as drift

  Scenario: Tenant isolation
    Given 1000 concurrent reconciles different tenants
    When all execute
    Then no cross-tenant interference

  Scenario: Cross-tenant injection rejected
    Given attacker SQL injection attempt
    Then sqlx prepared statement rejects

  Scenario: Audit emit fail-closed
    Given audit_outbox INSERT fails
    Then reconcile ROLLBACK; SEV-1 alert; no auto-fix persisted
```

## 9-32 (compact)

### 9. Design Decisions

- 9.1: Daily cron 03:00 UTC + jitter; **temporal contract: reconcile MUST start ≥30 min após mark/sweep latest completion** (Lote 10.6bis P1-5 fix; avoid snapshot inconsistency with mid-flight sweep state changes).
- 9.2: SQL aggregate **canonical `json_each(a.blob_refs) j WHERE j.value = digest`** (Lote 10.6bis P0-1; **NOT** `LIKE '%digest%'` — substring match silently breaks on schema evolution + collisions + amplifies into auto-fix corruption).
- 9.3: **Auto-fix scale-invariant percentage+absolute floor**: `drift_count ≤ 5 AND drift_percent ≤ 0.01%` (Lote 10.6bis P0-6; rationalize over single-floor-of-5 which mis-fires across tenant scales). Auto-fix failure: 3-attempt exponential backoff → `gc_drift_pending` table → SEV-2 (NOT SEV-1).
- 9.4: SEV escalation 0.1% / 1% (sprint contract §5.5 R-S06-10).
- 9.5: Snapshot via `reconcile_started_at_ms`; SQL predicate `a.created_at_ms < reconcile_started_at_ms` bounds race window deterministically (mirrors WI-S06-003 mark phase pattern; Lote 10.6bis P1-6 fix).
- 9.6: TenantCtx-only (Lote 10.4bis).
- 9.7: Audit fail-closed (lesson WI-S06-003).
- 9.8: Phase budget **separate sprint contract §5.5 R-S06-10.1: ≤1h p99 @ 1M blobs** (NOT sub-allocation of mark/sweep).
- 9.9: ADR-0042 forward (no new ADR).
- 9.10: PagerDuty integration SEV-2 + SEV-1.
- 9.11: D1 batch ≤250 row Lote 10.5bis lesson explicit (chunked iteration 1k blobs/chunk × bounded concurrency 4-8 per region).

### 10. Completeness Criteria

- [ ] **10.s06.005.1** Property tests 9 × 10k green (5 baseline + 4 Lote 10.6bis NEW); 100k nightly.
- [ ] **10.s06.005.2** Chaos suite 15 scenarios green (10 baseline + 5 Lote 10.6bis P0-7 NEW).
- [ ] **10.s06.005.3** **Sustained drift < 0.1% em 7d staging** (sprint contract DoD §10.s06.5).
- [ ] **10.s06.005.4** Auto-fix idempotent integration test (with conditional `WHERE refcount = stored_refcount` predicate).
- [ ] **10.s06.005.5** Métricas (7) emitted; SEV-2 alert global_drift > 0.1%; **SEV-1 alert per-tenant_drift > 1%** (sprint contract §5.5 R-S06-10 alignment; Lote 10.6bis P2-7 fix — was incorrectly listed as global > 1% which is unreachable).
- [ ] **10.s06.005.6** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s06.005.7** Cost regression gate per-reconcile-batch ≤ $0.000010 re-derived post P0-1 fix (json_each per-row extract O(json_array_size) + chunked iteration; previous $0.000005 estimate was incoherent for correlated subquery LIKE shape).
- [ ] **10.s06.005.8** RB-FM-302 (refcount drift) runbook stub forward.
- [ ] **10.s06.005.9** **Property test `prop_reconcile_json_blob_refs_evolution`** green (Lote 10.6bis P0-1 NEW; LIKE-defect regression test).
- [ ] **10.s06.005.10** **Property test `prop_auto_fix_scale_invariant`** green @ tenant sizes 10/1k/100k (Lote 10.6bis P0-6 NEW).
- [ ] **10.s06.005.11** **Property test `prop_auto_fix_failure_drift_pending`** green (Lote 10.6bis P0-6 NEW; D1 throttle → gc_drift_pending).
- [ ] **10.s06.005.12** **Property test `prop_auto_fix_conditional_predicate`** green (Lote 10.6bis P0-7 #11 NEW; no ping-pong).

### 11. DoD

- [ ] Module compila + integration tests green; All Gherkin green; Property 10k green; Migration cron DO em wrangler.toml; SEV-2/SEV-1 alerts integrated PagerDuty; Architect + AppSec + SRE reviews; PRR mini sign-off.

### 12. Invariants Validated

- **INV-GC-003** (HIGH, registry §3.4): refcount consistency; reconcile diário enforces.
- **INV-GC-RECONCILE-AUTO-FIX-BOUNDED** (HIGH, NEW promovida §3.17): auto-fix only ≤5 records; > 5 manual review.
- **INV-GC-RECONCILE-AUDIT-FAIL-CLOSED** (CRITICAL, NEW): audit emit failure → reconcile ROLLBACK.

### 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Reconcile module | `crates/corelink-gc/src/reconcile/` | Rust |
| Property tests | `crates/corelink-gc/tests/prop_reconcile.rs` | Rust |
| Chaos suite | `tests/chaos_gc_reconcile.rs` | Rust |
| RB-FM-302 stub (refcount drift) | `specs/05_quality/runbooks/` (filename per S-06 ship gate) | Markdown |

### 14. Quality Standards

- 14.s06.005.1-10: similar standards (zero unsafe; rustdoc 100%; coverage ≥ 90%; latency p99 ≤ 1h phase; SAST clean; 7 metrics; memory ≤ 10 MiB heap; cost gate per-reconcile-batch ≤ $0.000010).

### 15. Chaos Experiments (10)

§6.1.10.

### 16. PRR

Mini-PRR Architect + AppSec + SRE.

### 17. Sub-tasks

| ID | h |
|---|---|
| ST-001 module skeleton | 1 |
| ST-002 cron DO + jitter | 2 |
| ST-003 SQL aggregate query (chunked) | 3 |
| ST-004 drift detection thresholds | 2 |
| ST-005 auto-fix logic ≤ 5 records | 2 |
| ST-006 SEV escalation + PagerDuty | 2 |
| ST-007 audit emission outbox (5 events) | 2 |
| ST-008 metrics (7) | 2 |
| ST-009 property tests (5 × 10k) | 4 |
| ST-010 chaos suite (10) | 4 |
| ST-011 RB-FM-302 stub | 1.5 |
| ST-012 review iter | 3 |

Total Optimistic: ~28h. PERT: ~31h.

### 18-32 (compact final)

- 18 Dependencies: Hard WI-001 (worker skeleton); soft S-09 (PagerDuty integration); outbound WI-006 (TLA+ CI gate; this WI feeds property test).
- 19 PERT 31h; 20 Time-boxing 38h.
- 21 Observability 7 metrics; trace span `gc.reconcile.{phase, drift, auto_fix}`.
- 22 Cost: per-reconcile-batch ~$0.000005 (D1 SELECT aggregate); TCO 12m: ~$913/yr.
- 23 API Contract: ReconcilePhase trait + result types; `#[non_exhaustive]`.
- 24 Post-mortem: drift > 1% sustained → SEV-1 + RB-FM-302; auto-fix > threshold → CRITICAL.
- 25 Rollback: reconcile idempotent; auto-fix UPDATE rollback via UPDATE again; RTO ≤ 30 min; RPO 0.
- 26 Security: tenant_id NOT NULL; sqlx prepared; audit fail-closed.
- 27 Knowledge Transfer: tech talk 1h; doc; onboarding 5q.
- 28 Risk Register (12-row; Lote 10.6bis expansion): drift cascade L M HIGH L LOW; auto-fix > threshold L M HIGH L LOW; race UpdateAR M L LOW L LOW; cross-tenant L L CRITICAL L LOW; phase budget L L MEDIUM L LOW; audit fail L M MEDIUM L LOW; D1 throttle M L LOW L LOW; refcount manipulation L M MEDIUM L LOW; cost regression M L MEDIUM L LOW; SEV escalation noisy M L LOW L LOW; **SQL semantic defect (LIKE substring vs json_each) cascading to auto-fix corruption** (Lote 10.6bis P0-1) L H CRITICAL H LOW (SQL canonical idiom + property test regression); **auto-fix-vs-UpdateAR race ping-pong** (Lote 10.6bis P0-7 #11) L M MEDIUM L LOW (conditional WHERE refcount = stored_refcount predicate).
- 29 Review: D+0..D+5 standard.
- 30 Sign-off (HIGH_RISK 13): standard 12 mandatory + Crypto SME advisory (no cripto-load-bearing path; reconcile is analytics workload).
- 31 Change Log: 1.0.0 / 2026-04-25 / Gustavo (Lote 10.6); 1.1.0 / 2026-04-25 / Gustavo (Lote 10.6bis Part 2a P0 fixes: P0-1 SQL LIKE→json_each canonical idiom + snapshot bound; P0-6 auto-fix scale-invariant percentage+absolute floor + drift_pending failure mode; P0-7 5 NEW chaos adversarial scenarios; P1-5 cron timing ≥30min after mark/sweep; P1-6 race window snapshot SQL bound; P2-7 SEV-1 threshold per-tenant >1% not global).
- 32 Anti-patterns: ❌ SQL `LIKE '%digest%'` substring (use json_each); ❌ Single absolute-floor auto-fix (use percentage+absolute); ❌ Auto-fix without conditional predicate; ❌ Cross-tenant; ❌ Skip audit; ❌ Trust client tenant_id; ❌ Variable-time SQL; ❌ Hard-coded thresholds; ❌ SEV-1 global >1% (use per-tenant >1%).

---

**Fim WI-S06-005.** Próximo: WI-S06-006 (TLA+ CI gate + property test 100k race Mark+UpdateAR).

---
id: "WI-S06-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
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
| Título | Daily reconciliation job (cron 03:00 UTC + jitter); recompute `expected_refcount = count(ac_meta where blob_refs contains digest AND deleted_at IS NULL)` per (tenant_id, digest); compare with `blob_meta.refcount`; drift > 0.1% global = SEV-2; per-tenant > 1% = SEV-1; auto-fix small drifts (≤5 records correctly = small) com audit emit; manual review large drifts pause + alert; CTRL-GC-002 enforcement; INV-GC-003 sustained < 0.1% drift 7d (sprint contract DoD) |
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

1. **expected_refcount SQL aggregate** (per blob digest):
   ```sql
   SELECT
       blob_meta.tenant_id,
       blob_meta.digest,
       blob_meta.refcount AS stored_refcount,
       (SELECT COUNT(*) FROM ac_meta
        WHERE ac_meta.tenant_id = blob_meta.tenant_id
          AND ac_meta.blob_refs LIKE '%' || blob_meta.digest || '%'
          AND ac_meta.deleted_at_ms IS NULL) AS expected_refcount
   FROM blob_meta
   WHERE blob_meta.tenant_id = ?
     AND blob_meta.deleted_at_ms IS NULL;
   ```

2. **Drift threshold**:
   - Global drift % = drifts_detected / blobs_scanned.
   - Per-tenant drift % = max per-tenant drifts / per-tenant blobs.
   - SEV-2: 0.1% < global ≤ 1%.
   - SEV-1: per-tenant > 1%.

3. **Auto-fix discipline**:
   - Drift ≤ 5 records per tenant: auto-fix `UPDATE blob_meta SET refcount = expected_refcount`.
   - Drift > 5 records: pause + manual review + SEV-1 alert.
   - All auto-fixes audit-emitted (forensic trail).

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
   - `prop_auto_fix_bounded`: drifts ≤5 auto-fixed; >5 paused.
   - `prop_tenant_isolation`: cross-tenant reconcile independence.
   - `prop_drift_threshold_strict`: 0.1% / 1% boundaries respected.
   - `prop_audit_emit_per_action`: every drift detection + auto-fix audit emitted.
10. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. Sustained drift < 0.1% (sprint contract DoD §10.s06.5) → no SEV alert.
    - 2. Drift 0.5% global → SEV-2 alert; auto-fix small; metric tracks.
    - 3. Drift 2% per-tenant → SEV-1 alert; auto-fix paused; manual review required.
    - 4. Concurrent UpdateAR mid-reconcile → snapshot bounded < 1s drift.
    - 5. Cross-tenant scan injection → sqlx prepared rejects.
    - 6. D1 throttle → backoff + retry.
    - 7. Phase budget exceeded (5M blobs) → SEV-2 alert.
    - 8. Audit emit fail → reconcile ROLLBACK.
    - 9. Refcount manipulation attempt (customer admin) → reconcile detects + audit captures.
    - 10. Worker crash mid-reconcile → resume from checkpoint.

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

- 9.1: Daily cron 03:00 UTC + jitter (after mark/sweep cron 02:00).
- 9.2: SQL aggregate `LIKE '%digest%'` (D1 limitation; bounded perf).
- 9.3: Auto-fix threshold 5 records (configurable env).
- 9.4: SEV escalation 0.1% / 1% (sprint contract §5.5).
- 9.5: Snapshot via reconcile_started_at_ms (bounded race window).
- 9.6: TenantCtx-only (Lote 10.4bis).
- 9.7: Audit fail-closed (lesson WI-S06-003).
- 9.8: Phase budget 1h p99 @ 1M blobs.
- 9.9: ADR-0042 forward (no new ADR).
- 9.10: PagerDuty integration SEV-2 + SEV-1.

### 10. Completeness Criteria

- [ ] **10.s06.005.1** Property tests 5 × 10k green; 100k nightly.
- [ ] **10.s06.005.2** Chaos suite 10 scenarios green.
- [ ] **10.s06.005.3** **Sustained drift < 0.1% em 7d staging** (sprint contract DoD §10.s06.5).
- [ ] **10.s06.005.4** Auto-fix idempotent integration test.
- [ ] **10.s06.005.5** Métricas (7) emitted; CRITICAL alert global_drift > 1%.
- [ ] **10.s06.005.6** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s06.005.7** Cost regression gate per-reconcile-batch ≤ $0.000005.
- [ ] **10.s06.005.8** RB-FM-302 (refcount drift) runbook stub forward.

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
- 28 Risk Register (10-row): drift cascade L M HIGH L LOW; auto-fix > threshold L M HIGH L LOW; race UpdateAR M L LOW L LOW; cross-tenant L L CRITICAL L LOW; phase budget L L MEDIUM L LOW; audit fail L M MEDIUM L LOW; D1 throttle M L LOW L LOW; refcount manipulation L M MEDIUM L LOW; cost regression M L MEDIUM L LOW; SEV escalation noisy M L LOW L LOW.
- 29 Review: D+0..D+5 standard.
- 30 Sign-off (HIGH_RISK 13): standard 12 mandatory + Crypto SME advisory.
- 31 Change Log: 1.0.0 / 2026-04-25 / Gustavo (Lote 10.6).
- 32 Anti-patterns: ❌ Auto-fix > 5 records; ❌ Cross-tenant; ❌ Skip audit; ❌ Trust client tenant_id; ❌ Variable-time SQL; ❌ Hard-coded thresholds.

---

**Fim WI-S06-005.** Próximo: WI-S06-006 (TLA+ CI gate + property test 100k race Mark+UpdateAR).

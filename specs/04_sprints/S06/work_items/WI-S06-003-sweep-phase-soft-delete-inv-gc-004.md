---
id: "WI-S06-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005", "FF-HR-009"]
parent: "S-06"
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
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s06", "gc", "sweep", "soft-delete", "inv-gc-004", "audit", "high-risk"]
---

# WI-S06-003 — Sweep Phase: Soft-Delete `blob_meta.deleted_at` + Grace 72h CAS / 24h AC + **INV-GC-004 Enforce** (mark_started_at_ms strict `<` ac.created_at; TLA+ Obligation) + Audit Emission Per Sweep + Property Test 100k Race Mark+UpdateActionResult

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-003 |
| Título | Sweep phase: consume gc_candidates (status='candidate') from WI-S06-002; per-candidate enforce **INV-GC-004** (sweep checks `ac.created_at < gc_candidate.mark_started_at_ms` strict; if any AC entry references digest com created_at >= mark_started_at_ms → status='protected_re_ref'; do NOT delete); soft-delete `blob_meta.deleted_at = now()` for confirmed orphans; grace 72h CAS / 24h AC; audit emission per sweep com `prev_state` capturado; property test 100k race Mark+UpdateActionResult interleavings → 0 INV-GC-004 violations |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (sweep é onde INV-GC-001/004 enforcement happens; bug = data loss permanente), FF-HR-005 (controle integridade dados), FF-HR-009 (defense-in-depth — TLA+ + property test + chaos under load + grace reversible) |

## 1. Intent

Sweep phase é **onde reachable-vs-orphan decision becomes destructive** (soft-delete tombstone). Bug em INV-GC-004 enforce = reachable blob com refresh-after-mark é deletado → CRITICAL FM-300 INV-GC-001 violation. **TLA+ `gc_correctness.tla` `InvGCReRefProtected` é a obrigação formal**:

```rust
// File: crates/corelink-gc/src/sweep.rs

#![forbid(unsafe_code)]

pub use crate::sweep::{SweepPhase, SweepResult, SweepDecision};
pub use crate::error::SweepError;

#[async_trait]
pub trait SweepPhase: Send + Sync {
    /// Execute sweep phase consuming gc_candidates (status='candidate').
    /// Per-candidate: enforce INV-GC-004 (mark_started_at_ms strict < ac.created_at).
    /// If protected → status='protected_re_ref'.
    /// If confirmed orphan → soft-delete blob_meta + status='swept'.
    /// Audit emit per decision.
    async fn execute(
        &self,
        gc_run: &mut GcRun,                         // mutated: counters + status; mark_started_at_ms read-only
        tenant_id: &TenantId,
        region: &Region,
    ) -> Result<SweepResult, SweepError>;
}

pub struct SweepResult {
    pub mark_run_id: u64,
    pub candidates_processed: u64,
    pub blobs_swept_count: u64,                    // soft-deleted; grace pending
    pub blobs_protected_re_ref_count: u64,         // INV-GC-004 caught (re-referenced; do NOT delete)
    pub bytes_to_be_reclaimed: u64,                // sum of blob_size_bytes for swept; physical delete WI-S06-004 reclaims
    pub sweep_duration_ms: u64,
    pub audit_events_emitted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepDecision {
    /// Confirmed orphan: soft-delete blob_meta.deleted_at = now(); status='swept'.
    Sweep { swept_at_ms: u64, prev_state: BlobState },

    /// INV-GC-004 caught: ac.created_at >= mark_started_at_ms; do NOT delete.
    ProtectedReRef {
        protected_at_ms: u64,
        ac_digest: String,                          // which ac_meta entry triggered protection
        ac_created_at_ms: u64,                      // for forensic
        mark_started_at_ms: u64,                    // anchor
    },

    /// Already swept (idempotent re-run); no-op.
    AlreadySwept,
}

#[derive(Debug, Clone)]
pub struct BlobState {
    pub digest: String,
    pub size_bytes: u64,
    pub refcount: u32,                              // captured at sweep moment for audit
    pub last_referenced_at_ms: u64,
    pub created_at_ms: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum SweepError {
    #[error("mark_started_at_ms not set; mark phase did not complete; cannot sweep")]
    MarkAnchorMissing,                              // sweep requires mark phase ran successfully

    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("audit emission failed; sweep aborted (fail-closed)")]
    AuditEmissionFailed,                           // INV-OBS-AUDIT-CHAIN-INTEGRITY: sweep fails if audit fails

    #[error("phase budget exceeded: {duration_ms}ms > {budget_ms}ms")]
    PhaseBudgetExceeded { duration_ms: u64, budget_ms: u64 },
}
```

**Cripto-driven invariants enforced (LOAD-BEARING)**:

1. **INV-GC-004 strict `<` comparison** (TLA+ `gc_correctness.tla` `InvGCReRefProtected` obligation):
   ```sql
   -- Per gc_candidate row, sweep query:
   SELECT EXISTS (
       SELECT 1 FROM ac_meta
       WHERE tenant_id = $1
         AND blob_refs LIKE '%' || $2 || '%'         -- digest é em ac.outputs
         AND created_at_ms >= $3                     -- $3 = gc_candidate.mark_started_at_ms; STRICT >=
   ) AS reference_after_mark;
   ```
   Se TRUE → `status='protected_re_ref'`; do NOT soft-delete. Se FALSE → safe to soft-delete.

2. **Strict `<` (não `<=`)** — TLA+ obligation: comparison é STRICT; ac.created_at exatamente igual a mark_started_at_ms é caso fronteira; opta por strict < (mais conservador; favorable to reachable).

3. **Audit emission MANDATORY per sweep decision**: outbox pattern (Lote 10.4bis WI-S01-004); `corelink.gc.sweep.{soft_deleted, protected_re_ref}` events com `prev_state` BlobState capturado.

4. **Audit emit fail → sweep fails fail-closed**: INV-OBS-AUDIT-CHAIN-INTEGRITY prioritized; if audit_outbox INSERT fails, sweep ROLLBACK (no soft-delete); SEV-1 alert.

5. **Tenant-scoped strict** (Lote 10.4bis lesson): all D1 ops `WHERE tenant_id = ctx.tenant_id`; sqlx prepared.

6. **Idempotent re-run**: gc_candidate.status update atomic; re-run on already-swept = no-op; AlreadySwept SweepDecision.

7. **Soft-delete reversible**: `blob_meta.deleted_at` SET to unix_ms_T; undelete possible WITHIN grace window (re-upload OR admin endpoint; CAP-GC-002). After grace + physical-delete (WI-S06-004), irreversível.

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

Sweep phase é **executor of soft-delete decisions**. Bug em INV-GC-004 enforce produces FM-300 (refcount bug → reachable deleted) — single most-feared GC failure mode. HIGH_RISK em N dimensões:

1. **INV-GC-004 race com UpdateActionResult** (canonical TLA+ scenario): worker captures mark_started_at_ms = T; concurrent UpdateActionResult fires at T+1ms (creates new ac_meta entry referencing blob B; ac.created_at = T+1ms); mark phase doesn't see new ac_meta row (mark scan completed before T+1ms); sweep phase MUST detect via `ac.created_at >= mark_started_at_ms` check; if check missed → B incorrectly soft-deleted. Mitigação: SQL EXISTS check em sweep query (above §1.1); strict `<` semantics; property test 100k iter race scenarios; TLA+ obligation `InvGCReRefProtected`. Property test framework simulates concurrent Mark + UpdateActionResult interleavings; INV-GC-004 violation = test failure.

2. **Audit emit failure cascading**: INV-OBS-AUDIT-CHAIN-INTEGRITY (S-09 forward) requires every state transition emit audit event. If audit_outbox INSERT fails (D1 throttle, table corrupt), sweep MUST fail-closed (no soft-delete) — preserves both INV-GC-001 (reachable not deleted) AND INV-OBS-AUDIT-CHAIN-INTEGRITY. Fail-open would soft-delete without forensic trail. Mitigação: D1 batch (blob_meta UPDATE + audit_outbox INSERT) atomic; both succeed or both fail; SweepError::AuditEmissionFailed; SEV-1 alert.

3. **Grace period reversibility window**: 72h CAS / 24h AC (sprint contract §5.3). Customer re-upload mesmo digest WITHIN grace period → undelete (deleted_at=NULL); admin endpoint OR CAS write handler S-01 detects existing soft-deleted row + sets deleted_at=NULL. Mitigação: WI-S01-005 CAS write handler integration; chaos test re-upload soft-deleted blob → status='active' restored.

4. **Idempotent re-run mid-sweep crash**: worker crashed processing candidate 5000 of 10000; resume from checkpoint; candidate 5000 may be partial (status='candidate' still — atomic UPDATE ensures); chaos test #6 simulates.

5. **TenantCtx-only enforcement** (Lote 10.4bis lesson): all sweep queries `WHERE tenant_id = ctx.tenant_id`; sqlx prepared statements + clippy lint forbids `&str` SQL.

6. **DSR erasure interaction** (S-11 forward): regulatory erasure can bypass grace period via separate flow (CTRL-PRIV-014); aligned em CAP-GC-003 envelope; chaos test #7.

7. **Phase budget enforcement**: 5 min p99 @ 100k candidates per run (sub-allocation of 10 min sprint contract; mark gets 5 min, sweep gets 5 min). PhaseBudgetExceeded error; SEV-2 alert.

8. **Tombstone observability**: WI-S02-005 Read path returns 404 for tombstoned blob (Lote 10.2 reused pattern); customer SDK retries observe tombstone OR can manually undelete via admin API.

**Atacante adversarial scenarios**:

- **Cross-tenant sweep injection**: defense-in-depth (tenant_id NOT NULL + sqlx prepared + clippy lint).
- **mark_started_at_ms forgery via SQL injection**: prepared statements prevent.
- **INV-GC-004 bypass via timing oracle**: `ac.created_at >= mark_started_at_ms` is observable; attacker can't bypass com `>=` operator semantics; TLA+ formalizes.
- **Audit outbox poisoning**: cross-tenant audit emit checked via S-09 chain integrity; tampering detected daily verifier.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: sweep is INV-GC-001/004 enforcement point; bug = data loss permanente.
- **FF-HR-005**: controle integridade.
- **FF-HR-009**: defense-in-depth — TLA+ + property test 100k + chaos under load 30d + grace 72h reversible.
- **Reversibility**: soft-delete reversible WITHIN grace window; physical-delete irreversível (WI-S06-004).

13 sign-offs incl. **Crypto SME mandatory emphatic** (TLA+ obligation; INV-GC-004 cripto-coordenado boundary).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**: invisible (sweep runs background); customer-visible only via SLO-CORRECT-GC + customer dashboard reclaim metric (forward S-09).

**Persona 2 — Customer who deleted file by mistake**: re-uploads mesmo digest WITHIN grace 72h → undelete; CAS write handler (S-01) detects soft-delete row + sets deleted_at=NULL; "phew, didn't lose my data".

**Persona 3 — Compliance reviewer (LGPD/GDPR)**: reviews audit chain (S-09) for sweep events; verifies `prev_state` captured per BlobState; reviews INV-GC-004 enforcement via TLA+ + property test 100k.

**SLA addendum**:
- Sweep phase p99 ≤ 5 min @ 100k candidates.
- INV-GC-004 enforcement strict `<`; 0 violations em 30d staging chaos.
- Audit emission per sweep mandatory (fail-closed if audit fails).
- Grace period: 72h CAS / 24h AC; reversibility window respected.
- Tombstone visibility: blob reads return 404 (S-02 read path Lote 10.2 pattern reused).

## 4. Capability Mapping

- **CAP-GC-001** (Mark-and-sweep) — IMPLEMENTA primary sweep side.
- **CAP-GC-002** (Soft-delete reversible) — IMPLEMENTA primary.
- **CAP-GC-003** (Mark-phase-aware re-ref protection) — IMPLEMENTA primary (INV-GC-004).
- Trace: `failure_modes.md FM-300/305/404` + `invariant_registry.md INV-GC-001/004` + `specs/tla/gc_correctness.tla InvGCReRefProtected`.

## 5. Tipo

Sweep phase impl; HIGH_RISK; FF-HR-011 + FF-HR-005 + FF-HR-009.

## 6. Escopo (compact mas detalhado por critic load-bearing)

### 6.1 In-scope

1. `crates/corelink-gc/src/sweep/` module: per-candidate INV-GC-004 enforce + soft-delete + audit emit.

2. **INV-GC-004 enforcement SQL EXISTS check**:
   ```rust
   async fn check_re_referenced(
       d1: &D1,
       tenant_id: &TenantId,
       digest: &str,
       mark_started_at_ms: u64,
   ) -> Result<Option<AcRef>, SweepError> {
       let row: Option<AcRef> = sqlx::query_as!(
           AcRef,
           r#"SELECT action_digest, created_at_ms FROM ac_meta
              WHERE tenant_id = ? AND blob_refs LIKE ?
                AND created_at_ms >= ?  -- STRICT >= per TLA+ obligation
              LIMIT 1"#,
           tenant_id.to_string(),
           format!("%{}%", digest),
           mark_started_at_ms as i64,
       ).fetch_optional(d1).await?;
       Ok(row)
   }
   ```

3. **Soft-delete D1 UPDATE** (atomic + tenant-scoped):
   ```sql
   -- Sweep happy path (orphan confirmed):
   UPDATE blob_meta
   SET deleted_at_ms = unix_ms_now()
   WHERE tenant_id = ?
     AND digest = ?
     AND deleted_at_ms IS NULL  -- idempotent
   RETURNING size_bytes, refcount, last_referenced_at_ms, created_at_ms;
   -- RETURNING captures BlobState for audit emit (prev_state).
   ```

4. **Atomic D1 batch** (blob_meta UPDATE + gc_candidate UPDATE + audit_outbox INSERT):
   ```rust
   d1.batch([
       blob_meta_update,         // SET deleted_at_ms = ...
       gc_candidate_update,      // SET status = 'swept'
       audit_outbox_insert,      // event = corelink.gc.sweep.soft_deleted; prev_state = ...
   ]).await?;
   ```
   All succeed or all rollback.

5. **Protected re-ref path**:
   ```rust
   // INV-GC-004 caught:
   d1.batch([
       gc_candidate_update_protected,  // SET status='protected_re_ref'; protected_reason = "ac.created_at = T+1ms; mark_started_at = T"
       audit_outbox_insert_protected,  // event = corelink.gc.sweep.protected_re_ref
   ]).await?;
   ```

6. **Grace period values**: 72h CAS (= 259_200_000 ms) / 24h AC (= 86_400_000 ms); env override `CORELINK_GC_GRACE_CAS_MS` + `CORELINK_GC_GRACE_AC_MS`; sprint contract §5.3.

7. **Audit emission events** (outbox WI-S01-004):
   - `corelink.gc.sweep.phase_started`.
   - `corelink.gc.sweep.soft_deleted` (per blob; com prev_state).
   - `corelink.gc.sweep.protected_re_ref` (per protected; com forensic).
   - `corelink.gc.sweep.phase_completed` / `phase_failed`.

8. **Métricas**:
   - `corelink.gc.sweep.phase_started_total{region}`.
   - `corelink.gc.sweep.candidates_processed_total{region}`.
   - `corelink.gc.sweep.soft_deleted_total{tenant_id, region}`.
   - `corelink.gc.sweep.protected_re_ref_total{tenant_id, region}` (gauge; alert if > 5% sustained = mark phase too slow OR concurrent UpdateAR storm).
   - `corelink.gc.sweep.bytes_to_be_reclaimed{tenant_id, region}`.
   - `corelink.gc.sweep.duration_ms{region}` (histogram; SLO ≤ 5 min p99).
   - `corelink.gc.sweep.audit_emit_failed_total{region}` (alert if > 0; fail-closed mode active).
   - `corelink.gc.sweep.inv_gc_004_violations_total` (CRITICAL alert if > 0; must always be 0).

9. **Property tests** (10k iter PR; **100k nightly per sprint contract**):
   - **`prop_inv_gc_004_race_mark_update_ar`** (CRITICAL TLA+ obligation): 100k random interleavings of Mark + UpdateActionResult; INV-GC-004 violation count MUST be 0; if any violation, test fails CRITICAL (SEV-0); blocks merge.
   - `prop_sweep_idempotent`: re-sweep on already-swept candidate → AlreadySwept (no-op).
   - `prop_sweep_tenant_isolation`: 1000 concurrent sweeps different tenants; no cross-tenant interference.
   - `prop_sweep_audit_atomic`: simulate audit_outbox INSERT failure mid-batch; sweep ROLLBACK (no soft-delete); SweepError::AuditEmissionFailed.
   - `prop_grace_period_respected`: soft-delete + check deleted_at_ms; physical-delete (WI-S06-004) only fires post-grace.

10. **Mann-Whitney 3-prong cripto-grade timing test**:
    - Goal: cliente cannot distinguish sweep timing for "orphan confirmed" vs "protected_re_ref" via timing (paths via different SQL queries; might differ).
    - 10k samples per arm; |Δmedian| ≤ 5ms (middleware-grade; not cripto-tighter — this is internal observability not customer-facing path).

11. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. UpdateActionResult fires at mark_started_at_ms + 1ms → INV-GC-004 catches; protected_re_ref status; chaos test #11 (TLA+ obligation regression).
    - 2. Audit emit failure mid-sweep → sweep ROLLBACK; SEV-1 alert; chaos test asserts no soft-delete persists.
    - 3. D1 throttle mid-batch → backoff + retry; PAT-RETRY-IDEMPOTENT-001.
    - 4. Cross-tenant sweep injection → sqlx prepared rejects.
    - 5. Worker crash mid-sweep → idempotent resume; gc_candidate.status preserved.
    - 6. Phase budget exceeded (1M candidates) → PhaseBudgetExceeded; SEV-2 alert.
    - 7. DSR erasure trigger (S-11 forward stub) → bypass grace; immediate physical-delete signal.
    - 8. Re-upload soft-deleted blob within grace → CAS write handler S-01 detects + restores.
    - 9. Concurrent sweep + UpdateActionResult storm (1k QPS) → INV-GC-004 caught 100% (chaos under load test sprint contract DoD).
    - 10. mark_started_at_ms = NULL (mark phase failed) → SweepError::MarkAnchorMissing; sweep doesn't fire.
    - 11. Grace period boundary (deleted_at_ms = T - grace + 1ms) → physical-delete WI-S06-004 doesn't fire yet.
    - 12. Audit chain tampering daily verifier (S-09) → drift detected post-sweep.

### 6.2 Out-of-scope

- Mark phase (WI-S06-002).
- Physical delete (WI-S06-004).
- Reconcile (WI-S06-005).
- TLA+ CI gate integration (WI-S06-006).
- DSR erasure trigger flow (S-11 forward; this WI receives signal only).

## 7. Anti-Scope

- ❌ INV-GC-004 `<=` (must be strict `<`; TLA+ obligation strict).
- ❌ Skip audit emit (fail-closed mandatory).
- ❌ Soft-delete reversibility expired without grace window respect.
- ❌ Cross-tenant sweep.
- ❌ Skip phase budget enforcement.
- ❌ Trust client tenant_id (TenantCtx-only; lesson Lote 10.4bis).
- ❌ Sync emit audit em hot path (outbox WI-S01-004).
- ❌ ALTER TABLE ADD CONSTRAINT chk_* (Lote 10.4bis).
- ❌ Grace period < 72h CAS / 24h AC sem ADR + customer opt-in.
- ❌ Skip prev_state capture em audit (forensic trail mandatory).
- ❌ Variable-time SQL EXISTS comparison (constant-time não applicable; SQL plan optimization OK).
- ❌ Grace period drift via timestamp arithmetic bug (CHECK constraint forensic).

## 8. Acceptance Criteria (Gherkin) (compact 14 scenarios)

```gherkin
Feature: Sweep phase + INV-GC-004 enforce + audit emit

  Scenario: Sweep happy path (confirmed orphan)
    Given gc_candidate (digest=D, mark_started_at_ms=T, status='candidate')
    Given no ac_meta entry references D with created_at_ms >= T
    When SweepPhase::execute(gc_run, tenant, region)
    Then SQL EXISTS check returns NULL (no re-ref)
    And blob_meta.deleted_at_ms = unix_ms_now()
    And gc_candidate.status = 'swept'
    And audit emit corelink.gc.sweep.soft_deleted with prev_state captured
    And metric corelink.gc.sweep.soft_deleted_total +1

  Scenario: INV-GC-004 catches re-ref (CRITICAL TLA+ obligation)
    Given gc_candidate (digest=D, mark_started_at_ms=T)
    Given ac_meta entry exists with blob_refs containing D AND created_at_ms = T+1ms
    When sweep checks
    Then SQL EXISTS check returns ac entry; created_at_ms = T+1ms >= T (STRICT >=)
    Then SweepDecision::ProtectedReRef returned
    Then gc_candidate.status = 'protected_re_ref'; protected_reason = "ac.created_at = T+1; mark_started_at = T"
    And audit emit corelink.gc.sweep.protected_re_ref
    And blob_meta.deleted_at_ms unchanged (still NULL)
    And metric corelink.gc.sweep.protected_re_ref_total +1
    And property test prop_inv_gc_004_race_mark_update_ar 100k iter green

  Scenario: Strict < boundary (ac.created_at = mark_started_at_ms exactly)
    Given gc_candidate (mark_started_at_ms = T)
    Given ac_meta entry created_at_ms = T (exactly equal)
    When sweep
    Then SQL `>= T` returns ac entry (T >= T true)
    Then SweepDecision::ProtectedReRef
    (favorable to reachable; conservative protection)

  Scenario: Audit emit fail → sweep ROLLBACK
    Given audit_outbox INSERT fails (D1 throttle simulated)
    When sweep batch executes
    Then D1 batch atomic ROLLBACK
    And blob_meta.deleted_at_ms unchanged
    And gc_candidate.status unchanged (still 'candidate')
    And SweepError::AuditEmissionFailed returned
    And metric corelink.gc.sweep.audit_emit_failed_total +1
    And SEV-1 alert fired

  Scenario: Idempotent re-run on already-swept
    Given gc_candidate already status='swept'
    When sweep re-executes same candidate
    Then SweepDecision::AlreadySwept (no-op)
    And no audit re-emitted

  Scenario: Tenant isolation
    Given 1000 concurrent sweeps different tenants
    When all execute
    Then no cross-tenant interference; INV-GC-004 enforced per tenant

  Scenario: Cross-tenant sweep injection rejected
    Given attacker submits crafted SQL via PAT scope
    When sweep query
    Then sqlx prepared statement rejects
    And clippy custom lint forbids &str SQL literals at compile time

  Scenario: Worker crash mid-sweep idempotent resume
    Given sweep processed 5000 of 10000 candidates; crash at 5001
    When new worker resumes (Lote 10.6 worker resume)
    Then resumes at 5001; gc_candidate status preserved per-row atomic
    And total INV-GC-004 enforcement consistent

  Scenario: Phase budget exceeded
    Given 1M candidates to sweep
    When sweep runs
    Then phase budget 5 min exceeded
    And SweepError::PhaseBudgetExceeded; SEV-2 alert
    And manual re-run path

  Scenario: DSR erasure bypass grace (S-11 forward stub)
    Given DSR erasure signal received for tenant T digest D
    When sweep flow integrates DSR signal
    Then bypass grace path triggers immediate physical-delete (WI-S06-004 forward)
    And audit emit corelink.gc.sweep.dsr_erasure_bypass

  Scenario: Re-upload soft-deleted blob within grace (CAP-GC-002)
    Given blob_meta.deleted_at_ms = T (within grace 72h)
    When customer re-uploads mesmo digest via CAS write handler S-01 (Lote 10.1+10.2 patterns reused)
    Then S-01 handler detects soft-delete row + sets deleted_at_ms = NULL
    And audit emit corelink.gc.undelete_via_reupload

  Scenario: Race storm 1k QPS UpdateActionResult during sweep (sprint contract DoD)
    Given sweep processing 100k candidates
    Given concurrent 1k QPS UpdateActionResult
    When chaos suite runs 4h
    Then INV-GC-004 enforced 100% (zero violations)
    And metric corelink.gc.sweep.inv_gc_004_violations_total = 0 sustained

  Scenario: Property test 100k race Mark + UpdateActionResult interleavings
    Given property test framework simulates concurrent Mark + UpdateActionResult
    When 100k random interleavings execute
    Then INV-GC-004 enforced 100% (zero violations)
    And TLA+ `gc_correctness.tla` `InvGCReRefProtected` property aligned

  Scenario: Mann-Whitney sweep timing (orphan vs protected indistinguishable)
    Given 10k orphan + 10k protected_re_ref sweeps
    When latencies measured
    Then |Δmedian| ≤ 5ms (middleware-grade)
    And property test prop_sweep_timing green
```

## 9. Design Decisions (compact)

- **9.1** Strict `<` (não `<=`) per TLA+ obligation; conservative — favorable to reachable.
- **9.2** SQL EXISTS check em ac_meta (não scan completo); index-backed; perf p99 ≤ 50ms per check.
- **9.3** D1 batch atomic (blob_meta UPDATE + gc_candidate UPDATE + audit_outbox INSERT); fail-closed if any fails.
- **9.4** Audit emission MANDATORY (INV-OBS-AUDIT-CHAIN-INTEGRITY priority); fail-closed.
- **9.5** Grace period 72h CAS / 24h AC env-config (sprint contract §5.3).
- **9.6** prev_state BlobState captured RETURNING clause em D1 UPDATE.
- **9.7** Idempotent re-run: status='swept' UPDATE WHERE status='candidate' atomic; AlreadySwept se já 'swept'.
- **9.8** TenantCtx-only (Lote 10.4bis lesson).
- **9.9** Phase budget 5 min @ 100k candidates (sub-allocation 10 min sprint contract).
- **9.10** Cripto SME mandatory emphatic (TLA+ obligation; INV-GC-004 cripto-coordenado boundary).

## 10. Completeness Criteria SOTA

- [ ] **10.s06.003.1** Property tests 5 × 10k iter green; **100k nightly per sprint contract DoD** (race Mark + UpdateActionResult).
- [ ] **10.s06.003.2** Chaos suite 12 scenarios green (sprint contract HIGH_RISK ≥ 10).
- [ ] **10.s06.003.3** **TLA+ `gc_correctness.tla` `InvGCReRefProtected` aligned** com property test (WI-S06-006 forward CI gate).
- [ ] **10.s06.003.4** **0 INV-GC-004 violations em 30d staging chaos under load** (sprint contract DoD; post-sprint observation period).
- [ ] **10.s06.003.5** Audit emission per sweep mandatory (fail-closed); chaos test asserts.
- [ ] **10.s06.003.6** Grace period 72h CAS / 24h AC respected; reversibility chaos tested.
- [ ] **10.s06.003.7** prev_state BlobState captured per audit emit.
- [ ] **10.s06.003.8** Cargo-fuzz harness `fuzz_sweep_decision.rs` 1h CI nightly.
- [ ] **10.s06.003.9** Mann-Whitney 3-prong middleware-grade (sweep timing).
- [ ] **10.s06.003.10** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s06.003.11** Cost regression gate: per-sweep ≤ $0.000005 (1 EXISTS check + 1 D1 batch).
- [ ] **10.s06.003.12** Métricas (8 listadas §6.1.8) emitted; CRITICAL alert em INV-GC-004 violations > 0.

## 11. DoD

- [ ] Sweep module compila + integration tests green.
- [ ] All Gherkin green.
- [ ] Property tests 10k green; **100k nightly green** (sprint contract DoD).
- [ ] **0 INV-GC-004 violations em 30d staging** (post-sprint observation).
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Audit emission outbox operational; fail-closed validated.
- [ ] Architect + AppSec + **Crypto SME mandatory emphatic** + Security Lead reviews.
- [ ] PRR Architect + Crypto SME mini sign-off.
- [ ] Cost regression gate green.
- [ ] TLA+ `gc_correctness.tla` aligned (WI-S06-006 CI gate).

## 12. Invariants Validated

- **INV-GC-001** (CRITICAL, registry §3.4 + TLA+ `gc_correctness.tla`): reachable never deleted; sweep enforces via INV-GC-004 strict `<` + grace period + audit fail-closed.
- **INV-GC-004** (CRITICAL, registry §3.4 + TLA+ `InvGCReRefProtected`): mark-phase-aware re-ref protection; SQL EXISTS check `ac.created_at >= mark_started_at_ms` strict; property test 100k iter zero violations.
- **INV-GC-SWEEP-AUDIT-FAIL-CLOSED** (CRITICAL, NEW promovida §3.17): audit emit failure → sweep ROLLBACK; preserves both INV-GC-001 and INV-OBS-AUDIT-CHAIN-INTEGRITY.
- **INV-GC-SWEEP-IDEMPOTENT** (HIGH, NEW): re-run on already-swept = AlreadySwept no-op.
- **INV-GC-SWEEP-TENANT-SCOPED** (CRITICAL, NEW): all queries WHERE tenant_id; sqlx prepared.
- **INV-GC-GRACE-RESPECTED** (HIGH, NEW): physical-delete (WI-S06-004) only fires post-grace; CAP-GC-002 reversibility.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Sweep module | `crates/corelink-gc/src/sweep/` | Rust |
| Property tests | `crates/corelink-gc/tests/prop_sweep.rs` (incl. 100k race) | Rust |
| Mann-Whitney timing | `crates/corelink-gc/tests/timing_sweep.rs` | Rust |
| Cargo-fuzz | `crates/corelink-gc/fuzz/fuzz_targets/fuzz_sweep_decision.rs` | Rust |
| Chaos suite | `tests/chaos_gc_sweep.rs` | Rust |
| README + threat model | `crates/corelink-gc/src/sweep/README.md` | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s06.003.1: Zero `unsafe`; zero `unwrap`.
- 14.s06.003.2: rustdoc 100% public API.
- 14.s06.003.3: Test coverage ≥ 95% (cripto boundary).
- 14.s06.003.4: Latência: per-sweep p99 ≤ 50ms (1 EXISTS check + 1 batch); phase ≤ 5 min p99 @ 100k candidates.
- 14.s06.003.5: SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz 1h CI nightly.
- 14.s06.003.6: Métricas: 8 listadas §6.1.8.
- 14.s06.003.7: Memory bounded: per-sweep ≤ 100 KiB stack.
- 14.s06.003.8: Cost regression gate per-sweep ≤ $0.000010 (2× headroom).

## 15. Chaos Experiments (12)

Listed §6.1.11.

## 16. PRR

Mini-PRR Architect + **Crypto SME mandatory emphatic** + Security Lead.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Sweep module skeleton | 1.5 |
| ST-002 | INV-GC-004 SQL EXISTS check impl | 3 |
| ST-003 | Soft-delete D1 UPDATE + RETURNING prev_state | 3 |
| ST-004 | D1 batch atomic (blob_meta + gc_candidate + audit_outbox) | 3 |
| ST-005 | Audit emit fail-closed logic | 2 |
| ST-006 | Grace period env-config + validation | 1.5 |
| ST-007 | Idempotent re-run logic | 2 |
| ST-008 | Phase budget enforcement | 1.5 |
| ST-009 | DSR erasure signal handler stub (S-11 forward) | 2 |
| ST-010 | Re-upload undelete integration (CAS write handler S-01) | 2 |
| ST-011 | Property tests (5 × 10k; **100k race nightly**) | 6 |
| ST-012 | Mann-Whitney 3-prong timing test | 3 |
| ST-013 | Cargo-fuzz harness | 2 |
| ST-014 | Chaos suite (12 scenarios) | 5 |
| ST-015 | Métricas emit (8 metrics; CRITICAL alert INV-GC-004 violations) | 2.5 |
| ST-016 | Crypto SME review iter (TLA+ obligation alignment) | 4 |
| ST-017 | Architect + AppSec + Security Lead review iter | 3 |

**Total Optimistic**: ~47h. **PERT** (O=42h, M=49h, P=74h): **~52h**.

## 18. Dependencies

- Hard: WI-S06-001 + WI-S06-002 SEALED; WI-S01-001 blob_meta + WI-S01-004 audit_outbox + S-04 ac_meta + S-05 manifest_chunks SEALED; TLA+ `gc_correctness.tla` verified Lote 5.13.
- Soft: WI-S06-006 (TLA+ CI gate) — outbound; this WI provides property test foundation.
- Outbound: WI-S06-004 (physical-delete consumes 'swept' status); WI-S06-006 (CI gate); S-11 (DSR integration forward).

## 19. Effort PERT: 52h. ## 20. Time-boxing: 60h hard limit.

## 21. Observability

8 métricas §6.1.8. Trace span `gc.sweep.{phase, decision, audit_emit}`. Logs: INFO normal; WARN protected_re_ref spike (>5% sustained); ERROR audit emit fail; CRITICAL INV-GC-004 violation > 0 (must be 0 always).

## 22. Cost Analysis

**Per-sweep cost** (SQL EXISTS + D1 batch + audit emit):
- D1 SELECT (EXISTS check): ~$0.000001.
- D1 batch (3 statements): ~$0.000003.
- Audit outbox INSERT: ~$0.0000005.
- Per-sweep: ~$0.000005.

**TCO 12m projection** (5 regions × 100 tenants × 10k candidates/run × 1 run/dia):
- 5 × 100 × 10k × 365 × $0.000005 = **~$913/yr** sweep compute.

**Cost regression gate**: per-sweep ≤ $0.000010 (2× headroom).

## 23. API Contract

`SweepPhase` trait + `SweepResult`, `SweepDecision`, `BlobState`, `SweepError`. `#[non_exhaustive]` em `SweepDecision`.

## 24. Post-mortem Hooks

- **INV-GC-004 violation detected** (any > 0) → CRITICAL post-mortem + customer notification + ANPD/DPC consideration; SEV-0 incident response.
- Audit emit failure spike > 5/h → SEV-1 + outbox health review.
- Phase budget exceeded > 1h sustained → SEV-1 + sweep scaling.
- protected_re_ref rate > 10% sustained → SEV-2 (mark phase too slow OR concurrent UpdateAR storm).
- Grace period drift via timestamp arithmetic → CRITICAL (data loss reversibility broken).

## 25. Rollback / Recovery

Sweep idempotent; soft-delete reversible WITHIN grace; rollback via `blob_meta.deleted_at_ms = NULL` UPDATE; RTO ≤ 30 min; RPO 0 (within grace; post-grace physical-delete irreversível).

## 26. Security & Privacy (compact)

**STRIDE**: tenant_id NOT NULL; sqlx prepared; INV-GC-004 strict `<`; audit fail-closed; SEV-0 on violations. **LINDDUN**: tenant_id pseudonymous; prev_state BlobState forensic preserves audit chain; DSR erasure interaction documented.

## 27. Knowledge Transfer

- **Tech talk** (2h): "Sweep Phase + INV-GC-004 Enforcement + TLA+ Obligation + Audit Fail-Closed".
- **Doc** `crates/corelink-gc/src/sweep/README.md` — INV-GC-004 SQL EXISTS check rationale; strict `<` semantics; audit fail-closed contract.
- **Workshop** (3h): com Crypto SME + Architect + AppSec + downstream WI authors. Includes pair-program review of property test 100k race.
- **Onboarding test** (5 questions): INV-GC-004 strict `<` rationale, audit fail-closed motivation, grace period values, prev_state purpose, idempotent re-run mechanism.
- **External-facing**: blog post post-S-06 SEALED — "How CoreLink prevents GC data loss: TLA+ + property test 100k race + audit fail-closed".

## 28. Risk Register (12-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-GC-004 violation (mark-phase-aware bypass) | L | M | CRITICAL | L | LOW | Strict `<` SQL EXISTS; property test 100k race; TLA+ obligation; chaos under load 30d |
| R-002 | Audit emit failure → soft-delete persisted (INV violation) | L | M | CRITICAL | L | LOW | D1 batch atomic ROLLBACK; SweepError::AuditEmissionFailed; SEV-1 alert |
| R-003 | Soft-delete reversibility expired (grace bug) | L | L | HIGH | L | LOW | Env-config grace; physical-delete WI-S06-004 checks `deleted_at < now - grace`; chaos test boundary |
| R-004 | Cross-tenant sweep injection | L | L | CRITICAL | L | LOW | tenant_id NOT NULL + sqlx prepared + clippy lint |
| R-005 | DSR erasure interaction violates other tenants | L | L | HIGH | L | LOW | DSR scoped per-tenant; cross-tenant impossible by INV-TENANT-ISOLATION |
| R-006 | Worker crash mid-sweep partial state | M | L | LOW | L | LOW | Idempotent resume; AlreadySwept no-op; gc_candidate atomic UPDATE |
| R-007 | Phase budget exceeded for fat tenants | M | L | HIGH | L | LOW | PhaseBudgetExceeded error; SEV-2 alert; admin override S-13 |
| R-008 | Grace period drift via timestamp arithmetic bug | L | M | HIGH | L | LOW | Always unix ms; CHECK constraint; CI test boundary |
| R-009 | protected_re_ref rate spike (mark phase slow) | M | L | MEDIUM | L | LOW | Metric alert > 5% sustained; mark phase optimization S-09 forward |
| R-010 | Re-upload undelete integration bug (CAS write handler) | L | M | MEDIUM | L | LOW | Integration test S-01 + S-06 boundary; chaos test |
| R-011 | Cost regression > 10% per-sweep | M | L | MEDIUM | L | LOW | §14.10 cost gate |
| R-012 | TLA+ obligation drift (InvGCReRefProtected semantic shift) | L | M | HIGH | L | LOW | TLA+ CI gate (WI-S06-006); spec sync test |

## 29. Review Checkpoints

D+0 design (Architect + Crypto SME); D+2 AppSec; D+5 code review; D+6 Crypto SME independent review (TLA+ alignment); D+8 adversarial; D+9 chaos suite; D+10 PRR mini.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-GC-004 + audit fail-closed_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory**_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; LGPD Art. 16 retention review_ |
| 10 | Privacy | _TBD; DSR interaction review_ |
| 11 | Architect | _TBD; **mandatory** — INV-GC-004 strict `<` + audit fail-closed_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — TLA+ obligation alignment_ |
| 13 | Crypto SME | _**MANDATORY EMPHATIC** — TLA+ `gc_correctness.tla` `InvGCReRefProtected` independent review; property test 100k race verification; strict `<` semantics_ |

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S06-003 (Lote 10.6; SOTA pós-Lote 10.5bis lessons applied: TenantCtx-only; D1 batch atomic ROLLBACK; audit fail-closed; strict `<` per TLA+; property test 100k race; cargo-fuzz 1h; Mann-Whitney 3-prong middleware-grade; Crypto SME MANDATORY EMPHATIC).

## 32. Anti-patterns evitados

- ❌ INV-GC-004 `<=` (must be strict `<`); ❌ Skip audit emit (fail-closed mandatory); ❌ Soft-delete sem grace; ❌ Cross-tenant sweep; ❌ Skip phase budget; ❌ Trust client tenant_id; ❌ Sync emit em hot path; ❌ ALTER ADD CONSTRAINT chk_*; ❌ Grace < 72h CAS sem ADR; ❌ Skip prev_state capture; ❌ Variable-time SQL EXISTS sem prepared; ❌ Grace period drift via timestamp arithmetic; ❌ Trust ac.created_at sem strict `>=` semantics.

---

**Fim WI-S06-003.** Próximo: WI-S06-004 (Physical delete worker + R2 DeleteObject + idempotency post-grace).

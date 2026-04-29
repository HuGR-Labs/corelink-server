---
id: "WI-S06-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
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
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s06", "gc", "worker", "scheduler", "degrade-mode", "high-risk"]
---

# WI-S06-001 — Worker-gc Binary Skeleton + Cron Scheduler (per-region; jitter ±10min) + Degrade-Mode `gc-pause` Emergency Stop + Manual Trigger Admin API + gc_run Checkpoint Table

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-001 |
| Título | `crates/corelink-gc/` worker binary; sticky Durable Object per region (5 regions); cron 02:00 UTC + jitter ±10min para evitar thundering herd; degrade-mode `gc-pause` global via DO config-singleton (PAT-DEGRADE-001 alignment); manual trigger admin API stub (S-13 forward); `gc_run` checkpoint table (idempotent re-run; phase tracking); INV-GC-IDEMPOTENT-RERUN enforce |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (GC reachability load-bearing; worker bug = INV-GC-001 violation cascade), FF-HR-005 (GC é controle de integridade de dados) |

## 1. Intent

`crates/corelink-gc/` — worker binary que orquestra mark + sweep + physical-delete + reconcile (delegated WI-S06-002..005). Este WI provê o skeleton + scheduler + degrade-mode + checkpoint table:

```rust
// File: crates/corelink-gc/src/lib.rs

#![forbid(unsafe_code)]

pub use crate::scheduler::{GcScheduler, ScheduleConfig};
pub use crate::worker::{GcWorker, GcRun, GcPhase, GcStatus};
pub use crate::degrade::{DegradeMode, DegradeKind};
pub use crate::error::GcError;

pub mod scheduler;
pub mod worker;
pub mod degrade;
pub mod error;

#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    /// Cron expression: default "0 2 * * *" (02:00 UTC daily).
    pub cron: String,
    /// Jitter ±N minutes per region (avoid thundering herd 5 regions × 02:00 UTC).
    pub jitter_minutes: u32,         // default 10
    /// Region attribution.
    pub region: Region,              // 'sam' | 'iad' | 'lhr' | 'nrt' | 'syd'
    /// Per-tenant batch concurrency cap.
    pub max_concurrent_tenants: u32, // default 4
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcPhase {
    Idle,
    Mark { mark_started_at: u64 },           // unix ms; persisted em gc_run
    Sweep { sweep_started_at: u64, mark_run_id: u64 },
    PhysicalDelete { delete_started_at: u64 },
    Reconcile { reconcile_started_at: u64 },
    Completed { completed_at: u64 },
    Failed { phase: String, error_message: String, failed_at: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcStatus {
    /// Cron tick scheduled; worker hasn't started.
    Pending,
    /// Worker active; phase tracked em gc_run.phase.
    Running,
    /// Phases completed successfully.
    Succeeded,
    /// Worker crashed mid-phase; idempotent re-run safe.
    Crashed { phase_when_crashed: String },
    /// Aborted via degrade-mode `gc-pause`.
    Aborted,
    /// Phase failure (PhaseBudgetExceeded, PhaseFailure non-recoverable);
    /// Lote 10.6bis P0-4 fix: variant added (was missing); WI-S06-002 §6.1.10
    /// sets status='failed' which previously CHECK rejected; gap closed.
    Failed { phase: String, reason: String, failed_at_ms: u64 },
}

pub struct GcRun {
    pub run_id: u64,                          // UUIDv7-derived; monotonic per region
    pub region: Region,
    pub tenant_id: TenantId,                  // single-tenant per run
    pub phase: GcPhase,
    pub status: GcStatus,
    pub started_at_ms: u64,
    pub last_checkpoint_at_ms: u64,           // updated per batch; idempotent re-run anchors here
    pub mark_started_at_ms: Option<u64>,      // INV-GC-004 anchor for sweep phase
    pub blobs_marked_count: u64,
    pub blobs_swept_count: u64,
    pub blobs_physically_deleted_count: u64,
    pub bytes_reclaimed: u64,
    pub created_by_request_id: String,        // cron OR admin-trigger
}

#[async_trait]
pub trait GcWorker: Send + Sync {
    /// Entry point invoked by cron OR admin trigger.
    /// Idempotent: re-running for same `(region, tenant_id, run_id)` resumes from checkpoint.
    async fn execute_run(
        &self,
        config: &ScheduleConfig,
        tenant_id: &TenantId,
    ) -> Result<GcRun, GcError>;

    /// Probe degrade-mode: if `gc-pause` enabled, abort immediately.
    async fn check_degrade_mode(&self) -> Result<DegradeMode, GcError>;
}

#[derive(Debug, Clone)]
pub struct DegradeMode {
    pub kind: DegradeKind,
    pub enabled_by_pat_id: Option<String>,
    pub enabled_at_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradeKind {
    Off,                              // GC normal operation
    GcPause,                          // PAT-DEGRADE-001: GC suspended; cron triggers reject
    GcReadOnly,                       // emergency: only physical-delete suspended; mark+sweep continue
}

#[derive(thiserror::Error, Debug)]
pub enum GcError {
    #[error("degrade mode active: {0:?}")]
    DegradeModeActive(DegradeKind),

    #[error("checkpoint table corruption: {0}")]
    CheckpointCorruption(String),

    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("phase failed: {phase}: {reason}")]
    PhaseFailure { phase: String, reason: String },

    #[error("admin trigger unauthorized: PAT {0} lacks gc:trigger scope")]
    UnauthorizedTrigger(String),
}
```

```sql
-- File: migrations/006_gc_run.sql
-- Lote 10.4bis+10.5bis lessons applied: CHECK constraints inline; BEGIN/COMMIT removidos.

CREATE TABLE IF NOT EXISTS gc_run (
  -- Run identity (PK; UUIDv7-derived monotonic)
  run_id              TEXT        NOT NULL PRIMARY KEY,

  -- Tenant scope (NOT NULL; cross-tenant gc impossible by design)
  tenant_id           TEXT        NOT NULL,

  -- Region scope
  region              TEXT        NOT NULL,

  -- Phase tracking
  phase               TEXT        NOT NULL DEFAULT 'idle',  -- idle|mark|sweep|physical_delete|reconcile|completed|failed
  status              TEXT        NOT NULL DEFAULT 'pending',  -- pending|running|succeeded|crashed|aborted

  -- Lifecycle timestamps (unix ms)
  started_at_ms       INTEGER     NOT NULL,
  last_checkpoint_at_ms INTEGER   NOT NULL,
  completed_at_ms     INTEGER     NULL,
  failed_at_ms        INTEGER     NULL,

  -- INV-GC-004 anchor (mark_started_at is critical for sweep-phase-aware re-ref protection)
  mark_started_at_ms  INTEGER     NULL,                  -- set at Mark phase start; NULL until Mark begins

  -- Counters
  blobs_marked_count  INTEGER     NOT NULL DEFAULT 0,
  blobs_swept_count   INTEGER     NOT NULL DEFAULT 0,
  blobs_physically_deleted_count INTEGER NOT NULL DEFAULT 0,
  bytes_reclaimed     INTEGER     NOT NULL DEFAULT 0,

  -- Source attribution
  created_by_request_id TEXT      NOT NULL,              -- 'cron' OR admin PAT id
  failed_phase        TEXT        NULL,
  failed_reason       TEXT        NULL,

  -- CHECK constraints inlined (Lote 10.4bis lesson)
  CHECK (phase IN ('idle', 'mark', 'sweep', 'physical_delete', 'reconcile', 'completed', 'failed')),
  CHECK (status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted', 'failed')),  -- Lote 10.6bis P0-4 fix: 'failed' variant added; WI-S06-002 §6.1.10 sets status='failed' on PhaseBudgetExceeded
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  CHECK (last_checkpoint_at_ms >= started_at_ms),
  CHECK ((completed_at_ms IS NULL) OR (completed_at_ms >= started_at_ms)),
  CHECK (mark_started_at_ms IS NULL OR mark_started_at_ms >= started_at_ms),
  CHECK (blobs_marked_count >= 0 AND blobs_swept_count >= 0 AND blobs_physically_deleted_count >= 0)
);

-- Index: per-tenant + region + status (query "current running" per tenant)
CREATE INDEX IF NOT EXISTS idx_gc_run_tenant_region_status
  ON gc_run(tenant_id, region, status);

-- Index: stale runs (running + last_checkpoint old) — sweeper detects crashed workers
CREATE INDEX IF NOT EXISTS idx_gc_run_stale_running
  ON gc_run(status, last_checkpoint_at_ms)
  WHERE status = 'running';

-- Partial UNIQUE: only one running gc_run per (tenant, region) (Lote 10.5bis lesson partial UNIQUE)
CREATE UNIQUE INDEX IF NOT EXISTS uq_gc_run_running
  ON gc_run(tenant_id, region)
  WHERE status = 'running';
```

**Cripto-driven invariants**:

1. **Single running gc_run per (tenant, region)**: partial UNIQUE em `WHERE status = 'running'` (Lote 10.5bis lesson); previne race two cron ticks concurrent.
2. **Tenant-scoped strict**: `gc_run.tenant_id NOT NULL`; per-region per-tenant isolation; cross-tenant GC catastrophic FM-300.
3. **Idempotent re-run**: worker crashed mid-phase → status='crashed' → next cron tick OR admin trigger resumes from `last_checkpoint_at_ms`; PAT-RETRY-IDEMPOTENT-001.
4. **mark_started_at_ms é INV-GC-004 anchor**: persisted at Mark phase start; sweep phase enforces `ac.created_at >= mark_started_at_ms` protects (canonical TLA `gc_correctness.tla` L152-154 — protect-if-equal-or-newer; equivalent: sweep only deletes if all AC `ac.created_at < mark_started_at_ms`).
5. **Degrade-mode `gc-pause` global stop**: DO config-singleton; PAT-DEGRADE-001; emergency abort.

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

GC worker é **single point of failure** for INV-GC-001 (reachable never deleted). Bug em scheduler / checkpoint / phase orchestration cascades para mark / sweep / physical-delete:

1. **Two concurrent gc_runs (same tenant, same region)**: cron jitter mal-configurado fires twice; OR manual admin trigger overlaps cron tick. Mitigação: partial UNIQUE INDEX `WHERE status='running'`; second gc_run INSERT fails; handler retorna 409 `COR_GC_ALREADY_RUNNING`; integration test asserts.

2. **Cron thundering herd 5 regions × 02:00 UTC**: 5 worker instances fire simultaneously; D1 query load spike; potential D1 throttle. Mitigação: jitter ±10 min per region; effectively spread fires across 02:00..02:50 UTC; `corelink.gc.scheduler.cron_fired_total{region}` metric.

3. **Worker crash mid-phase com checkpoint stale**: worker dies after `last_checkpoint_at_ms = T`; next cron tick sees stale running gc_run. Mitigação: `idx_gc_run_stale_running` partial index detects status='running' AND last_checkpoint > 1h old → mark as crashed → next tick resumes; chaos test #5.

4. **Degrade-mode `gc-pause` race**: operator enables `gc-pause`; worker mid-execute may not see flag immediately. Mitigação: worker probes degrade-mode at every batch boundary (per WI-S06-002 mark loop); abort within 100ms of flag change; chaos test #6.

5. **Manual admin trigger floods**: malicious admin OR misconfig fires manual GC every 1 min; system overload. Mitigação: per-tenant rate limit (admin API S-13 forward; staging stub OK); audit emit cada manual trigger.

6. **Phase transition race**: worker completes Mark, transitions to Sweep; another worker (somehow concurrent) reads gc_run em Mark phase. Mitigação: phase transition é atomic D1 UPDATE; partial UNIQUE prevents concurrent; status change observable via `last_checkpoint_at_ms`.

7. **TenantCtx-only enforcement**: all D1 ops use `WHERE tenant_id = ctx.tenant_id`; no body/query/header trust (lesson Lote 10.4bis WI-S04-001).

8. **Audit emission per phase transition**: outbox pattern (lesson Lote 10.4bis: WI-S01-004 audit_outbox NOT WI-S01-005); `corelink.gc.phase_transitioned` event com from→to phases.

**Atacante adversarial scenarios**:

- **Cross-tenant gc_run injection**: defense-in-depth via tenant_id NOT NULL + sqlx prepared statements + clippy lint (no `&str` SQL literals).
- **Degrade-mode bypass**: admin attempts to clear gc-pause flag while incident active; audit chain captures (S-09).
- **Race exhaustion via crashed runs**: 1000 crashed runs accumulated; idx_gc_run_stale_running becomes hot. Mitigação: cron daily cleanup truncates `gc_run` rows > 30d old; size cap.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: GC reachability is the load-bearing invariant; worker scheduler bug cascades to deletion.
- **FF-HR-005**: GC integrity control; bug = customer trust loss permanente.
- **Reversibility**: worker crash recoverable via checkpoint; phase transitions atomic; gc_run tracking forensic.

11 sign-offs (canonical Lote 10.6 cycle 4 alignment with framework §33.5.4.3 HIGH_RISK lane 10–12).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**: invisible (worker runs em background); customer-visible only via `bytes_reclaimed_last_30d` metric em dashboard S-16 (forward).

**Persona 2 — DevOps reviewing operational readiness**: cron schedule per-region; degrade-mode `gc-pause` available via admin API stub; gc_run table queryable for incident triage.

**Persona 3 — On-call engineer**: incident "GC stalled"; checks `idx_gc_run_stale_running`; identifies crashed worker; manual trigger via admin API resumes from checkpoint.

**SLA addendum**:
- Cron tick latency ≤ 30s p99 (jitter aplicado).
- Degrade-mode propagation ≤ 100ms (next batch boundary).
- gc_run checkpoint write ≤ 100ms p99 (D1 single row UPDATE).
- Stale running detection ≤ 1h (worker crashed; partial index identifies).

## 4. Capability Mapping

- **CAP-GC-001** (Mark-and-sweep) — IMPLEMENTA partial (skeleton; mark/sweep em WI-002/003).
- **CAP-GC-007** (Worker non-blocking with writes) — IMPLEMENTA primary.
- Trace: `failure_modes.md FM-300/305/404` + `resilience_patterns.md PAT-DEGRADE-001` + `security_model.md CTRL-GC-001/002`.

## 5. Tipo

Worker skeleton; HIGH_RISK; FF-HR-011 + FF-HR-005.

## 6. Escopo (compact)

### 6.1 In-scope

1. Crate `corelink-gc` skeleton + module structure.
2. `GcScheduler` impl: cron + jitter ±10min per region.
3. `GcWorker` trait + `GcWorkerImpl` struct (orchestrates mark→sweep→physical-delete→reconcile delegated to WI-002..005).
4. **gc_run D1 schema** (CHECK inline; partial UNIQUE WHERE status='running' lesson Lote 10.5bis).
5. **Degrade-mode `gc-pause` DO config-singleton** wiring; PAT-DEGRADE-001 alignment.
6. **Manual trigger admin API stub** (`POST /v1/admin/gc/trigger?tenant_id=X&region=Y`); S-13 admin plane forward; staging stub returns 501 in non-staging envs.
7. **Wrangler DO binding** per region (5 instances).
8. **Audit emission outbox** (WI-S01-004 audit_outbox; lesson Lote 10.4bis): `corelink.gc.phase_transitioned`, `corelink.gc.run_started`, `corelink.gc.run_completed`, `corelink.gc.run_aborted`.
9. **Métricas**:
   - `corelink.gc.scheduler.cron_fired_total{region}`.
   - `corelink.gc.worker.run_started_total{tenant_id, region}`.
   - `corelink.gc.worker.run_completed_total{tenant_id, region, status}`.
   - `corelink.gc.worker.phase_duration_ms{phase, tenant_id, region}` (histogram).
   - `corelink.gc.degrade_mode_active{kind}` (gauge; alert if = `GcPause` sustained > 1h sem ADR).
   - `corelink.gc.stale_running_count` (gauge; alert > 5 sustained).
10. **Property tests** (10k iter PR; 100k nightly):
    - `prop_gc_run_idempotent_resume`: 1000 random crashed runs; resume from checkpoint = same final state.
    - `prop_gc_run_partial_unique`: 1000 concurrent INSERT same `(tenant, region, status='running')` → 100% rejected after first.
    - `prop_gc_run_phase_transitions`: random phase sequences; only valid transitions (idle→mark→sweep→...→completed) accepted.
    - `prop_degrade_mode_propagation`: enable gc-pause; worker aborts ≤ 100ms; integration test bound.
    - `prop_tenant_isolation_gc_run`: 1000 concurrent runs different tenants; no cross-tenant interference.
11. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. Worker crash mid-mark phase → idempotent resume.
    - 2. Cron thundering herd (force 5 regions simultaneously) → jitter spreads.
    - 3. Two cron ticks same `(tenant, region)` → second rejected via partial UNIQUE.
    - 4. Degrade-mode `gc-pause` mid-phase → abort ≤ 100ms.
    - 5. Stale running detection (last_checkpoint > 1h old) → mark crashed.
    - 6. Manual trigger flood → rate limit S-13 forward enforces.
    - 7. D1 throttle during checkpoint write → backoff + retry; PAT-RETRY-IDEMPOTENT-001.
    - 8. gc_run table size > 1M rows → cron daily truncate > 30d.
    - 9. Admin API unauthorized trigger → 403 + audit.
    - 10. Cross-region replication delay → per-region independence preserved.

### 6.2 Out-of-scope

- Mark phase impl (WI-S06-002).
- Sweep phase impl (WI-S06-003).
- Physical delete (WI-S06-004).
- Reconcile (WI-S06-005).
- TLA+ CI gate integration (WI-S06-006).
- DASH-GC dashboard (WI-S06-007).

## 7. Anti-Scope

- ❌ Cross-tenant gc_run records.
- ❌ Concurrent running runs same `(tenant, region)`.
- ❌ Skip checkpoint write (idempotency baseline).
- ❌ Skip degrade-mode probe per batch boundary.
- ❌ ALTER TABLE ADD CONSTRAINT chk_* (Lote 10.4bis lesson; CHECK inline mandatory).
- ❌ BEGIN/COMMIT em migration (wrangler implicit; Lote 10.4bis lesson).
- ❌ `with_tenant_ctx!` claims em D1 (Postgres-only; Lote 10.4bis lesson).
- ❌ Trust client-supplied tenant_id (TenantCtx-only; Lote 10.4bis lesson).
- ❌ Hard-coded cron schedule (env-config + admin override).
- ❌ Skip audit emission per phase transition.

## 8. Acceptance Criteria (Gherkin) (compact 10 scenarios)

```gherkin
Feature: Worker-gc skeleton + scheduler + degrade-mode

  Scenario: Cron tick fires per region with jitter
    Given 5 region instances configured
    When cron fires at 02:00 UTC
    Then each region's worker fires within 02:00..02:50 UTC (jitter ±10min)
    And metric corelink.gc.scheduler.cron_fired_total{region} +1 per region

  Scenario: Worker resumes from checkpoint after crash
    Given gc_run has status='running' last_checkpoint_at_ms = T
    Given worker crashed mid-mark phase
    When stale detection runs (status='running' AND last_checkpoint > 1h)
    Then status updated to 'crashed'
    When next cron tick OR manual trigger fires
    Then worker resumes from checkpoint (idempotent)
    And property test prop_gc_run_idempotent_resume green

  Scenario: Partial UNIQUE prevents concurrent runs
    Given gc_run row (tenant_A, region_sam, status='running') exists
    When second INSERT same (tenant_A, region_sam, status='running')
    Then UNIQUE INDEX uq_gc_run_running rejects
    And handler returns 409 COR_GC_ALREADY_RUNNING

  Scenario: Degrade-mode gc-pause aborts active worker
    Given worker mid-mark phase
    When operator enables degrade-mode gc-pause via admin API
    Then worker probes at next batch boundary (≤ 100ms)
    And worker transitions phase=failed status=aborted
    And audit emit corelink.gc.run_aborted

  Scenario: Tenant isolation
    Given 1000 concurrent gc_runs different tenants
    When all execute concurrently
    Then no cross-tenant interference
    And property test prop_tenant_isolation_gc_run green

  Scenario: Manual trigger via admin API (staging stub)
    Given admin PAT with gc:trigger scope
    When POST /v1/admin/gc/trigger?tenant_id=X&region=sam
    Then worker invoked OR 501 if non-staging
    And audit emit cas.gc.manual_trigger

  Scenario: Manual trigger unauthorized
    Given PAT without gc:trigger scope
    When POST /v1/admin/gc/trigger
    Then 403 + COR_AUTH_SCOPE_INSUFFICIENT
    And audit emit auth.denied.scope

  Scenario: Phase transitions valid
    Given gc_run em phase=mark
    When phase transitions
    Then valid: idle→mark→sweep→physical_delete→reconcile→completed
    And invalid (e.g., mark→completed direct) rejected

  Scenario: gc_run table size cap
    Given gc_run has 100000 rows > 30d old
    When daily cron cleanup runs
    Then DELETE WHERE completed_at_ms < now - 30d
    And table size manageable

  Scenario: Migration 006 applies idempotently (CHECK inline; partial UNIQUE)
    When wrangler d1 migrations apply CORELINK_DB --env staging
    Then gc_run table exists com 7 CHECK constraints + partial UNIQUE INDEX
    When re-applied
    Then no-op
```

## 9. Design Decisions (compact)

- **9.1** Sticky DO per region (5 instances; pattern reuse WI-S04-005 sweeper).
- **9.2** Cron jitter ±10min spread thundering herd.
- **9.3** Partial UNIQUE `WHERE status='running'` (Lote 10.5bis lesson partial UNIQUE for state-scoped uniqueness).
- **9.4** CHECK constraints inline (Lote 10.4bis SQLite/D1 lesson).
- **9.5** Degrade-mode probe per batch boundary (lesson WI-S04-005 cron alarm pattern).
- **9.6** Audit outbox WI-S01-004 (Lote 10.4bis citation lesson).
- **9.7** mark_started_at_ms é INV-GC-004 anchor — captured ONCE at Mark phase start; sweep enforces `ac.created_at >= mark_started_at_ms` protects (canonical TLA L152-154 protect-if-equal-or-newer).
- **9.8** Manual admin trigger é forward S-13; staging stub OK.
- **9.9** ADR forward — ADR-0042 (worker scheduler design + degrade-mode contract; ratificada em WI-S06-007 ship gate).
- **9.10** No new ADR-numbered for gc_run schema (governed by existing ADR-0036 schema migration governance).

## 10. Completeness Criteria SOTA

- [ ] **10.s06.001.1** Property tests 5 × 10k iter green; 100k nightly.
- [ ] **10.s06.001.2** Chaos suite 10 scenarios green.
- [ ] **10.s06.001.3** Migration 006 idempotent + CHECK inline + partial UNIQUE; SQLite/D1 dry-run em CI gate.
- [ ] **10.s06.001.4** Cron jitter ±10min validated em integration test (5 regions span 02:00..02:50 UTC).
- [ ] **10.s06.001.5** Degrade-mode `gc-pause` propagation ≤ 100ms validated em chaos test.
- [ ] **10.s06.001.6** Stale running detection ≤ 1h via partial index.
- [ ] **10.s06.001.7** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s06.001.8** Cost regression gate: per-cron-tick cost ≤ $0.000005 + per-checkpoint cost ≤ $0.000001.
- [ ] **10.s06.001.9** Métricas (6 listadas §6.1.9) emitted; dashboard widget partial (full em WI-S06-007).
- [ ] **10.s06.001.10** Audit emission outbox (4 events) operational.

## 11. DoD

- [ ] Crate compila + integration tests green.
- [ ] All Gherkin green.
- [ ] Property tests 10k green; 100k nightly.
- [ ] Migration 006 applied em staging.
- [ ] Cron schedule deployed 5 regions.
- [ ] Degrade-mode wired DO config-singleton.
- [ ] Manual trigger admin API stub deployed (501 fallback non-staging).
- [ ] Architect + AppSec + SRE reviews.
- [ ] PRR Architect mini sign-off.

## 12. Invariants Validated

- **INV-GC-IDEMPOTENT-RERUN** (HIGH, NEW promovida §3.17): worker crashed mid-phase; resume from checkpoint = same final state.
- **INV-GC-SINGLE-RUNNING-PER-TENANT-REGION** (HIGH, NEW): partial UNIQUE em `WHERE status='running'`.
- **INV-GC-PHASE-MONOTONIC** (HIGH, NEW): valid transitions only; reverse rejected.
- **INV-GC-MARK-STARTED-AT-IMMUTABLE** (CRITICAL, NEW): captured ONCE at Mark phase; strict `<` comparison em sweep (INV-GC-004 TLA+ obligation).
- **INV-GC-DEGRADE-MODE-PROBE-PER-BATCH** (HIGH, NEW): worker checks at every batch boundary; abort ≤ 100ms.
- **INV-AUTH-TENANTCTX-IMMUTABLE** (CRITICAL, registry §3.14): TenantCtx propagation; reuse S-03.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate `corelink-gc` | `crates/corelink-gc/` | Rust |
| GcScheduler | `crates/corelink-gc/src/scheduler.rs` | Rust |
| GcWorker trait + Impl | `crates/corelink-gc/src/worker.rs` | Rust |
| Degrade-mode | `crates/corelink-gc/src/degrade.rs` | Rust |
| Migration | `migrations/006_gc_run.sql` | SQL |
| Wrangler DO bindings | `wrangler.toml` | TOML |
| Property tests | `crates/corelink-gc/tests/prop_scheduler.rs` | Rust |
| Chaos suite | `tests/chaos_gc_scheduler.rs` | Rust |
| ADR-0042 (forward) | `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md` | Markdown |
| README | `crates/corelink-gc/README.md` | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s06.001.1: Zero `unsafe`; zero `unwrap` em src/.
- 14.s06.001.2: rustdoc 100% public API + 4 examples.
- 14.s06.001.3: Test coverage ≥ 90% (worker scheduler boundary).
- 14.s06.001.4: Latência: cron tick fire p99 ≤ 30s; checkpoint write p99 ≤ 100ms; degrade-mode propagation ≤ 100ms.
- 14.s06.001.5: SAST: cargo-audit + cargo-deny + clippy `-D warnings`.
- 14.s06.001.6: Métricas RED + GC-specific (6 metrics).
- 14.s06.001.7: Runbook stubs forward — `RB-FM-GC-WORKER-STALL` (consumed by WI-S06-007 PRR ship gate).
- 14.s06.001.8: Forward-compat: ADR-0042 documents scheduler design.
- 14.s06.001.9: Memory bounded: worker per-batch ≤ 1 MiB stack + ≤ 10 MiB heap.
- 14.s06.001.10: Cost regression gate: per-cron-tick + per-checkpoint cost gates.

## 15. Chaos Experiments (10; sprint contract HIGH_RISK ≥ 10)

Listed §6.1.11.

## 16. PRR

PRR HIGH_RISK 11 sign-offs gated em WI-S06-007. Mini-PRR: Architect + AppSec + SRE.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Crate skeleton | 1.5 |
| ST-002 | Migration 006 (gc_run table + partial UNIQUE + CHECK inline) | 2 |
| ST-003 | Wrangler DO bindings 5 regions | 1 |
| ST-004 | GcScheduler impl (cron + jitter) | 3 |
| ST-005 | GcWorker trait + Impl skeleton (delegate to WI-002..005) | 4 |
| ST-006 | Degrade-mode DO config-singleton | 2 |
| ST-007 | Manual trigger admin API stub | 2 |
| ST-008 | Audit emission outbox hooks (4 events) | 2 |
| ST-009 | Métricas emit (6 metrics) | 2 |
| ST-010 | Property tests (5 × 10k) | 4 |
| ST-011 | Chaos suite (10 scenarios) | 4 |
| ST-012 | Stale running detection cron | 2 |
| ST-013 | gc_run cleanup cron > 30d | 1 |
| ST-014 | rustdoc + 4 examples + README | 3 |
| ST-015 | ADR-0042 redação (forward) | 2 |
| ST-016 | Architect + AppSec + SRE review iter | 3 |

**Total Optimistic**: ~38h. **PERT** (O=33h, M=39h, P=60h): **~42h**.

## 18. Dependencies

- Hard: D1 framework; Wrangler 4.x; WI-S01-004 audit_outbox table SEALED; WI-S03-003 TenantCtx; WI-S04-005 cron DO pattern reuse.
- Soft: WI-S06-002..005 (worker phases delegate); S-13 admin plane (stub OK).
- Outbound: WI-S06-002 (mark consumes scheduler); WI-S06-007 (ship gate).

## 19. Effort PERT: 42h. ## 20. Time-boxing: 50h hard limit.

## 21. Observability

6 métricas listadas §6.1.9. Trace span `gc.scheduler.cron_tick`, `gc.worker.{phase}`, `gc.degrade.probe`. Logs structured JSON; INFO em normal; WARN em stale running; ERROR em degrade-mode; CRITICAL em phase failure cascade.

## 22. Cost Analysis

**Per-cron-tick cost** (worker invocation + scheduler):
- Worker invocation: $0.50/M.
- D1 SELECT + INSERT (gc_run row): ~$0.000002.
- KV degrade-mode probe: $0.50/M.
- Per-tick: ~$0.000005.

**Per-checkpoint cost** (D1 UPDATE per batch boundary):
- D1 UPDATE: $1/M.
- Per-checkpoint: ~$0.000001.

**TCO 12m projection** (5 regions × 100 tenants × 1 tick/dia + 100 checkpoints/run):
- Tick: 5 × 100 × 365 = 182k × $0.000005 = $0.91/yr.
- Checkpoint: 5 × 100 × 100 × 365 = 18.25M × $0.000001 = $18.25/yr.
- DO compute (5 regions × 30s/dia): negligible.
- Total: **~$20/yr** scheduler + checkpoint infra (worker phases em WI-002..005 cost separate).

**Cost regression gate**: per-tick ≤ $0.000010 (2× headroom); per-checkpoint ≤ $0.000002.

## 23. API Contract

Public crate API (semver post v1.0):
- `GcScheduler`, `GcWorker` traits; `ScheduleConfig`, `GcRun`, `GcPhase`, `GcStatus`, `DegradeMode`, `DegradeKind`, `GcError` types.
- `#[non_exhaustive]` em `GcPhase`, `GcStatus`, `DegradeKind` enums (forward-compat).

Admin API (S-13 forward; staging stub):
- `POST /v1/admin/gc/trigger?tenant_id=X&region=Y` → 200 OK | 403 | 501.
- `POST /v1/admin/gc/degrade?kind=GcPause&reason=...` → 200 OK | 403 | 501.

## 24. Post-mortem Hooks

- Two concurrent gc_runs same `(tenant, region)` detected → CRITICAL post-mortem (UNIQUE constraint violation; data race risk).
- Cron not firing > 24h sustained → SEV-1 (silent GC death; FM-305 escalation).
- Degrade-mode propagation > 1s sustained → SEV-2 (operational responsiveness).
- Manual trigger flood → SEV-2 (rate limit S-13 review).
- Stale running > 5 sustained → SEV-2 (worker crash cascade).

## 25. Rollback / Recovery

Wrangler version revert; gc_run table preserved (no DROP); RTO ≤ 10 min; RPO 0.

## 26. Security & Privacy (compact)

**STRIDE**: tenant_id NOT NULL; sqlx prepared statements; TenantCtx-only enforcement; degrade-mode admin-only; audit outbox per phase. **LINDDUN**: tenant_id pseudonymous; gc_run table no PII; reclaim metric customer-visible (no leak).

## 27. Knowledge Transfer

- Tech talk (1h): "GC Worker Scheduler + Degrade-Mode + Idempotent Resume".
- Doc `docs/internal/gc-worker-scheduler.md`.
- Onboarding test (5 questions): jitter rationale, partial UNIQUE rationale, mark_started_at_ms anchor purpose, degrade-mode propagation bound, stale detection mechanism.

## 28. Risk Register (12-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Two concurrent runs same (tenant, region) | L | L | CRITICAL | L | LOW | Partial UNIQUE WHERE status='running'; integration test |
| R-002 | Cron thundering herd 5 regions | M | L | LOW | L | LOW | Jitter ±10min; metric distribution check |
| R-003 | Worker crash mid-phase no resume | L | M | HIGH | L | LOW | Checkpoint per batch; idempotent resume; PAT-RETRY-IDEMPOTENT-001 |
| R-004 | Degrade-mode gc-pause race (slow propagation) | L | M | MEDIUM | L | LOW | Probe per batch ≤100ms; chaos test |
| R-005 | Manual admin trigger flood | M | L | MEDIUM | L | LOW | S-13 rate limit forward; audit emit per trigger |
| R-006 | gc_run table unbounded growth | M | L | LOW | L | LOW | Daily cron truncate > 30d |
| R-007 | Phase transition race (atomic D1 UPDATE) | L | L | MEDIUM | L | LOW | Single-row UPDATE atomic; partial UNIQUE |
| R-008 | Cross-tenant gc_run injection | L | L | CRITICAL | L | LOW | tenant_id NOT NULL; sqlx prepared; clippy lint |
| R-009 | Cron not firing > 24h | L | M | HIGH | L | LOW | External watchdog (CF Cron Trigger global) detects |
| R-010 | D1 throttle during checkpoint | M | L | LOW | L | LOW | Backoff + retry; PAT-RETRY-IDEMPOTENT-001 |
| R-011 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.10 cost gate |
| R-012 | mark_started_at_ms drift (timezone bug) | L | M | HIGH | L | LOW | Always unix ms; CI test; TLA+ obligation |

## 29. Review Checkpoints

D+0 design (Architect + SRE); D+1 AppSec; D+3 code review; D+4 chaos suite; D+5 PRR mini.

## 30. Sign-off (HIGH_RISK 11)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ |
| 4 | Security Lead | _TBD; **mandatory** — degrade-mode bypass review_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory**_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD_ |
| 10 | Privacy | _TBD_ |
| 11 | Architect | _TBD; **mandatory** — scheduler design + ADR-0042_ |
| 12 | AppSec | _TBD; **mandatory** — TenantCtx-only + degrade-mode_ |
| 13 | Crypto SME | _advisory; consume in WI-002/003/006 (cripto WIs)_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo | Criação WI-S06-001 (Lote 10.6; SOTA pós-Lote 10.5bis lessons applied: partial UNIQUE WHERE status='running'; CHECK inline; BEGIN/COMMIT removidos; tenant_id NOT NULL; TenantCtx-only enforcement; ADR-0042 forward). |
| 1.1.0 | 2026-04-25 | Gustavo | **Lote 10.6bis P0-4 fix**: GcStatus state-machine gap closure — variant `failed` added to enum (line 88) + migration CHECK constraint includes `'failed'` (line 209); `GcPhase::Failed { phase, error_message, failed_at }` carries error context; WI-S06-002 §6.1.10 sets `status='failed'` on `PhaseBudgetExceeded`. |
| 1.2.0 | 2026-04-25 | Gustavo | **Lote 10.6-tris NEW-P1-4 fix**: documented P0-4 resolution in change log explicitly (Sonnet R5 flagged change log silence on P0-4 absorption). State-machine final: `pending → running → {succeeded, crashed (recoverable via checkpoint resume), aborted (manual stop via degrade-mode), failed (terminal: PhaseBudgetExceeded OR PhaseFailure non-recoverable)}`. Cross-referenced WI-S06-002 PhaseBudgetExceeded → status='failed' mapping. |

## 32. Anti-patterns evitados

- ❌ Cross-tenant gc_run; ❌ Concurrent running same (tenant, region); ❌ Skip checkpoint write; ❌ Skip degrade-mode probe per batch; ❌ ALTER ADD CONSTRAINT chk_*; ❌ BEGIN/COMMIT em migration; ❌ with_tenant_ctx! claims em D1; ❌ Trust client tenant_id; ❌ Hard-coded cron schedule; ❌ Skip audit emission; ❌ Variable-time SQL; ❌ Buffered worker (zero-allocation Iterator pattern WI-S05-002 lesson).

---

**Fim WI-S06-001.** Próximo: WI-S06-002 (Mark phase: multi-pass scan D1 + batching + mark_started_at + jitter).

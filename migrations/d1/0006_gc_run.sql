-- CoreLink D1 (Cloudflare SQLite) — migration 0006 for `gc_run`
-- (S-06 Garbage Collection: worker checkpoint table; WI-S06-001 §1).
--
-- Canonical sources:
--   - specs/04_sprints/_sealed/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md §1
--   - specs/04_sprints/_sealed/S06/_spec_contract.md §5.1 (R-S06-1..3) + §5.2..§5.5
--   - specs/03_architecture/invariant_registry.md INV-GC-IDEMPOTENT-RERUN +
--     INV-GC-SINGLE-RUNNING-PER-TENANT-REGION + INV-GC-PHASE-MONOTONIC +
--     INV-GC-MARK-STARTED-AT-IMMUTABLE
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--   - specs/tla/gc_correctness.tla (TLA+ model; mark_started_at anchor)
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL): tenant_id NOT NULL; every query
--     filters by tenant_id; cross-tenant gc_run injection FM-300 impossible.
--   - INV-GC-SINGLE-RUNNING-PER-TENANT-REGION (HIGH; WI §12): partial UNIQUE
--     INDEX `uq_gc_run_running` on (tenant_id, region) WHERE status='running'
--     — second cron tick targeting the same (tenant, region) while the first
--     is in flight is rejected at storage level (Lote 10.5bis lesson:
--     state-conditioned UNIQUE; completed/aborted/failed rows legitimately
--     coexist as forensic trail).
--   - INV-GC-IDEMPOTENT-RERUN (HIGH; WI §12): row preserved across crash;
--     last_checkpoint_at_ms anchors next-tick resume.
--   - INV-GC-PHASE-MONOTONIC (HIGH; WI §12): phase domain enforced via CHECK
--     (transition graph enforced at handler level — CHECK on column would not
--     model the transition; same pattern as multipart_sessions §3.16).
--   - INV-GC-MARK-STARTED-AT-IMMUTABLE (CRITICAL; WI §12): mark_started_at_ms
--     is the canonical TLA `gc_correctness.tla` L152-154 anchor — set ONCE
--     when the worker enters the Mark phase; sweep enforces strict
--     `ac.created_at >= mark_started_at_ms` protect-if-equal-or-newer.
--
-- Conventions (mirror migrations/d1/0001..0003):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1).
--   - region stored as canonical 5-region literal (sam/iad/lhr/nrt/syd) with
--     CHECK constraint pinning the literal list — same canonical mirror as
--     corelink-ac-schema::AcRegion + corelink-multipart-schema::MultipartRegion.
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS` / `CREATE UNIQUE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate (INV-AUTH-MIGRATION-ADDITIVE applied across S-03..S-06 schemas).
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - sqlite3 :memory: < migrations/d1/0006_gc_run.sql executes without error
--     (caught at parser level antes deploy via crates/corelink-gc
--     migration_canonical test).
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--   - Lote 10.6bis P0-4 fix: status CHECK list includes `'failed'` so
--     PhaseBudgetExceeded / PhaseFailure (WI-S06-002 §6.1.10) can transition
--     status='failed' without violating the CHECK envelope.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- gc_run — per-(tenant, region) worker checkpoint row.
--
-- Run identity (PK) is the worker-minted run_id (UUIDv7-derived; monotonic).
-- The partial UNIQUE INDEX on (tenant_id, region) WHERE status='running' is
-- the canonical lock — ONE running gc_run per (tenant, region) at a time
-- (cron jitter, manual trigger overlap, OR sibling pod restart cannot race
-- past it).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS gc_run (
  -- Run identity (PK; UUIDv7-derived monotonic per WI §1).
  run_id              TEXT        NOT NULL PRIMARY KEY,

  -- Tenant scope (NOT NULL; cross-tenant gc impossible by design;
  -- defense-in-depth Layer 4 enforcement at storage level).
  tenant_id           TEXT        NOT NULL,

  -- Region scope (canonical 5-region literal list; CHECK below).
  region              TEXT        NOT NULL,

  -- Phase tracking (canonical 7-phase domain; transitions handled at
  -- handler level per INV-GC-PHASE-MONOTONIC enforcement seam).
  phase               TEXT        NOT NULL DEFAULT 'idle',
  -- Run status (canonical 6-state domain). Lote 10.6bis P0-4 fix: 'failed'
  -- variant present so WI-S06-002 §6.1.10 PhaseBudgetExceeded /
  -- PhaseFailure can transition without CHECK violation.
  status              TEXT        NOT NULL DEFAULT 'pending',

  -- Lifecycle timestamps (unix ms; monotonic per CHECK).
  started_at_ms       INTEGER     NOT NULL,
  last_checkpoint_at_ms INTEGER   NOT NULL,
  completed_at_ms     INTEGER     NULL,
  failed_at_ms        INTEGER     NULL,

  -- INV-GC-MARK-STARTED-AT-IMMUTABLE anchor — canonical TLA
  -- `gc_correctness.tla` L152-154 protect-if-equal-or-newer semantics.
  -- NULL until Mark phase begins; immutable thereafter (handler-level
  -- enforcement: subsequent UPDATEs MUST NOT touch this column).
  mark_started_at_ms  INTEGER     NULL,

  -- Counters (NOT NULL with default 0; CHECK >=0 below).
  blobs_marked_count  INTEGER     NOT NULL DEFAULT 0,
  blobs_swept_count   INTEGER     NOT NULL DEFAULT 0,
  blobs_physically_deleted_count INTEGER NOT NULL DEFAULT 0,
  bytes_reclaimed     INTEGER     NOT NULL DEFAULT 0,

  -- Source attribution: 'cron' | <admin pat_id_hex> for forensic trail.
  created_by_request_id TEXT      NOT NULL,
  -- Failure forensics (set ONLY when status transitions to 'failed' or
  -- 'crashed'; NULL on successful runs).
  failed_phase        TEXT        NULL,
  failed_reason       TEXT        NULL,

  -- chk_gc_run_phase: phase domain restricted to canonical 7 literals.
  CHECK (phase IN ('idle', 'mark', 'sweep', 'physical_delete', 'reconcile', 'completed', 'failed')),
  -- chk_gc_run_status: 6-state domain (Lote 10.6bis P0-4 includes 'failed').
  CHECK (status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted', 'failed')),
  -- chk_gc_run_region: canonical 5-region literal list (mirrors AC + multipart).
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  -- chk_gc_run_lifecycle_checkpoint: last_checkpoint_at_ms monotonic non-decreasing.
  CHECK (last_checkpoint_at_ms >= started_at_ms),
  -- chk_gc_run_lifecycle_completed: completed_at_ms >= started_at_ms when set.
  CHECK ((completed_at_ms IS NULL) OR (completed_at_ms >= started_at_ms)),
  -- chk_gc_run_lifecycle_failed: failed_at_ms >= started_at_ms when set.
  CHECK ((failed_at_ms IS NULL) OR (failed_at_ms >= started_at_ms)),
  -- chk_gc_run_mark_started: mark_started_at_ms >= started_at_ms when set.
  CHECK ((mark_started_at_ms IS NULL) OR (mark_started_at_ms >= started_at_ms)),
  -- chk_gc_run_counters_non_negative: monotonic counters never negative.
  CHECK (blobs_marked_count >= 0),
  CHECK (blobs_swept_count >= 0),
  CHECK (blobs_physically_deleted_count >= 0),
  CHECK (bytes_reclaimed >= 0)
);

-- Index: per-tenant + region + status — query "current running run" per
-- (tenant, region) without table scan (drives degrade-mode probe + admin
-- diagnostic surface).
CREATE INDEX IF NOT EXISTS idx_gc_run_tenant_region_status
  ON gc_run(tenant_id, region, status);

-- Partial index: stale running runs (last_checkpoint > 1h old) — sweeper
-- cron detects crashed workers (chaos test #5; SLO `stale_running_count`
-- gauge alert > 5 sustained).
CREATE INDEX IF NOT EXISTS idx_gc_run_stale_running
  ON gc_run(status, last_checkpoint_at_ms)
  WHERE status = 'running';

-- Partial UNIQUE INDEX (Lote 10.5bis lesson): only ONE running gc_run per
-- (tenant_id, region). Second concurrent INSERT (cron jitter race + manual
-- trigger overlap) rejected at storage level → handler returns 409
-- COR_GC_ALREADY_RUNNING.
CREATE UNIQUE INDEX IF NOT EXISTS uq_gc_run_running
  ON gc_run(tenant_id, region)
  WHERE status = 'running';

-- CoreLink D1 (Cloudflare SQLite) — migration 0007 for `gc_candidates`
-- (S-06 Garbage Collection: mark phase output table; WI-S06-002 §1).
--
-- Canonical sources:
--   - specs/04_sprints/S06/work_items/WI-S06-002-mark-phase-multi-pass-scan-mark-started-at.md §1 + §6
--   - specs/04_sprints/S06/_spec_contract.md §5.2 (R-S06-4..5)
--   - specs/03_architecture/invariant_registry.md INV-GC-MARK-STARTED-AT-IMMUTABLE +
--     INV-GC-001 + INV-GC-004 + INV-TENANT-ISOLATION
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--   - specs/tla/gc_correctness.tla (TLA+ model; mark_started_at_ms anchor;
--     `gc_correctness.tla` L152-154 protect-if-`>=` semantics).
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL): tenant_id NOT NULL; every query
--     filters by tenant_id; cross-tenant gc_candidates injection FM-300
--     impossible.
--   - INV-GC-MARK-STARTED-AT-IMMUTABLE (CRITICAL; WI §1 + §12): denormalized
--     `mark_started_at_ms` per candidate row aligned with `gc_run` row;
--     sweep enforces canonical TLA `>=` protect-if-equal-or-newer using this
--     anchor (gc_correctness.tla L152-154).
--   - INV-GC-MARK-RUN-FK (HIGH; WI §1.5): `mark_run_id` references
--     `gc_run.run_id`; cleaned by sweep phase post-completion.
--   - INV-GC-CANDIDATE-STATUS-DOMAIN (HIGH; WI §1): status restricted to
--     {candidate, swept, physically_deleted, protected_re_ref}.
--
-- Conventions (mirror migrations/d1/0001..0006):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1).
--   - digest stored as canonical lower-case hex BLAKE3-256 (64 chars).
--   - mark_run_id stored as canonical hyphenated lower-case UUID TEXT form
--     (matches `gc_run.run_id` representation).
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - sqlite3 :memory: < migrations/d1/0007_gc_candidates.sql executes without
--     error (caught at parser level antes deploy via crates/corelink-gc
--     migration_canonical_0007 test).
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- gc_candidates — per-(tenant, digest, mark_run) candidate row produced by
-- the mark phase (WI-S06-002) and consumed by the sweep phase (WI-S06-003).
--
-- Each row represents a blob digest the mark phase classified as
-- non-reachable for a specific gc_run (the mark_run_id FK pins the run that
-- produced the candidate). Sweep updates `status` to `swept` after the
-- soft-delete UPDATE; physical-delete updates to `physically_deleted` after
-- the R2 DeleteObject + row purge; INV-GC-004 enforcement (sweep phase)
-- updates to `protected_re_ref` if the canonical TLA `>=` protect-if-newer
-- check fires.
--
-- The composite PK `(tenant_id, digest, mark_run_id)` ensures idempotent
-- INSERT (re-running mark on the same run preserves rows) and tenant-leftmost
-- ordering for query planner efficiency.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS gc_candidates (
  -- Tenant scope (PK component 1; tenant-leftmost per data_model.md §2.1).
  tenant_id              TEXT     NOT NULL,

  -- Blob digest (PK component 2; canonical lower-case hex BLAKE3-256).
  digest                 TEXT     NOT NULL,

  -- INV-GC-004 anchor (denormalized from gc_run; sweep enforces canonical
  -- TLA `>=` protect-if-equal-or-newer per gc_correctness.tla L152-154).
  mark_started_at_ms     INTEGER  NOT NULL,

  -- gc_run FK (PK component 3; references gc_run.run_id).
  mark_run_id            TEXT     NOT NULL,

  -- Informational counters (used by bytes_reclaimed projection in sweep).
  blob_size_bytes        INTEGER  NOT NULL,
  blob_last_referenced_at_ms INTEGER NOT NULL,

  -- Status tracking — sweep updates → physical_delete updates → forensic.
  -- Domain restricted via CHECK below.
  status                 TEXT     NOT NULL DEFAULT 'candidate',

  -- Lifecycle timestamps (unix ms). created_at_ms is when mark inserted the
  -- row; the trailing 4 are set by downstream phases.
  created_at_ms          INTEGER  NOT NULL,
  swept_at_ms            INTEGER  NULL,
  physically_deleted_at_ms INTEGER NULL,
  protected_at_ms        INTEGER  NULL,

  -- Forensic trail for the protect-if-`>=` decision (e.g.
  -- "ac.created_at = T+1ms; mark_started_at = T").
  protected_reason       TEXT     NULL,

  -- Composite primary key — tenant-leftmost; `mark_run_id` last so a single
  -- digest may surface across multiple gc_run forensic trails.
  PRIMARY KEY (tenant_id, digest, mark_run_id),

  -- chk_gc_candidates_status: status domain (4 literals).
  CHECK (status IN ('candidate', 'swept', 'physically_deleted', 'protected_re_ref')),
  -- chk_gc_candidates_blob_size_non_negative.
  CHECK (blob_size_bytes >= 0),
  -- chk_gc_candidates_mark_anchor_monotonic: created_at_ms >=
  -- (mark_started_at_ms - 60s tolerance window). The tolerance accommodates
  -- mark phase batch drift between mark_started_at_ms capture and the
  -- per-row INSERT wall-clock instant; tighter than 60s would risk
  -- false-positive CHECK violations on slow batches.
  CHECK (created_at_ms >= mark_started_at_ms - 60000),
  -- chk_gc_candidates_swept_at_ms_monotonic: swept_at_ms >= created_at_ms
  -- when set.
  CHECK ((swept_at_ms IS NULL) OR (swept_at_ms >= created_at_ms)),
  -- chk_gc_candidates_physical_delete_after_swept: physical-delete is only
  -- legal after sweep set swept_at_ms; CHECK encodes the partial order.
  CHECK ((physically_deleted_at_ms IS NULL) OR (swept_at_ms IS NOT NULL)),
  CHECK ((physically_deleted_at_ms IS NULL) OR (physically_deleted_at_ms >= swept_at_ms))
);

-- Index: per-run + status (sweep phase consumes WHERE mark_run_id = X AND
-- status = 'candidate'). PARTIAL index keeps the index small even after
-- physical-delete migrates rows to terminal status.
CREATE INDEX IF NOT EXISTS idx_gc_candidates_run_status
  ON gc_candidates(mark_run_id, status);

-- Index: per-tenant + status (analytics; "how many candidates pending sweep
-- for tenant T").
CREATE INDEX IF NOT EXISTS idx_gc_candidates_tenant_status
  ON gc_candidates(tenant_id, status);

-- Index: protected re-ref forensics (sustained drift = INV-GC-004 violation
-- alert). PARTIAL index tightly scopes the index to the protected subset
-- because protect-if-`>=` is rare in steady state.
CREATE INDEX IF NOT EXISTS idx_gc_candidates_protected
  ON gc_candidates(tenant_id, protected_at_ms)
  WHERE status = 'protected_re_ref';

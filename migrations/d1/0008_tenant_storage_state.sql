-- CoreLink D1 (Cloudflare SQLite) — migration 0008 for `tenant_storage_state`
-- (S-07 Eviction + Quota: per-tenant running storage state; WI-S07-002 quota
-- trigger + WI-S07-003 quota middleware).
--
-- Canonical sources:
--   - specs/04_sprints/_sealed/S07/work_items/WI-S07-002-eviction-worker-lru-ttl-quota-trigger.md §1 + §6
--   - specs/04_sprints/_sealed/S07/work_items/WI-S07-003-quota-enforcement-middleware.md §6.1.5
--     (Lote 10.7bis P0-2 fix: NEW table separating STATE from POLICY).
--   - specs/04_sprints/_sealed/S07/_spec_contract.md §5 R-S07-2 + R-S07-3
--   - specs/03_architecture/invariant_registry.md INV-QUOTA-ENFORCEMENT +
--     INV-TENANT-ISOLATION + INV-EVICT-SOFT-DELETE-FIRST
--   - specs/03_architecture/adrs/ADR-0020-quota-enforcement-ownership.md (FROZEN)
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema separation rationale (Lote 10.7bis P0-2 R4 option B):
--   - `tenant_quota` table is POLICY (max_storage_bytes immutable per period;
--     tier mapping; admin override). Lives in S-04 / S-13 schema.
--   - `tenant_storage_state` table (this file) is STATE (mutable running
--     `bytes_used` counter + `last_evict_at_ms` watermark + DO/D1 sync
--     bookkeeping). Sweep + eviction + quota middleware all read this row.
--   - Eviction worker (WI-S07-002) reads `bytes_used` to decide whether the
--     95% trigger arm fires; on soft-delete path, eviction subtracts reclaimed
--     bytes from `bytes_used` (atomic batch with the `blob_meta.deleted_at`
--     UPDATE).
--   - Quota middleware (WI-S07-003) DO singleton owns the authoritative
--     `bytes_used` counter; writes back to D1 every 5min + on cold-start
--     restore.
--
-- Invariants enforced at storage layer:
--   - INV-TENANT-ISOLATION (CRITICAL): tenant_id is the PK; cross-tenant
--     state injection FM-300 impossible by design.
--   - INV-QUOTA-ENFORCEMENT (HIGH): bytes_used >= 0 + bytes_quota >= 0 via
--     CHECK; bytes_used <= bytes_quota is NOT enforced at storage level
--     (over-quota is a transient operational state during reservation race +
--     pending eviction; the DO actor + middleware enforce the boundary).
--   - INV-EVICT-SOFT-DELETE-FIRST (HIGH): `last_evict_at_ms` watermark
--     drives the eviction worker's idempotent re-run guard (skip if
--     `last_evict_at_ms > now - eviction_cooldown_ms`).
--   - INV-LRU-CONSISTENCY (HIGH; INV-S07): bytes_used drift detection via
--     `last_synced_at_ms`; reconcile (S-09 forward) catches DO/D1 drift.
--
-- Conventions (mirror migrations/d1/0001..0007):
--   - tenant_id stored as canonical UUIDv7 TEXT form (data_model.md §2.1).
--   - region stored as canonical 5-region literal (sam/iad/lhr/nrt/syd) with
--     CHECK constraint (mirrors gc_run.region domain).
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - sqlite3 :memory: < migrations/d1/0008_tenant_storage_state.sql executes
--     without error (caught at parser level antes deploy via crates/corelink-eviction
--     migration_canonical_0008 test).
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Backfill plan (post-deploy; NOT in this DDL):
--   - Reconcile job (WI-S06-005 inheritance) computes per-tenant
--     `SUM(blob_meta.size_bytes) WHERE deleted_at IS NULL` and seeds
--     `tenant_storage_state.bytes_used` row-by-row. The DDL itself ships an
--     EMPTY table; backfill runs as a one-shot post-deploy script after the
--     migration applies.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- tenant_storage_state — per-tenant running storage state row.
--
-- ONE row per (tenant_id, region) — composite PK with tenant-leftmost
-- ordering. region is denormalized so the eviction worker (per-region cron)
-- can scan candidates without joining tenant_quota.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tenant_storage_state (
  -- Tenant scope (PK component 1; tenant-leftmost per data_model.md §2.1).
  tenant_id              TEXT     NOT NULL,

  -- Region scope (PK component 2; canonical 5-region literal list).
  region                 TEXT     NOT NULL,

  -- Running bytes_used counter (mutable; DO authoritative; D1 is the
  -- 5min-sync source-of-truth). The eviction worker reads this to decide
  -- the 95% quota-trigger arm.
  bytes_used             INTEGER  NOT NULL DEFAULT 0,

  -- Quota ceiling snapshot (denormalized from tenant_quota.max_storage_bytes
  -- for the eviction worker to compute the 95% threshold without a join).
  -- DO refreshes this from tenant_quota on every check; eventual consistency
  -- with tenant_quota.updated_at watermark.
  bytes_quota            INTEGER  NOT NULL DEFAULT 0,

  -- Watermarks for the DO ↔ D1 sync envelope (Lote 10.7bis P0-2 +
  -- WI-S07-003 §6.1.5).
  bytes_used_updated_at_ms  INTEGER  NOT NULL,
  last_synced_at_ms      INTEGER  NOT NULL,

  -- Eviction worker watermark — set when WI-S07-002 fires the eviction
  -- pass for this (tenant, region). Drives idempotent re-run guard
  -- (skip if `last_evict_at_ms > now - eviction_cooldown_ms`).
  last_evict_at_ms       INTEGER  NULL,

  -- Eviction reclaim counter (sum of bytes reclaimed by the eviction
  -- worker across all runs; monotone; for DASH-EVICT widget consumption).
  bytes_reclaimed_lifetime INTEGER NOT NULL DEFAULT 0,

  -- Lifecycle timestamps.
  created_at_ms          INTEGER  NOT NULL,
  updated_at_ms          INTEGER  NOT NULL,

  -- Composite PK — tenant-leftmost; (tenant, region) per WI-S07-002 §6.1.6
  -- DO routing per tenant.primary_region (Lote 10.7bis P0-9).
  PRIMARY KEY (tenant_id, region),

  -- chk_tenant_storage_state_region: canonical 5-region literal list
  -- (mirrors gc_run.region domain).
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  -- chk_tenant_storage_state_bytes_used_non_negative.
  CHECK (bytes_used >= 0),
  -- chk_tenant_storage_state_bytes_quota_non_negative.
  CHECK (bytes_quota >= 0),
  -- chk_tenant_storage_state_bytes_reclaimed_non_negative: monotone
  -- counter never negative.
  CHECK (bytes_reclaimed_lifetime >= 0),
  -- chk_tenant_storage_state_lifecycle_updated_monotonic: updated_at_ms
  -- >= created_at_ms.
  CHECK (updated_at_ms >= created_at_ms),
  -- chk_tenant_storage_state_sync_monotonic: bytes_used_updated_at_ms
  -- >= created_at_ms; last_synced_at_ms >= created_at_ms.
  CHECK (bytes_used_updated_at_ms >= created_at_ms),
  CHECK (last_synced_at_ms >= created_at_ms),
  -- chk_tenant_storage_state_evict_after_create: last_evict_at_ms
  -- (when set) >= created_at_ms.
  CHECK ((last_evict_at_ms IS NULL) OR (last_evict_at_ms >= created_at_ms))
);

-- Index: per-region scan for the eviction cron (per-region worker scans
-- tenants WHERE bytes_used / bytes_quota >= 0.95 OR last_evict_at_ms < now -
-- 24h). Tenant-leftmost composite PK already supports per-tenant lookup;
-- this secondary index supports the per-region scan.
CREATE INDEX IF NOT EXISTS idx_tenant_storage_state_region_evict
  ON tenant_storage_state(region, last_evict_at_ms);

-- Index: utilization analytics (DASH-DEDUP + alerts; "tenants > 80%
-- utilization" widget). bytes_used + bytes_quota together drive the
-- ratio; index on bytes_used alone gives the planner a useful seek for
-- large-bytes_used scans.
CREATE INDEX IF NOT EXISTS idx_tenant_storage_state_bytes_used
  ON tenant_storage_state(bytes_used);

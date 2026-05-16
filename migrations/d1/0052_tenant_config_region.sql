-- Wave-21 closure (`specs/_audits/2026-05-16-neon-shadow-real-driver.md` §7):
-- per-tenant pinned-region for the Neon analytics shadow.
--
-- Wave-20 wired the production `TokioPgShadowSinkFactory` in
-- `apps/server/src/main.rs` to a hard-coded `Region::Iad` default
-- (TODO marked in the factory's `for_tenant` impl). Wave-21 replaces
-- that default with a `TenantRegionResolver` trait surface
-- (`crates/corelink-audit-chain/src/neon_shadow/tenant_region.rs`).
-- The production impl `D1TenantRegionResolver` queries the
-- `tenant_config.region` column added by this migration.
--
-- ## Region label semantics
--
-- The label is the canonical 3-letter Neon-project colocode from
-- `corelink_analytics::Region` (`iad / sjc / dfw / sea / ord /
-- lhr / fra / ams / cdg / mad / gru / eze / bog / nrt / sin / syd /
-- hkg / bom / icn / jnb / cpt / dxb`).
--
-- This is **distinct** from the macro-region label on the existing
-- `tenant.primary_region` column (added by migration `0028_tenant_primary_region.sql`,
-- values `wnam / enam / weur / sam / apac / afr`). The macro-region
-- governs the residency-enforcement triggers (`0023_residency_check_constraints.sql`);
-- the colocode pinned by this migration governs which Neon analytics
-- project the audit-shadow rows land in.
--
-- ## Default = 'IAD' (per task charter)
--
-- A `DEFAULT 'IAD'` lets the migration run additive against a
-- non-empty `tenant_config` table — every legacy row picks up IAD
-- (the wave-20 hard-coded default the resolver replaces). The
-- `D1TenantRegionResolver::fallback_region` provides a second-line
-- defense: even if the `tenant_config` row is missing entirely
-- (`Ok(None)` from `TenantConfigStore::region_label`), the resolver
-- falls back to the configured default.
--
-- ## Idempotency
--
-- - `CREATE TABLE IF NOT EXISTS` — safe if a fresh-init schema
--   already declared `tenant_config`.
-- - `ALTER TABLE ... ADD COLUMN IF NOT EXISTS region` — safe if the
--   migration is re-applied. D1 supports `IF NOT EXISTS` on
--   `ADD COLUMN` (SQLite ≥ 3.35).
-- - The CHECK list enumerates every `Region` variant. New variants
--   are non-trivial — they require a wave-NN cardinality re-estimate
--   (WI-S09-001 §6.1.13 chaos scenario 4) AND a follow-up migration
--   to extend the CHECK list.
--
-- INV cross-ref: `INV-AUDIT-SHADOW-REGION-PIN` (informal — pinned at
-- the boot-time `TokioPgShadowSinkFactory` log line).

-- ── 1. tenant_config table — minimal scaffold (additive-only) ──────────────
-- The full `tenant_config` schema is the subject of a future WI. This
-- migration creates the table with the minimum surface needed by
-- `D1TenantRegionResolver` so the wave-20 TODO closes cleanly without
-- coupling to that future WI.

CREATE TABLE IF NOT EXISTS tenant_config (
    tenant_id     TEXT PRIMARY KEY,
    region        TEXT NOT NULL DEFAULT 'IAD'
        CHECK (region IN (
            'IAD','SJC','DFW','SEA','ORD',
            'LHR','FRA','AMS','CDG','MAD',
            'GRU','EZE','BOG',
            'NRT','SIN','SYD','HKG','BOM','ICN',
            'JNB','CPT','DXB',
            -- Lower-case acceptance — `tenant_config.region` is the
            -- column the resolver reads via
            -- `corelink_audit_chain::parse_region_label` which is
            -- ASCII-case-insensitive. We accept both casings so a
            -- future canonicalization sweep doesn't break running
            -- queries.
            'iad','sjc','dfw','sea','ord',
            'lhr','fra','ams','cdg','mad',
            'gru','eze','bog',
            'nrt','sin','syd','hkg','bom','icn',
            'jnb','cpt','dxb'
        )),
    created_at_ms INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL DEFAULT 0
);

-- ── 2. Additive `region` column for pre-existing deployments ───────────────
-- If a fresh-init created `tenant_config` without the `region` column
-- (some legacy schema variant), this is the additive add per the wave-21
-- charter. The CHECK constraint is enforced at INSERT/UPDATE time only;
-- existing rows pick up the column-level DEFAULT 'IAD'.

ALTER TABLE tenant_config ADD COLUMN IF NOT EXISTS region TEXT NOT NULL DEFAULT 'IAD';

-- ── 3. Index for the resolver's read path ──────────────────────────────────
-- `tenant_id` is the PRIMARY KEY (already a covering index). No
-- additional index needed; this stub is informational.
--
-- The resolver's canonical query is:
--
--   SELECT region FROM tenant_config WHERE tenant_id = ?
--
-- (bound at `crates/corelink-audit-chain/src/neon_shadow/tenant_region.rs`
-- `TenantConfigStore::region_label` doc).

-- ── Roll-back (commented; opt-in if the resolver pin is reverted) ──────────
-- ALTER TABLE tenant_config DROP COLUMN region;
-- DROP TABLE IF EXISTS tenant_config;

-- WI-S14-002: tenants.primary_region NOT NULL CHECK + immutable trigger + legacy backfill.
--
-- Adds `primary_region` column to `tenant` table (S-14 canonical name `tenants` in
-- postgres/Neon; `tenant` in D1/SQLite per 0023 schema).  The `tenants` postgres table
-- already has this column per data_model.md §4.1 L151; this migration targets D1.
--
-- Canonical regions per privacy_model.md §7.1 S-14 Phase 1 production: wnam/enam/weur/sam.
-- (apac/afr Phase 2/3 are valid in the 6-region CHECK but not yet provisioned.)
--
-- Migration plan (additive-only, per DD-001 D1 migration discipline):
--   Step 1: ADD COLUMN primary_region TEXT (nullable, CHECK accepts NULL OR valid region).
--   Step 2: Backfill legacy rows — default 'enam' for rows with NULL primary_region.
--   Step 3: Re-enforcement via trigger: primary_region IMMUTABLE post-INSERT (reject UPDATE).
--
-- Roll-back: DROP TRIGGER + ALTER TABLE DROP COLUMN (at bottom, commented).
--
-- INV-REGION-NO-CROSS-LEAK CRITICAL (WI-S14-002 §12): runtime cobertura via
-- insert checks + 30k property test; this migration provides D1-level backstop.

-- ── Step 1: primary_region column ────────────────────────────────────────────
-- D1 compatibility: tenant.primary_region was created NOT NULL in migration
-- 0023_residency_check_constraints.sql. ALTER TABLE ADD COLUMN is skipped here
-- to avoid 'duplicate column name' error. Steps 2-5 (backfill + triggers + index)
-- still apply.

-- NOTE: backfill (Step 2 below) runs regardless; any pre-existing rows without
-- primary_region would get 'enam' default. The NOT NULL constraint in 0023
-- ensures all future inserts require a value.

-- ── Step 2: Backfill existing tenants (legacy single-region default = 'enam') ───────────────
-- Existing tenants pre-S14 were single-region ENAM (us-east); assign conservatively.
-- Post-backfill the NOT NULL constraint is enforced via trigger (D1 does not support
-- ALTER COLUMN SET NOT NULL in all versions; trigger provides equivalent enforcement).

UPDATE tenant SET primary_region = 'enam' WHERE primary_region IS NULL;

-- ── Step 3: BEFORE INSERT trigger — reject if primary_region is NULL or invalid ────────────
-- Ensures new tenant rows always carry a valid primary_region post-S14.

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_required
BEFORE INSERT ON tenant
FOR EACH ROW
WHEN NEW.primary_region IS NULL
BEGIN
    SELECT RAISE(ABORT, 'primary_region is required for new tenants (WI-S14-002: INV-REGION-NO-CROSS-LEAK)');
END;

-- ── Step 4: BEFORE UPDATE trigger — primary_region IMMUTABLE post-INSERT ─────────────────
-- Prevents mutation of primary_region after creation; mutation = re-routing existing
-- tenant data cross-region = Schrems II + LGPD Art. 33 §1º catastrophic violation.
-- Legitimate region migration requires manual ticket + admin role + dual-approval (S-13).

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_immutable
BEFORE UPDATE OF primary_region ON tenant
FOR EACH ROW
WHEN OLD.primary_region IS NOT NULL AND OLD.primary_region != NEW.primary_region
BEGIN
    SELECT RAISE(ABORT, 'primary_region is immutable post-INSERT (WI-S14-002: manual ticket + admin role + dual-approval required)');
END;

-- ── Step 5: BEFORE INSERT trigger — reject invalid region strings ─────────────────────────

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_valid_insert
BEFORE INSERT ON tenant
FOR EACH ROW
WHEN NEW.primary_region IS NOT NULL
  AND NEW.primary_region NOT IN ('wnam','enam','weur','sam','apac','afr')
BEGIN
    SELECT RAISE(ABORT, 'primary_region must be one of: wnam, enam, weur, sam, apac, afr');
END;

-- ── Index: tenant lookup by region (DO region_enforcer D1 query path) ────────────────────

CREATE INDEX IF NOT EXISTS idx_tenant_primary_region
    ON tenant(primary_region);

-- ── Roll-back statements (commented out; apply manually to revert) ────────────────────────
-- DROP TRIGGER IF EXISTS trg_tenant_primary_region_required;
-- DROP TRIGGER IF EXISTS trg_tenant_primary_region_immutable;
-- DROP TRIGGER IF EXISTS trg_tenant_primary_region_valid_insert;
-- DROP INDEX IF EXISTS idx_tenant_primary_region;
-- ALTER TABLE tenant DROP COLUMN IF EXISTS primary_region;

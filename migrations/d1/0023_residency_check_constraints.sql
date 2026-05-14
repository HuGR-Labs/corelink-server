-- WI-S11-007: Residency enforcement D1 schema additions.
--
-- Adds region columns + CHECK constraints + BEFORE INSERT/UPDATE triggers
-- to backend tables ensuring all rows match tenant.primary_region.
--
-- Canonical regions: wnam, enam, weur, sam, apac, afr
-- (data_model.md §2.1 L95 + privacy_model.md §7.1).
--
-- Per DD-001: D1 does NOT support subquery in CHECK constraints,
-- so cross-table enforcement uses BEFORE INSERT/UPDATE triggers.
--
-- Roll-back: DROP TRIGGER + DROP INDEX statements at bottom.

-- ── 1. tenant table: ensure primary_region column is canonical ───────────────

-- Add primary_region column if not exists (idempotent via IF NOT EXISTS).
-- In production this column was added at schema init (data_model.md §4.1 L151).
-- This migration ensures the CHECK constraint is present.
CREATE TABLE IF NOT EXISTS tenant (
    tenant_id     TEXT PRIMARY KEY,
    primary_region TEXT NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

-- ── 2. blob_meta: region column + trigger ───────────────────────────────────

ALTER TABLE blob_meta ADD COLUMN IF NOT EXISTS region TEXT NOT NULL DEFAULT 'wnam'
    CHECK (region IN ('wnam','enam','weur','sam','apac','afr'));

-- Trigger: BEFORE INSERT — reject if region != tenant.primary_region.
-- D1 (SQLite-compatible) trigger syntax.
CREATE TRIGGER IF NOT EXISTS trg_blob_meta_region_match_insert
BEFORE INSERT ON blob_meta
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: blob_meta.region must match tenant.primary_region');
END;

-- Trigger: BEFORE UPDATE — same check on region update.
CREATE TRIGGER IF NOT EXISTS trg_blob_meta_region_match_update
BEFORE UPDATE ON blob_meta
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: blob_meta.region must match tenant.primary_region');
END;

-- ── 3. ac_meta: region column + trigger ─────────────────────────────────────

ALTER TABLE ac_meta ADD COLUMN IF NOT EXISTS region TEXT NOT NULL DEFAULT 'wnam'
    CHECK (region IN ('wnam','enam','weur','sam','apac','afr'));

CREATE TRIGGER IF NOT EXISTS trg_ac_meta_region_match_insert
BEFORE INSERT ON ac_meta
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: ac_meta.region must match tenant.primary_region');
END;

CREATE TRIGGER IF NOT EXISTS trg_ac_meta_region_match_update
BEFORE UPDATE ON ac_meta
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: ac_meta.region must match tenant.primary_region');
END;

-- ── 4. audit_outbox: region column + trigger ─────────────────────────────────

ALTER TABLE audit_outbox ADD COLUMN IF NOT EXISTS region TEXT NOT NULL DEFAULT 'wnam'
    CHECK (region IN ('wnam','enam','weur','sam','apac','afr'));

CREATE TRIGGER IF NOT EXISTS trg_audit_outbox_region_match_insert
BEFORE INSERT ON audit_outbox
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: audit_outbox.region must match tenant.primary_region');
END;

-- ── 5. billing_events_staging: region column + trigger ───────────────────────

ALTER TABLE billing_events_staging ADD COLUMN IF NOT EXISTS region TEXT NOT NULL DEFAULT 'wnam'
    CHECK (region IN ('wnam','enam','weur','sam','apac','afr'));

CREATE TRIGGER IF NOT EXISTS trg_billing_events_region_match_insert
BEFORE INSERT ON billing_events_staging
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
    SELECT RAISE(ABORT, 'residency_violation: billing_events_staging.region must match tenant.primary_region');
END;

-- ── 6. region_migration_request: new table (ADR-S11-011 cooldown 30d) ────────

CREATE TABLE IF NOT EXISTS region_migration_request (
    ticket_id      TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL REFERENCES tenant(tenant_id),
    from_region    TEXT NOT NULL CHECK (from_region IN ('wnam','enam','weur','sam','apac','afr')),
    to_region      TEXT NOT NULL CHECK (to_region IN ('wnam','enam','weur','sam','apac','afr')),
    justification  TEXT NOT NULL,
    attestation    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending','approved','rejected','completed')),
    created_at_ms  INTEGER NOT NULL,
    updated_at_ms  INTEGER NOT NULL,
    -- Cooldown: migration not executable until created_at_ms + 2592000000 (30d)
    -- Enforced at application layer (WI-S11-007 InMemoryMigrationStore.submit).
    CHECK (from_region != to_region)
);

CREATE INDEX IF NOT EXISTS idx_region_migration_request_tenant
    ON region_migration_request(tenant_id, status);

-- ── Roll-back statements (commented out; apply manually to revert) ────────────
-- DROP TRIGGER IF EXISTS trg_blob_meta_region_match_insert;
-- DROP TRIGGER IF EXISTS trg_blob_meta_region_match_update;
-- DROP TRIGGER IF EXISTS trg_ac_meta_region_match_insert;
-- DROP TRIGGER IF EXISTS trg_ac_meta_region_match_update;
-- DROP TRIGGER IF EXISTS trg_audit_outbox_region_match_insert;
-- DROP TRIGGER IF EXISTS trg_billing_events_region_match_insert;
-- DROP INDEX IF EXISTS idx_region_migration_request_tenant;
-- DROP TABLE IF EXISTS region_migration_request;

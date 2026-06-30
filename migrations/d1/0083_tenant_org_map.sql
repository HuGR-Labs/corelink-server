-- 0083_tenant_org_map.sql
--
-- Tenant-per-org IDENTITY MAP (githugr ADR-0007 Epic A primitive): the
-- `clerk_org_id → tenant_id` resolution table that lets an external identity
-- authority (githugr's Option-B token exchange) resolve a Clerk **organization**
-- to its ISOLATED CoreLink tenant. CoreLink's multi-tenant isolation is already
-- per-`tenant_id` (the core product); the only missing piece was identity →
-- tenant resolution. This migration lands that read model — and NOTHING ELSE.
--
-- ## GATED-INERT (zero rows until provisioning)
--
-- This table is created EMPTY and stays empty until the provisioning /
-- onboarding authority writes a row. With no mapped rows, every resolution
-- lookup misses, so the showcase tenant `ee30f7ba` path for an unmapped org is
-- entirely UNCHANGED (the resolve endpoint returns `org_not_mapped` → the caller
-- keeps its current fixed-tenant behaviour). Purely additive, zero prod risk:
-- one NEW table that no hot path touches.
--
-- ## Writer authority (single)
--
-- Rows are WRITTEN ONLY by the provisioning / onboarding authority (the
-- owner/operator step that stands up a real per-org tenant). The internal
-- resolution endpoint (`POST /internal/v1/auth/resolve-tenant`,
-- `routes/auth_introspect.rs`) is LOOKUP-ONLY — it never auto-creates a mapping
-- (a missing org resolves to a 404, never a silent provision).
--
-- ## Shape
--
--   * `clerk_org_id`  — the Clerk organization id (`org_...`); the lookup key,
--                       so it is the PRIMARY KEY (one tenant per org).
--   * `tenant_id`     — the isolated CoreLink tenant UUID this org maps to.
--   * `created_at_ms` — Unix epoch milliseconds the mapping was provisioned.
--
-- An index on `tenant_id` supports the reverse lookup (and the DSR erasure
-- sweep, which deletes `WHERE tenant_id = ?` — this table is classified
-- tenant-keyed in `routes/dsr/adapter_d1.rs`, so an org→tenant mapping is
-- removed when its tenant is erased).
--
-- ## Additive policy
--
-- `CREATE TABLE IF NOT EXISTS` only — no DROP, no retype, no rewrite of
-- existing rows. INV-AUTH-MIGRATION-ADDITIVE (auth-migrations-additive-only).
-- The d1_migrations ledger guarantees exactly-once application
-- (d1-migrations-ledger-desync).

-- Clerk org → isolated CoreLink tenant resolution map (control-plane read model).
CREATE TABLE IF NOT EXISTS tenant_org_map (
    -- Clerk organization id (`org_...`) — the resolution key (one tenant per org).
    clerk_org_id  TEXT    PRIMARY KEY,
    -- The isolated CoreLink tenant UUID this org maps to.
    tenant_id     TEXT    NOT NULL,
    -- Unix epoch milliseconds the mapping was provisioned.
    created_at_ms INTEGER NOT NULL
);

-- Reverse lookup by tenant (and the DSR `WHERE tenant_id = ?` erasure sweep).
CREATE INDEX IF NOT EXISTS idx_tenant_org_map_tenant_id
    ON tenant_org_map (tenant_id);

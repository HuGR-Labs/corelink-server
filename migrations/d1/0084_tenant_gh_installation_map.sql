-- 0084_tenant_gh_installation_map.sql
--
-- GitHub-installation → isolated-tenant IDENTITY MAP (cf-multitenant PACKET,
-- DP2): the `installation_id → tenant_id` resolution table that lets the
-- multi-tenant runner-CI path resolve a GitHub **App installation** to its
-- ISOLATED CoreLink tenant. CoreLink's multi-tenant isolation is already
-- per-`tenant_id` (the core product); the only missing piece for the runner
-- fabric was GitHub-installation → tenant resolution. This migration lands that
-- read model — and NOTHING ELSE.
--
-- ## GATED-INERT (zero rows until provisioning)
--
-- This table is created EMPTY and stays empty until the provisioning /
-- onboarding authority writes a row. With no mapped rows, every resolution
-- lookup misses, so the current fixed-tenant runner path for an unmapped
-- installation is entirely UNCHANGED (the resolver returns an unmapped miss →
-- the caller keeps its current behaviour). Purely additive, zero prod risk:
-- one NEW table that no hot path touches.
--
-- ## Writer authority (single)
--
-- Rows are WRITTEN ONLY by the provisioning / onboarding authority (the
-- owner/operator step that stands up a real per-installation tenant). The
-- resolver is LOOKUP-ONLY — it never auto-creates a mapping (a missing
-- installation resolves to a miss, never a silent provision).
--
-- ## Shape
--
--   * `installation_id` — the GitHub App installation id; the lookup key, so it
--                         is the PRIMARY KEY (one tenant per installation).
--   * `tenant_id`       — the isolated CoreLink tenant UUID this installation
--                         maps to.
--   * `created_at_ms`   — Unix epoch milliseconds the mapping was provisioned.
--
-- An index on `tenant_id` supports the reverse lookup (and the DSR erasure
-- sweep, which deletes `WHERE tenant_id = ?` — this table is classified
-- tenant-keyed in `routes/dsr/adapter_d1.rs`, so an installation→tenant mapping
-- is removed when its tenant is erased).
--
-- ## Additive policy
--
-- `CREATE TABLE IF NOT EXISTS` only — no DROP, no retype, no rewrite of
-- existing rows. INV-AUTH-MIGRATION-ADDITIVE (auth-migrations-additive-only).
-- The d1_migrations ledger guarantees exactly-once application
-- (d1-migrations-ledger-desync).

-- GitHub App installation → isolated CoreLink tenant resolution map
-- (control-plane read model, runner-CI path).
CREATE TABLE IF NOT EXISTS tenant_gh_installation_map (
    -- GitHub App installation id — the resolution key (one tenant per install).
    installation_id TEXT    PRIMARY KEY,
    -- The isolated CoreLink tenant UUID this installation maps to.
    tenant_id       TEXT    NOT NULL,
    -- Unix epoch milliseconds the mapping was provisioned.
    created_at_ms   INTEGER NOT NULL
);

-- Reverse lookup by tenant (and the DSR `WHERE tenant_id = ?` erasure sweep).
CREATE INDEX IF NOT EXISTS idx_tenant_gh_installation_map_tenant_id
    ON tenant_gh_installation_map (tenant_id);

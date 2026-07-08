-- 0085_runner_repo_allowlist.sql
--
-- Per-tenant runner REPO ALLOWLIST (cf-multitenant PACKET, DP5): the single
-- source of truth for which `repo_full_name`s a tenant is permitted to run
-- CI on. Read by BOTH the Rust fabric AND the CF mint — ONE source, no
-- parallel list to drift. This migration lands that read model — and NOTHING
-- ELSE.
--
-- ## GATED-INERT (zero rows until provisioning)
--
-- This table is created EMPTY and stays empty until the provisioning /
-- onboarding authority writes a row. Enforcement is fail-closed at the reader:
-- with no allowlisted rows a tenant's runner-mint for any repo simply misses
-- the allowlist. Purely additive, zero prod risk: one NEW table that no hot
-- path touches until the reader consults it.
--
-- ## Writer authority (single)
--
-- Rows are WRITTEN ONLY by the provisioning / onboarding authority (the
-- owner/operator step that allowlists a repo for a tenant). Both readers (the
-- Rust fabric and the CF mint) are LOOKUP-ONLY — they never auto-add a repo
-- (a missing repo is a denied mint, never a silent allow).
--
-- ## Shape
--
--   * `tenant_id`      — the isolated CoreLink tenant UUID that owns the entry.
--   * `repo_full_name` — the GitHub `owner/repo` full name allowlisted for it.
--   * `created_at_ms`  — Unix epoch milliseconds the entry was provisioned.
--
-- The PRIMARY KEY is the tenant-leftmost composite `(tenant_id, repo_full_name)`
-- so `WHERE tenant_id = ?` is an exact prefix scan (the DSR erasure sweep keys
-- on it — this table is classified tenant-keyed in `routes/dsr/adapter_d1.rs`,
-- so a tenant's allowlist is removed when its tenant is erased). A secondary
-- index on `repo_full_name` supports the reverse lookup (which tenants
-- allowlist a given repo).
--
-- ## Additive policy
--
-- `CREATE TABLE IF NOT EXISTS` only — no DROP, no retype, no rewrite of
-- existing rows. INV-AUTH-MIGRATION-ADDITIVE (auth-migrations-additive-only).
-- The d1_migrations ledger guarantees exactly-once application
-- (d1-migrations-ledger-desync).

-- Per-tenant runner repo allowlist (single source read by Rust fabric + CF mint).
CREATE TABLE IF NOT EXISTS runner_repo_allowlist (
    -- The isolated CoreLink tenant UUID that owns the entry.
    tenant_id      TEXT    NOT NULL,
    -- The GitHub `owner/repo` full name allowlisted for the tenant.
    repo_full_name TEXT    NOT NULL,
    -- Unix epoch milliseconds the entry was provisioned.
    created_at_ms  INTEGER NOT NULL,
    -- Tenant-leftmost composite PK → `WHERE tenant_id = ?` is an exact prefix.
    PRIMARY KEY (tenant_id, repo_full_name)
);

-- Reverse lookup by repo (which tenants allowlist a given repo).
CREATE INDEX IF NOT EXISTS idx_runner_repo_allowlist_repo
    ON runner_repo_allowlist (repo_full_name);

-- 0088_workspaces.sql
--
-- Per-tenant WORKSPACES read/write model (customer dashboard BE-11): named
-- snapshots of a tenant's cached state. A workspace is a labelled reference to
-- a point-in-time snapshot of the tenant's cache; a "pinned" workspace is kept
-- warm (never GC-evicted) so a restore is instant. This migration lands that
-- one read/write model — and NOTHING ELSE.
--
-- Backs the Workspaces screen (`apps/admin-ui`, `CustomerWorkspace`), which was
-- all prose while `listWorkspaces` threw `NotWiredError` (BE-11). The
-- tenant-scoped CRUD lives in `crates/corelink-container/src/routes/workspaces.rs`.
--
-- ## GATED-INERT (zero rows until a customer creates one)
--
-- Created EMPTY; rows appear only when a tenant creates a workspace via
-- `POST /v1/customer/workspaces`. Purely additive, zero prod risk: one NEW
-- table that no hot path touches until the customer surface reads/writes it.
--
-- ## Writer authority (single)
--
-- Rows are written ONLY by the tenant that owns them, through the self-serve
-- customer surface — the tenant is DERIVED from the authenticated session
-- (never a client-supplied parameter), and every statement rides
-- `WHERE tenant_id = ?` (INV-TENANT-ISOLATION). No cross-tenant read or write
-- path exists.
--
-- ## Shape
--
--   * `tenant_id`     — the isolated CoreLink tenant UUID that owns the workspace.
--   * `workspace_id`  — server-minted UUID for the workspace (opaque handle).
--   * `name`          — human-readable label the customer chose.
--   * `size_bytes`    — bytes the snapshot references (0 until sizing lands).
--   * `snapshot_ref`  — opaque reference to the captured cache snapshot ("" = none yet).
--   * `pinned`        — 1 = kept warm (never GC-evicted), 0 = normal. Default 0.
--   * `created_at_ms` — Unix epoch milliseconds the workspace was created.
--
-- The PRIMARY KEY is the tenant-leftmost composite `(tenant_id, workspace_id)`
-- so `WHERE tenant_id = ?` is an exact prefix scan (and the DSR erasure sweep,
-- which keys on the tenant-leftmost prefix, removes a tenant's workspaces when
-- the tenant is erased). A secondary index on `tenant_id` supports the plain
-- list read.
--
-- ## Additive policy
--
-- `CREATE TABLE IF NOT EXISTS` only — no DROP, no retype, no rewrite of
-- existing rows. INV-AUTH-MIGRATION-ADDITIVE (auth-migrations-additive-only).
-- The d1_migrations ledger guarantees exactly-once application
-- (d1-migrations-ledger-desync).

-- Per-tenant named cache snapshots (customer Workspaces surface).
CREATE TABLE IF NOT EXISTS workspaces (
    -- The isolated CoreLink tenant UUID that owns the workspace.
    tenant_id     TEXT    NOT NULL,
    -- Server-minted UUID for the workspace (opaque handle on the wire).
    workspace_id  TEXT    NOT NULL,
    -- Human-readable label the customer chose.
    name          TEXT    NOT NULL,
    -- Bytes the snapshot references (0 until snapshot sizing lands).
    size_bytes    INTEGER NOT NULL DEFAULT 0,
    -- Opaque reference to the captured cache snapshot ("" = none yet).
    snapshot_ref  TEXT    NOT NULL DEFAULT '',
    -- 1 = kept warm (never GC-evicted), 0 = normal.
    pinned        INTEGER NOT NULL DEFAULT 0,
    -- Unix epoch milliseconds the workspace was created.
    created_at_ms INTEGER NOT NULL,
    -- Tenant-leftmost composite PK → `WHERE tenant_id = ?` is an exact prefix.
    PRIMARY KEY (tenant_id, workspace_id)
);

-- Tenant list read (newest-first is applied at query time via created_at_ms).
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant
    ON workspaces (tenant_id);

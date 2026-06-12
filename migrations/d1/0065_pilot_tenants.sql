-- hugit-P2 WP-FOUND — durable backing for the pilot-admin control plane.
--
-- Backs the D1-durable `PilotStore` in
-- `crates/corelink-container/src/routes/admin_pilot.rs` (list /
-- create / grant-tier / checkin). The wave-29 admin_pilot handlers
-- previously ran over a NON-durable `InMemoryPilotStore`; this table
-- gives the pilot-admin surface a row that survives container
-- restarts.
--
-- ## Why a dedicated table (not `pilot_signups`, 0053)
--
-- `pilot_signups` (0053) is the SIGNUP-side reservation ledger — its
-- columns (`email`, `company_name`, `tier_hint`, `token_id`, the
-- RESERVED/PROVISIONED/ACTIVE/EXPIRED/CANCELLED lifecycle) model an
-- inbound pilot SIGNUP, not the operator-side pilot TENANT record the
-- admin_pilot handlers mutate. The admin surface needs `slug`, `tier`,
-- `cap_bytes`, the NEW/RESERVED/PROVISIONED/ACTIVE/GRADUATED/TERMINATED
-- lifecycle, `tier_granted_at_ms`, and `first_blob_at_ms` — none of
-- which exist on `pilot_signups`. Reusing it would either drop
-- grant-tier state (`tier`, `cap_bytes`, `tier_granted_at_ms`) or
-- overload its columns. This table mirrors the canonical
-- `admin_pilot::PilotTenant` shape exactly, 1:1.
--
-- ## Idempotency / additive-only
--
-- `CREATE TABLE IF NOT EXISTS` lets the migration replay safely. The
-- table is additive-only (INV-AUTH-MIGRATION-ADDITIVE): it adds a NEW
-- table and never alters/drops an existing one, so it needs no ADR
-- waiver.

CREATE TABLE IF NOT EXISTS pilot_tenants (
    -- Canonical tenant UUID (v7 string; lexicographic-sortable, matches
    -- the wider D1 schema convention). Primary key.
    tenant_id TEXT PRIMARY KEY,
    -- Display slug (lowercase, hyphen-delimited).
    slug TEXT NOT NULL,
    -- Tier — pre-grant tenants carry `'free'`, post-grant `'pilot'`.
    tier TEXT NOT NULL,
    -- Storage cap in bytes (decimal-GB convention). 0 pre-grant.
    cap_bytes INTEGER NOT NULL DEFAULT 0,
    -- Pilot lifecycle state — the canonical admin_pilot 6-set.
    pilot_state TEXT NOT NULL CHECK (pilot_state IN (
        'NEW', 'RESERVED', 'PROVISIONED', 'ACTIVE', 'GRADUATED', 'TERMINATED'
    )),
    -- Signup completion wall-clock (Unix epoch ms).
    signup_at_ms INTEGER NOT NULL,
    -- Tier-grant wall-clock (Unix epoch ms). NULL pre-grant.
    tier_granted_at_ms INTEGER,
    -- First-blob wall-clock (Unix epoch ms). NULL until the tenant has
    -- committed a single blob (the 24h check-in alert key).
    first_blob_at_ms INTEGER
);

-- Operator list slice — `GET /v1/admin/pilots?state=…` filters by
-- `pilot_state` and orders by `signup_at_ms` ASC.
CREATE INDEX IF NOT EXISTS idx_pilot_tenants_state_signup
    ON pilot_tenants (pilot_state, signup_at_ms);

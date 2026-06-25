-- Migration 0074: team_member — durable multi-seat membership (ADR-S33-001).
--
-- CoreLink sells a paid `team` tier but the team feature was an honest v1 501
-- stub: `customer_d1.rs` synthesized a single Owner row for `team list` and
-- returned `NotImplemented` for `team invite`, because no membership table
-- existed. This table is that durable backing — the additive custom-membership
-- model (NOT a Clerk-Organizations migration) chosen in ADR-S33-001.
--
-- ## Model
--
-- A tenant's `clerk_user_id` remains the canonical OWNER. Additional seats are
-- rows here, scoped to `tenant_id`. An `active` member mints tenant-scoped PATs
-- (principal_id = user_id); SEAT REMOVAL flips the row to `removed` AND revokes
-- the member's PATs (the load-bearing security boundary — a removed member must
-- lose data-plane access, not just disappear from a list).
--
-- ## Status lifecycle
--
--   invited  — invitation issued, not yet accepted (Clerk email flow, WP-4)
--   active   — accepted / operator-provisioned; has working tenant-scoped access
--   removed  — seat revoked; PATs revoked; retained as an audit tombstone
--
-- ## Privacy
--
-- Only `email_hash` is stored (pseudonymized) — never the raw invitee email
-- (CTRL-PRIV-001), mirroring the rest of the D1 schema.

CREATE TABLE IF NOT EXISTS team_member (
    -- Tenant the seat belongs to (matches `tenant.tenant_id`).
    tenant_id     TEXT    NOT NULL,

    -- Clerk user id for an `active` member; for an `invited`-by-email row (WP-4)
    -- this carries the Clerk invitation id until acceptance binds the real user.
    user_id       TEXT    NOT NULL,

    -- Pseudonymized invitee email (HMAC/SHA-256 hash). NEVER the raw email.
    email_hash    TEXT,

    -- Seat role. `owner` is the tenant's clerk_user_id; the rest are added seats.
    role          TEXT    NOT NULL DEFAULT 'member'
                          CHECK (role IN ('owner', 'admin', 'member', 'viewer')),

    -- Lifecycle (see header).
    status        TEXT    NOT NULL DEFAULT 'invited'
                          CHECK (status IN ('invited', 'active', 'removed')),

    -- user_id of the seat that issued the invite (audit).
    invited_by    TEXT,

    -- Enqueue / acceptance instants (Unix epoch ms). joined_at_ms is NULL until
    -- the seat becomes `active`.
    invited_at_ms INTEGER NOT NULL,
    joined_at_ms  INTEGER,

    -- Exactly one row per (tenant, user/invitation).
    PRIMARY KEY (tenant_id, user_id)
);

-- List a tenant's seats (the team/list query) without a full scan.
CREATE INDEX IF NOT EXISTS idx_team_member_tenant_status
    ON team_member (tenant_id, status);

-- Resolve a user's membership(s) — backs the future session->tenant resolver
-- (ADR-S33-001 WP-5) and the per-seat authz lookups.
CREATE INDEX IF NOT EXISTS idx_team_member_user_status
    ON team_member (user_id, status);

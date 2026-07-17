-- Migration: 0091_admin_approvals.sql
-- Dual-approval ledger for the admin mutate plane (finding H5).
--
-- Backs `corelink-handler-admin::ApprovalLedger` / `ApprovalLedgerWriter` via
-- the container `D1ApprovalLedger`. An admin mutation (`set_tenant_tier`,
-- `rotate_admin_token`) is authorized ONLY by a row here that:
--   * was created by an independently-authenticated approver
--     (`POST /v1/admin/approve`, gated by CORELINK_ADMIN_APPROVER_AUTH_KEY),
--   * is scoped to the exact `resource` being mutated,
--   * has an `approver` DISTINCT from the mutation's initiator, and
--   * is UNCONSUMED (single-use — the consume flips `consumed` atomically).
--
-- This replaces the prior free-text `approver == initiator` string compare,
-- which a single key-holder could bypass by sending any other string.
--
-- INV-AUTH-MIGRATION-ADDITIVE: additive only — CREATE ... IF NOT EXISTS, no
-- DROP / no destructive ALTER. Safe to re-run.

CREATE TABLE IF NOT EXISTS admin_approvals (
    approval_id     TEXT     PRIMARY KEY,            -- opaque id the mutate token carries
    approver        TEXT     NOT NULL,               -- AUTHENTICATED approver identity (never a client-body value)
    resource        TEXT     NOT NULL,               -- scope: e.g. `tenant:<id>` / `admin_token:<id>`
    consumed        INTEGER  NOT NULL DEFAULT 0       -- 0 = spendable, 1 = spent (single-use)
        CHECK (consumed IN (0, 1)),
    created_at_ms   BIGINT   NOT NULL,               -- when the approval was recorded
    consumed_at_ms  BIGINT                            -- when it was spent (NULL until consumed)
);

-- Fast lookup of live (unconsumed) approvals for a given resource — the
-- operator console lists pending approvals a tenant still has outstanding.
CREATE INDEX IF NOT EXISTS idx_admin_approvals_resource_live
    ON admin_approvals (resource)
    WHERE consumed = 0;

-- Migration 0076: tenant_legal_hold — per-tenant legal hold gate (C-LEGALHOLD).
--
-- ## Why
--
-- Compliance and legal processes (litigation hold, regulatory freeze, fraud
-- investigation) require the ability to block self-serve data deletion (DSR,
-- account-delete) for a specific tenant without touching any other tenant.
-- Without this table, a DSR or account-delete enqueue would race against an
-- in-flight legal obligation with no durable signal to check.
--
-- A row in this table means ALL destructive erasure operations for `tenant_id`
-- MUST be refused until the hold is lifted (row deleted). The hold does NOT
-- block read or write data-plane ops — only irreversible erasure paths.
--
-- ## Additive policy
--
-- This migration is purely additive (CREATE TABLE IF NOT EXISTS, no DROP, no
-- ALTER of existing tables). It is safe to re-apply idempotently.
--
-- ## Privacy
--
-- `reason` is an internal operator note (litigation-hold reference, ticket id,
-- etc.) — it is NOT surfaced to the end-user tenant and is therefore not
-- subject to DSR subject-access disclosure rules.

CREATE TABLE IF NOT EXISTS tenant_legal_hold (
    -- Tenant under legal hold (matches `tenant.tenant_id`; one hold per tenant).
    tenant_id  TEXT    NOT NULL PRIMARY KEY,

    -- Operator-internal reason / reference (e.g. case id, ticket). Not PII.
    reason     TEXT    NOT NULL,

    -- Instant the hold was placed (Unix epoch ms) — audit anchor.
    held_at_ms BIGINT  NOT NULL
);

-- Migration 0055: tenant_billing — self-serve Stripe Checkout activation table
-- (Stream 2.10 — BILLINGSTEP-WIRE-CHECKOUT).
--
-- This is a denormalized billing-state row per tenant, written by the
-- signup-worker's `checkout.session.completed` webhook handler and kept
-- up-to-date by `customer.subscription.updated` / `customer.subscription.deleted`.
--
-- Relationship to existing tables:
--   - `tier_selections` (0039) — canonical tier + subscription_state FSM.
--     tenant_billing complements it with subscription-level billing detail
--     (period end, Stripe IDs) that the tier FSM does not track.
--   - `stripe_subscriptions` (0048) — append-only event log per event_id.
--     tenant_billing is the *mutable state mirror* that the UI reads for
--     the current billing status without scanning the full event log.
--   - `stripe_customers` (0048) — per-event customer echo row.
--     tenant_billing deduplicates the customer via UNIQUE on stripe_customer_id.
--
-- Idempotency: the webhook handler uses
--   INSERT INTO tenant_billing … ON CONFLICT (tenant_id) DO UPDATE SET …
-- so any Stripe retry for the same checkout.session.completed event is a
-- safe upsert (no duplicate rows, no status regression from a stale retry).
-- The separate UNIQUE INDEX on stripe_customer_id prevents the same Stripe
-- customer from activating two tenants.
--
-- Additive policy (INV-ONBOARD-ATOMIC-PROVISIONING): no DROP, no ALTER DROP.
-- IF NOT EXISTS on every statement.

CREATE TABLE IF NOT EXISTS tenant_billing (
    -- Opaque tenant id (matches tier_selections.tenant_id). PK.
    tenant_id              TEXT    NOT NULL PRIMARY KEY,

    -- Stripe customer id (`cus_…`). Set on checkout.session.completed.
    -- UNIQUE enforced via index below (SQLite-idiomatic — see note in
    -- migration 0054 about ALTER TABLE ADD COLUMN UNIQUE on D1).
    stripe_customer_id     TEXT,

    -- Stripe subscription id (`sub_…`).
    stripe_subscription_id TEXT,

    -- Canonical billing status:
    --   paid       — active paid subscription
    --   canceled   — subscription explicitly canceled (end of period reached)
    --   past_due   — subscription past_due / payment failure
    --   incomplete — Stripe subscription in incomplete state
    status                 TEXT    NOT NULL DEFAULT 'inactive'
        CHECK (status IN ('inactive', 'paid', 'canceled', 'past_due', 'incomplete')),

    -- Stripe price/plan id driving this subscription (e.g. price_1TbQZX…).
    plan                   TEXT,

    -- Unix ms timestamp of when the current billing period ends.
    -- NULL until first paid activation. Sourced from Stripe
    -- `current_period_end` (seconds → multiply × 1000).
    current_period_end_ms  BIGINT,

    -- Schema version (bump on additive change per INV-SCHEMA-VERSION).
    schema_version         INTEGER NOT NULL DEFAULT 1,

    -- Materialization timestamps (ms).
    created_at_ms          BIGINT  NOT NULL,
    updated_at_ms          BIGINT  NOT NULL
);

-- UNIQUE constraint on stripe_customer_id (NULL-safe: D1/SQLite NULLs are
-- not equal to each other, so multiple NULL values are allowed — uniqueness
-- only fires when the value is non-NULL, which is the desired behaviour here:
-- once a Stripe customer is bound to a tenant, it must not be re-used for
-- another tenant).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_billing_customer_id
    ON tenant_billing (stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;

-- Lookup index for the subscription id (used in subscription.updated /
-- subscription.deleted handlers to find the tenant by sub_…).
CREATE INDEX IF NOT EXISTS idx_tenant_billing_subscription_id
    ON tenant_billing (stripe_subscription_id)
    WHERE stripe_subscription_id IS NOT NULL;

-- Covering index for status-based queries (e.g. "all paid tenants").
CREATE INDEX IF NOT EXISTS idx_tenant_billing_status
    ON tenant_billing (status);

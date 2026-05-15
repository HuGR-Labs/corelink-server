-- Migration 0048: Stripe billing materializer tables (wave-17).
--
-- Companion to:
--   - `0018_stripe_idem_keys.sql` (event-type CHECK contract)
--   - `0039_tier_selection.sql` (canonical `tier_selections` table)
--   - `0044_stripe_webhook_events_processed.sql` (dedup table)
--
-- Provides the canonical D1 row writers consumed by the wave-17
-- `corelink-billing-stripe-materializer::D1SubscriptionStateHandler`.
-- One table per state-mutating arm of the canonical 10-element Stripe
-- event taxonomy plus the three observability-echo arms that still
-- write a row (`customer.created`, `customer.subscription.created`,
-- `charge.refunded`). The two pure-echo arms
-- (`customer.subscription.trial_will_end` and `invoice.created`) only
-- emit an audit row — no D1 table is needed.
--
-- All tables are written via `INSERT … ON CONFLICT (stripe_event_id)
-- DO NOTHING` semantics so a Stripe retry that bypasses the
-- `stripe_webhook_events_processed` dedup gate (operator replay
-- harness, BYOK-region split-brain) cannot duplicate rows.
--
-- Customer-sensitive columns (`stripe_customer_id`, `email_token`)
-- are stored as opaque tokens. The raw email lives in the
-- application-layer encrypted column store (AWS-RDS / Neon
-- envelope-encrypted column per ADR-0035); this migration only
-- references the token-id.
--
-- Idempotent / additive: every statement uses `IF NOT EXISTS`;
-- replays are safe (INV-AUTH-MIGRATION-ADDITIVE compliant).

-- ============================================================
-- stripe_customers — `customer.created` echo row
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_customers (
    -- Stripe customer id (e.g. `cus_…`). PRIMARY KEY for natural dedup.
    stripe_customer_id  TEXT    NOT NULL PRIMARY KEY,
    -- Tenant scope. FK-soft to `tier_selections.tenant_id` (not a
    -- hard FK because the customer can arrive before the tenant row
    -- exists in some Stripe-led onboarding flows).
    tenant_id           TEXT    NOT NULL,
    -- Stripe event id this row was derived from (forensics).
    stripe_event_id     TEXT    NOT NULL UNIQUE,
    -- Materialization timestamp (ms).
    materialized_at_ms  BIGINT  NOT NULL,
    -- Canonical JSON payload (full Stripe `data.object` subset we
    -- care about). Schema-on-read.
    payload_json        TEXT    NOT NULL,
    -- Schema version (bump on additive change).
    schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_stripe_customers_tenant
    ON stripe_customers (tenant_id);

-- ============================================================
-- stripe_subscriptions — `customer.subscription.*` rows
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_subscriptions (
    -- Stripe subscription id (`sub_…`). PRIMARY KEY natural dedup.
    stripe_subscription_id TEXT    NOT NULL PRIMARY KEY,
    -- Tenant scope (FK-soft to `tier_selections.tenant_id`).
    tenant_id              TEXT    NOT NULL,
    -- Stripe event id this row was derived from.
    stripe_event_id        TEXT    NOT NULL UNIQUE,
    -- Subscription status mirror (`active` | `past_due` | `canceled`
    -- | `unpaid` | `incomplete` | `incomplete_expired` | `trialing`).
    status                 TEXT    NOT NULL,
    -- Plan id (`plan_…` or `price_…`); drives tier reconciliation.
    plan_id                TEXT,
    -- Seat count (for per-seat plans). Defaults to 1.
    seat_count             INTEGER NOT NULL DEFAULT 1,
    materialized_at_ms     BIGINT  NOT NULL,
    payload_json           TEXT    NOT NULL,
    schema_version         INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_stripe_subscriptions_tenant
    ON stripe_subscriptions (tenant_id);

CREATE INDEX IF NOT EXISTS idx_stripe_subscriptions_status
    ON stripe_subscriptions (status);

-- ============================================================
-- stripe_invoices — `invoice.paid` / `invoice.payment_failed`
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_invoices (
    stripe_invoice_id   TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    stripe_event_id     TEXT    NOT NULL UNIQUE,
    -- Outcome (`paid` | `payment_failed`).
    outcome             TEXT    NOT NULL
        CHECK (outcome IN ('paid', 'payment_failed')),
    materialized_at_ms  BIGINT  NOT NULL,
    payload_json        TEXT    NOT NULL,
    schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_stripe_invoices_tenant
    ON stripe_invoices (tenant_id);

CREATE INDEX IF NOT EXISTS idx_stripe_invoices_outcome
    ON stripe_invoices (outcome);

-- ============================================================
-- stripe_disputes — `charge.dispute.created` (Sev1 path)
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_disputes (
    stripe_dispute_id   TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    stripe_event_id     TEXT    NOT NULL UNIQUE,
    materialized_at_ms  BIGINT  NOT NULL,
    payload_json        TEXT    NOT NULL,
    -- Severity bucket; always `sev1` for `charge.dispute.created`.
    severity            TEXT    NOT NULL DEFAULT 'sev1'
        CHECK (severity IN ('sev1', 'sev2', 'sev3', 'info')),
    schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_stripe_disputes_tenant
    ON stripe_disputes (tenant_id);

-- ============================================================
-- stripe_refunds — `charge.refunded`
-- ============================================================
CREATE TABLE IF NOT EXISTS stripe_refunds (
    -- The Stripe charge id (refunds are addressed via the parent
    -- charge in webhook deliveries).
    stripe_charge_id    TEXT    NOT NULL PRIMARY KEY,
    tenant_id           TEXT    NOT NULL,
    stripe_event_id     TEXT    NOT NULL UNIQUE,
    materialized_at_ms  BIGINT  NOT NULL,
    payload_json        TEXT    NOT NULL,
    schema_version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_stripe_refunds_tenant
    ON stripe_refunds (tenant_id);

-- ============================================================
-- INV-BILLING-TIER-CONSISTENT — reconciliation aux view
-- ============================================================
-- After every `customer.subscription.updated` reconciliation cycle,
-- the audit chain MUST be able to prove that
-- `tier_selections.tier == compute_tier(active_subscription)`. This
-- view exposes the join surface the daily reconciliation cron uses;
-- a non-empty result means the cron MUST page (drift detected).
CREATE VIEW IF NOT EXISTS stripe_tier_drift_view AS
SELECT
    ts.tenant_id          AS tenant_id,
    ts.tier               AS persisted_tier,
    ss.plan_id            AS active_plan_id,
    ss.status             AS active_status,
    ss.materialized_at_ms AS subscription_seen_at_ms
FROM tier_selections AS ts
LEFT JOIN stripe_subscriptions AS ss
       ON ss.tenant_id = ts.tenant_id
WHERE ss.status = 'active';

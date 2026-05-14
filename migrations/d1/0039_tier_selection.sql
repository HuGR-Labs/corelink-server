-- Migration 0037: Tier selection + Stripe Checkout activation
-- (WI-S19-004).
--
-- Creates three tables + a UNIQUE partial index for INV-ONBOARD-DPA-
-- FIRST defense-in-depth (Lote 10.19 codex P0 canonical fix; SQLite/D1
-- does NOT support `SELECT ... FOR UPDATE`, so we use
-- `BEGIN IMMEDIATE TRANSACTION` plus a UNIQUE partial index on
-- `subscription_state = 'active'` per WI §6.4).
--
--   tier_selections           — per-tenant tier + subscription state
--                               (in-process mirror lives in
--                               `corelink-tier-selection`
--                               `TierSelectionLedger`).
--   tier_selection_locks      — 60s row-level lock window per
--                               tenant; `INSERT OR IGNORE` prevents
--                               concurrent tier switches.
--   stripe_checkout_sessions  — in-flight Checkout Session mirror;
--                               cleared on webhook activation.
--
-- INV-ONBOARD-DPA-FIRST is HARD-enforced application-side BEFORE any
-- Stripe API call via `corelink-tier-selection`
-- `TierSelectionLedger::select_tier` — see
-- `crates/corelink-tier-selection/src/ledger.rs`. This migration
-- ships the durable mirror only; the lock is acquired inside the
-- same `BEGIN IMMEDIATE TRANSACTION` that performs the DPA check +
-- UPDATE.
--
-- Idempotency / additive: every statement uses `IF NOT EXISTS`;
-- replays are safe.

-- ============================================================
-- tier_selections — per-tenant tier + subscription state
-- ============================================================
CREATE TABLE IF NOT EXISTS tier_selections (
    -- Opaque tenant id (matches `tenant.tenant_id`).
    tenant_id                 TEXT    NOT NULL PRIMARY KEY,
    -- Selected tier: free | starter | team | pro | enterprise.
    -- Enterprise rows are only written via the inquiry-form flow
    -- (WI-S19-005); the tier-selection backend rejects direct
    -- enterprise activation with 422 `use_inquiry_form`.
    tier                      TEXT    NOT NULL
        CHECK (tier IN ('free', 'starter', 'team', 'pro', 'enterprise')),
    -- Canonical subscription state per WI §6.4.
    subscription_state        TEXT    NOT NULL
        CHECK (subscription_state IN ('inactive', 'pending_checkout', 'active')),
    -- Stripe customer id mapped atomically per WI §6.7
    -- (drift-prevention: set in the same tx that writes the
    -- session row).
    stripe_customer_id        TEXT,
    -- Activation timestamp (ms since epoch); NULL until `active`.
    subscription_started_at_ms BIGINT,
    -- Schema version (bump on additive change).
    schema_version            INTEGER NOT NULL DEFAULT 1,
    -- Correlation id from the audit chain.
    correlation_id            TEXT    NOT NULL,
    CONSTRAINT subscription_started_when_active CHECK (
        (subscription_state = 'active' AND subscription_started_at_ms IS NOT NULL)
        OR (subscription_state <> 'active')
    )
);

-- UNIQUE partial index defense-in-depth: at most one `active`
-- subscription per tenant. Combined with `BEGIN IMMEDIATE`
-- pessimistic lock, this eliminates the race window (Lote 10.19
-- canonical fix; replaces PostgreSQL-only `SELECT ... FOR UPDATE`).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_active_subscription
    ON tier_selections (tenant_id)
    WHERE subscription_state = 'active';

CREATE INDEX IF NOT EXISTS idx_tier_selections_tier
    ON tier_selections (tier);

CREATE INDEX IF NOT EXISTS idx_tier_selections_state
    ON tier_selections (subscription_state);

-- ============================================================
-- tier_selection_locks — 60s row-level lock window per tenant
-- ============================================================
-- Used via `INSERT OR IGNORE` to acquire a 60s pessimistic lock
-- before performing the DPA check + tier UPDATE. Concurrent callers
-- INSERT OR IGNORE will hit the PRIMARY KEY constraint and the
-- handler returns `TierError::LockHeld` (HTTP 409).
--
-- A daily GC cron removes rows with `expires_at_ms < now` (deferred
-- to PRR ship gate); the application also evicts expired rows
-- lazily in `select_tier()`.
CREATE TABLE IF NOT EXISTS tier_selection_locks (
    tenant_id      TEXT   NOT NULL PRIMARY KEY,
    acquired_at_ms BIGINT NOT NULL,
    expires_at_ms  BIGINT NOT NULL,
    correlation_id TEXT   NOT NULL,
    CONSTRAINT lock_window_60s
        CHECK (expires_at_ms - acquired_at_ms <= 60000),
    CONSTRAINT lock_positive_window
        CHECK (expires_at_ms > acquired_at_ms)
);

CREATE INDEX IF NOT EXISTS idx_tier_selection_locks_expires
    ON tier_selection_locks (expires_at_ms);

-- ============================================================
-- stripe_checkout_sessions — in-flight Checkout Session mirror
-- ============================================================
-- Created when a paid-tier (starter|team|pro) tier-select succeeds.
-- Cleared on webhook activation (`checkout.session.completed`).
-- Reconciliation cron compares Stripe API + D1 daily; mismatches
-- alert (`corelink_onboarding_stripe_customer_id_drift_total`).
CREATE TABLE IF NOT EXISTS stripe_checkout_sessions (
    session_id     TEXT   NOT NULL PRIMARY KEY,
    tenant_id      TEXT   NOT NULL,
    tier           TEXT   NOT NULL
        CHECK (tier IN ('starter', 'team', 'pro')),
    created_at_ms  BIGINT NOT NULL,
    correlation_id TEXT   NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tier_selections(tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_stripe_sessions_tenant
    ON stripe_checkout_sessions (tenant_id);

CREATE INDEX IF NOT EXISTS idx_stripe_sessions_created
    ON stripe_checkout_sessions (created_at_ms);

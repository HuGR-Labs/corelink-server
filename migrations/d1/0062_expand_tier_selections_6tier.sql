-- Migration 0062: widen the tier-string CHECK on `tier_selections.tier`
-- and `stripe_checkout_sessions.tier` to the 6-tier taxonomy.
--
-- Canonical taxonomy (FROZEN — 6 tiers):
--   free | solo | starter | pro | max | enterprise
--   requires_stripe_checkout = the 4 PAID tiers: solo | starter | pro | max.
--   enterprise  → inquiry form (unchanged);  free → instant activation.
--
-- What this migration changes vs migration 0039:
--   - tier_selections.tier      0039 CHECK accepted ('free','starter','team',
--                               'pro','enterprise'). This migration widens it to
--                               accept ('free','solo','starter','team','pro',
--                               'max','enterprise'). `solo` + `max` ADDED.
--   - stripe_checkout_sessions.tier  0039 CHECK accepted ('starter','team','pro').
--                               This migration widens it to accept the 4 PAID
--                               tiers ('solo','starter','pro','max'). `solo` +
--                               `max` ADDED.
--
-- ADDITIVE intent (INV-AUTH-MIGRATION-ADDITIVE, HIGH):
--   - This is a PURELY WIDENING change. No accepted value is removed: the legacy
--     `team` value is RETAINED in BOTH widened CHECKs for back-compat (any row or
--     admin tool that still writes `team` keeps working). The accepted SET only
--     grows.
--   - NO ROW IS DROPPED OR MUTATED. The data is copied 1:1 (`INSERT … SELECT *`)
--     into the rebuilt table; every column value, including existing
--     subscription_state / stripe_customer_id / timestamps / correlation_id, is
--     preserved byte-for-byte.
--
-- SQLite/D1 limitation forcing a table rebuild (the "12-step" recreate):
--   0039 declares both CHECKs as INLINE COLUMN constraints on CREATE TABLE.
--   SQLite (hence D1) has NO `ALTER TABLE … ALTER COLUMN` / `DROP CONSTRAINT` /
--   `ADD CONSTRAINT` to relax an EXISTING inline CHECK in place (see the SQLite
--   "Making Other Kinds Of Table Schema Changes" 12-step procedure). The 0057
--   precedent (`tenant.tier`) did NOT face this: it widened the taxonomy by
--   ADDING a *brand-new* column (`ALTER TABLE tenant ADD COLUMN tier … CHECK(…)`)
--   whose CHECK could be authored wide from the start — that idiom only works for
--   a column that does not yet exist, so it canNOT relax 0039's pre-existing
--   `tier` columns. The only SQLite-correct way to relax an existing inline CHECK
--   is to rebuild the table, which unavoidably uses DROP TABLE + RENAME TO.
--
--   Those two tokens are flagged by `scripts/check_migrations_additive.py`
--   (INV-AUTH-MIGRATION-ADDITIVE). Each is therefore annotated below with the
--   gate's own line-local suppression:
--       -- additive-allowed: ADR-0062 <reason>
--   The change is destructive in MECHANISM (rebuild) but ADDITIVE in EFFECT
--   (set-of-accepted-values only grows; zero data loss). The lead must land the
--   accompanying ADR-0062 recording this rebuild + its zero-data-loss proof
--   before this migration is applied to prod (see the WP risk flag).
--
-- Safety of the rebuild:
--   - `PRAGMA foreign_keys` is left OFF for the rebuild. (D1 FKs are logical /
--     per-connection inconsistent in CF Workers — see 0002/0003 — and
--     `stripe_checkout_sessions` has a soft FK to `tier_selections`; toggling it
--     off avoids any spurious FK action during the swap, matching the SQLite
--     12-step guidance.) The migration runs inside wrangler's per-file execution.
--   - The UNIQUE partial index `idx_tenant_active_subscription` (the
--     INV-ONBOARD-DPA-FIRST defense-in-depth one) and the two helper indexes are
--     RE-CREATED verbatim after the swap, so the at-most-one-active-subscription
--     invariant is preserved across the rebuild.
--   - `tier_selection_locks` is NOT touched (its CHECKs do not reference tier).
--
-- Idempotency: unlike 0039 this file is NOT re-runnable on its own (a rebuild is
--   inherently one-shot); it is guarded by the migration runner's sequential
--   numbering (`wrangler d1 migrations apply` records 0062 as applied exactly
--   once — same guarantee 0057 relies on).
--
-- Canonical sources kept in lock-step with this CHECK:
--   - migrations/d1/0039_tier_selection.sql (original inline CHECKs)
--   - migrations/d1/0057_tenant_tier.sql (tenant.tier widening precedent)
--   - crates/corelink-container/src/routes/admin.rs (TIER_SELECTIONS_TIERS;
--     the #35 follow-up this migration unblocks)
--   - crates/corelink-container/src/routes/tier_select_store.rs (tier_column)

PRAGMA foreign_keys = OFF;

-- ============================================================
-- 1. tier_selections — rebuild with the widened 6-tier CHECK
-- ============================================================
-- New table mirrors 0039 EXACTLY except the `tier` CHECK list, which gains
-- 'solo' + 'max' (and retains 'team').
CREATE TABLE IF NOT EXISTS tier_selections_new (
    tenant_id                 TEXT    NOT NULL PRIMARY KEY,
    -- Selected tier: free | solo | starter | team | pro | max | enterprise.
    -- 'team' retained for back-compat (additive widen — nothing removed).
    -- Enterprise rows are only written via the inquiry-form flow
    -- (WI-S19-005); the tier-selection backend rejects direct
    -- enterprise activation with 422 `use_inquiry_form`.
    tier                      TEXT    NOT NULL
        CHECK (tier IN ('free', 'solo', 'starter', 'team', 'pro', 'max', 'enterprise')),
    -- Canonical subscription state per WI §6.4.
    subscription_state        TEXT    NOT NULL
        CHECK (subscription_state IN ('inactive', 'pending_checkout', 'active')),
    -- Stripe customer id mapped atomically per WI §6.7.
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

-- Copy every existing row 1:1 (no filtering, no transformation — additive).
-- Explicit column list (not SELECT *) so the copy is order-independent and
-- self-documenting.
INSERT INTO tier_selections_new (
    tenant_id, tier, subscription_state, stripe_customer_id,
    subscription_started_at_ms, schema_version, correlation_id
)
SELECT
    tenant_id, tier, subscription_state, stripe_customer_id,
    subscription_started_at_ms, schema_version, correlation_id
FROM tier_selections;

DROP TABLE tier_selections;  -- additive-allowed: ADR-0062 rebuild to widen inline CHECK (set grows; rows copied 1:1 above)

ALTER TABLE tier_selections_new RENAME TO tier_selections;  -- additive-allowed: ADR-0062 finalize rebuild swap (no data loss)

-- Re-create the 0039 indexes verbatim (lost with the dropped table).
-- UNIQUE partial index: at most one `active` subscription per tenant
-- (INV-ONBOARD-DPA-FIRST defense-in-depth).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_active_subscription
    ON tier_selections (tenant_id)
    WHERE subscription_state = 'active';

CREATE INDEX IF NOT EXISTS idx_tier_selections_tier
    ON tier_selections (tier);

CREATE INDEX IF NOT EXISTS idx_tier_selections_state
    ON tier_selections (subscription_state);

-- ============================================================
-- 2. stripe_checkout_sessions — rebuild with the widened paid-tier CHECK
-- ============================================================
-- New table mirrors 0039 EXACTLY except the `tier` CHECK list, which becomes
-- the 4 PAID tiers ('solo','starter','pro','max') plus retained legacy 'team'.
CREATE TABLE IF NOT EXISTS stripe_checkout_sessions_new (
    session_id     TEXT   NOT NULL PRIMARY KEY,
    tenant_id      TEXT   NOT NULL,
    -- Paid tiers only: solo | starter | team | pro | max.
    -- 'team' retained for back-compat (additive widen — nothing removed).
    tier           TEXT   NOT NULL
        CHECK (tier IN ('solo', 'starter', 'team', 'pro', 'max')),
    created_at_ms  BIGINT NOT NULL,
    correlation_id TEXT   NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tier_selections(tenant_id)
);

INSERT INTO stripe_checkout_sessions_new (
    session_id, tenant_id, tier, created_at_ms, correlation_id
)
SELECT
    session_id, tenant_id, tier, created_at_ms, correlation_id
FROM stripe_checkout_sessions;

DROP TABLE stripe_checkout_sessions;  -- additive-allowed: ADR-0062 rebuild to widen inline CHECK (set grows; rows copied 1:1 above)

ALTER TABLE stripe_checkout_sessions_new RENAME TO stripe_checkout_sessions;  -- additive-allowed: ADR-0062 finalize rebuild swap (no data loss)

CREATE INDEX IF NOT EXISTS idx_stripe_sessions_tenant
    ON stripe_checkout_sessions (tenant_id);

CREATE INDEX IF NOT EXISTS idx_stripe_sessions_created
    ON stripe_checkout_sessions (created_at_ms);

PRAGMA foreign_keys = ON;

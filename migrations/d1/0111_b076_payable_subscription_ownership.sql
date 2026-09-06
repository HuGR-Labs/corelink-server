-- Migration 0111: B-076 payable-subscription ownership invariants.
--
-- A free seed row is active for every tenant, but it is not payable.  The
-- partial UNIQUE index therefore covers both a paid active subscription and a
-- paid pending Checkout.  D1/SQLite enforces this at the storage boundary,
-- including concurrent requests that race between the read guard and write.
-- The application additionally binds each write to its correlation-owned lock
-- and Stripe session/customer identity.

CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_one_payable_subscription
    ON tier_selections (tenant_id)
    WHERE tier != 'free' AND subscription_state IN ('pending_checkout', 'active');

CREATE INDEX IF NOT EXISTS idx_stripe_checkout_sessions_owner
    ON stripe_checkout_sessions (tenant_id, correlation_id, created_at_ms);

-- Recoverable ledger for the two durable mirrors above. D1-over-HTTP exposes
-- one statement per request rather than a Worker `db.batch()`, so the ledger
-- records the session/customer before either mirror write. A retry can repair
-- a crash between those writes, and stale pre-Stripe reservations are cleaned
-- by the executable reconciler in the tier-select store.
CREATE TABLE IF NOT EXISTS stripe_checkout_ownership_ledger (
    correlation_id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    tier TEXT NOT NULL,
    session_id TEXT,
    stripe_customer_id TEXT,
    state TEXT NOT NULL CHECK (state IN ('reserved', 'session_created', 'completed', 'abandoned')),
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tier_selections(tenant_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_checkout_ledger_one_open_axis
    ON stripe_checkout_ownership_ledger (tenant_id)
    WHERE state IN ('reserved', 'session_created');

CREATE UNIQUE INDEX IF NOT EXISTS idx_checkout_ledger_session
    ON stripe_checkout_ownership_ledger (session_id)
    WHERE session_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_billing_paid_subscription
    ON tenant_billing (stripe_subscription_id)
    WHERE stripe_subscription_id IS NOT NULL AND status = 'paid';

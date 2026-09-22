-- D1 ordinal 0137 — #1639. The provider, rather than webhook arrival or a
-- lexical SKU identity, owns these two ordering coordinates.

CREATE TABLE IF NOT EXISTS runner_entitlement_reconcile_fence (
    tenant_id TEXT PRIMARY KEY,
    stripe_subscription_id TEXT NOT NULL,
    subscription_created_at_ms INTEGER NOT NULL CHECK (subscription_created_at_ms > 0),
    stripe_event_created_at_ms INTEGER NOT NULL CHECK (stripe_event_created_at_ms > 0),
    is_granting INTEGER NOT NULL CHECK (is_granting IN (0, 1)),
    applied_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runner_entitlement_fence_subscription
    ON runner_entitlement_reconcile_fence(stripe_subscription_id);

ALTER TABLE stripe_subscriptions
    ADD COLUMN stripe_event_created_at_ms INTEGER NOT NULL DEFAULT 0;

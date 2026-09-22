-- D1 ordinal 0137 — #1639 / WP3.
--
-- The fence is separate from runners_entitlement so a revoke can leave a
-- durable watermark behind. A later process or instance cannot reinsert an
-- entitlement using an older Stripe authority key after the row is deleted.

CREATE TABLE IF NOT EXISTS runner_entitlement_reconcile_fence (
    tenant_id             TEXT PRIMARY KEY,
    stripe_subscription_id TEXT NOT NULL,
    authority_key         TEXT NOT NULL CHECK (length(authority_key) > 0),
    applied_at_ms         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runner_entitlement_fence_subscription
    ON runner_entitlement_reconcile_fence(stripe_subscription_id);

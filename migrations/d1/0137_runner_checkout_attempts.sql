-- 0137: durable logical Checkout attempts for the runner entitlement axis.
--
-- A row binds the provider idempotency key and exact selected parameters to
-- one monotonic tenant-local generation. An open prior row blocks a replacement
-- until its provider session is proved expired, or a reserved row is reconciled
-- as absent. This prevents a plan change from creating uncontrolled payable
-- sessions while retaining a stable retry identity across process restarts.

CREATE TABLE IF NOT EXISTS runner_checkout_attempts (
    tenant_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    tier TEXT NOT NULL CHECK (tier IN ('runner_starter', 'runner_pro', 'runner_team', 'runner_scale', 'runner_max')),
    price_id TEXT NOT NULL,
    stripe_customer_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    session_id TEXT UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('reserved', 'session_created', 'expired', 'abandoned', 'completed')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, generation)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_runner_checkout_attempt_one_open
    ON runner_checkout_attempts (tenant_id)
    WHERE state IN ('reserved', 'session_created');

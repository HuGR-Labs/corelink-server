//! Canonical D1 statements for the runner Checkout attempt ledger.
//!
//! The container/Worker adapter executes these statements in one D1
//! transaction. The partial unique index from migration 0133 is the final
//! cross-isolate guard: at most one `reserved` or `session_created` row can
//! exist for a tenant.

/// Reserve the next generation only when no open attempt exists.
///
/// Bind `?1..?6` as `(tenant_id, tier, price_id, stripe_customer_id,
/// ignored_idempotency_key, now_ms)`. The key is derived from the allocated
/// generation inside the statement so a concurrent reserve cannot share a
/// key. A zero-row result means the caller must read the
/// current row and return replay/expiry/recovery; it must never create a
/// second session speculatively.
pub const SQL_RESERVE_RUNNER_CHECKOUT_ATTEMPT: &str = "INSERT INTO runner_checkout_attempts (tenant_id, generation, tier, price_id, stripe_customer_id, idempotency_key, state, created_at_ms, updated_at_ms) SELECT ?1, COALESCE((SELECT MAX(generation) FROM runner_checkout_attempts WHERE tenant_id = ?1), 0) + 1, ?2, ?3, ?4, printf('checkout:v2:runner:%s:%d', ?1, COALESCE((SELECT MAX(generation) FROM runner_checkout_attempts WHERE tenant_id = ?1), 0) + 1), 'reserved', ?6, ?6 WHERE ?5 = ?5 AND NOT EXISTS (SELECT 1 FROM runner_checkout_attempts WHERE tenant_id = ?1 AND state IN ('reserved', 'session_created')) ON CONFLICT DO NOTHING RETURNING tenant_id, generation, tier, price_id, stripe_customer_id, idempotency_key, state, session_id";

/// Read the latest generation to classify exact replay, plan change, or a
/// stale reservation after `SQL_RESERVE_RUNNER_CHECKOUT_ATTEMPT` returned no row.
pub const SQL_READ_CURRENT_RUNNER_CHECKOUT_ATTEMPT: &str = "SELECT tenant_id, generation, tier, price_id, stripe_customer_id, idempotency_key, state, session_id FROM runner_checkout_attempts WHERE tenant_id = ?1 ORDER BY generation DESC LIMIT 1";

/// Record Stripe's response only for a still-reserved generation. The UNIQUE
/// `session_id` constraint also prevents one provider session being attached to
/// two attempts.
pub const SQL_RECORD_RUNNER_CHECKOUT_SESSION: &str = "UPDATE runner_checkout_attempts SET session_id = ?3, state = 'session_created', updated_at_ms = ?4 WHERE tenant_id = ?1 AND generation = ?2 AND state = 'reserved' AND session_id IS NULL";

/// Confirm provider expiry before a replacement is admitted.
pub const SQL_MARK_RUNNER_CHECKOUT_EXPIRED: &str = "UPDATE runner_checkout_attempts SET state = 'expired', updated_at_ms = ?4 WHERE tenant_id = ?1 AND generation = ?2 AND session_id = ?3 AND state = 'session_created'";

/// Close a crashed reservation only after provider lookup returned no session.
pub const SQL_MARK_RUNNER_CHECKOUT_ABANDONED: &str = "UPDATE runner_checkout_attempts SET state = 'abandoned', updated_at_ms = ?3 WHERE tenant_id = ?1 AND generation = ?2 AND state = 'reserved' AND session_id IS NULL";

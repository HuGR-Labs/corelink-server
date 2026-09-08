// ──────────────────────────────────────────────────────────────────────────────
// Durable orchestration (the ironclad heart) — store trait + fail-closed order.
//
// The store trait isolates the durable D1-over-HTTP effects so the
// orchestration ORDER (the load-bearing invariants) is testable natively
// against an in-memory store. The production `D1HttpTierSelectStore`
// (next increment) implements the same trait against D1 via
// `INSERT OR IGNORE ... RETURNING` (lock) + `SELECT` (DPA / active) +
// `UPDATE`/`INSERT` (persist). Generic (not `dyn`) to keep `async fn` in
// trait object-safety out of scope.
// ──────────────────────────────────────────────────────────────────────────────

/// The price/customer details returned by Stripe for a paid checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckoutCreated {
    /// Stripe-hosted Checkout URL (always `https://`).
    pub checkout_url: String,
    /// Stripe Checkout Session id (`cs_...`).
    pub session_id: String,
    /// Stripe customer id (`cus_...`).
    pub stripe_customer_id: String,
}

/// Durable effects backing the orchestration. Every method is fail-CLOSED:
/// an `Err` aborts the orchestration WITHOUT partial state (the caller
/// maps it to 500/502/409 as appropriate).
pub trait TierSelectStore {
    /// Acquire the 60s row lock for `tenant_id` (`INSERT OR IGNORE ...
    /// RETURNING`). Returns `Ok(true)` if acquired, `Ok(false)` if a live
    /// lock is already held (→ 409 `lock_held`).
    fn acquire_lock(
        &self,
        tenant_id: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// `true` iff `tenant_id` has accepted `dpa_version` (INV-ONBOARD-DPA-FIRST).
    fn is_dpa_accepted(
        &self,
        tenant_id: &str,
        dpa_version: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// `true` iff `tenant_id` already has an `active` (cache-axis) subscription.
    fn has_active_subscription(
        &self,
        tenant_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// Repair mirror rows from the recoverable checkout ledger and expire
    /// abandoned pre-Stripe reservations before the active-subscription gate.
    fn reconcile_pending_checkout(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// `true` iff `tenant_id` already holds an active/trialing RUNNER
    /// subscription (`runner_billing`, migration 0087). The runner axis is
    /// SEPARATE from the cache [`has_active_subscription`](Self::has_active_subscription):
    /// a tenant may hold a cache tier AND a runner tier simultaneously, so the
    /// orchestration guards each axis independently. This guards ONLY against a
    /// second concurrent runner subscription.
    fn has_active_runner_subscription(
        &self,
        tenant_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// Reserve the tenant's single payable slot before the network call. The
    /// reservation closes the lock-expiry window where Stripe could create a
    /// payable session before the D1 mirror is written.
    fn reserve_pending_checkout(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Persist a paid-tier pending-checkout through the ownership ledger and
    /// recoverable `tier_selections`/`stripe_checkout_sessions` mirrors.
    fn persist_pending_checkout(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        created: &CheckoutCreated,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Remove only this request's uncompleted reservation after a failed
    /// Stripe/mirror attempt; an activated row is never rolled back.
    fn abandon_pending_checkout(
        &self,
        tenant_id: &str,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Persist an instant free-tier activation.
    fn persist_free_active(
        &self,
        tenant_id: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Release only the row lock owned by this correlation (best-effort; lock
    /// also self-expires at 60s).
    fn release_lock(
        &self,
        tenant_id: &str,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// Create a Stripe Checkout Session for a paid tier. Isolated so the
/// orchestration is testable without real HTTP. The production adapter
/// calls `corelink_stripe_real::StripeRealClient` (sync, `reqwest::blocking`)
/// via `tokio::task::spawn_blocking`.
pub trait CheckoutCreator {
    /// Create the hosted Checkout Session, or `Err` (→ 502 `stripe_unavailable`).
    fn create(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        success_url: &str,
        cancel_url: &str,
    ) -> impl std::future::Future<Output = Result<CheckoutCreated, String>> + Send;
}

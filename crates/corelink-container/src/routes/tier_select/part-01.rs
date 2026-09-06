/// Minimal audit seam — the production sink writes the durable audit-chain
/// record. The orchestration calls `emit` BEFORE every state mutation; an
/// `Err` ABORTS (fail-CLOSED, INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
pub trait TierSelectAudit {
    /// Emit `event` for `tenant_id`. `Err` aborts the orchestration.
    fn emit(
        &self,
        event: &'static str,
        tenant_id: &str,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// The fail-CLOSED orchestration. Order (identical to
/// `corelink_tier_selection::ledger::select_tier`, whose 10k-iter property
/// tests pin these invariants):
///   1. audit `tier_select_attempted` BEFORE any state read/mutation
///   2. acquire durable lock        → `LockHeld` (409) if held
///   3. INV-ONBOARD-DPA-FIRST       → `DpaRequired` (403) if not accepted
///   4. UNIQUE active subscription  → `AlreadyActive` (409) if active
///   5. free → instant activate; paid → Stripe Checkout (only AFTER DPA)
///   6. persist; release lock
///
/// The lock is released on every terminal error path so the tenant can retry.
#[allow(clippy::too_many_arguments)]
pub async fn orchestrate_tier_select<S, C, A>(
    store: &S,
    checkout: &C,
    audit: &A,
    tenant_id: &str,
    tier: RequestedTier,
    success_url: &str,
    cancel_url: &str,
    dpa_version: &str,
    now_ms: i64,
    correlation_id: &str,
) -> Result<TierSelectResponse, TierSelectHttpError>
where
    S: TierSelectStore + Sync,
    C: CheckoutCreator + Sync,
    A: TierSelectAudit + Sync,
{
    // (1) Audit BEFORE any state read/mutation (fail-CLOSED).
    audit
        .emit("tier_select_attempted", tenant_id, correlation_id)
        .await
        .map_err(|_| TierSelectHttpError::Internal)?;

    // (2) Durable lock — cross-isolate mutex held across the Stripe call.
    let acquired = store
        .acquire_lock(tenant_id, now_ms, correlation_id)
        .await
        .map_err(|_| TierSelectHttpError::Internal)?;
    if !acquired {
        return Err(TierSelectHttpError::LockHeld);
    }

    // From here, ALWAYS release the lock on a terminal error.
    let result = orchestrate_locked(
        store,
        checkout,
        audit,
        tenant_id,
        tier,
        success_url,
        cancel_url,
        dpa_version,
        now_ms,
        correlation_id,
    )
    .await;
    if result.is_err() {
        let _ = store
            .abandon_pending_checkout(tenant_id, correlation_id)
            .await;
        // Best-effort release; the 60s window also self-expires.
        let _ = store.release_lock(tenant_id, correlation_id).await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn orchestrate_locked<S, C, A>(
    store: &S,
    checkout: &C,
    audit: &A,
    tenant_id: &str,
    tier: RequestedTier,
    success_url: &str,
    cancel_url: &str,
    dpa_version: &str,
    now_ms: i64,
    correlation_id: &str,
) -> Result<TierSelectResponse, TierSelectHttpError>
where
    S: TierSelectStore + Sync,
    C: CheckoutCreator + Sync,
    A: TierSelectAudit + Sync,
{
    // (3) INV-ONBOARD-DPA-FIRST — BEFORE any Stripe call, ALL tiers.
    let dpa_ok = store
        .is_dpa_accepted(tenant_id, dpa_version)
        .await
        .map_err(|_| TierSelectHttpError::Internal)?;
    if !dpa_ok {
        audit
            .emit("dpa_first_violation_attempt", tenant_id, correlation_id)
            .await
            .map_err(|_| TierSelectHttpError::Internal)?;
        return Err(TierSelectHttpError::DpaRequired);
    }

    store
        .reconcile_pending_checkout(tenant_id, now_ms)
        .await
        .map_err(|_| TierSelectHttpError::Internal)?;

    // (4) At most one active PAID subscription per AXIS. Runner is a separate
    // entitlement axis from the cache tier (migrations 0070/0087), so a
    // cache-active tenant may still buy runner and vice-versa; we guard each
    // axis against a *second* subscription of the SAME kind — never letting a
    // cache subscription block a runner purchase (or the reverse).
    //
    // "Active" here means an active PAID subscription. The free tier is an
    // instant activation, not a subscription, and signup seeds every tenant with
    // `tier_selections('free','active')` — so a tier-agnostic read makes this
    // guard reject the first purchase of every account that exists. Both store
    // methods therefore exclude the free/unentitled rows (`tier != 'free'` on the
    // cache axis; `status IN ('active','trialing')` already scopes the runner
    // axis to a real paid subscription).
    let active = if tier.is_runner() {
        store.has_active_runner_subscription(tenant_id).await
    } else {
        store.has_active_subscription(tenant_id).await
    }
    .map_err(|_| TierSelectHttpError::Internal)?;
    if active {
        return Err(TierSelectHttpError::AlreadyActive);
    }

    // (5) Dispatch.
    if tier.is_paid() {
        // Runner is a separate billing axis. Its Stripe session is recorded
        // by runner_billing in the webhook and must never enter the cache-only
        // tier_selections table (or consume the cache payable slot).
        if !tier.is_runner() {
            audit
                .emit("stripe_checkout_reserved", tenant_id, correlation_id)
                .await
                .map_err(|_| TierSelectHttpError::Internal)?;
            store
                .reserve_pending_checkout(tenant_id, tier, now_ms, correlation_id)
                .await
                .map_err(|_| TierSelectHttpError::Internal)?;
        }
        // Stripe call ONLY after the DPA gate passed.
        let created = checkout
            .create(tenant_id, tier, success_url, cancel_url)
            .await
            .map_err(|e| {
                // `checkout.create()` returns the real Stripe/transport error as a
                // String; without this it was discarded (`|_|`) and every failure
                // collapsed to an opaque `stripe_unavailable` 502 with no way to
                // tell an auth error (bad/rotated key) from a bad price id or a
                // transport fault. Log it (secret-free: `e` is Stripe's error
                // message, never the key) so prod checkout failures are diagnosable.
                tracing::error!(
                    tenant_id = %tenant_id,
                    correlation_id = %correlation_id,
                    stripe_error = %e,
                    "tier-select checkout create failed → 502 stripe_unavailable"
                );
                TierSelectHttpError::StripeUnavailable(Some(e))
            })?;
        // Defense-in-depth: never hand back a non-TLS Checkout URL.
        if !created.checkout_url.starts_with("https://") {
            return Err(TierSelectHttpError::StripeUnavailable(Some(
                "checkout url was not https".to_string(),
            )));
        }
        // Audit BEFORE the mirror mutation.
        audit
            .emit("stripe_checkout_session_created", tenant_id, correlation_id)
            .await
            .map_err(|_| TierSelectHttpError::Internal)?;
        // Runner is a SEPARATE entitlement axis (see `RequestedTier::is_runner`):
        // its subscription↔tenant mapping + entitlement are written by the
        // signup-worker Stripe webhook into `runner_billing` / `runners_entitlement`
        // (migrations 0087/0070), NOT the cache `tier_selections` /
        // `stripe_checkout_sessions` tables. Those are one-row-per-tenant with a
        // cache-only `tier` CHECK, so persisting a runner tier there would clobber
        // the tenant's cache tier and violate the CHECK. For a runner checkout we
        // therefore create the Checkout Session and DO NOT touch the cache tables;
        // the webhook is the single source of truth for the runner subscription.
        if !tier.is_runner() {
            store
                .persist_pending_checkout(tenant_id, tier, &created, now_ms, correlation_id)
                .await
                .map_err(|_| TierSelectHttpError::Internal)?;
        }
        let _ = store.release_lock(tenant_id, correlation_id).await;
        Ok(TierSelectResponse {
            checkout_url: Some(created.checkout_url),
            session_id: created.session_id,
        })
    } else {
        // Free — instant activation, no Stripe.
        audit
            .emit("tier_activated_free", tenant_id, correlation_id)
            .await
            .map_err(|_| TierSelectHttpError::Internal)?;
        store
            .persist_free_active(tenant_id, now_ms, correlation_id)
            .await
            .map_err(|_| TierSelectHttpError::Internal)?;
        let _ = store.release_lock(tenant_id, correlation_id).await;
        Ok(TierSelectResponse {
            checkout_url: None,
            session_id: format!("free_activation:{tenant_id}"),
        })
    }
}

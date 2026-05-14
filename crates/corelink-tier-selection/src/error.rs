//! `corelink-tier-selection` canonical error taxonomy.

use thiserror::Error;

/// Canonical `corelink-tier-selection` error taxonomy.
///
/// The `#[non_exhaustive]` marker reserves additive growth for
/// follow-on WIs (e.g. multi-currency / coupon support / tier
/// downgrade flow).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierError {
    /// INV-ONBOARD-DPA-FIRST HARD violation prevented: tenant
    /// attempted tier selection without DPA accepted. Maps to HTTP
    /// 451 `dpa_not_signed` per WI §6.4 + spec contract §5.3 R-S19-8.
    ///
    /// This error MUST be returned BEFORE any Stripe API call —
    /// fail-CLOSED per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    #[error("DPA acceptance required before tier selection (INV-ONBOARD-DPA-FIRST)")]
    DpaRequired,
    /// Enterprise tier direct Stripe Checkout bypass rejected at the
    /// backend per WI §6.6. Maps to HTTP 422 `use_inquiry_form`.
    #[error("Enterprise tier requires inquiry form (WI-S19-005); direct checkout rejected")]
    UseInquiryForm,
    /// Concurrent tier selection within the 60s lock window rejected
    /// per WI §6.4 (D1 row lock; `INSERT OR IGNORE` on
    /// `tier_selection_locks`). Caller MUST back off + retry.
    #[error("concurrent tier selection rejected (60s lock window)")]
    LockHeld,
    /// Subscription already active for this tenant per the UNIQUE
    /// partial index defense-in-depth. Maps to HTTP 409
    /// `subscription_already_active`.
    #[error("subscription already active for tenant")]
    AlreadyActive,
    /// Stripe Checkout Session creation failed (transport / 5xx /
    /// invalid input). Production wiring sets this when the real
    /// Stripe API call returns a non-2xx status.
    #[error("Stripe Checkout session creation failed: {0}")]
    Stripe(String),
    /// Stripe webhook signature verify failed (HMAC mismatch / parse
    /// error / replay outside 5-min window). Maps to HTTP 401.
    #[error("Stripe webhook signature invalid: {0}")]
    InvalidSignature(String),
    /// Stripe webhook duplicate event dedup hit. Idempotent: returns
    /// 200 to Stripe so it stops retrying.
    #[error("Stripe webhook event already processed: {0}")]
    DuplicateEvent(String),
    /// Audit-of-audit emit failed; the ledger path aborts per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern).
    #[error("tier-selection audit emit failed: {0}")]
    Audit(String),
    /// Internal invariant violation (e.g. `Mutex` poisoning, state
    /// corruption). Non-recoverable: caller MUST tear down the
    /// in-process mirror + reconstruct from the durable D1 mirror.
    #[error("internal tier-selection fault: {0}")]
    Internal(String),
}

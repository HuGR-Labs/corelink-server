//! Authoritative current-subscription read seam for Runners reconciliation.
//!
//! A Stripe webhook is an idempotent reconciliation trigger, not an ordered
//! subscription revision. The native adapter obtains this snapshot from the
//! provider before changing the Runners entitlement.

use std::fmt;

/// The provider's current view of one Stripe subscription.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CurrentSubscription {
    /// Stripe subscription identity.
    pub subscription_id: String,
    /// Current Stripe subscription status.
    pub status: String,
    /// Current price identifier for the subscription's sole Runners item.
    pub price_id: String,
}

impl CurrentSubscription {
    /// Construct a provider-authoritative subscription snapshot.
    #[must_use]
    pub fn new(
        subscription_id: impl Into<String>,
        status: impl Into<String>,
        price_id: impl Into<String>,
    ) -> Self {
        Self {
            subscription_id: subscription_id.into(),
            status: status.into(),
            price_id: price_id.into(),
        }
    }
}

/// Reads the current subscription state used to reconcile Runners entitlement.
///
/// Implementations must return the provider's current object, never a cached
/// webhook payload. Failures are transient because the materializer must not
/// substitute delivery order for state authority.
pub trait CurrentSubscriptionAuthority: fmt::Debug + Send + Sync {
    /// Return the current state for `subscription_id`.
    fn current_subscription(&self, subscription_id: &str) -> Result<CurrentSubscription, String>;
}

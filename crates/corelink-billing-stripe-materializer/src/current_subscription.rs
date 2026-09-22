//! Provider-authoritative subscription snapshot for Runners reconciliation.

use std::fmt;

/// The current provider view of one Stripe subscription.
///
/// `subscription_created_at_ms` is the replacement generation. It is never
/// inferred from a price, status, period boundary, or local receive time.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CurrentSubscription {
    /// Stripe subscription identity.
    pub subscription_id: String,
    /// Current provider status.
    pub status: String,
    /// Current price identity, used only to resolve the entitlement ladder.
    pub price_id: String,
    /// Provider creation time in milliseconds; orders replacement identities.
    pub subscription_created_at_ms: u64,
}

impl CurrentSubscription {
    /// Construct a complete provider snapshot.
    #[must_use]
    pub fn new(
        subscription_id: impl Into<String>,
        status: impl Into<String>,
        price_id: impl Into<String>,
        subscription_created_at_ms: u64,
    ) -> Self {
        Self {
            subscription_id: subscription_id.into(),
            status: status.into(),
            price_id: price_id.into(),
            subscription_created_at_ms,
        }
    }
}

/// Reads the current provider object before a Runners entitlement changes.
pub trait CurrentSubscriptionAuthority: fmt::Debug + Send + Sync {
    /// Return the current Stripe object for `subscription_id`.
    fn current_subscription(&self, subscription_id: &str) -> Result<CurrentSubscription, String>;

    /// Return the current customer subscription state associated with a webhook
    /// subscription. Implementations that can query Stripe's customer list must
    /// return the complete list; the default keeps existing single-object test
    /// authorities conservative.
    fn current_customer_subscriptions(
        &self,
        subscription_id: &str,
    ) -> Result<Vec<CurrentSubscription>, String> {
        Ok(vec![self.current_subscription(subscription_id)?])
    }
}

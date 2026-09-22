//! Authoritative current-subscription read seam for Runners reconciliation.
//!
//! A Stripe webhook is an idempotent reconciliation trigger, not an ordered
//! subscription revision. The native adapter obtains this snapshot from the
//! provider before changing the Runners entitlement.

use std::fmt;

fn legacy_authority_key() -> String {
    "legacy:00000000000000000000".to_owned()
}

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
    /// Monotonic comparison key derived from the provider-authoritative
    /// Stripe object. It is persisted with the entitlement fence and must not
    /// be replaced by webhook delivery time or process-local sequence.
    #[serde(default = "legacy_authority_key")]
    pub authority_key: String,
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
            authority_key: legacy_authority_key(),
        }
    }

    /// Construct a snapshot with an explicit provider-derived ordering key.
    #[must_use]
    pub fn with_authority_key(
        subscription_id: impl Into<String>,
        status: impl Into<String>,
        price_id: impl Into<String>,
        authority_key: impl Into<String>,
    ) -> Self {
        Self {
            subscription_id: subscription_id.into(),
            status: status.into(),
            price_id: price_id.into(),
            authority_key: authority_key.into(),
        }
    }

    /// Build the stable ordering key used by the native Stripe authority.
    /// `current_period_end` is a provider-owned revision boundary; the status
    /// rank makes a non-granting state win over a same-period grant on replay.
    #[must_use]
    pub fn stripe_authority_key(current_period_end: i64, status: &str, price_id: &str) -> String {
        let period = current_period_end.max(0);
        let rank = match status {
            "active" => 10,
            "trialing" => 20,
            "past_due" => 30,
            "unpaid" => 40,
            "incomplete" => 50,
            "incomplete_expired" => 60,
            "paused" => 70,
            "canceled" => 80,
            _ => 90,
        };
        format!("stripe:{period:020}:{rank:03}:{price_id}")
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

//! Native Stripe adapter for authoritative subscription reconciliation.
//!
//! Webhook payloads are delivery notifications. Runner entitlement changes
//! use this adapter to read the current Stripe object before writing state.
//! The response mapper is deliberately strict: a missing collection, missing
//! price, or multiple items is not enough evidence to mutate entitlement.

use std::sync::Arc;

use corelink_billing_stripe_materializer::{CurrentSubscription, CurrentSubscriptionAuthority};
use corelink_stripe_real::{client::SubscriptionObject, StripeRealClient};

/// Native provider-backed [`CurrentSubscriptionAuthority`].
#[derive(Clone, Debug)]
pub struct StripeCurrentSubscriptionAuthority {
    stripe: Arc<StripeRealClient>,
}

impl StripeCurrentSubscriptionAuthority {
    /// Construct an authority over a shared native Stripe client.
    #[must_use]
    pub fn new(stripe: Arc<StripeRealClient>) -> Self {
        Self { stripe }
    }

    /// Map the provider response into the materializer's strict snapshot.
    ///
    /// The requested id is checked by [`CurrentSubscriptionAuthority::current_subscription`]
    /// after the network read, keeping this pure mapper useful for fixtures.
    pub fn map_subscription(response: &SubscriptionObject) -> Result<CurrentSubscription, String> {
        if response.id.trim().is_empty() {
            return Err("Stripe subscription response has an empty id".to_owned());
        }
        if response.status.trim().is_empty() {
            return Err(format!(
                "Stripe subscription {} has an empty status",
                response.id
            ));
        }
        let items = response
            .items
            .as_ref()
            .ok_or_else(|| format!("Stripe subscription {} has no items", response.id))?;
        let [item] = items.data.as_slice() else {
            return Err(format!(
                "Stripe subscription {} must have exactly one current item (found {})",
                response.id,
                items.data.len()
            ));
        };
        let price = item
            .price
            .as_ref()
            .ok_or_else(|| format!("Stripe subscription {} item has no price", response.id))?;
        if price.id.trim().is_empty() {
            return Err(format!(
                "Stripe subscription {} item has an empty price id",
                response.id
            ));
        }
        let created = response.created.ok_or_else(|| {
            format!(
                "Stripe subscription {} has no provider creation time",
                response.id
            )
        })?;
        let created_ms = u64::try_from(created)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1_000))
            .ok_or_else(|| {
                format!(
                    "Stripe subscription {} has invalid creation time",
                    response.id
                )
            })?;
        Ok(CurrentSubscription::new(
            response.id.clone(),
            response.status.clone(),
            price.id.clone(),
            created_ms,
        ))
    }
}

impl CurrentSubscriptionAuthority for StripeCurrentSubscriptionAuthority {
    fn current_subscription(&self, subscription_id: &str) -> Result<CurrentSubscription, String> {
        let response = self
            .stripe
            .get_subscription(subscription_id)
            .map_err(|error| {
                format!("Stripe get_subscription({subscription_id}) failed: {error}")
            })?;
        let current = Self::map_subscription(&response)?;
        if current.subscription_id != subscription_id {
            return Err(format!(
                "Stripe returned subscription {} for requested {subscription_id}",
                current.subscription_id
            ));
        }
        Ok(current)
    }

    fn current_customer_subscriptions(
        &self,
        subscription_id: &str,
    ) -> Result<Vec<CurrentSubscription>, String> {
        let requested = self
            .stripe
            .get_subscription(subscription_id)
            .map_err(|error| {
                format!("Stripe get_subscription({subscription_id}) failed: {error}")
            })?;
        let customer_id = requested.customer.trim();
        if customer_id.is_empty() {
            return Err(format!(
                "Stripe subscription {subscription_id} has an empty customer"
            ));
        }
        let listed = self
            .stripe
            .list_customer_subscriptions(customer_id)
            .map_err(|error| {
                format!("Stripe list subscriptions for {customer_id} failed: {error}")
            })?;
        if listed.has_more {
            return Err(format!(
                "Stripe customer {customer_id} subscription list is truncated"
            ));
        }
        let snapshots = listed
            .data
            .iter()
            .map(Self::map_subscription)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(snapshots)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "fixtures deserialize static JSON and unwrap only in test assertions"
)]
mod tests {
    use super::*;
    fn response(items: serde_json::Value) -> SubscriptionObject {
        serde_json::from_value(serde_json::json!({
            "id": "sub_current",
            "status": "active",
            "customer": "cus_1",
            "created": 1800000000,
            "items": { "data": items },
        }))
        .unwrap()
    }

    fn item(price: Option<&str>) -> serde_json::Value {
        serde_json::json!({ "price": price.map(|id| serde_json::json!({ "id": id })) })
    }

    #[test]
    fn maps_current_id_status_and_item_price() {
        let current =
            StripeCurrentSubscriptionAuthority::map_subscription(&response(serde_json::json!([
                item(Some("price_runner_team"))
            ])))
            .unwrap();
        assert_eq!(
            current,
            CurrentSubscription::new(
                "sub_current",
                "active",
                "price_runner_team",
                1_800_000_000_000,
            )
        );
    }

    #[test]
    fn missing_or_multiple_items_fail_closed() {
        for items in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!([item(Some("price_a")), item(Some("price_b"))]),
        ] {
            let value = if items.is_null() {
                serde_json::from_value(serde_json::json!({
                    "id": "sub_current", "status": "active", "customer": "cus_1"
                }))
                .unwrap()
            } else {
                response(items)
            };
            assert!(StripeCurrentSubscriptionAuthority::map_subscription(&value).is_err());
        }
    }

    #[test]
    fn missing_price_fails_closed_even_for_non_runner_item() {
        let value = response(serde_json::json!([item(None)]));
        assert!(StripeCurrentSubscriptionAuthority::map_subscription(&value).is_err());
    }
}

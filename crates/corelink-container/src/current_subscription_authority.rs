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
        let current_period_end = response.current_period_end.ok_or_else(|| {
            format!(
                "Stripe subscription {} has no current_period_end authority key",
                response.id
            )
        })?;
        Ok(CurrentSubscription::with_authority_key(
            response.id.clone(),
            response.status.clone(),
            price.id.clone(),
            CurrentSubscription::stripe_authority_key(
                current_period_end,
                &response.status,
                &price.id,
            ),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(items: serde_json::Value) -> SubscriptionObject {
        serde_json::from_value(serde_json::json!({
            "id": "sub_current",
            "status": "active",
            "customer": "cus_1",
            "current_period_end": 1800000000,
            "items": items,
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
            CurrentSubscription::with_authority_key(
                "sub_current",
                "active",
                "price_runner_team",
                CurrentSubscription::stripe_authority_key(
                    1_800_000_000,
                    "active",
                    "price_runner_team",
                ),
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

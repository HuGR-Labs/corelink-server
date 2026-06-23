//! Runners-tier entitlement resolution (Stripe price → `runners_entitlement`).
//!
//! The Runners product is a SEPARATE entitlement axis from the cache tier
//! ([`crate::tier`]): buying a Runners SKU grants a per-tenant parallel-runner
//! concurrency cap (`max_concurrency`) plus a monthly compute ceiling
//! (`max_vcpu_h`), written to the `runners_entitlement` D1 table (migrations
//! 0070/0072) — NOT to `tier_selections`. A cache-tier purchase never touches
//! runner concurrency and vice-versa (Option-B decoupling, owner-ratified).
//!
//! Pricing (owner-ratified 2026-06-16, `corelink-runners docs/product/pricing.md
//! §2`; loss-proof at the real ~$0.10/vCPU-h Cloudflare-Containers basis):
//!
//! | Tier    | $/mo | concurrency | vCPU-h/mo |
//! |---------|------|-------------|-----------|
//! | Starter | $16  | 20          | 100       |
//! | Pro     | $40  | 40          | 240       |
//! | Team    | $100 | 80          | 600       |
//! | Scale   | $200 | 160         | 1,200     |
//! | Max     | $400 | 320         | 2,400     |
//!
//! The mapping is keyed on the live Stripe price id (`STRIPE_PRICE_ID_RUNNER_*`
//! env), so it is empty (→ every resolve is `None` → the materializer falls back
//! to the cache-tier path) until the operator creates the Runners Prices and sets
//! those secrets at launch — exactly like the cache `STRIPE_PRICE_ID_*` gating.

use std::collections::HashMap;
use std::fmt;

/// A per-tenant Runners entitlement: the two axes the `runners_entitlement` row
/// carries (`max_concurrency` reject-if-absent, `max_vcpu_h` wall-off-if-absent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnersEntitlement {
    /// Max parallel runners (instantaneous concurrency cap). Always > 0.
    pub max_concurrency: u32,
    /// Monthly cumulative compute ceiling, in vCPU-hours.
    pub max_vcpu_h: u32,
}

/// Resolve a Stripe price/plan id to its Runners entitlement, or `None` when the
/// id is NOT a Runners-tier price (the caller then treats the subscription as a
/// cache-tier event). Abstracted as a trait so the materializer is testable
/// without env wiring.
pub trait RunnersEntitlementResolver: fmt::Debug + Send + Sync {
    /// `Some(entitlement)` iff `plan_id` is a known Runners-tier price.
    fn resolve(&self, plan_id: &str) -> Option<RunnersEntitlement>;
}

/// In-memory map `price_id → RunnersEntitlement`, built from the
/// `STRIPE_PRICE_ID_RUNNER_*` env at startup. Empty map ⇒ every resolve is
/// `None` (Runners seeding is dormant until the Prices exist).
#[derive(Debug, Default)]
pub struct InMemoryRunnersEntitlementResolver {
    by_price: HashMap<String, RunnersEntitlement>,
}

impl InMemoryRunnersEntitlementResolver {
    /// Construct an empty resolver (no Runners prices wired).
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_price: HashMap::new(),
        }
    }

    /// Register a `price_id → entitlement` mapping. An empty/blank `price_id`
    /// is ignored (an unset `STRIPE_PRICE_ID_RUNNER_*` env reads as ""), so the
    /// tier stays dormant rather than mapping the empty string.
    #[must_use]
    pub fn with_price(mut self, price_id: &str, ent: RunnersEntitlement) -> Self {
        let p = price_id.trim();
        if !p.is_empty() {
            self.by_price.insert(p.to_string(), ent);
        }
        self
    }

    /// Number of wired Runners prices (0 ⇒ dormant).
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_price.len()
    }

    /// `true` when no Runners prices are wired (the resolve is always `None`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_price.is_empty()
    }
}

impl RunnersEntitlementResolver for InMemoryRunnersEntitlementResolver {
    fn resolve(&self, plan_id: &str) -> Option<RunnersEntitlement> {
        self.by_price.get(plan_id).copied()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn empty_resolver_resolves_nothing() {
        let r = InMemoryRunnersEntitlementResolver::new();
        assert!(r.is_empty());
        assert_eq!(r.resolve("price_live_runner_starter"), None);
    }

    #[test]
    fn maps_known_price_to_entitlement() {
        let r = InMemoryRunnersEntitlementResolver::new()
            .with_price(
                "price_starter",
                RunnersEntitlement {
                    max_concurrency: 20,
                    max_vcpu_h: 100,
                },
            )
            .with_price(
                "price_max",
                RunnersEntitlement {
                    max_concurrency: 320,
                    max_vcpu_h: 2400,
                },
            );
        assert_eq!(r.len(), 2);
        assert_eq!(
            r.resolve("price_starter"),
            Some(RunnersEntitlement {
                max_concurrency: 20,
                max_vcpu_h: 100
            })
        );
        assert_eq!(
            r.resolve("price_max"),
            Some(RunnersEntitlement {
                max_concurrency: 320,
                max_vcpu_h: 2400
            })
        );
        // A cache-tier price (not a Runners price) resolves to None.
        assert_eq!(r.resolve("price_cache_pro"), None);
    }

    #[test]
    fn blank_price_id_is_ignored() {
        // An unset STRIPE_PRICE_ID_RUNNER_* env reads as "" — must not map.
        let r = InMemoryRunnersEntitlementResolver::new().with_price(
            "",
            RunnersEntitlement {
                max_concurrency: 20,
                max_vcpu_h: 100,
            },
        );
        assert!(r.is_empty());
        assert_eq!(r.resolve(""), None);
    }
}

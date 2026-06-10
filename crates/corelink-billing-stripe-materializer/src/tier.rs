//! Tier-selection bridge.
//!
//! On `customer.subscription.updated`, the materializer re-computes
//! the canonical tier from `(plan_id, seat_count)` and, if the result
//! differs from the persisted `tier_selections.tier`, writes the new
//! tier + emits `corelink.tenant.tier_changed.v1` audit.
//!
//! The mapping table is **explicit** (no implicit fallback): unknown
//! plan ids surface as [`TierSelectError::UnknownPlan`] so the
//! materializer returns `MaterializerError::InvalidPayload` (HTTP 422
//! — Stripe stops retrying, operator gets paged via the dashboard).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use corelink_tier_selection::tier::TierKind;

/// Error category surfaced by [`TierSelector`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierSelectError {
    /// Plan id not in the canonical mapping. Surfaces as 422.
    UnknownPlan(String),
    /// Transient backend error (mutex poisoned, etc.).
    Transient(String),
}

impl fmt::Display for TierSelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPlan(s) => write!(f, "tier-select unknown plan: {s}"),
            Self::Transient(s) => write!(f, "tier-select transient: {s}"),
        }
    }
}

impl std::error::Error for TierSelectError {}

/// Canonical Stripe plan-id → CoreLink tier mapping.
///
/// The mapping is opaque to the materializer (driven by config so the
/// production binder can flip plan ids without code change). Tests
/// inject a fixed mapping via [`InMemoryTierSelector::with_mapping`].
pub trait TierSelector: fmt::Debug + Send + Sync {
    /// Compute the canonical tier for `(plan_id, seat_count)`.
    fn compute_tier(&self, plan_id: &str, seat_count: u64) -> Result<TierKind, TierSelectError>;
}

/// Native in-memory mirror.
#[derive(Clone, Debug, Default)]
pub struct InMemoryTierSelector {
    /// Plan id → tier mapping (rebuilt on every `with_mapping`).
    plan_to_tier: Arc<Mutex<HashMap<String, TierKind>>>,
}

impl InMemoryTierSelector {
    /// Construct an empty selector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the plan-id → tier mapping with `pairs`.
    #[must_use]
    pub fn with_mapping(pairs: &[(&str, TierKind)]) -> Self {
        let me = Self::new();
        for (plan, tier) in pairs {
            me.register(plan, *tier);
        }
        me
    }

    /// Register a single `(plan_id, tier)` pair.
    pub fn register(&self, plan_id: &str, tier: TierKind) {
        if let Ok(mut g) = self.plan_to_tier.lock() {
            g.insert(plan_id.to_string(), tier);
        }
    }
}

impl TierSelector for InMemoryTierSelector {
    fn compute_tier(&self, plan_id: &str, _seat_count: u64) -> Result<TierKind, TierSelectError> {
        let g = self
            .plan_to_tier
            .lock()
            .map_err(|e| TierSelectError::Transient(format!("mutex poisoned: {e}")))?;
        g.get(plan_id)
            .copied()
            .ok_or_else(|| TierSelectError::UnknownPlan(plan_id.to_string()))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn known_plan_resolves_to_tier() {
        let sel = InMemoryTierSelector::with_mapping(&[
            ("plan_solo", TierKind::Solo),
            ("plan_starter", TierKind::Starter),
            ("plan_pro", TierKind::Pro),
            ("plan_max", TierKind::Max),
        ]);
        assert_eq!(sel.compute_tier("plan_pro", 5).unwrap(), TierKind::Pro);
        assert_eq!(sel.compute_tier("plan_solo", 1).unwrap(), TierKind::Solo);
        assert_eq!(sel.compute_tier("plan_max", 1).unwrap(), TierKind::Max);
    }

    #[test]
    fn unknown_plan_errors() {
        let sel = InMemoryTierSelector::new();
        let err = sel.compute_tier("plan_x", 1).unwrap_err();
        assert!(matches!(err, TierSelectError::UnknownPlan(_)));
    }
}

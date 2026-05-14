//! Tier taxonomy + escalation discipline.

/// Canonical 3-tier escalation taxonomy per WI §6.1.1 (Tier 1
/// primary responder + Tier 2 escalate + Tier 3 architect / security
/// deep escalation). The `#[non_exhaustive]` marker reserves additive
/// growth for follow-on WIs (e.g. dedicated security oncall lane).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Tier {
    /// Tier 1 — primary responder (oncall executes).
    #[default]
    Tier1,
    /// Tier 2 — escalate (covers Tier 1 unack after 5min).
    Tier2,
    /// Tier 3 — architect / security if needed (10min unack).
    Tier3,
}

impl Tier {
    /// Canonical slug per PagerDuty schedule name suffix.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tier1 => "tier-1",
            Self::Tier2 => "tier-2",
            Self::Tier3 => "tier-3",
        }
    }

    /// Escalation hold-down (seconds) before paging the next tier per
    /// WI §6.1.1 + §1 escalation_policies (5min Tier1→Tier2; 10min
    /// Tier2→Tier3; Tier3 has no further escalation arm).
    #[must_use]
    pub const fn escalation_delay_seconds(self) -> Option<u64> {
        match self {
            Self::Tier1 => Some(300),
            Self::Tier2 => Some(600),
            Self::Tier3 => None,
        }
    }
}

impl core::fmt::Display for Tier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element list of [`Tier`] used for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_tiers() -> &'static [Tier; 3] {
    &[Tier::Tier1, Tier::Tier2, Tier::Tier3]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn tier_canonical_slugs() {
        assert_eq!(Tier::Tier1.as_str(), "tier-1");
        assert_eq!(Tier::Tier2.as_str(), "tier-2");
        assert_eq!(Tier::Tier3.as_str(), "tier-3");
    }

    #[test]
    fn escalation_delays() {
        assert_eq!(Tier::Tier1.escalation_delay_seconds(), Some(300));
        assert_eq!(Tier::Tier2.escalation_delay_seconds(), Some(600));
        assert_eq!(Tier::Tier3.escalation_delay_seconds(), None);
    }

    #[test]
    fn canonical_tiers_surface_stable() {
        let tiers = canonical_tiers();
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0], Tier::Tier1);
        assert_eq!(tiers[1], Tier::Tier2);
        assert_eq!(tiers[2], Tier::Tier3);
    }
}

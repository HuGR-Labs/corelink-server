//! Canonical 6-tier taxonomy per WI-S19-004 §6.1.

/// 6 tiers canonical (per spec contract §5.3 R-S19-7).
///
/// - [`Self::Free`] — instant activation; no Stripe Checkout; rate-limited.
/// - [`Self::Solo`] — paid; Stripe Checkout redirect.
/// - [`Self::Starter`] — paid; Stripe Checkout redirect.
/// - [`Self::Pro`] — paid; Stripe Checkout redirect.
/// - [`Self::Max`] — paid; Stripe Checkout redirect.
/// - [`Self::Enterprise`] — `"Contact us"`; routes to inquiry form
///   (WI-S19-005); direct Stripe Checkout REJECTED with
///   [`crate::error::TierError::UseInquiryForm`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TierKind {
    /// Free tier — instant activation. STILL requires DPA per WI §6.5
    /// (no exception; DPA is contract-wide).
    Free,
    /// Solo tier — paid; Stripe Checkout redirect.
    Solo,
    /// Starter tier — paid; Stripe Checkout redirect.
    Starter,
    /// Pro tier — paid; Stripe Checkout redirect.
    Pro,
    /// Max tier — paid; Stripe Checkout redirect.
    Max,
    /// Enterprise tier — `"Contact us"` form route (WI-S19-005).
    Enterprise,
    /// Runner Starter tier — paid; Stripe Checkout redirect (runner SKU).
    RunnerStarter,
    /// Runner Pro tier — paid; Stripe Checkout redirect (runner SKU).
    RunnerPro,
    /// Runner Team tier — paid; Stripe Checkout redirect (runner SKU).
    RunnerTeam,
    /// Runner Scale tier — paid; Stripe Checkout redirect (runner SKU).
    RunnerScale,
    /// Runner Max tier — paid; Stripe Checkout redirect (runner SKU).
    RunnerMax,
}

impl TierKind {
    /// Canonical snake_case wire string per métricas `{tier}` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Solo => "solo",
            Self::Starter => "starter",
            Self::Pro => "pro",
            Self::Max => "max",
            Self::Enterprise => "enterprise",
            Self::RunnerStarter => "runner_starter",
            Self::RunnerPro => "runner_pro",
            Self::RunnerTeam => "runner_team",
            Self::RunnerScale => "runner_scale",
            Self::RunnerMax => "runner_max",
        }
    }

    /// Returns true if this tier requires Stripe Checkout redirect.
    ///
    /// Free skips Stripe (instant activation); Enterprise routes to
    /// the inquiry form (not Stripe Checkout); paid tiers
    /// (Solo / Starter / Pro / Max and the Runner SKUs) require Stripe
    /// Checkout.
    #[must_use]
    pub const fn requires_stripe_checkout(self) -> bool {
        matches!(
            self,
            Self::Solo
                | Self::Starter
                | Self::Pro
                | Self::Max
                | Self::RunnerStarter
                | Self::RunnerPro
                | Self::RunnerTeam
                | Self::RunnerScale
                | Self::RunnerMax
        )
    }

    /// Returns true if this tier routes to the inquiry form
    /// (WI-S19-005) instead of Stripe Checkout.
    #[must_use]
    pub const fn routes_to_inquiry_form(self) -> bool {
        matches!(self, Self::Enterprise)
    }
}

impl core::fmt::Display for TierKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 11-element list for surface-stability regression tests.
#[must_use]
pub const fn canonical_tiers() -> &'static [TierKind; 11] {
    &[
        TierKind::Free,
        TierKind::Solo,
        TierKind::Starter,
        TierKind::Pro,
        TierKind::Max,
        TierKind::Enterprise,
        TierKind::RunnerStarter,
        TierKind::RunnerPro,
        TierKind::RunnerTeam,
        TierKind::RunnerScale,
        TierKind::RunnerMax,
    ]
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
    fn free_does_not_require_stripe() {
        assert!(!TierKind::Free.requires_stripe_checkout());
    }

    #[test]
    fn enterprise_routes_to_inquiry_form() {
        assert!(TierKind::Enterprise.routes_to_inquiry_form());
        assert!(!TierKind::Enterprise.requires_stripe_checkout());
    }

    #[test]
    fn paid_tiers_require_stripe() {
        for t in [
            TierKind::Solo,
            TierKind::Starter,
            TierKind::Pro,
            TierKind::Max,
            TierKind::RunnerStarter,
            TierKind::RunnerPro,
            TierKind::RunnerTeam,
            TierKind::RunnerScale,
            TierKind::RunnerMax,
        ] {
            assert!(t.requires_stripe_checkout(), "{t:?} should require stripe");
            assert!(!t.routes_to_inquiry_form());
        }
    }

    #[test]
    fn canonical_strings_stable() {
        let strs: Vec<_> = canonical_tiers().iter().map(|t| t.as_str()).collect();
        assert_eq!(
            strs,
            [
                "free",
                "solo",
                "starter",
                "pro",
                "max",
                "enterprise",
                "runner_starter",
                "runner_pro",
                "runner_team",
                "runner_scale",
                "runner_max",
            ]
        );
    }
}

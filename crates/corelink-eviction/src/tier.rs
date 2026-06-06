//! Per-tier TTL resolver — canonical defaults per ADR-0019 (TTL
//! ownership) + admin-override hard cap (Lote 10.7-tris cycle 3 fix).
//!
//! ## Canonical TTL ladder (per WI-S07-002 §6.1.3 + ADR-0019)
//!
//! | Tier       | Default TTL | Admin override cap |
//! |------------|-------------|--------------------|
//! | Free       | 7 d         | n/a                |
//! | Solo       | 30 d        | n/a                |
//! | Team       | 90 d        | n/a                |
//! | Business   | 365 d       | n/a                |
//! | Enterprise | 365 d       | 730 d              |
//!
//! Enterprise default is 365d (Lote 10.7bis P0-7 fix; was incorrectly
//! 730d before — 730d is the override cap, NOT the default). Enterprise
//! customers can request a shorter OR longer TTL via S-13 admin API
//! (out-of-scope for WI-S07-002), but the cap of 730d is hard-enforced
//! at config validation time per CAP-EVICT-002 boundary.
//!
//! ## INV-EVICT-TTL-CAP-RESPECTED (MEDIUM)
//!
//! Enterprise TTL override > 730d MUST be rejected. The
//! [`ttl_for_tier_with_override`] surface returns
//! [`TierTtlOverrideError::ExceedsMaxTtl`] when the requested override
//! exceeds the cap. Pinned by `prop_ttl_enterprise_cap_respected`.

use thiserror::Error;

/// Free-tier TTL (days).
pub const FREE_TTL_DAYS: u32 = 7;
/// Solo-tier TTL (days).
pub const SOLO_TTL_DAYS: u32 = 30;
/// Team-tier TTL (days).
pub const TEAM_TTL_DAYS: u32 = 90;
/// Business-tier TTL (days).
pub const BUSINESS_TTL_DAYS: u32 = 365;
/// Enterprise default TTL (days; ADR-0019 §Decision; Lote 10.7bis P0-7
/// canonical — was incorrectly 730d).
pub const ENTERPRISE_TTL_DAYS_DEFAULT: u32 = 365;
/// Enterprise admin-override hard cap (days; CAP-EVICT-002 boundary).
pub const MAX_ENTERPRISE_TTL_DAYS: u32 = 730;

/// Convert days to milliseconds (saturating).
const fn days_to_ms(days: u32) -> u64 {
    (days as u64).saturating_mul(86_400_000)
}

/// Canonical 5-tier domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Tier {
    /// Free tier — TTL 7d.
    Free,
    /// Solo tier — TTL 30d.
    Solo,
    /// Team tier — TTL 90d.
    Team,
    /// Business tier — TTL 365d.
    Business,
    /// Enterprise tier — TTL 365d default (admin override cap 730d).
    Enterprise,
}

impl Tier {
    /// Canonical lower-snake-case mnemonic for metric labels +
    /// audit records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Solo => "solo",
            Self::Team => "team",
            Self::Business => "business",
            Self::Enterprise => "enterprise",
        }
    }
}

impl core::fmt::Display for Tier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve the canonical default TTL (ms) for a tier per ADR-0019.
#[must_use]
pub const fn ttl_for_tier(tier: Tier) -> u64 {
    match tier {
        Tier::Free => days_to_ms(FREE_TTL_DAYS),
        Tier::Solo => days_to_ms(SOLO_TTL_DAYS),
        Tier::Team => days_to_ms(TEAM_TTL_DAYS),
        Tier::Business => days_to_ms(BUSINESS_TTL_DAYS),
        Tier::Enterprise => days_to_ms(ENTERPRISE_TTL_DAYS_DEFAULT),
    }
}

/// Errors surfaced by [`ttl_for_tier_with_override`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierTtlOverrideError {
    /// Admin override is only legal on the Enterprise tier — non-
    /// Enterprise tiers MUST take the canonical default per
    /// ADR-0019 §Decision (no per-tenant override on lower tiers).
    #[error("ttl override is only permitted on the Enterprise tier (got {tier:?})")]
    OverrideNotAllowedForTier {
        /// The non-Enterprise tier that attempted to apply an override.
        tier: Tier,
    },
    /// Requested TTL exceeds the canonical 730d cap.
    /// INV-EVICT-TTL-CAP-RESPECTED enforcement point.
    #[error("ttl override {requested_days}d exceeds Enterprise hard cap {max_days}d")]
    ExceedsMaxTtl {
        /// Requested override (days).
        requested_days: u32,
        /// Hard cap (canonical 730d).
        max_days: u32,
    },
}

/// Resolve the TTL (ms) honouring an optional Enterprise admin
/// override.
///
/// - When `override_days` is `None`, returns [`ttl_for_tier`] (the
///   canonical default).
/// - When `override_days` is `Some(days)`:
///   - The tier MUST be `Enterprise` — non-Enterprise tiers reject
///     with [`TierTtlOverrideError::OverrideNotAllowedForTier`].
///   - The override MUST be `<= MAX_ENTERPRISE_TTL_DAYS` (730) —
///     larger values reject with [`TierTtlOverrideError::ExceedsMaxTtl`].
///   - On success, the override TTL (ms) is returned.
///
/// # Errors
///
/// See [`TierTtlOverrideError`] variants.
pub fn ttl_for_tier_with_override(
    tier: Tier,
    override_days: Option<u32>,
) -> Result<u64, TierTtlOverrideError> {
    match override_days {
        None => Ok(ttl_for_tier(tier)),
        Some(d) => {
            if tier != Tier::Enterprise {
                return Err(TierTtlOverrideError::OverrideNotAllowedForTier { tier });
            }
            if d > MAX_ENTERPRISE_TTL_DAYS {
                return Err(TierTtlOverrideError::ExceedsMaxTtl {
                    requested_days: d,
                    max_days: MAX_ENTERPRISE_TTL_DAYS,
                });
            }
            Ok(days_to_ms(d))
        }
    }
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
    fn canonical_ttl_constants_pinned() {
        assert_eq!(FREE_TTL_DAYS, 7);
        assert_eq!(SOLO_TTL_DAYS, 30);
        assert_eq!(TEAM_TTL_DAYS, 90);
        assert_eq!(BUSINESS_TTL_DAYS, 365);
        // Lote 10.7bis P0-7: canonical Enterprise DEFAULT is 365d
        // (NOT 730d; 730d is the admin override cap).
        assert_eq!(ENTERPRISE_TTL_DAYS_DEFAULT, 365);
        assert_eq!(MAX_ENTERPRISE_TTL_DAYS, 730);
    }

    #[test]
    fn ttl_for_tier_free_is_seven_days_ms() {
        assert_eq!(ttl_for_tier(Tier::Free), 7 * 86_400_000);
    }

    #[test]
    fn ttl_for_tier_enterprise_default_is_365_days_ms() {
        assert_eq!(ttl_for_tier(Tier::Enterprise), 365 * 86_400_000);
    }

    #[test]
    fn ttl_ladder_monotonic() {
        let v = [
            ttl_for_tier(Tier::Free),
            ttl_for_tier(Tier::Solo),
            ttl_for_tier(Tier::Team),
            ttl_for_tier(Tier::Business),
            ttl_for_tier(Tier::Enterprise),
        ];
        // Each tier must be >= predecessor (Enterprise default ==
        // Business default; both 365d per ADR-0019).
        for w in v.windows(2) {
            if let [a, b] = w {
                assert!(b >= a, "TTL ladder regressed: {a} -> {b}");
            }
        }
    }

    #[test]
    fn override_none_returns_canonical_default() {
        for tier in [
            Tier::Free,
            Tier::Solo,
            Tier::Team,
            Tier::Business,
            Tier::Enterprise,
        ] {
            assert_eq!(
                ttl_for_tier_with_override(tier, None).unwrap(),
                ttl_for_tier(tier)
            );
        }
    }

    #[test]
    fn override_below_cap_accepted_for_enterprise() {
        let v = ttl_for_tier_with_override(Tier::Enterprise, Some(500)).unwrap();
        assert_eq!(v, 500 * 86_400_000);
    }

    #[test]
    fn override_exactly_at_cap_accepted_for_enterprise() {
        let v =
            ttl_for_tier_with_override(Tier::Enterprise, Some(MAX_ENTERPRISE_TTL_DAYS)).unwrap();
        assert_eq!(v, days_to_ms(MAX_ENTERPRISE_TTL_DAYS));
    }

    #[test]
    fn override_above_cap_rejected_for_enterprise() {
        let err = ttl_for_tier_with_override(Tier::Enterprise, Some(MAX_ENTERPRISE_TTL_DAYS + 1))
            .unwrap_err();
        match err {
            TierTtlOverrideError::ExceedsMaxTtl {
                requested_days,
                max_days,
            } => {
                assert_eq!(requested_days, MAX_ENTERPRISE_TTL_DAYS + 1);
                assert_eq!(max_days, MAX_ENTERPRISE_TTL_DAYS);
            }
            other => panic!("expected ExceedsMaxTtl, got {other:?}"),
        }
    }

    #[test]
    fn override_rejected_for_non_enterprise() {
        for tier in [Tier::Free, Tier::Solo, Tier::Team, Tier::Business] {
            let err = ttl_for_tier_with_override(tier, Some(10)).unwrap_err();
            assert!(matches!(
                err,
                TierTtlOverrideError::OverrideNotAllowedForTier { .. }
            ));
        }
    }

    #[test]
    fn tier_as_str_canonical() {
        assert_eq!(Tier::Free.as_str(), "free");
        assert_eq!(Tier::Solo.as_str(), "solo");
        assert_eq!(Tier::Team.as_str(), "team");
        assert_eq!(Tier::Business.as_str(), "business");
        assert_eq!(Tier::Enterprise.as_str(), "enterprise");
    }
}

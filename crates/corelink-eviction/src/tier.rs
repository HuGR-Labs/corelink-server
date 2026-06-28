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

impl Tier {
    /// Resolve a tier from a canonical billing/slug string (strict).
    ///
    /// CF-3 fix: the sold taxonomy (ADR-S19-001) is the **6-tier**
    /// `{free, solo, starter, pro, max, enterprise}` and the live
    /// `tier_selections.tier` strings are `"starter"/"pro"/"max"`, but the
    /// eviction `Tier` domain is **5-arm**. Without this resolver the paid
    /// slugs had no path into [`Tier`], so an eviction-side string→Tier
    /// lookup would drop a paying Pro/Max tenant onto the free-tier TTL.
    ///
    /// This mirrors the ratified canonical mapping
    /// `corelink_ratelimit::tier_for_billing_label` (Q5/Q5a,
    /// `PROPOSAL-2026-06-10-ADMIN-PILOT-TENANT-RATE-MAPPING` §3.3/§3.4.1) so
    /// the eviction TTL ladder agrees with rate-limiting and the published
    /// rate card (the retention promise, §3.5 Q5c):
    ///
    /// | slug                       | → `Tier`     | eviction TTL |
    /// |----------------------------|--------------|--------------|
    /// | `free`                     | `Free`       | 7 d          |
    /// | `solo`                     | `Solo`       | 30 d         |
    /// | `starter` / `team`         | `Team`       | 90 d         |
    /// | `pro` / `org` / `max`      | `Business`   | 365 d        |
    /// | `enterprise`               | `Enterprise` | 365 d        |
    ///
    /// `max` maps to `Business` (NOT `Enterprise`) per the ratified Q5a
    /// decision — this is a deliberate product choice carried over verbatim.
    ///
    /// Unknown slugs return [`UnknownTierSlug`]. Callers on the eviction
    /// hot-path that need an infallible resolution MUST use
    /// [`Tier::from_slug_paid_safe`] (non-free fallback) so a future or
    /// mistyped slug can never silently mis-price a payer to the free TTL.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownTierSlug`] when `label` is not a recognised slug.
    pub fn from_slug(label: &str) -> Result<Self, UnknownTierSlug> {
        match label {
            "free" => Ok(Self::Free),
            "solo" => Ok(Self::Solo),
            "starter" | "team" => Ok(Self::Team),
            "pro" | "org" | "max" => Ok(Self::Business),
            "enterprise" => Ok(Self::Enterprise),
            other => Err(UnknownTierSlug(other.to_owned())),
        }
    }

    /// Infallible production slug→`Tier` resolver with a **non-free**
    /// default (CF-3 safety floor).
    ///
    /// Identical to [`Tier::from_slug`] for every recognised slug; any
    /// unrecognised slug falls back to `Team` (90 d) — the same non-free
    /// fallback as `corelink_ratelimit::tier_for_billing_label`. The
    /// guarantee: **no slug ever resolves to the free-tier TTL floor unless
    /// it is literally `"free"`.**
    #[must_use]
    pub fn from_slug_paid_safe(label: &str) -> Self {
        Self::from_slug(label).unwrap_or(Self::Team)
    }
}

/// Error returned by [`Tier::from_slug`] / [`Tier::from_str`] /
/// [`TryFrom<&str>`] when the slug is not a recognised tier mnemonic.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("unknown tier slug: {0:?}")]
pub struct UnknownTierSlug(
    /// The unrecognised slug.
    pub String,
);

impl core::str::FromStr for Tier {
    type Err = UnknownTierSlug;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_slug(s)
    }
}

impl TryFrom<&str> for Tier {
    type Error = UnknownTierSlug;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_slug(s)
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

    /// CF-3 regression pin: every SOLD slug (ADR-S19-001 6-tier taxonomy)
    /// resolves to its intended eviction TTL, and NO paid slug lands on the
    /// free-tier floor. Mirrors `tier_for_billing_label` (Q5/Q5a).
    #[test]
    fn sold_slugs_map_to_intended_eviction_ttl() {
        let day_ms = 86_400_000_u64;
        // (slug, expected Tier, expected TTL days)
        let cases: [(&str, Tier, u64); 6] = [
            ("free", Tier::Free, 7),
            ("solo", Tier::Solo, 30),
            ("starter", Tier::Team, 90),
            ("pro", Tier::Business, 365),
            ("max", Tier::Business, 365), // Q5a: Business, NOT Enterprise.
            ("enterprise", Tier::Enterprise, 365),
        ];
        for (slug, want_tier, want_days) in cases {
            let tier = Tier::from_slug(slug).unwrap();
            assert_eq!(tier, want_tier, "slug {slug} resolved to wrong tier");
            assert_eq!(
                ttl_for_tier(tier),
                want_days * day_ms,
                "slug {slug} resolved to wrong eviction TTL"
            );
            // CF-3 core invariant: a paid slug must never collapse to the
            // free-tier TTL floor.
            if slug != "free" {
                assert_ne!(
                    ttl_for_tier(tier),
                    days_to_ms(FREE_TTL_DAYS),
                    "PAID slug {slug} silently dropped to the free TTL floor"
                );
            }
        }
    }

    #[test]
    fn legacy_d1_slugs_mirror_canonical_mapping() {
        // D1 legacy aliases per Q5: team→Team, org→Business.
        assert_eq!(Tier::from_slug("team").unwrap(), Tier::Team);
        assert_eq!(Tier::from_slug("org").unwrap(), Tier::Business);
    }

    #[test]
    fn unknown_slug_is_strict_error_but_paid_safe_falls_to_non_free() {
        // Strict resolver rejects unknown slugs.
        let err = Tier::from_slug("platinum").unwrap_err();
        assert_eq!(err, UnknownTierSlug("platinum".to_owned()));
        // Infallible resolver never drops an unknown (possibly paid/future)
        // slug to the free floor — it lands on Team (non-free).
        let t = Tier::from_slug_paid_safe("platinum");
        assert_eq!(t, Tier::Team);
        assert_ne!(ttl_for_tier(t), days_to_ms(FREE_TTL_DAYS));
        // ...but a literal "free" still resolves to Free.
        assert_eq!(Tier::from_slug_paid_safe("free"), Tier::Free);
    }

    #[test]
    fn fromstr_and_tryfrom_agree_with_from_slug() {
        use core::str::FromStr;
        for slug in ["free", "solo", "starter", "team", "pro", "org", "max", "enterprise"] {
            let via_slug = Tier::from_slug(slug).unwrap();
            assert_eq!(Tier::from_str(slug).unwrap(), via_slug);
            assert_eq!(Tier::try_from(slug).unwrap(), via_slug);
        }
        assert!(Tier::from_str("nope").is_err());
        assert!(Tier::try_from("nope").is_err());
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

//! [`TierTtlResolver`] trait + [`EnvConfigTierTtlResolver`] S-04 GA
//! fallback + [`MockTierTtlResolver`] test fixture (WI-S04-005 §6.1.2,
//! ADR-0019).
//!
//! ## Boundary contract (ADR-0019)
//!
//! S-04 owns the TTL **infrastructure** (cron worker plus
//! refresh-on-hit plus tenant-scoped expiry enforcement). S-07 owns
//! the **per-tier defaults** (free=7d, solo=30d, team=90d,
//! business=365d, enterprise=customer-configurable). The trait
//! [`TierTtlResolver`] is the boundary mechanism documented in
//! ADR-0019: S-04 GA wires [`EnvConfigTierTtlResolver`] (single
//! global default); S-07 SEALED swaps in `S07PerTierTtlResolver`
//! (lookups against `tenant_quota.tier`) without rebuilding the
//! cron worker or the GET handler.

use core::fmt;

use super::super::handler::DEFAULT_AC_TTL_EXTEND_MS;

/// Default TTL applied by [`EnvConfigTierTtlResolver`] when no
/// `CORELINK_AC_TTL_DEFAULT_MS` env override is supplied — **1 h**
/// (the canonical S-04 GA fallback per WI §6.1.2 + WI §6.1.3 v1.2.0
/// alignment + ADR-0019 §Migration plan stage 1).
///
/// Mirrors [`super::super::handler::DEFAULT_AC_TTL_EXTEND_MS`] so the
/// refresh-on-hit gate + the cron worker observe the same single
/// source of truth for the GA fallback. WI v1.0.0 cited "90 d default"
/// pre-S-07; the GA tier ships with the conservative 1 h alignment so
/// the cron tick has a clear functional path during S-04 staging
/// (per-tier 7 d–365 d post-S-07 SEALED via ADR-0019).
pub const DEFAULT_TIER_TTL_MS: u64 = DEFAULT_AC_TTL_EXTEND_MS;

/// Five canonical tenant tiers per ADR-0019 + S-07 CAP-EVICT-002. The
/// AC TTL handler + cron worker interpret the value through the
/// [`TierTtlResolver`] trait so the boundary between S-04 GA fallback
/// (single global default) and S-07 SEALED (per-tier table) is
/// explicit at the type-system level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TenantTier {
    /// Free tier — 7 d post-S-07. Pricing-tier semantics dictate
    /// shorter TTL.
    Free,
    /// Solo tier — 30 d post-S-07.
    Solo,
    /// Team tier — 90 d post-S-07.
    Team,
    /// Business tier — 365 d post-S-07.
    Business,
    /// Enterprise tier — customer-configurable (default 365 d, max
    /// 730 d) per ADR-0019.
    Enterprise,
}

impl TenantTier {
    /// Stable canonical lower-case label (mirrors
    /// `tenant_quota.tier` SQL enum value the production binding
    /// reads).
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

impl fmt::Display for TenantTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-tier TTL resolver — the canonical boundary between S-04
/// (infrastructure) and S-07 (per-tier defaults) per ADR-0019.
///
/// The resolver is consulted at two seams:
///
/// 1. The GET handler's refresh-on-hit step (extends `expires_at`).
/// 2. The UPDATE handler's first-time INSERT (sets `expires_at`).
///
/// The cron worker (this WI) consumes the resolver indirectly: rows
/// already carry `expires_at` materialized at INSERT time, so the
/// cron does **not** consult the resolver per row — it only honors
/// the existing column value.
pub trait TierTtlResolver: Send + Sync + fmt::Debug {
    /// Resolve the TTL **delta** (milliseconds) to apply for this
    /// tier. Callers add the delta to `now_ms` to get `expires_at`.
    ///
    /// **Production wiring**: `S07PerTierTtlResolver` reads from a
    /// config-singleton table seeded at deploy time
    /// (free=7d, solo=30d, team=90d, business=365d,
    /// enterprise=customer-configurable, capped at 730d).
    ///
    /// **S-04 GA fallback**: [`EnvConfigTierTtlResolver`] returns a
    /// single global default for every tier ([`DEFAULT_TIER_TTL_MS`]
    /// or the `CORELINK_AC_TTL_DEFAULT_MS` env override).
    fn resolve_ttl_ms(&self, tier: TenantTier) -> u64;
}

/// S-04 GA fallback resolver — single global default for every tier.
/// Mirrors WI-S04-005 §6.1.2 + ADR-0019 §Migration plan step 1: pre-
/// S-07, all tenants share the same TTL (90 d → 1 h aligned with
/// handler's `DEFAULT_AC_TTL_EXTEND_MS` for staging).
#[derive(Clone, Copy, Debug)]
pub struct EnvConfigTierTtlResolver {
    ttl_ms: u64,
}

impl Default for EnvConfigTierTtlResolver {
    fn default() -> Self {
        Self::new(DEFAULT_TIER_TTL_MS)
    }
}

impl EnvConfigTierTtlResolver {
    /// Construct with an explicit TTL value (production: read from
    /// `CORELINK_AC_TTL_DEFAULT_MS` env at boot time).
    #[must_use]
    pub const fn new(ttl_ms: u64) -> Self {
        Self { ttl_ms }
    }

    /// Configured TTL (ms).
    #[must_use]
    pub const fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }
}

impl TierTtlResolver for EnvConfigTierTtlResolver {
    fn resolve_ttl_ms(&self, _tier: TenantTier) -> u64 {
        self.ttl_ms
    }
}

/// Test-only resolver letting fixtures pin per-tier values inline. Not
/// exposed under any non-test build profile in the public surface;
/// production wiring uses [`EnvConfigTierTtlResolver`] (S-04 GA) or
/// `S07PerTierTtlResolver` (post-S-07).
#[derive(Clone, Debug, Default)]
pub struct MockTierTtlResolver {
    free_ms: u64,
    solo_ms: u64,
    team_ms: u64,
    business_ms: u64,
    enterprise_ms: u64,
}

impl MockTierTtlResolver {
    /// Construct a mock resolver with explicit per-tier values.
    #[must_use]
    pub const fn new(
        free_ms: u64,
        solo_ms: u64,
        team_ms: u64,
        business_ms: u64,
        enterprise_ms: u64,
    ) -> Self {
        Self {
            free_ms,
            solo_ms,
            team_ms,
            business_ms,
            enterprise_ms,
        }
    }

    /// Construct the canonical S-07 forward profile (free=7d /
    /// solo=30d / team=90d / business=365d / enterprise=365d default
    /// per ADR-0019). Useful for property tests verifying the
    /// boundary swap leaves the cron worker behavior unchanged.
    #[must_use]
    pub const fn s07_canonical() -> Self {
        // Days × 24h × 60m × 60s × 1000ms.
        const DAY_MS: u64 = 86_400_000;
        Self {
            free_ms: 7 * DAY_MS,
            solo_ms: 30 * DAY_MS,
            team_ms: 90 * DAY_MS,
            business_ms: 365 * DAY_MS,
            enterprise_ms: 365 * DAY_MS,
        }
    }
}

impl TierTtlResolver for MockTierTtlResolver {
    fn resolve_ttl_ms(&self, tier: TenantTier) -> u64 {
        match tier {
            TenantTier::Free => self.free_ms,
            TenantTier::Solo => self.solo_ms,
            TenantTier::Team => self.team_ms,
            TenantTier::Business => self.business_ms,
            TenantTier::Enterprise => self.enterprise_ms,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn env_config_resolver_returns_single_default_across_tiers() {
        let r = EnvConfigTierTtlResolver::new(7_200_000); // 2h
        assert_eq!(r.resolve_ttl_ms(TenantTier::Free), 7_200_000);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Solo), 7_200_000);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Team), 7_200_000);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Business), 7_200_000);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Enterprise), 7_200_000);
    }

    #[test]
    fn env_config_default_matches_handler_constant() {
        let r = EnvConfigTierTtlResolver::default();
        assert_eq!(r.ttl_ms(), DEFAULT_TIER_TTL_MS);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Free), DEFAULT_TIER_TTL_MS);
    }

    #[test]
    fn s07_canonical_per_tier_durations() {
        const DAY_MS: u64 = 86_400_000;
        let r = MockTierTtlResolver::s07_canonical();
        assert_eq!(r.resolve_ttl_ms(TenantTier::Free), 7 * DAY_MS);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Solo), 30 * DAY_MS);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Team), 90 * DAY_MS);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Business), 365 * DAY_MS);
        assert_eq!(r.resolve_ttl_ms(TenantTier::Enterprise), 365 * DAY_MS);
    }

    #[test]
    fn each_tier_has_canonical_label() {
        let v = [
            TenantTier::Free,
            TenantTier::Solo,
            TenantTier::Team,
            TenantTier::Business,
            TenantTier::Enterprise,
        ];
        let mut labels = std::collections::BTreeSet::new();
        for t in v {
            assert!(!t.as_str().is_empty());
            assert!(labels.insert(t.as_str()), "duplicate label: {t}");
        }
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn boundary_swap_preserves_resolver_trait_object() {
        // Demonstrate the ADR-0019 boundary: a function that consumes
        // `&dyn TierTtlResolver` works against both S-04 GA fallback
        // and the S-07-shaped mock without code changes.
        fn resolve(r: &dyn TierTtlResolver, t: TenantTier) -> u64 {
            r.resolve_ttl_ms(t)
        }
        let s04 = EnvConfigTierTtlResolver::new(3_600_000);
        let s07 = MockTierTtlResolver::s07_canonical();
        assert_eq!(resolve(&s04, TenantTier::Free), 3_600_000);
        assert_ne!(resolve(&s07, TenantTier::Free), 3_600_000);
    }
}

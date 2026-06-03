//! Rate-limit config knobs surfaced by the production binding.
//!
//! Defaults pin canonical thresholds from WI-S08-001 §6.1 + spec
//! contract §10.s08.2 (≤ 3ms p99 middleware overhead) + RFC 6585 §4
//! Retry-After semantic.

use crate::tier::{TEAM_BURST, TEAM_REFILL_RPS};

/// Canonical default refill rate (tokens / second) when no per-tenant
/// plan override is observed yet (defaults to the team-tier refill so a
/// fresh tenant lands on a reasonable baseline pre plan resolution).
pub const DEFAULT_REFILL_RATE_PER_SEC: u32 = TEAM_REFILL_RPS;

/// Canonical default burst capacity matching the team-tier ladder.
pub const DEFAULT_BURST_CAPACITY: u32 = TEAM_BURST;

/// Canonical Retry-After floor (seconds) per RFC 6585 §4 minimum
/// granularity. 1 second floors the Retry-After value so a hot retry
/// loop client cannot busy-spin on `Retry-After: 0`.
pub const DEFAULT_RETRY_AFTER_FLOOR_SECS: u64 = 1;

/// Canonical Retry-After hard ceiling for canceled tenants
/// (`refill_rate_per_sec == 0`) — mirrors WI §6.1.3 Lote 10.8bis P1-1
/// division-by-zero guard: 7 days = "effectively never" in HTTP client
/// retry semantics.
pub const RETRY_AFTER_CANCELED_TENANT_SECS: u64 = 7 * 86_400;

/// Canonical Retry-After hard ceiling for non-canceled tenants. Caps
/// the maximum value any production response header carries so a
/// programmer error in the refill_rate cannot ship a `Retry-After: 30d`
/// surprise. Mirrors `corelink-quota::RETRY_AFTER_HARD_CEILING_SECS`.
pub const RETRY_AFTER_HARD_CEILING_SECS: u64 = 86_400;

/// Knobs driving the rate-limit decision engine.
///
/// All fields are private; access via inherent methods so the
/// production wiring cannot accidentally drift from canonical defaults
/// without going through [`RateLimitConfig::with_overrides`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Default refill rate (tokens / second) per WI §6.1.4 — applied
    /// when a fresh bucket is materialised before any plan resolution.
    default_refill_rate_per_sec: u32,
    /// Default burst capacity (tokens) per WI §6.1.4.
    default_burst_capacity: u32,
    /// Retry-After floor (seconds; default 1s per RFC 6585 §4).
    retry_after_floor_secs: u64,
    /// Retry-After hard ceiling (seconds; default 86_400s = 1 day).
    retry_after_hard_ceiling_secs: u64,
    /// Retry-After value for canceled tenants (refill_rate=0 path;
    /// default 7 days per WI §6.1.3 Lote 10.8bis P1-1 guard).
    retry_after_canceled_tenant_secs: u64,
}

impl RateLimitConfig {
    /// Construct with the canonical defaults pinned to the spec contract.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            default_refill_rate_per_sec: DEFAULT_REFILL_RATE_PER_SEC,
            default_burst_capacity: DEFAULT_BURST_CAPACITY,
            retry_after_floor_secs: DEFAULT_RETRY_AFTER_FLOOR_SECS,
            retry_after_hard_ceiling_secs: RETRY_AFTER_HARD_CEILING_SECS,
            retry_after_canceled_tenant_secs: RETRY_AFTER_CANCELED_TENANT_SECS,
        }
    }

    /// Construct with explicit knob overrides (test wiring + admin-tier
    /// customisation in S-13 forward).
    ///
    /// Returns `None` when:
    /// - `default_burst_capacity == 0` (a 0-burst bucket can never
    ///   admit a request).
    /// - `retry_after_floor_secs >= retry_after_hard_ceiling_secs`
    ///   (boundary inversion makes the ceiling unreachable).
    /// - `retry_after_canceled_tenant_secs < retry_after_hard_ceiling_secs`
    ///   (canceled-tenant should saturate strictly above the live-tenant
    ///   ceiling).
    #[must_use]
    pub const fn with_overrides(
        default_refill_rate_per_sec: u32,
        default_burst_capacity: u32,
        retry_after_floor_secs: u64,
        retry_after_hard_ceiling_secs: u64,
        retry_after_canceled_tenant_secs: u64,
    ) -> Option<Self> {
        if default_burst_capacity == 0 {
            return None;
        }
        if retry_after_floor_secs >= retry_after_hard_ceiling_secs {
            return None;
        }
        if retry_after_canceled_tenant_secs < retry_after_hard_ceiling_secs {
            return None;
        }
        Some(Self {
            default_refill_rate_per_sec,
            default_burst_capacity,
            retry_after_floor_secs,
            retry_after_hard_ceiling_secs,
            retry_after_canceled_tenant_secs,
        })
    }

    /// Default refill rate (tokens / second).
    #[must_use]
    pub const fn default_refill_rate_per_sec(&self) -> u32 {
        self.default_refill_rate_per_sec
    }

    /// Default burst capacity (tokens).
    #[must_use]
    pub const fn default_burst_capacity(&self) -> u32 {
        self.default_burst_capacity
    }

    /// Retry-After floor (seconds).
    #[must_use]
    pub const fn retry_after_floor_secs(&self) -> u64 {
        self.retry_after_floor_secs
    }

    /// Retry-After hard ceiling (seconds; live-tenant cap).
    #[must_use]
    pub const fn retry_after_hard_ceiling_secs(&self) -> u64 {
        self.retry_after_hard_ceiling_secs
    }

    /// Retry-After canonical canceled-tenant value (seconds).
    #[must_use]
    pub const fn retry_after_canceled_tenant_secs(&self) -> u64 {
        self.retry_after_canceled_tenant_secs
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self::canonical()
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
    fn canonical_constants_pinned() {
        assert_eq!(DEFAULT_REFILL_RATE_PER_SEC, 200);
        assert_eq!(DEFAULT_BURST_CAPACITY, 1000);
        assert_eq!(DEFAULT_RETRY_AFTER_FLOOR_SECS, 1);
        assert_eq!(RETRY_AFTER_HARD_CEILING_SECS, 86_400);
        assert_eq!(RETRY_AFTER_CANCELED_TENANT_SECS, 7 * 86_400);
    }

    #[test]
    fn canonical_config_uses_canonical_defaults() {
        let c = RateLimitConfig::canonical();
        assert_eq!(c.default_refill_rate_per_sec(), 200);
        assert_eq!(c.default_burst_capacity(), 1000);
        assert_eq!(c.retry_after_floor_secs(), 1);
        assert_eq!(c.retry_after_hard_ceiling_secs(), 86_400);
        assert_eq!(c.retry_after_canceled_tenant_secs(), 7 * 86_400);
    }

    #[test]
    fn default_matches_canonical() {
        assert_eq!(RateLimitConfig::default(), RateLimitConfig::canonical());
    }

    #[test]
    fn with_overrides_accepts_valid() {
        let c = RateLimitConfig::with_overrides(100, 500, 5, 3600, 86_400).unwrap();
        assert_eq!(c.default_refill_rate_per_sec(), 100);
        assert_eq!(c.default_burst_capacity(), 500);
        assert_eq!(c.retry_after_floor_secs(), 5);
        assert_eq!(c.retry_after_hard_ceiling_secs(), 3600);
        assert_eq!(c.retry_after_canceled_tenant_secs(), 86_400);
    }

    #[test]
    fn with_overrides_rejects_zero_burst_capacity() {
        assert!(RateLimitConfig::with_overrides(100, 0, 1, 60, 86_400).is_none());
    }

    #[test]
    fn with_overrides_rejects_floor_at_or_above_ceiling() {
        // Inversion — floor MUST be strictly below the live-tenant
        // ceiling.
        assert!(RateLimitConfig::with_overrides(100, 500, 60, 60, 86_400).is_none());
        assert!(RateLimitConfig::with_overrides(100, 500, 600, 60, 86_400).is_none());
    }

    #[test]
    fn with_overrides_rejects_canceled_tenant_below_ceiling() {
        // Canceled tenant value MUST saturate strictly above the live
        // ceiling (matches the WI §6.1.3 7-day "effectively never"
        // canonical value).
        assert!(RateLimitConfig::with_overrides(100, 500, 1, 86_400, 3600).is_none());
    }
}

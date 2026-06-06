//! Canonical 5-tier refill-rate ladder per WI-S08-001 §6.1.4 +
//! Lote 10.7bis P0-7 (5-tier vocabulary FROZEN at the data model
//! layer).
//!
//! ## Canonical refill rate ladder (per WI-S08-001 §1 + slo_catalog §3.1)
//!
//! | Tier       | Refill rate (RPS) | Burst capacity (tokens) |
//! |------------|-------------------|-------------------------|
//! | Free       | 10                | 50                      |
//! | Solo       | 50                | 200                     |
//! | Team       | 200               | 1000                    |
//! | Business   | 1000              | 5000                    |
//! | Enterprise | 10000             | 50000                   |
//!
//! Refill rates inspired by Stripe API livemode 25 RPS baseline + AWS
//! API Gateway 10k RPS default (spec_contract §16). Enterprise rate is
//! negotiable per contract; admin override via S-13 admin plane sets a
//! per-tenant override that supersedes the default.
//!
//! The 5-tier vocabulary is canonical at `data_model.md §3` Plan +
//! `crates/corelink-eviction::tier::Tier` byte-for-byte; this module
//! only ships the rate ladder so the rate-limit DO can short-circuit
//! cold-start without needing the eviction crate's TTL ladder.

use corelink_eviction::Tier;

/// Free-tier refill rate (tokens / second).
pub const FREE_REFILL_RPS: u32 = 10;
/// Free-tier burst capacity (tokens).
pub const FREE_BURST: u32 = 50;
/// Solo-tier refill rate (tokens / second).
pub const SOLO_REFILL_RPS: u32 = 50;
/// Solo-tier burst capacity (tokens).
pub const SOLO_BURST: u32 = 200;
/// Team-tier refill rate (tokens / second).
pub const TEAM_REFILL_RPS: u32 = 200;
/// Team-tier burst capacity (tokens).
pub const TEAM_BURST: u32 = 1000;
/// Business-tier refill rate (tokens / second).
pub const BUSINESS_REFILL_RPS: u32 = 1000;
/// Business-tier burst capacity (tokens).
pub const BUSINESS_BURST: u32 = 5000;
/// Enterprise-tier refill rate (tokens / second; admin override per
/// contract via S-13 admin plane).
pub const ENTERPRISE_REFILL_RPS: u32 = 10_000;
/// Enterprise-tier burst capacity (tokens).
pub const ENTERPRISE_BURST: u32 = 50_000;

/// Canonical 5-tier refill ladder snapshot (informational; for
/// dashboard widget configuration + cross-component regression tests).
pub const TIER_RATE_LADDER: [(Tier, u32, u32); 5] = [
    (Tier::Free, FREE_REFILL_RPS, FREE_BURST),
    (Tier::Solo, SOLO_REFILL_RPS, SOLO_BURST),
    (Tier::Team, TEAM_REFILL_RPS, TEAM_BURST),
    (Tier::Business, BUSINESS_REFILL_RPS, BUSINESS_BURST),
    (Tier::Enterprise, ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST),
];

/// Resolve the canonical `(refill_rate_per_sec, burst_capacity)` pair
/// for a tier per WI-S08-001 §6.1.4.
///
/// `Tier` is `#[non_exhaustive]` upstream so future tier additions
/// don't break this crate's compile; the wildcard arm falls back to
/// the enterprise rate (most-permissive default — preferred over
/// crashing or denying when an unknown tier is observed in
/// production).
#[must_use]
pub fn refill_rate_for_tier(tier: Tier) -> (u32, u32) {
    match tier {
        Tier::Free => (FREE_REFILL_RPS, FREE_BURST),
        Tier::Solo => (SOLO_REFILL_RPS, SOLO_BURST),
        Tier::Team => (TEAM_REFILL_RPS, TEAM_BURST),
        Tier::Business => (BUSINESS_REFILL_RPS, BUSINESS_BURST),
        Tier::Enterprise => (ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST),
        // Forward-compatibility — unknown tier gets enterprise default
        // (most-permissive; conservative-on-availability per §10
        // anti-scope: we never accidentally over-throttle a legit
        // tenant whose plan got renamed in a follow-on sprint).
        _ => (ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST),
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
    fn ladder_has_five_tiers() {
        assert_eq!(TIER_RATE_LADDER.len(), 5);
    }

    #[test]
    fn refill_rate_strictly_increasing() {
        let mut last = 0_u32;
        for &(_, rate, _) in TIER_RATE_LADDER.iter() {
            assert!(rate > last, "rate must increase across tiers");
            last = rate;
        }
    }

    #[test]
    fn burst_strictly_increasing() {
        let mut last = 0_u32;
        for &(_, _, burst) in TIER_RATE_LADDER.iter() {
            assert!(burst > last, "burst must increase across tiers");
            last = burst;
        }
    }

    #[test]
    fn burst_at_least_4x_refill_rate() {
        // Burst MUST be >= 4× sustained rate so a legitimate parallel-build
        // spike doesn't immediately 429. The canonical 5-tier ladder lands
        // every tier at 4-5× by construction (free 5×; solo 4×; team 5×;
        // business 5×; enterprise 5×). Stripe livemode 25 RPS / 100
        // testmode = 4× analogue — chosen to avoid penalising small Solo
        // tenants on bursty parallel uploads.
        for &(t, rate, burst) in TIER_RATE_LADDER.iter() {
            assert!(burst >= rate * 4, "tier {t} burst {burst} < 4× rate {rate}");
        }
    }

    #[test]
    fn lookup_matches_constants() {
        assert_eq!(
            refill_rate_for_tier(Tier::Free),
            (FREE_REFILL_RPS, FREE_BURST)
        );
        assert_eq!(
            refill_rate_for_tier(Tier::Solo),
            (SOLO_REFILL_RPS, SOLO_BURST)
        );
        assert_eq!(
            refill_rate_for_tier(Tier::Team),
            (TEAM_REFILL_RPS, TEAM_BURST)
        );
        assert_eq!(
            refill_rate_for_tier(Tier::Business),
            (BUSINESS_REFILL_RPS, BUSINESS_BURST)
        );
        assert_eq!(
            refill_rate_for_tier(Tier::Enterprise),
            (ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST),
        );
    }

    #[test]
    fn enterprise_rate_matches_aws_api_gateway_baseline() {
        // Spec contract §16 benchmark — AWS API Gateway default 10k RPS.
        assert_eq!(ENTERPRISE_REFILL_RPS, 10_000);
    }

    #[test]
    fn free_burst_matches_spec_contract_table() {
        assert_eq!(FREE_BURST, 50);
    }
}

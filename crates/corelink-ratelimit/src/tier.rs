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

/// Map a **billing-tier wire label** (the snake_case strings stored in
/// `tier_selections.tier` / `tenant.tier`) to the canonical 5-tier
/// operational [`Tier`] ladder.
///
/// Single Rust authority per
/// `PROPOSAL-2026-06-10-ADMIN-PILOT-TENANT-RATE-MAPPING` §3.3 / §3.4.1
/// (mapping ratified Q5, 2026-06-10):
///
/// | Billing label (wire)    | → operational `Tier` |
/// |-------------------------|----------------------|
/// | `free`                  | `Free`               |
/// | `solo`                  | `Solo`               |
/// | `starter`               | `Team`               |
/// | `pro`                   | `Business`           |
/// | `max`                   | `Business` — **NOT Enterprise** (ratified Q5a: request-quota enforcement is a no-op today, so Enterprise's 10k rps would leave an 80M-cap tenant unbounded at 324×; Business bounds the worst case at 32×) |
/// | `enterprise`            | `Enterprise`         |
/// | `team` (D1 legacy)      | `Team`               |
/// | `org` (D1 legacy "pro") | `Business`           |
/// | `pilot` / anything else | `Team` (fallback)    |
///
/// The `pilot`/unknown → `Team` fallback is **zero behavior change**:
/// `RateLimitConfig::canonical()` already gives every unresolved tenant
/// the Team rate (200 rps / 1000 burst). It deliberately narrows ONLY
/// at the string level — the enum **wildcard-arm** fallback in
/// [`refill_rate_for_tier`] (unknown `Tier` variant → Enterprise rate)
/// is a different, forward-compatibility concern and stays untouched.
///
/// Labels are matched exactly (canonical wire strings are lower
/// snake_case); any non-canonical spelling takes the `Team` fallback.
///
/// Coupling note (§3.5, ratified Q5c): the returned [`Tier`] also
/// selects the eviction TTL ladder (`corelink_eviction::ttl_for_tier`),
/// so this mapping is a customer-visible **retention promise** — the
/// published rate card (`apps/docs/src/lib/pricing.ts::TIER_RATE_CARD`
/// `retentionDays`) MUST stay consistent with
/// `tier_for_billing_label` ∘ `ttl_for_tier`.
#[must_use]
pub fn tier_for_billing_label(label: &str) -> Tier {
    match label {
        "free" => Tier::Free,
        "solo" => Tier::Solo,
        "starter" | "team" => Tier::Team,
        "pro" | "org" | "max" => Tier::Business,
        "enterprise" => Tier::Enterprise,
        // pilot / unknown → Team: today's implicit canonical() default.
        _ => Tier::Team,
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

    // --- tier_for_billing_label (PROPOSAL §3.3, ratified Q5) ---

    #[test]
    fn billing_label_mapping_total_over_canonical_taxonomy() {
        // The FROZEN 6-tier billing taxonomy (pricing.ts CANONICAL_TIERS).
        assert_eq!(tier_for_billing_label("free"), Tier::Free);
        assert_eq!(tier_for_billing_label("solo"), Tier::Solo);
        assert_eq!(tier_for_billing_label("starter"), Tier::Team);
        assert_eq!(tier_for_billing_label("pro"), Tier::Business);
        assert_eq!(tier_for_billing_label("max"), Tier::Business);
        assert_eq!(tier_for_billing_label("enterprise"), Tier::Enterprise);
    }

    #[test]
    fn billing_label_mapping_legacy_d1_values() {
        // D1 legacy values retained by migration 0062.
        assert_eq!(tier_for_billing_label("team"), Tier::Team);
        // org = pre-S-19 "pro"; quota.ts already treats org as pro's class.
        assert_eq!(tier_for_billing_label("org"), Tier::Business);
    }

    #[test]
    fn billing_label_max_is_business_not_enterprise() {
        // Ratified Q5a pin: max must NEVER map to Enterprise (10k rps
        // would leave an 80M-cap tenant unbounded while checkRequestQuota
        // is a no-op; it would also erase the Enterprise upsell surface).
        assert_ne!(tier_for_billing_label("max"), Tier::Enterprise);
        assert_eq!(tier_for_billing_label("max"), Tier::Business);
    }

    #[test]
    fn billing_label_pilot_and_unknown_fall_back_to_team() {
        // Ratified Q5b: zero behavior change — canonical() default IS
        // Team (200 rps / 1000 burst). NOT the wildcard-arm Enterprise
        // fallback of refill_rate_for_tier (string level only).
        assert_eq!(tier_for_billing_label("pilot"), Tier::Team);
        assert_eq!(tier_for_billing_label(""), Tier::Team);
        assert_eq!(tier_for_billing_label("business"), Tier::Team);
        assert_eq!(tier_for_billing_label("ultimate"), Tier::Team);
        // Non-canonical spellings (wire strings are lower snake_case).
        assert_eq!(tier_for_billing_label("Free"), Tier::Team);
        assert_eq!(tier_for_billing_label("MAX"), Tier::Team);
        assert_eq!(tier_for_billing_label(" solo "), Tier::Team);
    }

    #[test]
    fn billing_label_mapping_monotonic_over_price_ladder() {
        // A higher (more expensive) billing tier must never map to a
        // LOWER operational class than a cheaper one. Tier derives Ord
        // in ladder order (Free < Solo < Team < Business < Enterprise).
        let price_ladder = ["free", "solo", "starter", "pro", "max", "enterprise"];
        let mapped: Vec<Tier> = price_ladder
            .iter()
            .map(|l| tier_for_billing_label(l))
            .collect();
        for pair in mapped.windows(2) {
            if let [a, b] = pair {
                assert!(
                    b >= a,
                    "billing ladder maps non-monotonically: {a:?} -> {b:?}"
                );
            }
        }
    }

    #[test]
    fn billing_label_fallback_never_exceeds_cheapest_paid_team_class() {
        // The unknown-label fallback (Team) must not grant more than the
        // canonical() implicit default — pin both rate and burst.
        let t = tier_for_billing_label("definitely-not-a-tier");
        assert_eq!(refill_rate_for_tier(t), (TEAM_REFILL_RPS, TEAM_BURST));
    }
}

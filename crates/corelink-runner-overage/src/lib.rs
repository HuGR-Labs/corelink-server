//! `corelink-runner-overage` — the pure, integer-only money core for
//! runner compute-overage billing (WI-S10-007, Phase B / shadow).
//!
//! This crate owns exactly ONE thing: given a tenant's aggregated runner
//! compute for a billing period (in vCPU-seconds, the billable
//! `runner_vcpu_seconds` meter) and its runner tier, compute
//!
//!   1. the **overage** above the tier's included `max_vcpu_h` allowance, and
//!   2. the **Stripe meter quantity** (vCPU-hours) + the **shadow charge**
//!      (what *would* be billed) for that overage.
//!
//! It performs NO I/O (no D1, no HTTP, no Stripe) and holds NO credentials —
//! it is the deterministic arithmetic the aggregator binary and the shadow
//! ledger call. Keeping it isolated + zero-dependency makes the money math
//! unit-falsifiable in milliseconds and impossible to get wrong via a hidden
//! network path.
//!
//! ## Money discipline: integers only, never `f64`
//!
//! Currency and quantities are computed in fixed-point integers. The billable
//! unit is the vCPU-**second** (what the emitter pushes); the allowance and the
//! Stripe price tiers are in vCPU-**hours** (`SECONDS_PER_VCPU_HOUR = 3600`).
//! The shadow charge is returned in **millicents** (1/1000 of a US cent) so a
//! sub-cent per-vCPU-hour rate never loses precision to floating point. Callers
//! that need whole cents divide by 1000 at the very end (display only).
//!
//! ## The rate is a PLACEHOLDER until the live Stripe Price
//!
//! [`DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR`] = 20 (`$0.20`/vCPU-hour) is the
//! owner-confirmed target (2026-08-12), derived first-principles: base-tier
//! marginal rate ≈ `$0.167`/vCPU-h, GitHub Actions ≈ `$0.24`/vCPU-h,
//! Hetzner-class COGS ≈ `$0.005–0.012` → `$0.20` is ~95% margin, −17% vs
//! GitHub, and nudges the upgrade exactly at each tier boundary. The AUTHORITY
//! on the number that actually bills is the Stripe graduated Price (created in
//! test mode first); this constant drives the SHADOW computation only.

#![forbid(unsafe_code)]

/// Seconds in one vCPU-hour. The billable meter is emitted in vCPU-seconds; the
/// allowance and Stripe price tiers are denominated in vCPU-hours.
pub const SECONDS_PER_VCPU_HOUR: u64 = 3600;

/// Owner-confirmed target overage rate (2026-08-12), in **whole US cents per
/// vCPU-hour** — `20` == `$0.20`/vCPU-hour. Drives the SHADOW charge only; the
/// live Stripe graduated Price is the billing authority. PLACEHOLDER until that
/// Price is created (the `.env.local` test key was expired at authoring time).
pub const DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR: u64 = 20;

/// A self-serve Runner subscription tier. The `max_vcpu_h` ladder mirrors the
/// operator-provisioned `runners_entitlement` axes and the checkout backend's
/// frozen `RUNNER_TIER_ENTITLEMENT` map
/// (`apps/signup-worker/src/webhooks/stripe.ts`):
/// starter 100 · pro 240 · team 600 · scale 1200 · max 2400 vCPU-h.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RunnerTier {
    /// `runner_starter` — 100 vCPU-h/mo included.
    Starter,
    /// `runner_pro` — 240 vCPU-h/mo included.
    Pro,
    /// `runner_team` — 600 vCPU-h/mo included.
    Team,
    /// `runner_scale` — 1200 vCPU-h/mo included.
    Scale,
    /// `runner_max` — 2400 vCPU-h/mo included.
    Max,
}

impl RunnerTier {
    /// The tier's included compute allowance, in vCPU-hours.
    #[must_use]
    pub const fn max_vcpu_h(self) -> u64 {
        match self {
            Self::Starter => 100,
            Self::Pro => 240,
            Self::Team => 600,
            Self::Scale => 1200,
            Self::Max => 2400,
        }
    }

    /// The tier's included allowance expressed in vCPU-**seconds**
    /// (`max_vcpu_h * 3600`) — the unit the aggregated meter total is in, so the
    /// overage subtraction needs no unit conversion.
    #[must_use]
    pub const fn allowance_vcpu_seconds(self) -> u64 {
        self.max_vcpu_h() * SECONDS_PER_VCPU_HOUR
    }

    /// The canonical SKU string (`runner_starter`..`runner_max`) — the value the
    /// checkout backend stamps into `runners_entitlement.sku` / Stripe
    /// `metadata[tier]`.
    #[must_use]
    pub const fn sku(self) -> &'static str {
        match self {
            Self::Starter => "runner_starter",
            Self::Pro => "runner_pro",
            Self::Team => "runner_team",
            Self::Scale => "runner_scale",
            Self::Max => "runner_max",
        }
    }

    /// Parse a runner tier from its canonical SKU string; `None` for any
    /// unknown value (fail-safe, mirrors the checkout backend's `asRunnerTier`).
    #[must_use]
    pub fn from_sku(raw: &str) -> Option<Self> {
        match raw {
            "runner_starter" => Some(Self::Starter),
            "runner_pro" => Some(Self::Pro),
            "runner_team" => Some(Self::Team),
            "runner_scale" => Some(Self::Scale),
            "runner_max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// The overage above a tier's included allowance, in vCPU-**seconds**:
/// `max(0, total - allowance)`. Saturating, so a tenant under its allowance is
/// `0` (never a wraparound / negative bill).
#[must_use]
pub fn overage_vcpu_seconds(total_vcpu_seconds: u128, tier: RunnerTier) -> u128 {
    total_vcpu_seconds.saturating_sub(u128::from(tier.allowance_vcpu_seconds()))
}

/// The Stripe **meter quantity** for a given overage, in vCPU-hours, as a
/// fixed-precision decimal STRING (6 fractional digits). The Stripe graduated
/// Price is denominated in vCPU-hours, so the Meter Event carries hours, not
/// seconds. Rendering as a decimal string (not `f64`) keeps the exact rational
/// `seconds / 3600` free of binary-float error.
///
/// Example: `overage_vcpu_seconds = 504000` (140 vCPU-h) → `"140.000000"`.
#[must_use]
pub fn overage_vcpu_hours_decimal(overage_vcpu_seconds: u128) -> String {
    let denom = u128::from(SECONDS_PER_VCPU_HOUR);
    let whole = overage_vcpu_seconds / denom;
    // Fractional part scaled to 6 dp: (rem * 1_000_000) / 3600, floored.
    let rem = overage_vcpu_seconds % denom;
    let frac = (rem * 1_000_000) / denom;
    format!("{whole}.{frac:06}")
}

/// The **shadow charge** for a given overage, in **millicents** (1/1000 of a US
/// cent), floored. `rate_cents_per_vcpu_hour` is whole cents per vCPU-hour
/// (e.g. [`DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR`] = 20 for `$0.20`/h).
///
/// `millicents = overage_seconds * rate_cents * 1000 / 3600`
///
/// All-integer: the `* 1000` (cents → millicents) is applied before the `/ 3600`
/// (seconds → hours) so sub-cent-per-hour precision survives. Saturating on the
/// multiply so an absurd input can never panic on overflow.
#[must_use]
pub fn shadow_charge_millicents(overage_vcpu_seconds: u128, rate_cents_per_vcpu_hour: u64) -> u128 {
    overage_vcpu_seconds
        .saturating_mul(u128::from(rate_cents_per_vcpu_hour))
        .saturating_mul(1000)
        / u128::from(SECONDS_PER_VCPU_HOUR)
}

/// The shadow charge rendered as whole US cents (millicents / 1000, floored) —
/// display convenience for the shadow ledger / operator report. The canonical
/// stored value stays in millicents to avoid compounding rounding.
#[must_use]
pub fn millicents_to_cents(millicents: u128) -> u128 {
    millicents / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_allowances_match_the_frozen_ladder() {
        assert_eq!(RunnerTier::Starter.max_vcpu_h(), 100);
        assert_eq!(RunnerTier::Pro.max_vcpu_h(), 240);
        assert_eq!(RunnerTier::Team.max_vcpu_h(), 600);
        assert_eq!(RunnerTier::Scale.max_vcpu_h(), 1200);
        assert_eq!(RunnerTier::Max.max_vcpu_h(), 2400);
    }

    #[test]
    fn allowance_in_seconds_is_hours_times_3600() {
        assert_eq!(RunnerTier::Starter.allowance_vcpu_seconds(), 100 * 3600);
        assert_eq!(RunnerTier::Max.allowance_vcpu_seconds(), 2400 * 3600);
    }

    #[test]
    fn sku_round_trips() {
        for t in [
            RunnerTier::Starter,
            RunnerTier::Pro,
            RunnerTier::Team,
            RunnerTier::Scale,
            RunnerTier::Max,
        ] {
            assert_eq!(RunnerTier::from_sku(t.sku()), Some(t));
        }
        assert_eq!(RunnerTier::from_sku("runner_bogus"), None);
        assert_eq!(RunnerTier::from_sku(""), None);
    }

    #[test]
    fn under_allowance_has_zero_overage_and_zero_charge() {
        // Starter used 50 vCPU-h of its 100 included.
        let total = 50u128 * 3600;
        let over = overage_vcpu_seconds(total, RunnerTier::Starter);
        assert_eq!(over, 0);
        assert_eq!(
            shadow_charge_millicents(over, DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR),
            0
        );
        assert_eq!(overage_vcpu_hours_decimal(over), "0.000000");
    }

    #[test]
    fn exactly_at_allowance_is_zero_overage() {
        let total = 100u128 * 3600; // exactly the Starter allowance
        assert_eq!(overage_vcpu_seconds(total, RunnerTier::Starter), 0);
    }

    #[test]
    fn starter_at_240_hours_bills_the_140_hour_overage_at_20c() {
        // The price-calc upgrade-nudge example: a Starter (100h included) that
        // ran 240 vCPU-h is 140 vCPU-h over. At $0.20/vCPU-h that is $28.00.
        let total = 240u128 * 3600;
        let over = overage_vcpu_seconds(total, RunnerTier::Starter);
        assert_eq!(over, 140 * 3600);
        assert_eq!(overage_vcpu_hours_decimal(over), "140.000000");

        let mc = shadow_charge_millicents(over, DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR);
        // 140h * $0.20 = $28.00 = 2800 cents = 2_800_000 millicents.
        assert_eq!(mc, 2_800_000);
        assert_eq!(millicents_to_cents(mc), 2800);
    }

    #[test]
    fn fractional_hour_overage_keeps_sub_cent_precision() {
        // 1 vCPU-second over, at $0.20/vCPU-h.
        // millicents = 1 * 20 * 1000 / 3600 = 20000/3600 = 5 (floored).
        let mc = shadow_charge_millicents(1, DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR);
        assert_eq!(mc, 5); // 0.005 cent — would round to 0 cents, but survives in millicents
        assert_eq!(millicents_to_cents(mc), 0);
    }

    #[test]
    fn half_hour_overage_decimal_is_exact() {
        let over = 1800u128; // 0.5 vCPU-h
        assert_eq!(overage_vcpu_hours_decimal(over), "0.500000");
        // 0.5h * $0.20 = $0.10 = 10 cents = 10_000 millicents.
        let mc = shadow_charge_millicents(over, DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR);
        assert_eq!(mc, 10_000);
        assert_eq!(millicents_to_cents(mc), 10);
    }

    #[test]
    fn charge_is_monotonic_in_overage() {
        let rate = DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR;
        let a = shadow_charge_millicents(1000, rate);
        let b = shadow_charge_millicents(2000, rate);
        assert!(b > a, "more overage must never cost less");
    }
}

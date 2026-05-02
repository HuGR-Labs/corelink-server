//! [`ScheduleConfig`] + deterministic per-region jitter helper.
//!
//! WI-S06-001 §1 + S-06 spec contract §5.1 freeze the canonical cron
//! schedule:
//!
//! - Cron expression: `"0 2 * * *"` (02:00 UTC daily).
//! - Jitter: ±10 min per region, deterministic so the 5 regions spread
//!   evenly across `02:00..02:50 UTC` instead of thundering herd at
//!   exact 02:00.
//! - Region pin: one DO per region (5 instances).
//! - Per-tenant batch concurrency cap: 4 (default).
//!
//! The jitter is **deterministic per region ordinal** (not RNG-based)
//! because:
//!
//! 1. We need cron-tick reproducibility for chaos tests + property
//!    tests (every region's offset is the same wall-clock tick across
//!    runs — load distribution is verifiable in CI without flaky RNG
//!    seeds).
//! 2. RNG-based jitter would introduce `rand` dependency just for a
//!    spread we can compute in const time.
//! 3. Production wiring composes [`jitter_ms_for_region`] with the
//!    cron-fired wall-clock instant; every tick from a given region
//!    lands at `cron_fire_ms + jitter_ms_for_region(region,
//!    jitter_minutes)` which deterministically spreads the 5 regions
//!    evenly across `±jitter_minutes`.

use thiserror::Error;

use crate::region::GcRegion;

/// Default cron expression (02:00 UTC daily).
pub const DEFAULT_CRON_EXPR: &str = "0 2 * * *";

/// Default jitter window in minutes — ±10 min per region.
pub const DEFAULT_JITTER_MINUTES: u32 = 10;

/// Canonical ceiling on jitter window. The 02:00..02:50 UTC budget for
/// daily GC has slack but more than 30 min would push a regional tick
/// into peak-hour traffic; `MAX_JITTER_MINUTES = 30` is the operator
/// hard ceiling per WI §1 / S-13 admin plane forward.
pub const MAX_JITTER_MINUTES: u32 = 30;

/// Default per-tenant batch concurrency cap. WI §1 freezes the
/// canonical default at 4 — bounded so a single GC tick cannot
/// monopolise the worker DO for one large tenant.
pub const DEFAULT_MAX_CONCURRENT_TENANTS: u32 = 4;

/// Errors surfaced by [`ScheduleConfig::new`] and helpers.
#[derive(Debug, Error)]
pub enum ScheduleConfigError {
    /// Caller supplied a jitter > [`MAX_JITTER_MINUTES`].
    #[error("jitter_minutes {requested} exceeds canonical ceiling {ceiling}")]
    JitterMinutesExceeded {
        /// Requested jitter.
        requested: u32,
        /// Canonical ceiling.
        ceiling: u32,
    },
    /// Cron expression is empty (canonical empty rejection — the
    /// production cron parser will reject as well, but we surface the
    /// error at construction time so misconfiguration cannot reach
    /// the scheduler).
    #[error("cron expression is empty")]
    EmptyCron,
}

/// Per-region cron schedule + jitter + per-tenant concurrency cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleConfig {
    cron: String,
    jitter_minutes: u32,
    region: GcRegion,
    max_concurrent_tenants: u32,
}

impl ScheduleConfig {
    /// Construct a schedule config from explicit parameters.
    ///
    /// # Errors
    ///
    /// - [`ScheduleConfigError::EmptyCron`] if `cron` is empty.
    /// - [`ScheduleConfigError::JitterMinutesExceeded`] if
    ///   `jitter_minutes > `[`MAX_JITTER_MINUTES`].
    pub fn new(
        cron: impl Into<String>,
        jitter_minutes: u32,
        region: GcRegion,
        max_concurrent_tenants: u32,
    ) -> Result<Self, ScheduleConfigError> {
        let cron = cron.into();
        if cron.trim().is_empty() {
            return Err(ScheduleConfigError::EmptyCron);
        }
        if jitter_minutes > MAX_JITTER_MINUTES {
            return Err(ScheduleConfigError::JitterMinutesExceeded {
                requested: jitter_minutes,
                ceiling: MAX_JITTER_MINUTES,
            });
        }
        Ok(Self {
            cron,
            jitter_minutes,
            region,
            max_concurrent_tenants,
        })
    }

    /// Build a config with canonical defaults pinned to `region`
    /// (cron `"0 2 * * *"`, jitter ±10 min, 4 concurrent tenants).
    ///
    /// # Errors
    ///
    /// Cannot fail under canonical defaults; surfaced as `Result` for
    /// symmetry with [`Self::new`].
    pub fn with_defaults(region: GcRegion) -> Result<Self, ScheduleConfigError> {
        Self::new(
            DEFAULT_CRON_EXPR,
            DEFAULT_JITTER_MINUTES,
            region,
            DEFAULT_MAX_CONCURRENT_TENANTS,
        )
    }

    /// Cron expression (e.g. `"0 2 * * *"`).
    #[must_use]
    pub fn cron(&self) -> &str {
        &self.cron
    }

    /// Jitter window in minutes.
    #[must_use]
    pub const fn jitter_minutes(&self) -> u32 {
        self.jitter_minutes
    }

    /// Region this config is pinned to.
    #[must_use]
    pub const fn region(&self) -> GcRegion {
        self.region
    }

    /// Per-tenant batch concurrency cap.
    #[must_use]
    pub const fn max_concurrent_tenants(&self) -> u32 {
        self.max_concurrent_tenants
    }
}

/// Compute the deterministic jitter offset (in milliseconds) for the
/// given region + jitter window.
///
/// Spreads the 5 canonical regions evenly across `[0,
/// jitter_minutes]` (in minutes; `* 60_000` to ms). Region ordinal
/// drives the bucket; the helper returns a non-negative offset added
/// to the canonical cron-fire instant.
///
/// **Why non-negative**: the cron expression fires at canonical 02:00
/// UTC; jitter is a forward-only spread (regions land at 02:00, 02:02,
/// 02:05, 02:07, 02:10 for `jitter_minutes = 10`). Backward spread
/// would push some regions into 01:50 which is outside the
/// 02:00..02:50 UTC budget WI §6.1.4 freezes.
///
/// **Determinism guarantee**: every call with the same
/// `(region, jitter_minutes)` returns the same offset. Property tests
/// pin this; chaos tests verify the 5-region spread on a single tick.
#[must_use]
pub const fn jitter_ms_for_region(region: GcRegion, jitter_minutes: u32) -> u64 {
    if jitter_minutes == 0 {
        return 0;
    }
    // 5 regions × evenly-spaced buckets: ordinal `o` lands at
    // `(o / (N-1)) * jitter_minutes` minutes — but we want
    // [0, jitter_minutes] inclusive on both ends so divide by `N - 1`
    // (=4) so the spread covers the full window. For `N=5`:
    //   ord 0 → 0/4 = 0
    //   ord 1 → 1/4 = 0.25 * jitter
    //   ord 2 → 2/4 = 0.5  * jitter
    //   ord 3 → 3/4 = 0.75 * jitter
    //   ord 4 → 4/4 = 1.0  * jitter
    // Computed in u64 to avoid f32 / rounding issues; multiplication
    // first then division for integer-arith fidelity.
    let ord = region.ordinal() as u64;
    let jitter_ms = (jitter_minutes as u64).saturating_mul(60_000);
    // ord ∈ {0,1,2,3,4} so multiplication is bounded.
    let scaled = jitter_ms.saturating_mul(ord);
    scaled / 4
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
    fn defaults_are_canonical() {
        let cfg = ScheduleConfig::with_defaults(GcRegion::Sam).unwrap();
        assert_eq!(cfg.cron(), DEFAULT_CRON_EXPR);
        assert_eq!(cfg.jitter_minutes(), DEFAULT_JITTER_MINUTES);
        assert_eq!(cfg.max_concurrent_tenants(), DEFAULT_MAX_CONCURRENT_TENANTS);
        assert_eq!(cfg.region(), GcRegion::Sam);
    }

    #[test]
    fn empty_cron_rejected() {
        let err = ScheduleConfig::new("", 5, GcRegion::Iad, 4).unwrap_err();
        assert!(matches!(err, ScheduleConfigError::EmptyCron));
        let err2 = ScheduleConfig::new("   ", 5, GcRegion::Iad, 4).unwrap_err();
        assert!(matches!(err2, ScheduleConfigError::EmptyCron));
    }

    #[test]
    fn jitter_above_ceiling_rejected() {
        let err = ScheduleConfig::new(
            DEFAULT_CRON_EXPR,
            MAX_JITTER_MINUTES + 1,
            GcRegion::Lhr,
            4,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ScheduleConfigError::JitterMinutesExceeded { ceiling, .. } if ceiling == MAX_JITTER_MINUTES
        ));
    }

    #[test]
    fn jitter_zero_minutes_returns_zero_for_every_region() {
        for region in GcRegion::all() {
            assert_eq!(jitter_ms_for_region(*region, 0), 0);
        }
    }

    #[test]
    fn jitter_evenly_spread_across_5_regions() {
        // 10-min window: ordinals {0,1,2,3,4} land at
        // {0,2.5,5,7.5,10} min == {0,150_000,300_000,450_000,600_000} ms.
        let expected: [u64; 5] = [0, 150_000, 300_000, 450_000, 600_000];
        for (region, want) in GcRegion::all().iter().zip(expected.iter()) {
            let got = jitter_ms_for_region(*region, 10);
            assert_eq!(got, *want, "region={region:?}");
        }
    }

    #[test]
    fn jitter_first_and_last_region_endpoints() {
        // Endpoint sanity: `Sam` (ord 0) always lands at 0; `Syd`
        // (ord 4) always lands at the full jitter window.
        for jitter in 1..=MAX_JITTER_MINUTES {
            assert_eq!(jitter_ms_for_region(GcRegion::Sam, jitter), 0);
            assert_eq!(
                jitter_ms_for_region(GcRegion::Syd, jitter),
                u64::from(jitter) * 60_000
            );
        }
    }

    #[test]
    fn jitter_deterministic_repeated_call() {
        for region in GcRegion::all() {
            let a = jitter_ms_for_region(*region, DEFAULT_JITTER_MINUTES);
            let b = jitter_ms_for_region(*region, DEFAULT_JITTER_MINUTES);
            assert_eq!(a, b);
        }
    }
}

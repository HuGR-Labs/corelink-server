//! SLO definition + canonical SLI taxonomy per `slo_catalog.md`.
//!
//! ## Why an enum
//!
//! Closed taxonomy at compile-time per WI §9.2 + Lote 10.8bis
//! discipline: a `String` slot here would let a typo'd SLI bypass
//! cardinality + alert coverage CI gates. The `#[non_exhaustive]`
//! marker reserves additive growth for follow-on WIs without
//! breaking downstream sinks.

use crate::error::SloError;

/// Canonical SLI taxonomy from `slo_catalog.md` §4.x. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. S-13 admin-plane SLIs forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Sli {
    /// `SLI-AVAIL-CAS-GET` per `slo_catalog.md §4.2`. CAS read path
    /// is the hot path of product value.
    AvailCasGet,
    /// `SLI-AVAIL-CAS-PUT` per `slo_catalog.md §4.3`.
    AvailCasPut,
    /// `SLI-AVAIL-AC-LOOKUP` per `slo_catalog.md §4` — Action Cache
    /// lookup availability.
    AvailAcLookup,
    /// `SLI-AVAIL-AUTH` per `slo_catalog.md §4` — auth control plane
    /// availability.
    AvailAuth,
    /// `SLI-LATENCY-CAS-GET-P99` per `slo_catalog.md §4` — p99
    /// latency target for CAS GET.
    LatencyCasGetP99,
    /// `SLO-DEDUP-RATIO` per `slo_catalog.md §4` — deduplication
    /// ratio (S-07 inheritance; quality SLI).
    DedupRatio,
    /// `SLI-RATE-LIMIT-WITHIN-QUOTA` per `slo_catalog.md §4` —
    /// fraction of within-quota requests that are NOT rate-limited
    /// (S-08 inheritance; rate-limiter false-positive bound).
    RateLimitWithinQuota,
}

impl Sli {
    /// Canonical SLI slug used in alert `labels.slo`, audit records,
    /// and PagerDuty dedup-key construction. Mirrors the `slo_catalog`
    /// SLI ID exactly.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AvailCasGet => "SLI-AVAIL-CAS-GET",
            Self::AvailCasPut => "SLI-AVAIL-CAS-PUT",
            Self::AvailAcLookup => "SLI-AVAIL-AC-LOOKUP",
            Self::AvailAuth => "SLI-AVAIL-AUTH",
            Self::LatencyCasGetP99 => "SLI-LATENCY-CAS-GET-P99",
            Self::DedupRatio => "SLO-DEDUP-RATIO",
            Self::RateLimitWithinQuota => "SLI-RATE-LIMIT-WITHIN-QUOTA",
        }
    }

    /// Lower-case Prometheus-compatible metric base name (per
    /// OpenMetrics 1.0 naming convention).
    #[must_use]
    pub const fn prometheus_metric_base(self) -> &'static str {
        match self {
            Self::AvailCasGet => "corelink_cas_get",
            Self::AvailCasPut => "corelink_cas_put",
            Self::AvailAcLookup => "corelink_ac_lookup",
            Self::AvailAuth => "corelink_auth_attempts",
            Self::LatencyCasGetP99 => "corelink_cas_get_latency",
            Self::DedupRatio => "corelink_dedup_ratio",
            Self::RateLimitWithinQuota => "corelink_rate_limited_within_quota",
        }
    }
}

impl core::fmt::Display for Sli {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 7-element SLI list per `slo_catalog.md §4.x` and WI
/// §6.1.1. Pinned at the type system layer for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_slis() -> &'static [Sli; 7] {
    &[
        Sli::AvailCasGet,
        Sli::AvailCasPut,
        Sli::AvailAcLookup,
        Sli::AvailAuth,
        Sli::LatencyCasGetP99,
        Sli::DedupRatio,
        Sli::RateLimitWithinQuota,
    ]
}

/// Canonical SLO definition: the SLI being measured + the target
/// availability percent + the derived error budget percent. Used by
/// [`crate::calculator::BurnRateCalculator`] to compute the
/// per-window burn-rate decision per Google SRE Workbook Ch 5
/// Table 4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SloDefinition {
    /// The SLI being measured.
    pub sli: Sli,
    /// Target availability as a fraction in `(0.0, 1.0]` (e.g. 0.999
    /// for 99.9 %). Strict upper bound 1.0 inclusive (a 100 %
    /// target with zero budget is allowed; some correctness SLIs are
    /// 100 % per `slo_catalog.md §3.1` — they treat any error as
    /// a budget breach which translates to a `PageSev0` arm at any
    /// non-zero error rate).
    pub target_pct: f64,
    /// Derived error budget percent: `1.0 - target_pct`. Stored
    /// pre-computed so the calculator does not pay a subtraction per
    /// evaluation hot-path.
    pub error_budget_pct: f64,
    /// Rolling SLO window in days (canonical 30 per `slo_catalog.md`).
    pub window_days: u32,
}

impl SloDefinition {
    /// Construct a canonical 30-day-window SLO from `(sli, target_pct)`.
    ///
    /// # Errors
    ///
    /// Returns [`SloError::InvalidSloDefinition`] when:
    /// - `target_pct` is NaN, infinite, ≤ 0.0, or > 1.0.
    pub fn new(sli: Sli, target_pct: f64) -> Result<Self, SloError> {
        Self::with_window_days(sli, target_pct, 30)
    }

    /// Construct an SLO with an explicit rolling window in days.
    ///
    /// # Errors
    ///
    /// Returns [`SloError::InvalidSloDefinition`] when:
    /// - `target_pct` is NaN, infinite, ≤ 0.0, or > 1.0;
    /// - `window_days` is 0.
    pub fn with_window_days(
        sli: Sli,
        target_pct: f64,
        window_days: u32,
    ) -> Result<Self, SloError> {
        if !target_pct.is_finite() || target_pct <= 0.0 || target_pct > 1.0 {
            return Err(SloError::InvalidSloDefinition(format!(
                "target_pct must be in (0.0, 1.0]; got {target_pct}"
            )));
        }
        if window_days == 0 {
            return Err(SloError::InvalidSloDefinition(
                "window_days must be ≥ 1".to_string(),
            ));
        }
        let error_budget_pct = 1.0 - target_pct;
        Ok(Self {
            sli,
            target_pct,
            error_budget_pct,
            window_days,
        })
    }

    /// Return the canonical SLI slug for audit records + alert
    /// annotations.
    #[must_use]
    pub const fn sli_slug(&self) -> &'static str {
        self.sli.slug()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_slis_unique_slugs_pinned() {
        let v = canonical_slis();
        assert_eq!(v.len(), 7);
        let mut set = std::collections::HashSet::new();
        for s in v {
            assert!(s.slug().starts_with("SLI-") || s.slug().starts_with("SLO-"));
            assert!(set.insert(s.slug()), "duplicate sli slug: {s}");
        }
    }

    #[test]
    fn prometheus_metric_base_lowercase_underscore() {
        for s in canonical_slis() {
            let base = s.prometheus_metric_base();
            assert!(base.starts_with("corelink_"));
            assert!(base.bytes().all(|b| b == b'_' || b.is_ascii_lowercase()));
        }
    }

    #[test]
    fn slo_new_rejects_invalid_target_pct() {
        assert!(SloDefinition::new(Sli::AvailCasGet, 0.0).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, -0.1).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, 1.1).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, f64::NAN).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, f64::INFINITY).is_err());
    }

    #[test]
    fn slo_new_accepts_canonical_targets() {
        let s = SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap();
        assert_eq!(s.sli, Sli::AvailCasGet);
        assert_eq!(s.target_pct, 0.999);
        assert!((s.error_budget_pct - 0.001).abs() < 1e-12);
        assert_eq!(s.window_days, 30);
    }

    #[test]
    fn slo_with_window_days_rejects_zero() {
        assert!(
            SloDefinition::with_window_days(Sli::AvailCasGet, 0.999, 0)
                .is_err()
        );
    }

    #[test]
    fn slo_target_100pct_yields_zero_budget() {
        let s = SloDefinition::new(Sli::DedupRatio, 1.0).unwrap();
        assert_eq!(s.target_pct, 1.0);
        assert_eq!(s.error_budget_pct, 0.0);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Sli::AvailCasGet), "SLI-AVAIL-CAS-GET");
    }
}

//! Burn-rate calculator: pure-logic Google SRE Workbook Ch 5 Table 4
//! decision tree.
//!
//! ## Why pure-logic
//!
//! The calculator is intentionally side-effect-free + the orchestrator
//! ([`crate::alert::MultiBurnRateAlert`]) handles the audit + dispatch
//! envelope. This makes the canonical Google SRE Workbook Table 4
//! boundary discipline a falsifiability target via 10k-iter property
//! tests against [`BurnRateCalculator::decide`] directly — without
//! needing to construct a full orchestrator + audit sink fixture.
//!
//! ## Boundary discipline
//!
//! Per Google SRE Workbook Ch 5 §"The matrix": the decision arm is the
//! canonical `error_rate ≥ multiplier × error_budget_pct` (≥, NOT >).
//! Equality at the boundary fires the corresponding arm — calibration
//! drift on the boundary inequality direction is a documented S-09
//! sprint-close pitfall (slipping `>` for `≥` would silently raise
//! recall by ~3 % per 14.4× × 0.1 % calibration = 1.44 % boundary;
//! pinned by `prop_alert_decision_canonical_table4`).

use crate::decision::AlertDecision;
use crate::definition::SloDefinition;
use crate::window::BurnRateWindow;

/// Canonical burn-rate sample: the (errors, total) tuple observed in a
/// given evaluation window. The ratio `errors / total` is the
/// short-window error rate; the calculator compares it against
/// `multiplier × error_budget_pct` to decide the alert arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurnRateSample {
    /// Number of errors observed in the window.
    pub errors_in_window: u64,
    /// Total events observed in the window (errors + successes;
    /// 4xx-legitimate excluded per `slo_catalog.md §2 filters
    /// "valid"`).
    pub total_in_window: u64,
}

impl BurnRateSample {
    /// Construct a fresh sample.
    #[must_use]
    pub const fn new(errors_in_window: u64, total_in_window: u64) -> Self {
        Self {
            errors_in_window,
            total_in_window,
        }
    }
}

/// Pure-logic burn-rate calculator. The struct is `Copy` + zero-sized;
/// the orchestrator owns the per-instance state (per-tenant ledger +
/// audit sink + dispatcher) so the calculator can be exercised
/// directly in property tests against the Google SRE Workbook Ch 5
/// Table 4 boundary discipline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BurnRateCalculator;

impl BurnRateCalculator {
    /// Construct a fresh calculator (zero-sized; const-time).
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Compute the canonical error rate for a sample. Returns 0.0
    /// when `total_in_window` is 0 (a degenerate "no traffic" sample
    /// is always `Quiet` per the canonical Google SRE convention —
    /// the alert path is for sustained burn, not for absent traffic;
    /// absent-traffic alerting is a separate canary primitive
    /// deferred to WI-S09-007 synthetic canary). This is also a
    /// natural division-by-zero defense.
    #[must_use]
    pub fn error_rate_in_window(&self, sample: BurnRateSample) -> f64 {
        if sample.total_in_window == 0 {
            return 0.0;
        }
        // Cast through f64 explicitly so the saturating_add() of u64s
        // upstream cannot perturb the ratio. The cast is lossless for
        // u64 → f64 below 2^53; alert windows never accumulate that
        // many events (would imply 28T events in a 1h window).
        let errors = sample.errors_in_window as f64;
        let total = sample.total_in_window as f64;
        errors / total
    }

    /// Decide the canonical alert arm per Google SRE Workbook Ch 5
    /// Table 4 boundary discipline:
    ///
    /// - `Fast1h × 14.4×` arm above threshold → `PageSev0`.
    /// - `Medium6h × 6×` arm above threshold → `PageSev1`.
    /// - `Slow24h × 3×` arm above threshold → `TicketSev2`.
    /// - `Long3d × 1×` arm above threshold → `TicketSev3`.
    /// - else → `Quiet`.
    ///
    /// Boundary inequality is `error_rate ≥ multiplier ×
    /// error_budget_pct` per Google SRE canonical. The implementation
    /// uses `error_rate >= threshold` semantics; IEEE 754 imprecision
    /// at the exact mathematical boundary may shift one ULP either
    /// direction (a documented Google SRE alerting calibration
    /// pitfall), so production calibration adds a configurable
    /// debounce window via Prometheus `for:` clause per
    /// [`BurnRateWindow::alertmanager_for_seconds`] rather than
    /// relying on per-sample boundary precision.
    #[must_use]
    pub fn decide(
        &self,
        slo: SloDefinition,
        window: BurnRateWindow,
        sample: BurnRateSample,
    ) -> AlertDecision {
        let error_rate = self.error_rate_in_window(sample);
        if error_rate <= 0.0 {
            return AlertDecision::Quiet;
        }
        let threshold = window.threshold_multiplier() * slo.error_budget_pct;
        if !error_rate.is_finite() || error_rate < threshold {
            return AlertDecision::Quiet;
        }
        match window {
            BurnRateWindow::Fast1h => AlertDecision::PageSev0,
            BurnRateWindow::Medium6h => AlertDecision::PageSev1,
            BurnRateWindow::Slow24h => AlertDecision::TicketSev2,
            BurnRateWindow::Long3d => AlertDecision::TicketSev3,
        }
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
    use crate::definition::Sli;

    fn slo_999() -> SloDefinition {
        SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap()
    }

    #[test]
    fn error_rate_zero_total_returns_zero() {
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(0, 0);
        assert_eq!(c.error_rate_in_window(s), 0.0);
    }

    #[test]
    fn error_rate_canonical_division() {
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(7, 100);
        assert_eq!(c.error_rate_in_window(s), 0.07);
    }

    #[test]
    fn decide_quiet_on_zero_burn() {
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(0, 1_000_000);
        for w in *crate::window::canonical_burn_rate_windows() {
            assert_eq!(c.decide(slo_999(), w, s), AlertDecision::Quiet);
        }
    }

    #[test]
    fn decide_page_sev0_on_fast_1h_above_14_4x() {
        let c = BurnRateCalculator::new();
        // 99.9 % SLO → 0.1 % budget × 14.4 = 1.44 % threshold.
        // Sample at 2 % error rate.
        let s = BurnRateSample::new(20, 1_000);
        let d = c.decide(slo_999(), BurnRateWindow::Fast1h, s);
        assert_eq!(d, AlertDecision::PageSev0);
    }

    #[test]
    fn decide_page_sev1_on_medium_6h_above_6x() {
        let c = BurnRateCalculator::new();
        // 0.1 % × 6 = 0.6 % threshold; sample at 1 %.
        let s = BurnRateSample::new(10, 1_000);
        let d = c.decide(slo_999(), BurnRateWindow::Medium6h, s);
        assert_eq!(d, AlertDecision::PageSev1);
    }

    #[test]
    fn decide_ticket_sev2_on_slow_24h_above_3x() {
        let c = BurnRateCalculator::new();
        // 0.1 % × 3 = 0.3 % threshold; sample at 0.5 %.
        let s = BurnRateSample::new(5, 1_000);
        let d = c.decide(slo_999(), BurnRateWindow::Slow24h, s);
        assert_eq!(d, AlertDecision::TicketSev2);
    }

    #[test]
    fn decide_ticket_sev3_on_long_3d_above_1x() {
        let c = BurnRateCalculator::new();
        // 0.1 % × 1 = 0.1 % threshold; sample at 0.2 %.
        let s = BurnRateSample::new(2, 1_000);
        let d = c.decide(slo_999(), BurnRateWindow::Long3d, s);
        assert_eq!(d, AlertDecision::TicketSev3);
    }

    #[test]
    fn decide_at_canonical_boundary_with_clean_arithmetic() {
        // Google SRE canonical inequality is `error_rate >= threshold`;
        // floats are not exact at every boundary, so the test pins
        // a sample one ULP-equivalent above the threshold (0.001 *
        // 14.4 = 0.0144 in math; 145/10000 = 0.0145 is unambiguously
        // above the IEEE 754 representation regardless of associativity).
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(145, 10_000);
        let d = c.decide(slo_999(), BurnRateWindow::Fast1h, s);
        assert_eq!(d, AlertDecision::PageSev0);
    }

    #[test]
    fn decide_just_below_boundary_quiet() {
        let c = BurnRateCalculator::new();
        // 1.43 % < 1.44 %.
        let s = BurnRateSample::new(143, 10_000);
        let d = c.decide(slo_999(), BurnRateWindow::Fast1h, s);
        assert_eq!(d, AlertDecision::Quiet);
    }

    #[test]
    fn decide_correctness_slo_pages_at_any_nonzero_error() {
        // Correctness SLOs at 100 % have zero budget; ANY non-zero
        // error fires the canonical arm at every window.
        let c = BurnRateCalculator::new();
        let slo = SloDefinition::new(Sli::DedupRatio, 1.0).unwrap();
        let s = BurnRateSample::new(1, 1_000_000_000);
        assert_eq!(
            c.decide(slo, BurnRateWindow::Fast1h, s),
            AlertDecision::PageSev0
        );
        assert_eq!(
            c.decide(slo, BurnRateWindow::Long3d, s),
            AlertDecision::TicketSev3
        );
    }

    #[test]
    fn decide_quiet_when_total_zero() {
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(0, 0);
        for w in *crate::window::canonical_burn_rate_windows() {
            assert_eq!(c.decide(slo_999(), w, s), AlertDecision::Quiet);
        }
    }
}

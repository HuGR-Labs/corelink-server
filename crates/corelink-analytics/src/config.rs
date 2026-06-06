//! Tunable knobs for the analytics emitter — pinned to canonical
//! defaults per WI-S09-001 §1 invariants.
//!
//! ## F-001 closure
//!
//! The config struct is plain-data; per-instance lifetime mirrors the
//! [`crate::observer::InMemoryRedMetrics`] orchestrator (no
//! `static LazyLock`). Tests instantiate fresh configs per case.

use crate::canonical::RedMetricKind;

/// Canonical default per-metric cardinality budget (max unique label
/// tuples allowed before the validator fails-closed). Lifted from
/// WI-S09-001 §1 invariant 1 + sprint contract §5.1 R-S09-2.
pub const CANONICAL_PER_METRIC_BUDGET: u64 = 20_000;

/// Canonical default global cardinality budget (max unique label
/// tuples summed across all metrics). Lifted from WI-S09-001 §1
/// invariant 1 + sprint contract §5.1 R-S09-2.
pub const CANONICAL_GLOBAL_BUDGET: u64 = 100_000;

/// Canonical histogram bucket boundaries (seconds; OpenMetrics 1.0 +
/// Prom default for HTTP-like duration distributions). 11 boundaries
/// per Lote 10.9bis P0-I; combined with `+Inf` bucket + `_sum` +
/// `_count` series this expands the cardinality of a single histogram
/// `(label_combo)` to 13 effective series.
///
/// Boundaries: 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s,
/// 5s, 10s.
pub const CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES: [f64; 11] = [
    0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.0, 2.5, 5.0, 10.0,
];

/// Tunable knobs for the analytics emitter.
///
/// `Clone + Debug` to make property tests + integration tests
/// ergonomic; field accessors return owned values to keep the public
/// surface stable across knob amendments.
#[derive(Clone, Debug)]
pub struct AnalyticsConfig {
    per_metric_budget: u64,
    global_budget: u64,
    histogram_bucket_boundaries: Vec<f64>,
    cardinality_approaching_pct: f64,
}

impl AnalyticsConfig {
    /// Construct with the canonical defaults.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            per_metric_budget: CANONICAL_PER_METRIC_BUDGET,
            global_budget: CANONICAL_GLOBAL_BUDGET,
            histogram_bucket_boundaries: CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES.to_vec(),
            cardinality_approaching_pct: 0.80,
        }
    }

    /// Per-metric cardinality budget (canonical 20k).
    #[must_use]
    pub const fn per_metric_budget(&self) -> u64 {
        self.per_metric_budget
    }

    /// Global cardinality budget (canonical 100k).
    #[must_use]
    pub const fn global_budget(&self) -> u64 {
        self.global_budget
    }

    /// Canonical histogram bucket boundaries (seconds).
    #[must_use]
    pub fn histogram_bucket_boundaries(&self) -> &[f64] {
        &self.histogram_bucket_boundaries
    }

    /// Threshold (`unique_tuples / per_metric_budget`) above which the
    /// validator emits `corelink_metrics_cardinality_approaching_budget_total`
    /// SEV-3 audit. Canonical 0.80 (80% per WI §6.1.11).
    #[must_use]
    pub const fn cardinality_approaching_pct(&self) -> f64 {
        self.cardinality_approaching_pct
    }

    /// Per-metric budget override (used by the durable mirror at cold
    /// start when a row in `analytics_cardinality_budgets` carries an
    /// explicit non-default budget). Returns the canonical default
    /// when no override is registered.
    ///
    /// This wrapper exists so the production wiring composes a
    /// per-metric `HashMap<RedMetricKind, u64>` override layer at
    /// construction time and the validator queries via this function;
    /// the in-memory test surface returns the canonical default
    /// uniformly.
    #[must_use]
    pub const fn budget_for_metric(&self, _metric: RedMetricKind) -> u64 {
        self.per_metric_budget
    }

    /// Construct with explicit per-metric + global budgets. Production
    /// code uses this only at cold-start hydration from the durable
    /// `analytics_cardinality_budgets` D1 mirror; tests use this to
    /// dial the budget down so the rejection arm fires without
    /// generating 20 001 unique tuples per case.
    ///
    /// Both budgets MUST be at least 1; per-metric MUST NOT exceed
    /// global (defense-in-depth: a single metric can't be configured
    /// to consume the entire global budget unless the global budget
    /// is also that large).
    #[must_use]
    pub fn with_budgets(per_metric: u64, global: u64) -> Self {
        let per_metric = per_metric.max(1);
        let global = global.max(per_metric);
        Self {
            per_metric_budget: per_metric,
            global_budget: global,
            histogram_bucket_boundaries: CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES.to_vec(),
            cardinality_approaching_pct: 0.80,
        }
    }
}

impl Default for AnalyticsConfig {
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
    fn canonical_budgets_pinned() {
        let c = AnalyticsConfig::canonical();
        assert_eq!(c.per_metric_budget(), 20_000);
        assert_eq!(c.global_budget(), 100_000);
        assert!((c.cardinality_approaching_pct() - 0.80).abs() < 1e-9);
    }

    #[test]
    fn histogram_buckets_have_11_canonical_boundaries() {
        let c = AnalyticsConfig::canonical();
        assert_eq!(c.histogram_bucket_boundaries().len(), 11);
        // Ascending order is required for bucket placement
        // determinism.
        for window in c.histogram_bucket_boundaries().windows(2) {
            let lo = window.first().copied().unwrap_or_default();
            let hi = window.get(1).copied().unwrap_or_default();
            assert!(
                lo < hi,
                "histogram boundaries not strictly ascending: lo={lo} hi={hi}"
            );
        }
    }

    #[test]
    fn budget_for_every_canonical_metric_returns_default() {
        let c = AnalyticsConfig::canonical();
        for k in crate::canonical::canonical_metric_kinds() {
            assert_eq!(c.budget_for_metric(*k), 20_000);
        }
    }

    #[test]
    fn default_matches_canonical() {
        let a = AnalyticsConfig::default();
        let b = AnalyticsConfig::canonical();
        assert_eq!(a.per_metric_budget(), b.per_metric_budget());
        assert_eq!(a.global_budget(), b.global_budget());
        assert_eq!(
            a.histogram_bucket_boundaries(),
            b.histogram_bucket_boundaries()
        );
    }

    #[test]
    fn with_budgets_clamps_to_minimum_one() {
        let c = AnalyticsConfig::with_budgets(0, 0);
        assert_eq!(c.per_metric_budget(), 1);
        assert_eq!(c.global_budget(), 1);
    }

    #[test]
    fn with_budgets_clamps_global_to_at_least_per_metric() {
        let c = AnalyticsConfig::with_budgets(100, 50);
        assert_eq!(c.per_metric_budget(), 100);
        assert_eq!(c.global_budget(), 100);
    }

    #[test]
    fn with_budgets_overrides_per_metric_lookup() {
        let c = AnalyticsConfig::with_budgets(50, 1000);
        for k in crate::canonical::canonical_metric_kinds() {
            assert_eq!(c.budget_for_metric(*k), 50);
        }
    }
}

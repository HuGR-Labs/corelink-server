//! RED metrics observer trait + InMemory capture sink + Failing sink.
//!
//! ## RED triple per metric
//!
//! Per `observability_model.md §4.2` + Google SRE Workbook §6 RED
//! method definition:
//!
//! - **Rate** (counter; monotone increment) — `record_rate(metric,
//!   labels, by)`.
//! - **Errors** (counter; monotone increment) — `record_error(metric,
//!   labels, by)`.
//! - **Duration** (histogram observation; bucketed per
//!   `AnalyticsConfig::histogram_bucket_boundaries`) — `record_duration(
//!   metric, labels, value_seconds)`.
//!
//! Gauge (overwrite semantics) is also part of the surface for the
//! USE metrics + `corelink_dedup_ratio` / `corelink_privacy_dsr_active_total`.
//!
//! ## F-001 closure
//!
//! The InMemory sink holds state under a per-instance `Arc<Mutex<>>`
//! (NOT a process-global `static LazyLock`). Tests instantiate fresh
//! sinks per case so the orchestrator harness cannot accidentally
//! leak state across cases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::canonical::RedMetricKind;
use crate::labels::MetricLabelTuple;

/// Errors surfaced by [`RedMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RedMetricsObserverError {
    /// Metric backend transport failure (e.g. AE write rejected /
    /// counter sink down).
    #[error("RED metrics observer backend error: {0}")]
    Backend(String),
}

/// Trait surface every backend (production CF Workers Analytics
/// Engine binding / Prom remote write shim / in-memory test fake)
/// implements.
pub trait RedMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment the rate counter for `metric` × `labels` by `by`.
    /// `by == 0` is a no-op at the observer layer (the validator
    /// still counts the unique tuple per the `prop_idempotent_zero_value_emit`
    /// invariant).
    ///
    /// # Errors
    ///
    /// Returns [`RedMetricsObserverError::Backend`] on backend failure.
    fn record_rate(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        by: u64,
    ) -> Result<(), RedMetricsObserverError>;

    /// Increment the error counter for `metric` × `labels` by `by`.
    ///
    /// # Errors
    ///
    /// Returns [`RedMetricsObserverError::Backend`] on backend failure.
    fn record_error(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        by: u64,
    ) -> Result<(), RedMetricsObserverError>;

    /// Observe a duration sample (seconds) for `metric` × `labels`.
    /// The value is placed into the canonical bucket per
    /// `AnalyticsConfig::histogram_bucket_boundaries`. Negative values
    /// are clamped to 0.0 by the observer (no panic).
    ///
    /// # Errors
    ///
    /// Returns [`RedMetricsObserverError::Backend`] on backend failure.
    fn record_duration(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        value_seconds: f64,
    ) -> Result<(), RedMetricsObserverError>;

    /// Set a gauge value for `metric` × `labels` (overwrite). Caller
    /// is responsible for ensuring the `metric` is in
    /// `RedMetricKind::is_gauge()`; the observer does NOT enforce kind
    /// consistency — that's the validator + the type system at the
    /// trait surface.
    ///
    /// # Errors
    ///
    /// Returns [`RedMetricsObserverError::Backend`] on backend failure.
    fn set_gauge(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        value: f64,
    ) -> Result<(), RedMetricsObserverError>;
}

/// In-memory RED metrics observer. Captures every recorded value for
/// property-test assertion.
///
/// F-001 closure preserved — per-instance `Arc<Mutex<>>` (no
/// process-global state). Cloning shares the underlying buffers.
#[derive(Clone, Default, Debug)]
pub struct InMemoryRedMetrics {
    rate_counters: Arc<Mutex<HashMap<MetricKey, u64>>>,
    error_counters: Arc<Mutex<HashMap<MetricKey, u64>>>,
    duration_buckets: Arc<Mutex<HashMap<MetricKey, BucketHistogram>>>,
    gauges: Arc<Mutex<HashMap<MetricKey, f64>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MetricKey {
    metric: RedMetricKind,
    labels: MetricLabelTuple,
}

#[derive(Clone, Debug, Default)]
struct BucketHistogram {
    boundaries: Vec<f64>,
    counts_per_bucket: Vec<u64>,
    sum: f64,
    count: u64,
}

impl BucketHistogram {
    fn with_boundaries(boundaries: &[f64]) -> Self {
        let n = boundaries.len();
        // n boundaries → n + 1 buckets (the last is `+Inf`).
        Self {
            boundaries: boundaries.to_vec(),
            counts_per_bucket: vec![0; n.saturating_add(1)],
            sum: 0.0,
            count: 0,
        }
    }

    fn observe(&mut self, value_seconds: f64) {
        let v = if value_seconds.is_nan() || value_seconds.is_sign_negative() {
            0.0
        } else {
            value_seconds
        };
        self.sum += v;
        self.count = self.count.saturating_add(1);
        let mut placed = false;
        for (idx, boundary) in self.boundaries.iter().enumerate() {
            if v <= *boundary {
                if let Some(slot) = self.counts_per_bucket.get_mut(idx) {
                    *slot = slot.saturating_add(1);
                }
                placed = true;
                break;
            }
        }
        if !placed {
            // `+Inf` bucket — always the last slot.
            if let Some(slot) = self.counts_per_bucket.last_mut() {
                *slot = slot.saturating_add(1);
            }
        }
    }
}

impl InMemoryRedMetrics {
    /// Construct a fresh observer with the canonical histogram bucket
    /// boundaries (Lote 10.9bis P0-I; 11 boundaries + `+Inf`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the rate counter for a `metric` × `labels` tuple.
    #[must_use]
    pub fn rate_counter(&self, metric: RedMetricKind, labels: MetricLabelTuple) -> u64 {
        let g = match self.rate_counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&MetricKey { metric, labels })
            .copied()
            .unwrap_or_default()
    }

    /// Snapshot the error counter for a `metric` × `labels` tuple.
    #[must_use]
    pub fn error_counter(&self, metric: RedMetricKind, labels: MetricLabelTuple) -> u64 {
        let g = match self.error_counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&MetricKey { metric, labels })
            .copied()
            .unwrap_or_default()
    }

    /// Snapshot the duration histogram bucket counts for a `metric`
    /// × `labels` tuple. Returns `None` when no observation was
    /// ever recorded for the tuple.
    #[must_use]
    pub fn duration_buckets(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
    ) -> Option<Vec<u64>> {
        let g = match self.duration_buckets.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&MetricKey { metric, labels })
            .map(|h| h.counts_per_bucket.clone())
    }

    /// Snapshot the duration histogram aggregate `(sum, count)` for a
    /// `metric` × `labels` tuple.
    #[must_use]
    pub fn duration_sum_count(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
    ) -> Option<(f64, u64)> {
        let g = match self.duration_buckets.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&MetricKey { metric, labels })
            .map(|h| (h.sum, h.count))
    }

    /// Snapshot the gauge value for a `metric` × `labels` tuple.
    #[must_use]
    pub fn gauge(&self, metric: RedMetricKind, labels: MetricLabelTuple) -> Option<f64> {
        let g = match self.gauges.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&MetricKey { metric, labels }).copied()
    }

    /// Aggregate counter total across every label combination for the
    /// given metric.
    #[must_use]
    pub fn rate_counter_total(&self, metric: RedMetricKind) -> u64 {
        let g = match self.rate_counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter()
            .filter(|(k, _)| k.metric == metric)
            .map(|(_, v)| *v)
            .sum()
    }

    /// Aggregate error counter total across every label combination
    /// for the given metric.
    #[must_use]
    pub fn error_counter_total(&self, metric: RedMetricKind) -> u64 {
        let g = match self.error_counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter()
            .filter(|(k, _)| k.metric == metric)
            .map(|(_, v)| *v)
            .sum()
    }

    /// Compute the canonical bucket index for a value given the
    /// canonical boundaries. Exposed for property-test assertions
    /// (`prop_red_duration_histogram_bucket_correct`).
    ///
    /// Returns `boundaries.len()` (the `+Inf` slot) when `value`
    /// exceeds every boundary.
    #[must_use]
    pub fn canonical_bucket_index(boundaries: &[f64], value: f64) -> usize {
        let v = if value.is_nan() || value.is_sign_negative() {
            0.0
        } else {
            value
        };
        for (idx, boundary) in boundaries.iter().enumerate() {
            if v <= *boundary {
                return idx;
            }
        }
        boundaries.len()
    }
}

impl RedMetricsObserver for InMemoryRedMetrics {
    fn record_rate(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        by: u64,
    ) -> Result<(), RedMetricsObserverError> {
        let mut g = self.rate_counters.lock().map_err(|_| {
            RedMetricsObserverError::Backend("rate counter mutex poisoned".to_string())
        })?;
        let entry = g.entry(MetricKey { metric, labels }).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }

    fn record_error(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        by: u64,
    ) -> Result<(), RedMetricsObserverError> {
        let mut g = self.error_counters.lock().map_err(|_| {
            RedMetricsObserverError::Backend("error counter mutex poisoned".to_string())
        })?;
        let entry = g.entry(MetricKey { metric, labels }).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }

    fn record_duration(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        value_seconds: f64,
    ) -> Result<(), RedMetricsObserverError> {
        let mut g = self.duration_buckets.lock().map_err(|_| {
            RedMetricsObserverError::Backend("duration bucket mutex poisoned".to_string())
        })?;
        let entry = g.entry(MetricKey { metric, labels }).or_insert_with(|| {
            BucketHistogram::with_boundaries(&crate::config::CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES)
        });
        entry.observe(value_seconds);
        Ok(())
    }

    fn set_gauge(
        &self,
        metric: RedMetricKind,
        labels: MetricLabelTuple,
        value: f64,
    ) -> Result<(), RedMetricsObserverError> {
        let mut g = self
            .gauges
            .lock()
            .map_err(|_| RedMetricsObserverError::Backend("gauge mutex poisoned".to_string()))?;
        g.insert(MetricKey { metric, labels }, value);
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of the audit
/// fail-closed envelope (the validator MUST roll back on observer
/// failure when the wiring layer composes audit-emit-then-observer-call;
/// the in-tree validator surface is the audit-then-observer canonical
/// order).
#[derive(Debug, Default)]
pub struct FailingRedMetrics;

impl FailingRedMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RedMetricsObserver for FailingRedMetrics {
    fn record_rate(
        &self,
        _m: RedMetricKind,
        _l: MetricLabelTuple,
        _by: u64,
    ) -> Result<(), RedMetricsObserverError> {
        Err(RedMetricsObserverError::Backend(
            "induced RED metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_error(
        &self,
        _m: RedMetricKind,
        _l: MetricLabelTuple,
        _by: u64,
    ) -> Result<(), RedMetricsObserverError> {
        Err(RedMetricsObserverError::Backend(
            "induced RED metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_duration(
        &self,
        _m: RedMetricKind,
        _l: MetricLabelTuple,
        _v: f64,
    ) -> Result<(), RedMetricsObserverError> {
        Err(RedMetricsObserverError::Backend(
            "induced RED metrics failure (test fixture)".to_string(),
        ))
    }

    fn set_gauge(
        &self,
        _m: RedMetricKind,
        _l: MetricLabelTuple,
        _v: f64,
    ) -> Result<(), RedMetricsObserverError> {
        Err(RedMetricsObserverError::Backend(
            "induced RED metrics failure (test fixture)".to_string(),
        ))
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
    use crate::config::CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES;
    use crate::labels::{Region, Tier};

    fn labels() -> MetricLabelTuple {
        MetricLabelTuple::tenant_region(Tier::Team, Region::Iad)
    }

    #[test]
    fn rate_counter_increments_under_label_tuple() {
        let m = InMemoryRedMetrics::new();
        m.record_rate(RedMetricKind::CasPutRequestsTotal, labels(), 1)
            .unwrap();
        m.record_rate(RedMetricKind::CasPutRequestsTotal, labels(), 2)
            .unwrap();
        assert_eq!(
            m.rate_counter(RedMetricKind::CasPutRequestsTotal, labels()),
            3
        );
        assert_eq!(m.rate_counter_total(RedMetricKind::CasPutRequestsTotal), 3);
    }

    #[test]
    fn error_counter_increments_under_label_tuple() {
        let m = InMemoryRedMetrics::new();
        m.record_error(RedMetricKind::CasPutRequestsTotal, labels(), 5)
            .unwrap();
        assert_eq!(
            m.error_counter(RedMetricKind::CasPutRequestsTotal, labels()),
            5
        );
    }

    #[test]
    fn duration_observation_lands_in_canonical_bucket() {
        let m = InMemoryRedMetrics::new();
        // 0.004s = 4ms; first canonical boundary is 0.005s → bucket 0.
        m.record_duration(RedMetricKind::CasPutDurationSeconds, labels(), 0.004)
            .unwrap();
        let buckets = m
            .duration_buckets(RedMetricKind::CasPutDurationSeconds, labels())
            .unwrap();
        // 11 boundaries + +Inf = 12 buckets.
        assert_eq!(buckets.len(), 12);
        assert_eq!(buckets[0], 1);
        for (idx, c) in buckets.iter().enumerate().skip(1) {
            assert_eq!(*c, 0, "non-zero count at bucket {idx}: {c}");
        }
        let (sum, count) = m
            .duration_sum_count(RedMetricKind::CasPutDurationSeconds, labels())
            .unwrap();
        assert!((sum - 0.004).abs() < 1e-9);
        assert_eq!(count, 1);
    }

    #[test]
    fn duration_above_all_boundaries_lands_in_inf_bucket() {
        let m = InMemoryRedMetrics::new();
        m.record_duration(RedMetricKind::CasPutDurationSeconds, labels(), 999.0)
            .unwrap();
        let buckets = m
            .duration_buckets(RedMetricKind::CasPutDurationSeconds, labels())
            .unwrap();
        let last = buckets.last().copied().unwrap_or(0);
        assert_eq!(last, 1);
    }

    #[test]
    fn duration_negative_clamped_to_zero() {
        let m = InMemoryRedMetrics::new();
        m.record_duration(RedMetricKind::CasPutDurationSeconds, labels(), -1.0)
            .unwrap();
        let (sum, _count) = m
            .duration_sum_count(RedMetricKind::CasPutDurationSeconds, labels())
            .unwrap();
        assert_eq!(sum, 0.0);
    }

    #[test]
    fn gauge_overwrites_previous_value() {
        let m = InMemoryRedMetrics::new();
        m.set_gauge(RedMetricKind::DedupRatio, labels(), 0.5)
            .unwrap();
        m.set_gauge(RedMetricKind::DedupRatio, labels(), 0.75)
            .unwrap();
        assert_eq!(m.gauge(RedMetricKind::DedupRatio, labels()), Some(0.75));
    }

    #[test]
    fn canonical_bucket_index_helper_matches_observation() {
        let bnd = &CANONICAL_HISTOGRAM_BUCKET_BOUNDARIES;
        assert_eq!(InMemoryRedMetrics::canonical_bucket_index(bnd, 0.001), 0);
        assert_eq!(InMemoryRedMetrics::canonical_bucket_index(bnd, 0.005), 0);
        assert_eq!(InMemoryRedMetrics::canonical_bucket_index(bnd, 0.006), 1);
        assert_eq!(InMemoryRedMetrics::canonical_bucket_index(bnd, 999.0), 11);
        assert_eq!(InMemoryRedMetrics::canonical_bucket_index(bnd, -1.0), 0);
    }

    #[test]
    fn failing_observer_returns_backend_error_for_each_arm() {
        let m = FailingRedMetrics::new();
        assert!(matches!(
            m.record_rate(RedMetricKind::CasPutRequestsTotal, labels(), 1)
                .unwrap_err(),
            RedMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_error(RedMetricKind::CasPutRequestsTotal, labels(), 1)
                .unwrap_err(),
            RedMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_duration(RedMetricKind::CasPutDurationSeconds, labels(), 1.0)
                .unwrap_err(),
            RedMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.set_gauge(RedMetricKind::DedupRatio, labels(), 0.5)
                .unwrap_err(),
            RedMetricsObserverError::Backend(_)
        ));
    }

    #[test]
    fn cloned_observer_shares_buffer() {
        let s1 = InMemoryRedMetrics::new();
        let s2 = s1.clone();
        s1.record_rate(RedMetricKind::CasPutRequestsTotal, labels(), 7)
            .unwrap();
        assert_eq!(
            s2.rate_counter(RedMetricKind::CasPutRequestsTotal, labels()),
            7
        );
    }

    #[test]
    fn rate_counter_zero_increment_no_op() {
        let m = InMemoryRedMetrics::new();
        m.record_rate(RedMetricKind::CasPutRequestsTotal, labels(), 0)
            .unwrap();
        assert_eq!(
            m.rate_counter(RedMetricKind::CasPutRequestsTotal, labels()),
            0
        );
    }

    #[test]
    fn label_tuples_distinct_under_metric_isolation() {
        let m = InMemoryRedMetrics::new();
        let team_iad = MetricLabelTuple::tenant_region(Tier::Team, Region::Iad);
        let free_iad = MetricLabelTuple::tenant_region(Tier::Free, Region::Iad);
        m.record_rate(RedMetricKind::CasPutRequestsTotal, team_iad, 10)
            .unwrap();
        m.record_rate(RedMetricKind::CasPutRequestsTotal, free_iad, 5)
            .unwrap();
        assert_eq!(
            m.rate_counter(RedMetricKind::CasPutRequestsTotal, team_iad),
            10
        );
        assert_eq!(
            m.rate_counter(RedMetricKind::CasPutRequestsTotal, free_iad),
            5
        );
        assert_eq!(m.rate_counter_total(RedMetricKind::CasPutRequestsTotal), 15);
    }
}

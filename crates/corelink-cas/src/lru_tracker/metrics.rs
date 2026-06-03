//! LRU tracker metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S07-004 §6.1.9 freezes the canonical 6 metrics every emission
//! path uses (CloudEvent dotted naming spec → underscored Prometheus
//! exposed name per `prometheus.io/docs/practices/naming/`; Lote
//! 10.7-tris cycle 6 clarification):
//!
//! - `corelink.lru.records_total{tenant,region}` (counter; one per
//!   `record_access` Recorded decision arm).
//! - `corelink.lru.coalesced_total` (counter; multiple records collapsed
//!   into one queue entry within the flush window).
//! - `corelink.lru.dropped_total{reason}` (counter; queue overflow OR
//!   flush errors).
//! - `corelink.lru.batch_flush_duration_ms` (histogram; per flush pass).
//! - `corelink.lru.drift_ms` (histogram; per-flush max
//!   `(now - oldest_pending_access_ms)` delta; SLO p99 ≤ 30s sustained).
//! - `corelink.lru.consistency_violation_total{tenant}` (counter;
//!   alert SEV-1 if `> 0` per WI-S07-005 alert taxonomy).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

/// Canonical metric names per WI §6.1.9 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 6] {
    &[
        "corelink.lru.records_total",
        "corelink.lru.coalesced_total",
        "corelink.lru.dropped_total",
        "corelink.lru.batch_flush_duration_ms",
        "corelink.lru.drift_ms",
        "corelink.lru.consistency_violation_total",
    ]
}

/// Errors surfaced by [`LruMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LruMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("lru metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink and
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LruMetricKind {
    /// `corelink.lru.records_total`.
    RecordsTotal,
    /// `corelink.lru.coalesced_total`.
    CoalescedTotal,
    /// `corelink.lru.dropped_total`.
    DroppedTotal,
    /// `corelink.lru.batch_flush_duration_ms`.
    BatchFlushDurationMs,
    /// `corelink.lru.drift_ms`.
    DriftMs,
    /// `corelink.lru.consistency_violation_total` — alert SEV-1.
    ConsistencyViolationTotal,
}

impl LruMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecordsTotal => "corelink.lru.records_total",
            Self::CoalescedTotal => "corelink.lru.coalesced_total",
            Self::DroppedTotal => "corelink.lru.dropped_total",
            Self::BatchFlushDurationMs => "corelink.lru.batch_flush_duration_ms",
            Self::DriftMs => "corelink.lru.drift_ms",
            Self::ConsistencyViolationTotal => "corelink.lru.consistency_violation_total",
        }
    }
}

/// Reason label for `dropped_total` — closed canonical 2-set per WI
/// §6.1.9 + §6.1.3 overflow handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LruDropReason {
    /// `reason=queue_full` — bounded queue at cap; oldest dropped.
    QueueFull,
    /// `reason=flush_error` — per-row backend error fired during flush.
    FlushError,
}

impl LruDropReason {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueueFull => "queue_full",
            Self::FlushError => "flush_error",
        }
    }
}

/// LRU metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait LruMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.lru.records_total{tenant,region}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn record_recorded(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<(), LruMetricsObserverError>;

    /// Increment `corelink.lru.coalesced_total` by 1 — emitted when an
    /// existing in-memory queue entry's `accessed_at_ms` is overwritten
    /// by a later record (the second + later records on the same
    /// (tenant, digest) are coalesced; this counter exposes the
    /// write-amplification savings).
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn record_coalesced(&self) -> Result<(), LruMetricsObserverError>;

    /// Increment `corelink.lru.dropped_total{reason}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn record_dropped(&self, reason: LruDropReason) -> Result<(), LruMetricsObserverError>;

    /// Observe `corelink.lru.batch_flush_duration_ms` — histogram.
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn observe_flush_duration_ms(&self, duration_ms: u64) -> Result<(), LruMetricsObserverError>;

    /// Observe `corelink.lru.drift_ms` — histogram. Max
    /// `(now - oldest_pending_access_ms)` observed at flush start.
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn observe_drift_ms(&self, drift_ms: u64) -> Result<(), LruMetricsObserverError>;

    /// Increment `corelink.lru.consistency_violation_total{tenant}` by 1.
    /// Alert SEV-1 when `> 0` (operator pager wake-up; INV-LRU-CONSISTENCY
    /// guard fired).
    ///
    /// # Errors
    ///
    /// Returns [`LruMetricsObserverError::Backend`] on backend failure.
    fn record_consistency_violation(&self, tenant_id: Uuid) -> Result<(), LruMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryLruMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    drift_samples: Vec<u64>,
    flush_duration_samples: Vec<u64>,
}

impl InMemoryLruMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a fully-qualified label string.
    #[must_use]
    pub fn counter(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.counters.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .counters
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Per-metric aggregate counter snapshot summing every label
    /// fragment under the canonical metric name.
    #[must_use]
    pub fn counter_total(&self, kind: LruMetricKind) -> u64 {
        let prefix = kind.as_str();
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .counters
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(_, v)| *v)
            .sum()
    }

    /// Snapshot of drift samples.
    #[must_use]
    pub fn drift_samples(&self) -> Vec<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.drift_samples.clone()
    }

    /// Snapshot of flush duration samples.
    #[must_use]
    pub fn flush_duration_samples(&self) -> Vec<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.flush_duration_samples.clone()
    }

    fn bump_counter(&self, label: String, by: u64) -> Result<(), LruMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            LruMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }
}

impl LruMetricsObserver for InMemoryLruMetrics {
    fn record_recorded(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
    ) -> Result<(), LruMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},region={}}}",
            LruMetricKind::RecordsTotal.as_str(),
            region.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn record_coalesced(&self) -> Result<(), LruMetricsObserverError> {
        let label = LruMetricKind::CoalescedTotal.as_str().to_string();
        self.bump_counter(label, 1)
    }

    fn record_dropped(&self, reason: LruDropReason) -> Result<(), LruMetricsObserverError> {
        let label = format!(
            "{}{{reason={}}}",
            LruMetricKind::DroppedTotal.as_str(),
            reason.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn observe_flush_duration_ms(&self, duration_ms: u64) -> Result<(), LruMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            LruMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        guard.flush_duration_samples.push(duration_ms);
        Ok(())
    }

    fn observe_drift_ms(&self, drift_ms: u64) -> Result<(), LruMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            LruMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        guard.drift_samples.push(drift_ms);
        Ok(())
    }

    fn record_consistency_violation(&self, tenant_id: Uuid) -> Result<(), LruMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            LruMetricKind::ConsistencyViolationTotal.as_str()
        );
        self.bump_counter(label, 1)
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingLruMetrics;

impl FailingLruMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl LruMetricsObserver for FailingLruMetrics {
    fn record_recorded(
        &self,
        _tenant_id: Uuid,
        _region: EvictionRegion,
    ) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_coalesced(&self) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_dropped(&self, _reason: LruDropReason) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_flush_duration_ms(&self, _duration_ms: u64) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_drift_ms(&self, _drift_ms: u64) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_consistency_violation(
        &self,
        _tenant_id: Uuid,
    ) -> Result<(), LruMetricsObserverError> {
        Err(LruMetricsObserverError::Backend(
            "induced lru metrics failure (test fixture)".to_string(),
        ))
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

    fn ten() -> Uuid {
        Uuid::from_u128(0x1)
    }

    #[test]
    fn canonical_metric_names_align_with_enum() {
        let names = canonical_metric_names();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&LruMetricKind::RecordsTotal.as_str()));
        assert!(names.contains(&LruMetricKind::CoalescedTotal.as_str()));
        assert!(names.contains(&LruMetricKind::DroppedTotal.as_str()));
        assert!(names.contains(&LruMetricKind::BatchFlushDurationMs.as_str()));
        assert!(names.contains(&LruMetricKind::DriftMs.as_str()));
        assert!(names.contains(&LruMetricKind::ConsistencyViolationTotal.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            LruMetricKind::RecordsTotal,
            LruMetricKind::CoalescedTotal,
            LruMetricKind::DroppedTotal,
            LruMetricKind::BatchFlushDurationMs,
            LruMetricKind::DriftMs,
            LruMetricKind::ConsistencyViolationTotal,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.lru."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn drop_reason_canonical_strings() {
        assert_eq!(LruDropReason::QueueFull.as_str(), "queue_full");
        assert_eq!(LruDropReason::FlushError.as_str(), "flush_error");
    }

    #[test]
    fn records_total_increments_per_tenant_and_region() {
        let m = InMemoryLruMetrics::new();
        m.record_recorded(ten(), EvictionRegion::Sam).unwrap();
        m.record_recorded(ten(), EvictionRegion::Sam).unwrap();
        m.record_recorded(ten(), EvictionRegion::Iad).unwrap();
        assert_eq!(m.counter_total(LruMetricKind::RecordsTotal), 3);
    }

    #[test]
    fn coalesced_total_increments() {
        let m = InMemoryLruMetrics::new();
        m.record_coalesced().unwrap();
        m.record_coalesced().unwrap();
        assert_eq!(m.counter_total(LruMetricKind::CoalescedTotal), 2);
    }

    #[test]
    fn dropped_total_segregates_by_reason() {
        let m = InMemoryLruMetrics::new();
        m.record_dropped(LruDropReason::QueueFull).unwrap();
        m.record_dropped(LruDropReason::QueueFull).unwrap();
        m.record_dropped(LruDropReason::FlushError).unwrap();
        let qf = m.counter(&format!(
            "{}{{reason=queue_full}}",
            LruMetricKind::DroppedTotal.as_str()
        ));
        let fe = m.counter(&format!(
            "{}{{reason=flush_error}}",
            LruMetricKind::DroppedTotal.as_str()
        ));
        assert_eq!(qf, 2);
        assert_eq!(fe, 1);
        assert_eq!(m.counter_total(LruMetricKind::DroppedTotal), 3);
    }

    #[test]
    fn drift_samples_captured_in_order() {
        let m = InMemoryLruMetrics::new();
        m.observe_drift_ms(100).unwrap();
        m.observe_drift_ms(50).unwrap();
        m.observe_drift_ms(0).unwrap();
        assert_eq!(m.drift_samples(), vec![100, 50, 0]);
    }

    #[test]
    fn flush_duration_samples_captured_in_order() {
        let m = InMemoryLruMetrics::new();
        m.observe_flush_duration_ms(5).unwrap();
        m.observe_flush_duration_ms(10).unwrap();
        assert_eq!(m.flush_duration_samples(), vec![5, 10]);
    }

    #[test]
    fn consistency_violation_increments_per_tenant() {
        let m = InMemoryLruMetrics::new();
        m.record_consistency_violation(ten()).unwrap();
        m.record_consistency_violation(ten()).unwrap();
        assert_eq!(m.counter_total(LruMetricKind::ConsistencyViolationTotal), 2);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingLruMetrics::new();
        assert!(matches!(
            m.record_recorded(ten(), EvictionRegion::Sam).unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_coalesced().unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_dropped(LruDropReason::QueueFull).unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_flush_duration_ms(10).unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_drift_ms(10).unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_consistency_violation(ten()).unwrap_err(),
            LruMetricsObserverError::Backend(_)
        ));
    }
}

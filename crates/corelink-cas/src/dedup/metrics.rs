//! Dedup metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S07-001 §6.1.7 (R-S07-2 dedup-on-write counters) freezes the
//! canonical 3 metrics every emission path uses:
//!
//! - `corelink.dedup.chunks_inserted_total{tenant_id, region}` (counter
//!   — first-time INSERT into `chunks` table; bumped per fresh chunk).
//! - `corelink.dedup.chunks_reused_total{tenant_id, region}` (counter —
//!   ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET refcount =
//!   refcount + 1; bumped per dedup hit).
//! - `corelink.dedup.find_missing_blobs_total{tenant_id, region, result}`
//!   (counter — `result ∈ { ok, denied, error }`; bumped once per
//!   `FindMissingBlobs` handler invocation).
//!
//! Dedup ratio is computed downstream as `chunks_reused_total /
//! (chunks_reused_total + chunks_inserted_total)` per spec_contract §1
//! SOTA framing (target ≥ 3× sustained 7d staging on Docker workloads;
//! NativeLink baseline 2.1×, BuildBuddy 2.8×).
//!
//! ## Why three counters (and not a histogram)
//!
//! Counters are O(1) emit + add cheaply on the chunk-write hot path.
//! The dedup-ratio dashboard widget recomputes the ratio at query time
//! via Prometheus / Grafana arithmetic (the canonical pattern per
//! `corelink-gc::metrics` + spec_contract §SOTA framing). A histogram
//! per chunk-write would saturate the cardinality budget on a 1k-tenant
//! deployment.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical metric names per WI §6.1.7 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 3] {
    &[
        "corelink.dedup.chunks_inserted_total",
        "corelink.dedup.chunks_reused_total",
        "corelink.dedup.find_missing_blobs_total",
    ]
}

/// Errors surfaced by [`DedupMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DedupMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("dedup metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink and
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DedupMetricKind {
    /// First-time INSERT counter increment
    /// (`corelink.dedup.chunks_inserted_total`).
    ChunksInserted,
    /// ON CONFLICT (refcount increment) counter increment
    /// (`corelink.dedup.chunks_reused_total`).
    ChunksReused,
    /// FindMissingBlobs handler invocation counter increment
    /// (`corelink.dedup.find_missing_blobs_total`).
    FindMissingBlobs,
}

impl DedupMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChunksInserted => "corelink.dedup.chunks_inserted_total",
            Self::ChunksReused => "corelink.dedup.chunks_reused_total",
            Self::FindMissingBlobs => "corelink.dedup.find_missing_blobs_total",
        }
    }
}

/// Outcome label for the `find_missing_blobs_total{result}` dimension.
/// Mirrors [`crate::dedup::audit::DedupAuditOutcome`] — kept as a separate
/// enum so the metrics layer does not depend on the audit layer at
/// type level (the audit layer is the load-bearing fail-closed
/// envelope; the metrics layer is permitted to log-and-continue per
/// WI §14.s07.001.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FindMissingBlobsResult {
    /// Handler returned a successful set-difference response.
    Ok,
    /// Handler aborted on cross-tenant attempt (CTRL-ISO-005).
    Denied,
    /// Handler aborted on transport / programmer error.
    Error,
}

impl FindMissingBlobsResult {
    /// Canonical lower-snake-case label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Denied => "denied",
            Self::Error => "error",
        }
    }
}

/// Dedup metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait DedupMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.dedup.chunks_inserted_total{tenant_id}`.
    /// Called per first-time chunk INSERT (the
    /// [`crate::dedup::write::ChunkWriteOutcome::Inserted`] arm).
    ///
    /// # Errors
    ///
    /// Returns [`DedupMetricsObserverError::Backend`] on any backend
    /// failure.
    fn record_chunks_inserted(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError>;

    /// Increment `corelink.dedup.chunks_reused_total{tenant_id}`.
    /// Called per ON CONFLICT (refcount increment) on the chunk
    /// upsert path (the [`crate::dedup::write::ChunkWriteOutcome::Reused`]
    /// arm).
    ///
    /// # Errors
    ///
    /// Returns [`DedupMetricsObserverError::Backend`] on any backend
    /// failure.
    fn record_chunks_reused(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError>;

    /// Increment `corelink.dedup.find_missing_blobs_total{tenant_id,
    /// result}` — bumped once per handler invocation (after the
    /// set-difference computation, regardless of outcome).
    ///
    /// # Errors
    ///
    /// Returns [`DedupMetricsObserverError::Backend`] on any backend
    /// failure.
    fn record_find_missing_blobs(
        &self,
        tenant_id: Uuid,
        result: FindMissingBlobsResult,
    ) -> Result<(), DedupMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryDedupMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
}

impl InMemoryDedupMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a fully-qualified label string (canonical
    /// metric name optionally suffixed with a `{tenant=…,result=…}`
    /// fragment).
    #[must_use]
    pub fn counter(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.counters.get(label).copied().unwrap_or_default(),
            Err(p) => p.into_inner().counters.get(label).copied().unwrap_or_default(),
        }
    }

    /// Per-metric aggregate counter snapshot summing every label
    /// fragment under the canonical metric name.
    #[must_use]
    pub fn counter_total(&self, kind: DedupMetricKind) -> u64 {
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

    /// Compute the dedup ratio = `chunks_reused / (chunks_reused +
    /// chunks_inserted)`.
    ///
    /// Returns `None` when no chunk writes have been observed yet
    /// (denominator zero). The ratio is bounded `[0.0, 1.0]`; a
    /// value of `0.75` corresponds to 4× compression (3 reuses per
    /// 1 insert ≈ 75% reused).
    #[must_use]
    pub fn dedup_ratio(&self) -> Option<f64> {
        let inserted = self.counter_total(DedupMetricKind::ChunksInserted);
        let reused = self.counter_total(DedupMetricKind::ChunksReused);
        let total = inserted.checked_add(reused)?;
        if total == 0 {
            return None;
        }
        // Use `f64::from` to avoid lossy conversion warnings; both u64
        // values are bounded by counter usage and well within f64
        // precision range for any realistic dedup workload.
        let r = reused as f64;
        let t = total as f64;
        Some(r / t)
    }
}

impl DedupMetricsObserver for InMemoryDedupMetrics {
    fn record_chunks_inserted(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            DedupMetricKind::ChunksInserted.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            DedupMetricsObserverError::Backend(
                "metrics observer mutex poisoned".to_string(),
            )
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(1);
        Ok(())
    }

    fn record_chunks_reused(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            DedupMetricKind::ChunksReused.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            DedupMetricsObserverError::Backend(
                "metrics observer mutex poisoned".to_string(),
            )
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(1);
        Ok(())
    }

    fn record_find_missing_blobs(
        &self,
        tenant_id: Uuid,
        result: FindMissingBlobsResult,
    ) -> Result<(), DedupMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},result={}}}",
            DedupMetricKind::FindMissingBlobs.as_str(),
            result.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            DedupMetricsObserverError::Backend(
                "metrics observer mutex poisoned".to_string(),
            )
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(1);
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingDedupMetrics;

impl FailingDedupMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DedupMetricsObserver for FailingDedupMetrics {
    fn record_chunks_inserted(
        &self,
        _tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError> {
        Err(DedupMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_chunks_reused(
        &self,
        _tenant_id: Uuid,
    ) -> Result<(), DedupMetricsObserverError> {
        Err(DedupMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_find_missing_blobs(
        &self,
        _tenant_id: Uuid,
        _result: FindMissingBlobsResult,
    ) -> Result<(), DedupMetricsObserverError> {
        Err(DedupMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
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
        assert_eq!(names.len(), 3);
        assert!(names.contains(&DedupMetricKind::ChunksInserted.as_str()));
        assert!(names.contains(&DedupMetricKind::ChunksReused.as_str()));
        assert!(names.contains(&DedupMetricKind::FindMissingBlobs.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            DedupMetricKind::ChunksInserted,
            DedupMetricKind::ChunksReused,
            DedupMetricKind::FindMissingBlobs,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.dedup."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn result_canonical_labels() {
        assert_eq!(FindMissingBlobsResult::Ok.as_str(), "ok");
        assert_eq!(FindMissingBlobsResult::Denied.as_str(), "denied");
        assert_eq!(FindMissingBlobsResult::Error.as_str(), "error");
    }

    #[test]
    fn in_memory_metrics_counts_inserted_and_reused() {
        let m = InMemoryDedupMetrics::new();
        m.record_chunks_inserted(ten()).unwrap();
        m.record_chunks_inserted(ten()).unwrap();
        m.record_chunks_reused(ten()).unwrap();
        assert_eq!(m.counter_total(DedupMetricKind::ChunksInserted), 2);
        assert_eq!(m.counter_total(DedupMetricKind::ChunksReused), 1);
        assert_eq!(m.counter_total(DedupMetricKind::FindMissingBlobs), 0);
    }

    #[test]
    fn dedup_ratio_none_when_no_writes() {
        let m = InMemoryDedupMetrics::new();
        assert!(m.dedup_ratio().is_none());
    }

    #[test]
    fn dedup_ratio_bounded_zero_one() {
        let m = InMemoryDedupMetrics::new();
        // 1 inserted + 3 reused = 75% reused (4× dedup compression).
        m.record_chunks_inserted(ten()).unwrap();
        m.record_chunks_reused(ten()).unwrap();
        m.record_chunks_reused(ten()).unwrap();
        m.record_chunks_reused(ten()).unwrap();
        let r = m.dedup_ratio().unwrap();
        assert!((r - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn find_missing_blobs_counter_per_result() {
        let m = InMemoryDedupMetrics::new();
        m.record_find_missing_blobs(ten(), FindMissingBlobsResult::Ok)
            .unwrap();
        m.record_find_missing_blobs(ten(), FindMissingBlobsResult::Ok)
            .unwrap();
        m.record_find_missing_blobs(ten(), FindMissingBlobsResult::Denied)
            .unwrap();
        assert_eq!(m.counter_total(DedupMetricKind::FindMissingBlobs), 3);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingDedupMetrics::new();
        assert!(matches!(
            m.record_chunks_inserted(ten()).unwrap_err(),
            DedupMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_chunks_reused(ten()).unwrap_err(),
            DedupMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_find_missing_blobs(ten(), FindMissingBlobsResult::Ok)
                .unwrap_err(),
            DedupMetricsObserverError::Backend(_)
        ));
    }
}

//! Eviction metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S07-002 §6.1.10 freezes the canonical 9 metrics every emission
//! path uses (CloudEvent dotted naming spec → underscored Prometheus
//! exposed name per `prometheus.io/docs/practices/naming/`; Lote
//! 10.7-tris cycle 6 clarification):
//!
//! - `corelink.evict.cron_fired_total{region}` (counter)
//! - `corelink.evict.candidates_scanned_total{tenant_id}` (counter)
//! - `corelink.evict.ttl_expired_total{tenant_id, tier}` (counter)
//! - `corelink.evict.lru_evicted_total{tenant_id}` (counter)
//! - `corelink.evict.bytes_reclaimed_total{tenant_id, tier}` (counter)
//! - `corelink.evict.cascade_prevented_total{tenant_id}` (counter;
//!   alert if drops abruptly = bug signal)
//! - `corelink.evict.quota_trigger_fired_total{tenant_id}` (counter)
//! - `corelink.evict.duration_ms{region}` (histogram-like; in-memory
//!   fake records the last value + a saturating sum for assertions)
//! - `corelink.evict.gc_invariant_violation_total` (counter; SEV-0
//!   if > 0 — CRITICAL INV-GC-001 inheritance violation; sustained 0
//!   is the load-bearing signal)

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::region::EvictionRegion;
use crate::tier::Tier;

/// Canonical metric names per WI §6.1.10 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 9] {
    &[
        "corelink.evict.cron_fired_total",
        "corelink.evict.candidates_scanned_total",
        "corelink.evict.ttl_expired_total",
        "corelink.evict.lru_evicted_total",
        "corelink.evict.bytes_reclaimed_total",
        "corelink.evict.cascade_prevented_total",
        "corelink.evict.quota_trigger_fired_total",
        "corelink.evict.duration_ms",
        "corelink.evict.gc_invariant_violation_total",
    ]
}

/// Errors surfaced by [`EvictionMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EvictionMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("eviction metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink and
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EvictionMetricKind {
    /// `corelink.evict.cron_fired_total`.
    CronFired,
    /// `corelink.evict.candidates_scanned_total`.
    CandidatesScanned,
    /// `corelink.evict.ttl_expired_total`.
    TtlExpired,
    /// `corelink.evict.lru_evicted_total`.
    LruEvicted,
    /// `corelink.evict.bytes_reclaimed_total`.
    BytesReclaimed,
    /// `corelink.evict.cascade_prevented_total`.
    CascadePrevented,
    /// `corelink.evict.quota_trigger_fired_total`.
    QuotaTriggerFired,
    /// `corelink.evict.duration_ms`.
    DurationMs,
    /// `corelink.evict.gc_invariant_violation_total` — SEV-0 alert
    /// gate; sustained 0 is the load-bearing signal.
    GcInvariantViolation,
}

impl EvictionMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CronFired => "corelink.evict.cron_fired_total",
            Self::CandidatesScanned => "corelink.evict.candidates_scanned_total",
            Self::TtlExpired => "corelink.evict.ttl_expired_total",
            Self::LruEvicted => "corelink.evict.lru_evicted_total",
            Self::BytesReclaimed => "corelink.evict.bytes_reclaimed_total",
            Self::CascadePrevented => "corelink.evict.cascade_prevented_total",
            Self::QuotaTriggerFired => "corelink.evict.quota_trigger_fired_total",
            Self::DurationMs => "corelink.evict.duration_ms",
            Self::GcInvariantViolation => "corelink.evict.gc_invariant_violation_total",
        }
    }
}

/// Eviction metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait EvictionMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.evict.cron_fired_total{region}`.
    /// Emitted once per per-region cron tick.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_cron_fired(&self, region: EvictionRegion)
        -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.candidates_scanned_total{tenant_id}`
    /// by `n` per scan pass.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_candidates_scanned(
        &self,
        tenant_id: Uuid,
        n: u64,
    ) -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.ttl_expired_total{tenant_id, tier}`.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_ttl_expired(
        &self,
        tenant_id: Uuid,
        tier: Tier,
    ) -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.lru_evicted_total{tenant_id}`.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_lru_evicted(&self, tenant_id: Uuid) -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.bytes_reclaimed_total{tenant_id,
    /// tier}` by `bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_bytes_reclaimed(
        &self,
        tenant_id: Uuid,
        tier: Tier,
        bytes: u64,
    ) -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.cascade_prevented_total{tenant_id}`.
    /// Sustained drop = signal that the reachable check is no longer
    /// firing (potentially a wiring bug).
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_cascade_prevented(&self, tenant_id: Uuid)
        -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.quota_trigger_fired_total{tenant_id}`.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_quota_trigger_fired(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), EvictionMetricsObserverError>;

    /// Observe `corelink.evict.duration_ms{region}` — histogram-like.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_duration_ms(
        &self,
        region: EvictionRegion,
        duration_ms: u64,
    ) -> Result<(), EvictionMetricsObserverError>;

    /// Increment `corelink.evict.gc_invariant_violation_total` —
    /// SEV-0 alert gate. Sustained 0 = load-bearing signal that
    /// INV-GC-001 inheritance holds.
    ///
    /// # Errors
    ///
    /// Returns [`EvictionMetricsObserverError::Backend`] on backend
    /// failure.
    fn record_gc_invariant_violation(&self) -> Result<(), EvictionMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryEvictionMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    last_duration_ms_per_region: HashMap<String, u64>,
}

impl InMemoryEvictionMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a fully-qualified label string (canonical
    /// metric name optionally suffixed with a `{tenant=…,tier=…}`
    /// fragment).
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
    pub fn counter_total(&self, kind: EvictionMetricKind) -> u64 {
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

    /// Most-recently observed duration_ms for a region.
    #[must_use]
    pub fn last_duration_ms(&self, region: EvictionRegion) -> Option<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .last_duration_ms_per_region
            .get(region.as_str())
            .copied()
    }

    fn bump(&self, label: String, by: u64) -> Result<(), EvictionMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            EvictionMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }
}

impl EvictionMetricsObserver for InMemoryEvictionMetrics {
    fn record_cron_fired(
        &self,
        region: EvictionRegion,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{region={}}}",
            EvictionMetricKind::CronFired.as_str(),
            region.as_str()
        );
        self.bump(label, 1)
    }

    fn record_candidates_scanned(
        &self,
        tenant_id: Uuid,
        n: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            EvictionMetricKind::CandidatesScanned.as_str()
        );
        self.bump(label, n)
    }

    fn record_ttl_expired(
        &self,
        tenant_id: Uuid,
        tier: Tier,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},tier={}}}",
            EvictionMetricKind::TtlExpired.as_str(),
            tier.as_str()
        );
        self.bump(label, 1)
    }

    fn record_lru_evicted(&self, tenant_id: Uuid) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            EvictionMetricKind::LruEvicted.as_str()
        );
        self.bump(label, 1)
    }

    fn record_bytes_reclaimed(
        &self,
        tenant_id: Uuid,
        tier: Tier,
        bytes: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},tier={}}}",
            EvictionMetricKind::BytesReclaimed.as_str(),
            tier.as_str()
        );
        self.bump(label, bytes)
    }

    fn record_cascade_prevented(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            EvictionMetricKind::CascadePrevented.as_str()
        );
        self.bump(label, 1)
    }

    fn record_quota_trigger_fired(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), EvictionMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            EvictionMetricKind::QuotaTriggerFired.as_str()
        );
        self.bump(label, 1)
    }

    fn record_duration_ms(
        &self,
        region: EvictionRegion,
        duration_ms: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        // Track total + last value per region.
        let label = format!(
            "{}{{region={}}}",
            EvictionMetricKind::DurationMs.as_str(),
            region.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            EvictionMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(duration_ms);
        guard
            .last_duration_ms_per_region
            .insert(region.as_str().to_string(), duration_ms);
        Ok(())
    }

    fn record_gc_invariant_violation(&self) -> Result<(), EvictionMetricsObserverError> {
        let label = EvictionMetricKind::GcInvariantViolation
            .as_str()
            .to_string();
        self.bump(label, 1)
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingEvictionMetrics;

impl FailingEvictionMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EvictionMetricsObserver for FailingEvictionMetrics {
    fn record_cron_fired(
        &self,
        _region: EvictionRegion,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_candidates_scanned(
        &self,
        _tenant_id: Uuid,
        _n: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_ttl_expired(
        &self,
        _tenant_id: Uuid,
        _tier: Tier,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_lru_evicted(&self, _tenant_id: Uuid) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_bytes_reclaimed(
        &self,
        _tenant_id: Uuid,
        _tier: Tier,
        _bytes: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_cascade_prevented(
        &self,
        _tenant_id: Uuid,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_quota_trigger_fired(
        &self,
        _tenant_id: Uuid,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_duration_ms(
        &self,
        _region: EvictionRegion,
        _duration_ms: u64,
    ) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
            "induced metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_gc_invariant_violation(&self) -> Result<(), EvictionMetricsObserverError> {
        Err(EvictionMetricsObserverError::Backend(
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
        assert_eq!(names.len(), 9);
        assert!(names.contains(&EvictionMetricKind::CronFired.as_str()));
        assert!(names.contains(&EvictionMetricKind::CandidatesScanned.as_str()));
        assert!(names.contains(&EvictionMetricKind::TtlExpired.as_str()));
        assert!(names.contains(&EvictionMetricKind::LruEvicted.as_str()));
        assert!(names.contains(&EvictionMetricKind::BytesReclaimed.as_str()));
        assert!(names.contains(&EvictionMetricKind::CascadePrevented.as_str()));
        assert!(names.contains(&EvictionMetricKind::QuotaTriggerFired.as_str()));
        assert!(names.contains(&EvictionMetricKind::DurationMs.as_str()));
        assert!(names.contains(&EvictionMetricKind::GcInvariantViolation.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            EvictionMetricKind::CronFired,
            EvictionMetricKind::CandidatesScanned,
            EvictionMetricKind::TtlExpired,
            EvictionMetricKind::LruEvicted,
            EvictionMetricKind::BytesReclaimed,
            EvictionMetricKind::CascadePrevented,
            EvictionMetricKind::QuotaTriggerFired,
            EvictionMetricKind::DurationMs,
            EvictionMetricKind::GcInvariantViolation,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.evict."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 9);
    }

    #[test]
    fn cron_fired_increments_per_region() {
        let m = InMemoryEvictionMetrics::new();
        m.record_cron_fired(EvictionRegion::Sam).unwrap();
        m.record_cron_fired(EvictionRegion::Sam).unwrap();
        m.record_cron_fired(EvictionRegion::Iad).unwrap();
        assert_eq!(m.counter_total(EvictionMetricKind::CronFired), 3);
    }

    #[test]
    fn ttl_expired_increment_per_tenant_tier() {
        let m = InMemoryEvictionMetrics::new();
        m.record_ttl_expired(ten(), Tier::Free).unwrap();
        m.record_ttl_expired(ten(), Tier::Free).unwrap();
        m.record_ttl_expired(ten(), Tier::Enterprise).unwrap();
        assert_eq!(m.counter_total(EvictionMetricKind::TtlExpired), 3);
    }

    #[test]
    fn bytes_reclaimed_aggregates_correctly() {
        let m = InMemoryEvictionMetrics::new();
        m.record_bytes_reclaimed(ten(), Tier::Free, 1000).unwrap();
        m.record_bytes_reclaimed(ten(), Tier::Free, 5000).unwrap();
        assert_eq!(m.counter_total(EvictionMetricKind::BytesReclaimed), 6000);
    }

    #[test]
    fn duration_ms_records_last_value_per_region() {
        let m = InMemoryEvictionMetrics::new();
        m.record_duration_ms(EvictionRegion::Sam, 100).unwrap();
        m.record_duration_ms(EvictionRegion::Sam, 250).unwrap();
        assert_eq!(m.last_duration_ms(EvictionRegion::Sam), Some(250));
        assert_eq!(m.last_duration_ms(EvictionRegion::Iad), None);
    }

    #[test]
    fn gc_invariant_violation_canary_starts_at_zero() {
        let m = InMemoryEvictionMetrics::new();
        assert_eq!(m.counter_total(EvictionMetricKind::GcInvariantViolation), 0);
        m.record_gc_invariant_violation().unwrap();
        assert_eq!(m.counter_total(EvictionMetricKind::GcInvariantViolation), 1);
    }

    #[test]
    fn cascade_prevented_per_tenant() {
        let m = InMemoryEvictionMetrics::new();
        let a = Uuid::from_u128(0xa);
        let b = Uuid::from_u128(0xb);
        m.record_cascade_prevented(a).unwrap();
        m.record_cascade_prevented(a).unwrap();
        m.record_cascade_prevented(b).unwrap();
        assert_eq!(m.counter_total(EvictionMetricKind::CascadePrevented), 3);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingEvictionMetrics::new();
        assert!(matches!(
            m.record_cron_fired(EvictionRegion::Sam).unwrap_err(),
            EvictionMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_ttl_expired(ten(), Tier::Free).unwrap_err(),
            EvictionMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_gc_invariant_violation().unwrap_err(),
            EvictionMetricsObserverError::Backend(_)
        ));
    }
}

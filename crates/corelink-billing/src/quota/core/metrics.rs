//! Quota metrics observer trait + canonical metric name list +
//! InMemory capture sink.
//!
//! WI-S07-003 §6.1 freezes the canonical 4 metrics every emission path
//! uses (CloudEvent dotted naming spec → underscored Prometheus exposed
//! name per `prometheus.io/docs/practices/naming/`; Lote 10.7-tris
//! cycle 6 clarification):
//!
//! - `corelink.quota.check_total{result=allow|deny|reserve}` (counter)
//! - `corelink.quota.denials_total{tenant_id}` (counter; alert SEV-1)
//! - `corelink.quota.reservation_active{tenant_id, region}` (gauge)
//! - `corelink.quota.check_duration_ms{result=allow|deny|reserve}`
//!   (histogram-like; SLO target p99 < 3ms per spec_contract §10.s07.3)

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

/// Canonical metric names per WI §6.1 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 4] {
    &[
        "corelink.quota.check_total",
        "corelink.quota.denials_total",
        "corelink.quota.reservation_active",
        "corelink.quota.check_duration_ms",
    ]
}

/// Errors surfaced by [`QuotaMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("quota metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink and
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuotaMetricKind {
    /// `corelink.quota.check_total`.
    CheckTotal,
    /// `corelink.quota.denials_total` — alert SEV-1.
    DenialsTotal,
    /// `corelink.quota.reservation_active` — gauge.
    ReservationActive,
    /// `corelink.quota.check_duration_ms` — histogram (SLO target
    /// p99 < 3ms per spec_contract §10.s07.3).
    CheckDurationMs,
}

impl QuotaMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckTotal => "corelink.quota.check_total",
            Self::DenialsTotal => "corelink.quota.denials_total",
            Self::ReservationActive => "corelink.quota.reservation_active",
            Self::CheckDurationMs => "corelink.quota.check_duration_ms",
        }
    }
}

/// Result label for `check_total` + `check_duration_ms`. Closed
/// canonical 3-set per WI §6.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuotaCheckResultLabel {
    /// `result=allow`.
    Allow,
    /// `result=deny`.
    Deny,
    /// `result=reserve`.
    Reserve,
}

impl QuotaCheckResultLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Reserve => "reserve",
        }
    }
}

/// Quota metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait QuotaMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.quota.check_total{result=...}` by 1.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaMetricsObserverError::Backend`] on backend failure.
    fn record_check(&self, result: QuotaCheckResultLabel) -> Result<(), QuotaMetricsObserverError>;

    /// Increment `corelink.quota.denials_total{tenant_id}` by 1 — only
    /// emitted on the PROVISIONAL 429 deny arm.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaMetricsObserverError::Backend`] on backend failure.
    fn record_denial(&self, tenant_id: Uuid) -> Result<(), QuotaMetricsObserverError>;

    /// Set the gauge `corelink.quota.reservation_active{tenant_id,
    /// region}` to `count`.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaMetricsObserverError::Backend`] on backend failure.
    fn observe_reservation_active(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        count: u64,
    ) -> Result<(), QuotaMetricsObserverError>;

    /// Observe `corelink.quota.check_duration_ms{result=...}` —
    /// histogram-like.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaMetricsObserverError::Backend`] on backend failure.
    fn record_check_duration_ms(
        &self,
        result: QuotaCheckResultLabel,
        duration_ms: u64,
    ) -> Result<(), QuotaMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion. F-001 closure preserved (per-instance
/// `Mutex`; no process-global state).
#[derive(Debug, Default)]
pub struct InMemoryQuotaMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    gauges: HashMap<String, u64>,
    last_duration_per_result: HashMap<&'static str, u64>,
    duration_samples_per_result: HashMap<&'static str, Vec<u64>>,
}

impl InMemoryQuotaMetrics {
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
    pub fn counter_total(&self, kind: QuotaMetricKind) -> u64 {
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

    /// Gauge snapshot.
    #[must_use]
    pub fn gauge(&self, label: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.gauges.get(label).copied().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .gauges
                .get(label)
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Most-recently observed duration_ms for a result label.
    #[must_use]
    pub fn last_duration_ms(&self, result: QuotaCheckResultLabel) -> Option<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.last_duration_per_result.get(result.as_str()).copied()
    }

    /// Snapshot of every duration sample observed for the given result
    /// label (informational; used by `prop_check_duration_under_3ms_p99`
    /// to assert SLO bound).
    #[must_use]
    pub fn duration_samples(&self, result: QuotaCheckResultLabel) -> Vec<u64> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .duration_samples_per_result
            .get(result.as_str())
            .cloned()
            .unwrap_or_default()
    }

    fn bump_counter(&self, label: String, by: u64) -> Result<(), QuotaMetricsObserverError> {
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(by);
        Ok(())
    }
}

impl QuotaMetricsObserver for InMemoryQuotaMetrics {
    fn record_check(&self, result: QuotaCheckResultLabel) -> Result<(), QuotaMetricsObserverError> {
        let label = format!(
            "{}{{result={}}}",
            QuotaMetricKind::CheckTotal.as_str(),
            result.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn record_denial(&self, tenant_id: Uuid) -> Result<(), QuotaMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id}}}",
            QuotaMetricKind::DenialsTotal.as_str()
        );
        self.bump_counter(label, 1)
    }

    fn observe_reservation_active(
        &self,
        tenant_id: Uuid,
        region: EvictionRegion,
        count: u64,
    ) -> Result<(), QuotaMetricsObserverError> {
        let label = format!(
            "{}{{tenant={tenant_id},region={}}}",
            QuotaMetricKind::ReservationActive.as_str(),
            region.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        guard.gauges.insert(label, count);
        Ok(())
    }

    fn record_check_duration_ms(
        &self,
        result: QuotaCheckResultLabel,
        duration_ms: u64,
    ) -> Result<(), QuotaMetricsObserverError> {
        let label = format!(
            "{}{{result={}}}",
            QuotaMetricKind::CheckDurationMs.as_str(),
            result.as_str()
        );
        let mut guard = self.inner.lock().map_err(|_| {
            QuotaMetricsObserverError::Backend("metrics observer mutex poisoned".to_string())
        })?;
        let entry = guard.counters.entry(label).or_insert(0);
        *entry = entry.saturating_add(duration_ms);
        guard
            .last_duration_per_result
            .insert(result.as_str(), duration_ms);
        guard
            .duration_samples_per_result
            .entry(result.as_str())
            .or_default()
            .push(duration_ms);
        Ok(())
    }
}

/// Always-failing observer for adversarial tests of the
/// log-and-continue downgrade path.
#[derive(Debug, Default)]
pub struct FailingQuotaMetrics;

impl FailingQuotaMetrics {
    /// Construct a fresh always-failing observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl QuotaMetricsObserver for FailingQuotaMetrics {
    fn record_check(
        &self,
        _result: QuotaCheckResultLabel,
    ) -> Result<(), QuotaMetricsObserverError> {
        Err(QuotaMetricsObserverError::Backend(
            "induced quota metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_denial(&self, _tenant_id: Uuid) -> Result<(), QuotaMetricsObserverError> {
        Err(QuotaMetricsObserverError::Backend(
            "induced quota metrics failure (test fixture)".to_string(),
        ))
    }

    fn observe_reservation_active(
        &self,
        _tenant_id: Uuid,
        _region: EvictionRegion,
        _count: u64,
    ) -> Result<(), QuotaMetricsObserverError> {
        Err(QuotaMetricsObserverError::Backend(
            "induced quota metrics failure (test fixture)".to_string(),
        ))
    }

    fn record_check_duration_ms(
        &self,
        _result: QuotaCheckResultLabel,
        _duration_ms: u64,
    ) -> Result<(), QuotaMetricsObserverError> {
        Err(QuotaMetricsObserverError::Backend(
            "induced quota metrics failure (test fixture)".to_string(),
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
        assert_eq!(names.len(), 4);
        assert!(names.contains(&QuotaMetricKind::CheckTotal.as_str()));
        assert!(names.contains(&QuotaMetricKind::DenialsTotal.as_str()));
        assert!(names.contains(&QuotaMetricKind::ReservationActive.as_str()));
        assert!(names.contains(&QuotaMetricKind::CheckDurationMs.as_str()));
    }

    #[test]
    fn metric_kinds_unique_canonical_strings() {
        let v = [
            QuotaMetricKind::CheckTotal,
            QuotaMetricKind::DenialsTotal,
            QuotaMetricKind::ReservationActive,
            QuotaMetricKind::CheckDurationMs,
        ];
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(k.as_str().starts_with("corelink.quota."));
            assert!(set.insert(k.as_str()), "duplicate metric: {}", k.as_str());
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn check_total_increments_per_result_label() {
        let m = InMemoryQuotaMetrics::new();
        m.record_check(QuotaCheckResultLabel::Allow).unwrap();
        m.record_check(QuotaCheckResultLabel::Allow).unwrap();
        m.record_check(QuotaCheckResultLabel::Reserve).unwrap();
        m.record_check(QuotaCheckResultLabel::Deny).unwrap();
        assert_eq!(m.counter_total(QuotaMetricKind::CheckTotal), 4);
    }

    #[test]
    fn denials_total_increments_per_tenant() {
        let m = InMemoryQuotaMetrics::new();
        m.record_denial(ten()).unwrap();
        m.record_denial(ten()).unwrap();
        assert_eq!(m.counter_total(QuotaMetricKind::DenialsTotal), 2);
    }

    #[test]
    fn reservation_active_overwrites_gauge() {
        let m = InMemoryQuotaMetrics::new();
        m.observe_reservation_active(ten(), EvictionRegion::Sam, 3)
            .unwrap();
        let label = format!(
            "{}{{tenant={},region=sam}}",
            QuotaMetricKind::ReservationActive.as_str(),
            ten()
        );
        assert_eq!(m.gauge(&label), 3);
        // Overwrite with a smaller value.
        m.observe_reservation_active(ten(), EvictionRegion::Sam, 1)
            .unwrap();
        assert_eq!(m.gauge(&label), 1);
    }

    #[test]
    fn check_duration_records_last_value_per_result() {
        let m = InMemoryQuotaMetrics::new();
        m.record_check_duration_ms(QuotaCheckResultLabel::Allow, 1)
            .unwrap();
        m.record_check_duration_ms(QuotaCheckResultLabel::Allow, 2)
            .unwrap();
        assert_eq!(m.last_duration_ms(QuotaCheckResultLabel::Allow), Some(2));
        let samples = m.duration_samples(QuotaCheckResultLabel::Allow);
        assert_eq!(samples, vec![1, 2]);
    }

    #[test]
    fn failing_metrics_returns_backend_error() {
        let m = FailingQuotaMetrics::new();
        assert!(matches!(
            m.record_check(QuotaCheckResultLabel::Allow).unwrap_err(),
            QuotaMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_denial(ten()).unwrap_err(),
            QuotaMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.observe_reservation_active(ten(), EvictionRegion::Sam, 0)
                .unwrap_err(),
            QuotaMetricsObserverError::Backend(_)
        ));
        assert!(matches!(
            m.record_check_duration_ms(QuotaCheckResultLabel::Allow, 1)
                .unwrap_err(),
            QuotaMetricsObserverError::Backend(_)
        ));
    }

    #[test]
    fn check_result_label_canonical_strings() {
        assert_eq!(QuotaCheckResultLabel::Allow.as_str(), "allow");
        assert_eq!(QuotaCheckResultLabel::Deny.as_str(), "deny");
        assert_eq!(QuotaCheckResultLabel::Reserve.as_str(), "reserve");
    }
}

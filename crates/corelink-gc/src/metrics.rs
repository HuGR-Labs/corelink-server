//! GC metrics observer trait + canonical metric name list + InMemory
//! capture sink.
//!
//! WI-S06-001 §6.1.9 freezes the canonical 6 metrics:
//!
//! - `corelink.gc.scheduler.cron_fired_total{region}` (counter).
//! - `corelink.gc.worker.run_started_total{tenant_id, region}` (counter).
//! - `corelink.gc.worker.run_completed_total{tenant_id, region, status}` (counter).
//! - `corelink.gc.worker.phase_duration_ms{phase, tenant_id, region}` (histogram).
//! - `corelink.gc.degrade_mode_active{kind}` (gauge; alert if `gc-pause`
//!   sustained > 1 h sem ADR).
//! - `corelink.gc.stale_running_count` (gauge; alert > 5 sustained).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::degrade::DegradeKind;
use crate::region::GcRegion;
use crate::run::{GcPhase, GcStatus};

/// Canonical metric names per WI §6.1.9 — pinned for cross-component
/// regression tests + dashboard widget configuration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str; 6] {
    &[
        "corelink.gc.scheduler.cron_fired_total",
        "corelink.gc.worker.run_started_total",
        "corelink.gc.worker.run_completed_total",
        "corelink.gc.worker.phase_duration_ms",
        "corelink.gc.degrade_mode_active",
        "corelink.gc.stale_running_count",
    ]
}

/// Errors surfaced by [`GcMetricsObserver`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GcMetricsObserverError {
    /// Metric backend transport failure (e.g. counter sink down).
    #[error("gc metrics observer backend error: {0}")]
    Backend(String),
}

/// Coarse metric kind tag — drives dispatch in the in-memory sink +
/// makes property tests easier to assert against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GcMetricKind {
    /// Cron-tick counter increment.
    CronFired,
    /// Worker run-started counter increment.
    RunStarted,
    /// Worker run-completed counter increment.
    RunCompleted,
    /// Phase duration histogram observation (ms).
    PhaseDurationMs,
    /// Degrade-mode active gauge (1 if enabled, 0 if `Off`).
    DegradeModeActive,
    /// Stale-running count gauge (number of `Running` rows whose
    /// `last_checkpoint_at_ms` is > 1 h old).
    StaleRunningCount,
}

impl GcMetricKind {
    /// Canonical metric name (matches [`canonical_metric_names`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CronFired => "corelink.gc.scheduler.cron_fired_total",
            Self::RunStarted => "corelink.gc.worker.run_started_total",
            Self::RunCompleted => "corelink.gc.worker.run_completed_total",
            Self::PhaseDurationMs => "corelink.gc.worker.phase_duration_ms",
            Self::DegradeModeActive => "corelink.gc.degrade_mode_active",
            Self::StaleRunningCount => "corelink.gc.stale_running_count",
        }
    }
}

/// GC metrics observer trait every backend (statsd / prometheus /
/// CloudWatch / in-memory) implements.
pub trait GcMetricsObserver: Send + Sync + core::fmt::Debug {
    /// Increment `corelink.gc.scheduler.cron_fired_total{region}`.
    fn record_cron_fired(&self, region: GcRegion) -> Result<(), GcMetricsObserverError>;

    /// Increment `corelink.gc.worker.run_started_total{tenant_id,
    /// region}`.
    fn record_run_started(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<(), GcMetricsObserverError>;

    /// Increment `corelink.gc.worker.run_completed_total{tenant_id,
    /// region, status}`.
    fn record_run_completed(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        status: GcStatus,
    ) -> Result<(), GcMetricsObserverError>;

    /// Histogram observation:
    /// `corelink.gc.worker.phase_duration_ms{phase, tenant_id,
    /// region}`.
    fn record_phase_duration_ms(
        &self,
        phase: GcPhase,
        tenant_id: Uuid,
        region: GcRegion,
        duration_ms: u64,
    ) -> Result<(), GcMetricsObserverError>;

    /// Gauge: `corelink.gc.degrade_mode_active{kind}` — `1` when
    /// non-`Off`, `0` otherwise.
    fn record_degrade_mode_active(&self, kind: DegradeKind) -> Result<(), GcMetricsObserverError>;

    /// Gauge: `corelink.gc.stale_running_count`.
    fn record_stale_running_count(&self, count: u64) -> Result<(), GcMetricsObserverError>;
}

/// In-memory metrics observer. Captures every recorded metric for
/// property test assertion.
#[derive(Debug, Default)]
pub struct InMemoryGcMetrics {
    inner: Mutex<MetricsState>,
}

#[derive(Debug, Default)]
struct MetricsState {
    counters: HashMap<String, u64>,
    histogram: HashMap<String, Vec<u64>>,
    gauges: HashMap<String, i64>,
}

impl InMemoryGcMetrics {
    /// Construct a fresh observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counter snapshot for a canonical name + label fragment.
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

    /// Sum of every counter whose label key starts with `prefix`.
    #[must_use]
    pub fn counter_sum_prefix(&self, prefix: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g
                .counters
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(_, v)| *v)
                .sum(),
            Err(p) => p
                .into_inner()
                .counters
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(_, v)| *v)
                .sum(),
        }
    }

    /// Histogram snapshot for a canonical label.
    #[must_use]
    pub fn histogram(&self, label: &str) -> Vec<u64> {
        match self.inner.lock() {
            Ok(g) => g.histogram.get(label).cloned().unwrap_or_default(),
            Err(p) => p
                .into_inner()
                .histogram
                .get(label)
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Gauge snapshot for a canonical label.
    #[must_use]
    pub fn gauge(&self, label: &str) -> i64 {
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

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MetricsState>, GcMetricsObserverError> {
        self.inner
            .lock()
            .map_err(|_| GcMetricsObserverError::Backend("metrics mutex poisoned".to_string()))
    }
}

impl GcMetricsObserver for InMemoryGcMetrics {
    fn record_cron_fired(&self, region: GcRegion) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = format!(
            "{}|region={}",
            GcMetricKind::CronFired.as_str(),
            region.as_str()
        );
        *guard.counters.entry(key).or_insert(0) += 1;
        Ok(())
    }

    fn record_run_started(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = format!(
            "{}|tenant_id={}|region={}",
            GcMetricKind::RunStarted.as_str(),
            tenant_id,
            region.as_str()
        );
        *guard.counters.entry(key).or_insert(0) += 1;
        Ok(())
    }

    fn record_run_completed(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        status: GcStatus,
    ) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = format!(
            "{}|tenant_id={}|region={}|status={}",
            GcMetricKind::RunCompleted.as_str(),
            tenant_id,
            region.as_str(),
            status.as_str()
        );
        *guard.counters.entry(key).or_insert(0) += 1;
        Ok(())
    }

    fn record_phase_duration_ms(
        &self,
        phase: GcPhase,
        tenant_id: Uuid,
        region: GcRegion,
        duration_ms: u64,
    ) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = format!(
            "{}|phase={}|tenant_id={}|region={}",
            GcMetricKind::PhaseDurationMs.as_str(),
            phase.as_str(),
            tenant_id,
            region.as_str()
        );
        guard.histogram.entry(key).or_default().push(duration_ms);
        Ok(())
    }

    fn record_degrade_mode_active(&self, kind: DegradeKind) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = format!(
            "{}|kind={}",
            GcMetricKind::DegradeModeActive.as_str(),
            kind.as_str()
        );
        let value = if kind == DegradeKind::Off { 0 } else { 1 };
        guard.gauges.insert(key, value);
        Ok(())
    }

    fn record_stale_running_count(&self, count: u64) -> Result<(), GcMetricsObserverError> {
        let mut guard = self.lock()?;
        let key = GcMetricKind::StaleRunningCount.as_str().to_owned();
        // Cast to i64 saturatingly — count is always small in practice
        // (gauge alerts at > 5 sustained per WI §6.1.9).
        let value = i64::try_from(count).unwrap_or(i64::MAX);
        guard.gauges.insert(key, value);
        Ok(())
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
    fn canonical_names_count_is_six() {
        assert_eq!(canonical_metric_names().len(), 6);
        for name in canonical_metric_names() {
            assert!(name.starts_with("corelink.gc."));
        }
    }

    #[test]
    fn cron_fired_increments() {
        let m = InMemoryGcMetrics::new();
        m.record_cron_fired(GcRegion::Sam).unwrap();
        m.record_cron_fired(GcRegion::Sam).unwrap();
        m.record_cron_fired(GcRegion::Iad).unwrap();
        let prefix = "corelink.gc.scheduler.cron_fired_total";
        assert_eq!(m.counter_sum_prefix(prefix), 3);
        assert_eq!(
            m.counter("corelink.gc.scheduler.cron_fired_total|region=sam"),
            2
        );
        assert_eq!(
            m.counter("corelink.gc.scheduler.cron_fired_total|region=iad"),
            1
        );
    }

    #[test]
    fn run_started_per_tenant() {
        let m = InMemoryGcMetrics::new();
        let t1 = Uuid::from_u128(1);
        let t2 = Uuid::from_u128(2);
        m.record_run_started(t1, GcRegion::Sam).unwrap();
        m.record_run_started(t2, GcRegion::Sam).unwrap();
        assert_eq!(
            m.counter_sum_prefix("corelink.gc.worker.run_started_total"),
            2
        );
    }

    #[test]
    fn phase_duration_observed() {
        let m = InMemoryGcMetrics::new();
        let t = Uuid::nil();
        m.record_phase_duration_ms(GcPhase::Mark, t, GcRegion::Sam, 1000)
            .unwrap();
        m.record_phase_duration_ms(GcPhase::Mark, t, GcRegion::Sam, 1500)
            .unwrap();
        let label =
            format!("corelink.gc.worker.phase_duration_ms|phase=mark|tenant_id={t}|region=sam");
        let h = m.histogram(&label);
        assert_eq!(h, vec![1000, 1500]);
    }

    #[test]
    fn degrade_gauge_zero_when_off() {
        let m = InMemoryGcMetrics::new();
        m.record_degrade_mode_active(DegradeKind::Off).unwrap();
        m.record_degrade_mode_active(DegradeKind::GcPause).unwrap();
        assert_eq!(m.gauge("corelink.gc.degrade_mode_active|kind=off"), 0);
        assert_eq!(m.gauge("corelink.gc.degrade_mode_active|kind=gc-pause"), 1);
    }

    #[test]
    fn stale_running_gauge_clamped_to_max_i64() {
        let m = InMemoryGcMetrics::new();
        m.record_stale_running_count(u64::MAX).unwrap();
        assert_eq!(m.gauge("corelink.gc.stale_running_count"), i64::MAX);
    }
}

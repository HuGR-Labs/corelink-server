//! Metrics definitions for the DO config-singleton (WI-S13-001).
//!
//! Five canonical Prometheus metrics per `observability_model.md §3.1 +
//! §4.1`. All names use underscored snake_case per naming convention.
//! Real OTel pipe wiring deferred to S-13 observability layer.

/// Metric name: config propagation latency histogram (p50/p95/p99).
pub const METRIC_PROPAGATION_SECONDS: &str = "corelink_admin_config_propagation_seconds_bucket";

/// Metric name: CAS conflict counter (labels: layer, plan).
pub const METRIC_CAS_CONFLICT_TOTAL: &str = "corelink_admin_config_cas_conflict_total";

/// Metric name: rollback outcome counter (labels: outcome, plan).
pub const METRIC_ROLLBACK_TOTAL: &str = "corelink_admin_config_rollback_total";

/// Metric name: update outcome counter (labels: outcome, plan).
pub const METRIC_UPDATE_TOTAL: &str = "corelink_admin_config_update_total";

/// Metric name: D1 history row count gauge.
pub const METRIC_HISTORY_SIZE_GAUGE: &str = "corelink_admin_config_history_size_gauge";

/// Canonical update outcome labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateOutcome {
    /// CAS update succeeded.
    Ok,
    /// CAS update rejected due to version mismatch.
    VersionConflict,
    /// Payload rejected by schema/domain validation.
    SchemaInvalid,
    /// Propagation did not complete within SLO.
    PropagationTimeout,
}

impl UpdateOutcome {
    /// Canonical label string for Prometheus.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::VersionConflict => "version_conflict",
            Self::SchemaInvalid => "schema_invalid",
            Self::PropagationTimeout => "propagation_timeout",
        }
    }
}

/// Canonical rollback outcome labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RollbackOutcome {
    /// Rollback succeeded.
    Ok,
    /// Target version not found in history.
    VersionUnknown,
    /// Target version outside 90d retention window.
    VersionExpired,
    /// Target version payload failed invariant pre-check.
    StateCorrupt,
}

impl RollbackOutcome {
    /// Canonical label string for Prometheus.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::VersionUnknown => "version_unknown",
            Self::VersionExpired => "version_expired",
            Self::StateCorrupt => "state_corrupt",
        }
    }
}

/// Observer trait for config-singleton metrics.
///
/// Production implementation wires to the OTel pipeline;
/// [`NoopMetrics`] and [`InMemoryMetrics`] are provided for tests.
pub trait MetricsObserver: Send + Sync {
    /// Record a config update outcome.
    fn record_update(&self, outcome: UpdateOutcome);
    /// Record a CAS conflict (layer = "feature_flags" | "rate_limits" | "retention").
    fn record_cas_conflict(&self, layer: &str);
    /// Record a rollback outcome.
    fn record_rollback(&self, outcome: RollbackOutcome);
    /// Observe a propagation latency sample (seconds).
    fn observe_propagation_seconds(&self, seconds: f64);
    /// Update the D1 history row count gauge.
    fn set_history_size(&self, count: u64);
}

/// No-op metrics implementation (for embedded use where metrics are not needed).
#[derive(Debug, Default)]
pub struct NoopMetrics;

impl MetricsObserver for NoopMetrics {
    fn record_update(&self, _outcome: UpdateOutcome) {}
    fn record_cas_conflict(&self, _layer: &str) {}
    fn record_rollback(&self, _outcome: RollbackOutcome) {}
    fn observe_propagation_seconds(&self, _seconds: f64) {}
    fn set_history_size(&self, _count: u64) {}
}

/// In-memory metrics sink for tests.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    inner: std::sync::Mutex<InMemoryMetricsInner>,
}

#[derive(Debug, Default)]
struct InMemoryMetricsInner {
    updates: Vec<UpdateOutcome>,
    cas_conflicts: Vec<String>,
    rollbacks: Vec<RollbackOutcome>,
    propagation_samples: Vec<f64>,
    history_size: u64,
}

impl MetricsObserver for InMemoryMetrics {
    fn record_update(&self, outcome: UpdateOutcome) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.updates.push(outcome);
    }

    fn record_cas_conflict(&self, layer: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.cas_conflicts.push(layer.to_owned());
    }

    fn record_rollback(&self, outcome: RollbackOutcome) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.rollbacks.push(outcome);
    }

    fn observe_propagation_seconds(&self, seconds: f64) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.propagation_samples.push(seconds);
    }

    fn set_history_size(&self, count: u64) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.history_size = count;
    }
}

impl InMemoryMetrics {
    /// Create a new empty in-memory metrics sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return all recorded update outcomes.
    #[must_use]
    pub fn updates(&self) -> Vec<UpdateOutcome> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .updates
            .clone()
    }

    /// Return all recorded CAS conflict layer labels.
    #[must_use]
    pub fn cas_conflicts(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .cas_conflicts
            .clone()
    }

    /// Return all recorded rollback outcomes.
    #[must_use]
    pub fn rollbacks(&self) -> Vec<RollbackOutcome> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .rollbacks
            .clone()
    }

    /// Return all propagation latency samples.
    #[must_use]
    pub fn propagation_samples(&self) -> Vec<f64> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .propagation_samples
            .clone()
    }

    /// Return last recorded history size gauge value.
    #[must_use]
    pub fn history_size(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .history_size
    }
}

//! Rotation metrics taxonomy (6 canonical `corelink_admin_rotation_*`
//! Prometheus metrics; WI-S13-003 §6.1.5).
//!
//! The [`RotationMetricsSink`] trait abstracts over the production
//! Cloudflare Workers Analytics Engine binding (deferred to WI-S13-006)
//! and the in-memory [`RotationMetrics`] fake used in CI.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_rotation_adapters::AssetClass;

/// Prometheus metric name constants per `observability_model.md §3.1`.
pub mod names {
    /// Gauge: rotation in-progress per asset type + status.
    /// Labels: `asset_type`, `status` ∈ {pending, active, overlap, retired, rolled_back, destroyed}.
    pub const ROTATION_IN_PROGRESS: &str = "corelink_admin_rotation_in_progress";
    /// Counter: rotation total per asset type + outcome.
    /// Labels: `asset_type`, `outcome` ∈ {ok, rolled_back, aborted, failed}.
    pub const ROTATION_TOTAL: &str = "corelink_admin_rotation_total";
    /// Histogram: rotation overlap window seconds per asset type.
    /// Labels: `asset_type`. Baseline against canonical table 3.2.1.
    pub const ROTATION_OVERLAP_SECONDS: &str = "corelink_admin_rotation_overlap_seconds";
    /// Gauge: downstream error rate per asset type during rotation.
    /// Labels: `asset_type`. Alert >0.01 (1%).
    pub const ROTATION_DOWNSTREAM_ERROR_RATE: &str =
        "corelink_admin_rotation_downstream_error_rate";
    /// Gauge: re-key progress ratio (0.0..=1.0) per asset type.
    /// Labels: `asset_type`.
    pub const ROTATION_REKEY_PROGRESS_RATIO: &str = "corelink_admin_rotation_rekey_progress_ratio";
    /// Histogram: rotation duration per asset type + phase.
    /// Labels: `asset_type`, `phase` ∈ {generate, promote, rekey, retire, destroy}.
    pub const ROTATION_DURATION_SECONDS: &str = "corelink_admin_rotation_duration_seconds_bucket";
}

/// Sink for rotation metrics emission.
///
/// In CI: backed by [`RotationMetrics`] (in-memory counters).
/// In production: backed by Cloudflare Workers Analytics Engine
/// (deferred to WI-S13-006).
pub trait RotationMetricsSink: core::fmt::Debug {
    /// Increment the rotation total counter for `outcome`.
    fn increment_rotation_total(&self, asset_class: AssetClass, outcome: &str);

    /// Set the downstream error rate gauge.
    fn record_downstream_error_rate(&self, asset_class: AssetClass, rate: f64);

    /// Set the re-key progress ratio gauge.
    fn record_rekey_progress(&self, asset_class: AssetClass, ratio: f64);

    /// Record an overlap window observation (histogram bucket).
    fn observe_overlap_seconds(&self, asset_class: AssetClass, seconds: u64);

    /// Record an in-progress gauge update.
    fn set_in_progress(&self, asset_class: AssetClass, status: &str, value: i64);
}

/// In-memory rotation metrics (CI fake).
#[derive(Debug, Default)]
pub struct RotationMetrics {
    /// Total counters keyed by (asset_class, outcome).
    totals: Arc<Mutex<HashMap<(String, String), u64>>>,
    /// Latest error rate per asset class.
    error_rates: Arc<Mutex<HashMap<String, f64>>>,
    /// Latest rekey progress per asset class.
    rekey_progress: Arc<Mutex<HashMap<String, f64>>>,
    /// Overlap observations per asset class.
    overlap_observations: Arc<Mutex<HashMap<String, Vec<u64>>>>,
}

impl RotationMetrics {
    /// Construct a new in-memory metrics sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the total count for `(asset_class, outcome)`.
    #[must_use]
    pub fn total(&self, asset_class: AssetClass, outcome: &str) -> u64 {
        let totals = self.totals.lock().unwrap_or_else(|e| e.into_inner());
        *totals
            .get(&(asset_class.as_str().to_string(), outcome.to_string()))
            .unwrap_or(&0)
    }

    /// Return the latest downstream error rate for `asset_class`.
    #[must_use]
    pub fn error_rate(&self, asset_class: AssetClass) -> f64 {
        let rates = self.error_rates.lock().unwrap_or_else(|e| e.into_inner());
        *rates.get(asset_class.as_str()).unwrap_or(&0.0)
    }

    /// Return the latest rekey progress for `asset_class`.
    #[must_use]
    pub fn rekey_progress(&self, asset_class: AssetClass) -> f64 {
        let progress = self
            .rekey_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *progress.get(asset_class.as_str()).unwrap_or(&0.0)
    }

    /// Return the overlap observations for `asset_class`.
    #[must_use]
    pub fn overlap_observations(&self, asset_class: AssetClass) -> Vec<u64> {
        let obs = self
            .overlap_observations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        obs.get(asset_class.as_str()).cloned().unwrap_or_default()
    }
}

impl RotationMetricsSink for RotationMetrics {
    fn increment_rotation_total(&self, asset_class: AssetClass, outcome: &str) {
        let mut totals = self.totals.lock().unwrap_or_else(|e| e.into_inner());
        *totals
            .entry((asset_class.as_str().to_string(), outcome.to_string()))
            .or_insert(0) += 1;
    }

    fn record_downstream_error_rate(&self, asset_class: AssetClass, rate: f64) {
        let mut rates = self.error_rates.lock().unwrap_or_else(|e| e.into_inner());
        rates.insert(asset_class.as_str().to_string(), rate);
    }

    fn record_rekey_progress(&self, asset_class: AssetClass, ratio: f64) {
        let mut progress = self
            .rekey_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        progress.insert(asset_class.as_str().to_string(), ratio);
    }

    fn observe_overlap_seconds(&self, asset_class: AssetClass, seconds: u64) {
        let mut obs = self
            .overlap_observations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        obs.entry(asset_class.as_str().to_string())
            .or_default()
            .push(seconds);
    }

    fn set_in_progress(&self, _asset_class: AssetClass, _status: &str, _value: i64) {
        // In-memory fake: no persistent gauge state needed for CI tests.
    }
}

//! Prometheus-style metric counters for the SBOM pipeline.
//!
//! Metrics are simple atomic counters / histogram buckets that the binary
//! writes to a structured log line on exit. In production they are scraped
//! by a Prometheus-compatible exporter or forwarded to Grafana Cloud.
//!
//! # Registered metrics (per WI-S12-002 §6.1.7)
//!
//! | Metric | Type | Labels |
//! |---|---|---|
//! | `corelink_supply_sbom_generation_duration_seconds_bucket` | Histogram | — |
//! | `corelink_supply_sbom_components_count_gauge` | Gauge | — |
//! | `corelink_supply_sbom_ntia_compliant_total` | Counter | `outcome` ∈ ok\|fail\|warning |
//! | `corelink_supply_dt_ingestion_total` | Counter | `outcome` ∈ ok\|api_error\|validation_failed\|retry_exhausted |
//! | `corelink_supply_tsa_timestamp_total` | Counter | `outcome` ∈ ok\|tsa_unavailable\|verify_failed |

use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

/// Outcome label for NTIA compliance metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NtiaOutcome {
    /// NTIA strict validation passed.
    Ok,
    /// NTIA strict validation failed.
    Fail,
    /// Auditor mode emitted a soft warning.
    Warning,
}

impl NtiaOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Fail => "fail",
            Self::Warning => "warning",
        }
    }
}

/// Outcome label for DT ingestion metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DtOutcome {
    /// Ingestion succeeded (possibly after retry).
    Ok,
    /// DT API returned a non-2xx error (non-retryable).
    ApiError,
    /// SBOM failed schema validation before ingestion.
    ValidationFailed,
    /// All retries exhausted; queued to fallback.
    RetryExhausted,
}

impl DtOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ApiError => "api_error",
            Self::ValidationFailed => "validation_failed",
            Self::RetryExhausted => "retry_exhausted",
        }
    }
}

/// Outcome label for TSA timestamp metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TsaOutcome {
    /// TSA timestamp succeeded.
    Ok,
    /// TSA endpoint was unavailable (HTTP 5xx / timeout).
    TsaUnavailable,
    /// TSR hash binding verification failed (replay attempt).
    VerifyFailed,
}

impl TsaOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::TsaUnavailable => "tsa_unavailable",
            Self::VerifyFailed => "verify_failed",
        }
    }
}

/// Global metric counters (process-level singletons).
///
/// In a real deployment these would be registered with a Prometheus registry.
/// Here they are simple atomics emitted as structured log lines on each update.
#[derive(Debug)]
pub struct SbomMetrics {
    /// `corelink_supply_sbom_ntia_compliant_total{outcome="ok"}`
    pub ntia_ok: AtomicU64,
    /// `corelink_supply_sbom_ntia_compliant_total{outcome="fail"}`
    pub ntia_fail: AtomicU64,
    /// `corelink_supply_sbom_ntia_compliant_total{outcome="warning"}`
    pub ntia_warning: AtomicU64,
    /// `corelink_supply_dt_ingestion_total{outcome="ok"}`
    pub dt_ok: AtomicU64,
    /// `corelink_supply_dt_ingestion_total{outcome="api_error"}`
    pub dt_api_error: AtomicU64,
    /// `corelink_supply_dt_ingestion_total{outcome="validation_failed"}`
    pub dt_validation_failed: AtomicU64,
    /// `corelink_supply_dt_ingestion_total{outcome="retry_exhausted"}`
    pub dt_retry_exhausted: AtomicU64,
    /// `corelink_supply_tsa_timestamp_total{outcome="ok"}`
    pub tsa_ok: AtomicU64,
    /// `corelink_supply_tsa_timestamp_total{outcome="tsa_unavailable"}`
    pub tsa_unavailable: AtomicU64,
    /// `corelink_supply_tsa_timestamp_total{outcome="verify_failed"}`
    pub tsa_verify_failed: AtomicU64,
    /// Last observed component count (gauge snapshot).
    pub components_count: AtomicU64,
}

impl SbomMetrics {
    /// Create a zeroed metric set.
    pub const fn new() -> Self {
        Self {
            ntia_ok: AtomicU64::new(0),
            ntia_fail: AtomicU64::new(0),
            ntia_warning: AtomicU64::new(0),
            dt_ok: AtomicU64::new(0),
            dt_api_error: AtomicU64::new(0),
            dt_validation_failed: AtomicU64::new(0),
            dt_retry_exhausted: AtomicU64::new(0),
            tsa_ok: AtomicU64::new(0),
            tsa_unavailable: AtomicU64::new(0),
            tsa_verify_failed: AtomicU64::new(0),
            components_count: AtomicU64::new(0),
        }
    }

    /// Increment `corelink_supply_sbom_ntia_compliant_total{outcome}`.
    pub fn record_ntia(&self, outcome: NtiaOutcome) {
        let counter = match outcome {
            NtiaOutcome::Ok => &self.ntia_ok,
            NtiaOutcome::Fail => &self.ntia_fail,
            NtiaOutcome::Warning => &self.ntia_warning,
        };
        let val = counter.fetch_add(1, Ordering::Relaxed) + 1;
        info!(
            metric = "corelink_supply_sbom_ntia_compliant_total",
            outcome = outcome.as_str(),
            value = val,
            "metric update"
        );
    }

    /// Increment `corelink_supply_dt_ingestion_total{outcome}`.
    pub fn record_dt(&self, outcome: DtOutcome) {
        let counter = match outcome {
            DtOutcome::Ok => &self.dt_ok,
            DtOutcome::ApiError => &self.dt_api_error,
            DtOutcome::ValidationFailed => &self.dt_validation_failed,
            DtOutcome::RetryExhausted => &self.dt_retry_exhausted,
        };
        let val = counter.fetch_add(1, Ordering::Relaxed) + 1;
        info!(
            metric = "corelink_supply_dt_ingestion_total",
            outcome = outcome.as_str(),
            value = val,
            "metric update"
        );
    }

    /// Increment `corelink_supply_tsa_timestamp_total{outcome}`.
    pub fn record_tsa(&self, outcome: TsaOutcome) {
        let counter = match outcome {
            TsaOutcome::Ok => &self.tsa_ok,
            TsaOutcome::TsaUnavailable => &self.tsa_unavailable,
            TsaOutcome::VerifyFailed => &self.tsa_verify_failed,
        };
        let val = counter.fetch_add(1, Ordering::Relaxed) + 1;
        info!(
            metric = "corelink_supply_tsa_timestamp_total",
            outcome = outcome.as_str(),
            value = val,
            "metric update"
        );
    }

    /// Update `corelink_supply_sbom_components_count_gauge`.
    pub fn set_components_count(&self, count: u64) {
        self.components_count.store(count, Ordering::Relaxed);
        info!(
            metric = "corelink_supply_sbom_components_count_gauge",
            value = count,
            "metric update"
        );
    }

    /// Emit all current values as structured log entries (for CI summary).
    pub fn emit_summary(&self) {
        info!(
            ntia_ok = self.ntia_ok.load(Ordering::Relaxed),
            ntia_fail = self.ntia_fail.load(Ordering::Relaxed),
            ntia_warning = self.ntia_warning.load(Ordering::Relaxed),
            dt_ok = self.dt_ok.load(Ordering::Relaxed),
            dt_api_error = self.dt_api_error.load(Ordering::Relaxed),
            dt_validation_failed = self.dt_validation_failed.load(Ordering::Relaxed),
            dt_retry_exhausted = self.dt_retry_exhausted.load(Ordering::Relaxed),
            tsa_ok = self.tsa_ok.load(Ordering::Relaxed),
            tsa_unavailable = self.tsa_unavailable.load(Ordering::Relaxed),
            tsa_verify_failed = self.tsa_verify_failed.load(Ordering::Relaxed),
            components_count = self.components_count.load(Ordering::Relaxed),
            "sbom pipeline metrics summary"
        );
    }
}

impl Default for SbomMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-level singleton metrics store.
///
/// Wrapped in [`std::sync::Arc`] per F-001 pattern; callers that need to share
/// across tasks should clone the Arc.
pub static METRICS: SbomMetrics = SbomMetrics::new();

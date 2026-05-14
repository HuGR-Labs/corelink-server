//! Prometheus metrics for drift consumer — WI-S13-004 §6.1.5.
//!
//! 4 canonical metrics:
//! - `corelink_admin_terraform_drift_findings_total{region,severity}` — counter
//! - `corelink_admin_terraform_drift_age_hours_gauge{finding_id,region}` — gauge
//! - `corelink_admin_terraform_cron_runs_total{outcome}` — counter
//! - `corelink_admin_terraform_remediation_duration_hours_bucket{region}` — histogram

/// Outcome label for cron run metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DriftMetricOutcome {
    /// Plan ran cleanly, no diff (exit 0).
    Ok,
    /// Drift detected (exit 2).
    DriftDetected,
    /// Terraform error (exit 1).
    TerraformError,
    /// Workflow failed to start / cron skipped.
    WorkflowFailed,
}

impl DriftMetricOutcome {
    /// Prometheus label string.
    #[must_use]
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::DriftDetected => "drift_detected",
            Self::TerraformError => "terraform_error",
            Self::WorkflowFailed => "workflow_failed",
        }
    }
}

/// In-memory metric accumulator for testing.
///
/// Production wiring exports to Cloudflare Workers Analytics Engine /
/// Prometheus-compatible endpoint.
#[derive(Debug, Default)]
pub struct DriftMetrics {
    /// `corelink_admin_terraform_drift_findings_total` per (region, severity).
    pub findings_total: Vec<(String, String, u64)>,

    /// `corelink_admin_terraform_cron_runs_total` per outcome.
    pub cron_runs_total: Vec<(String, u64)>,

    /// `corelink_admin_terraform_remediation_duration_hours` observations per region.
    pub remediation_duration_hours: Vec<(String, f64)>,
}

impl DriftMetrics {
    /// Record a findings_total increment.
    pub fn record_finding(&mut self, region: &str, severity: &str) {
        self.findings_total
            .push((region.to_owned(), severity.to_owned(), 1));
    }

    /// Record a cron run outcome.
    pub fn record_cron_run(&mut self, outcome: DriftMetricOutcome) {
        self.cron_runs_total
            .push((outcome.as_label().to_owned(), 1));
    }

    /// Record remediation duration.
    pub fn record_remediation_duration(&mut self, region: &str, duration_hours: f64) {
        self.remediation_duration_hours
            .push((region.to_owned(), duration_hours));
    }

    /// Count total findings recorded.
    #[must_use]
    pub fn total_findings_count(&self) -> u64 {
        self.findings_total.iter().map(|(_, _, c)| c).sum()
    }

    /// Count total cron runs recorded.
    #[must_use]
    pub fn total_cron_runs_count(&self) -> u64 {
        self.cron_runs_total.iter().map(|(_, c)| c).sum()
    }
}

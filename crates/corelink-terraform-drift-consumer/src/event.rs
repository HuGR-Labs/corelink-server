//! Event types for WI-S13-004 drift consumer.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical set of CoreLink regions for terraform matrix.
pub const REGIONS: &[&str] = &["us-east", "us-west", "eu-west", "ap-southeast", "sa-east"];

/// Severity classification for a drift finding.
///
/// Derived from `diff_count` by [`crate::classifier::DefaultDriftClassifier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DriftSeverity {
    /// `diff_count == 0` — clean run (inserted for cron health tracking).
    None,
    /// `diff_count` 1–2 (minor; e.g. tag update).
    Low,
    /// `diff_count` 3–10 (moderate; structural resource changes).
    Medium,
    /// `diff_count > 10` or critical resource type changed.
    High,
}

impl DriftSeverity {
    /// Prometheus label string matching WI-S13-004 §6.1.5.
    #[must_use]
    pub fn as_label(&self) -> &'static str {
        match self {
            DriftSeverity::None => "none",
            DriftSeverity::Low => "low",
            DriftSeverity::Medium => "medium",
            DriftSeverity::High => "high",
        }
    }
}

/// Lifecycle status of a drift finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DriftStatus {
    /// Drift detected; no remediation decision yet.
    Open,
    /// SRE is actively investigating root cause.
    Investigating,
    /// Drift reconciled per RB-FM-206 decision tree.
    Remediated,
    /// Accepted drift; documented in acceptable patterns ADR.
    Wontfix,
}

/// Remediation decision per RB-FM-206 decision tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RemediationDecision {
    /// `terraform apply` to reconcile IaC to actual state.
    Apply,
    /// Root cause investigation — defer apply decision.
    Investigate,
    /// Manual change reverted; terraform apply after revert.
    Revert,
}

/// Input event from GitHub Actions webhook (terraform plan run result).
///
/// Produced by the GitHub Actions workflow step that calls back to the
/// Worker endpoint after each `terraform plan` completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftPlanEvent {
    /// Target region (must be one of [`REGIONS`]).
    pub region: String,

    /// Epoch ms when the plan ran.
    pub detected_at_ms: u64,

    /// Terraform plan exit code: 0 = no diff; 1 = error; 2 = diff.
    pub tf_exit_code: i32,

    /// Number of resources with diff (from plan output parsing).
    pub plan_diff_count: u32,

    /// Short summary (top 5 resources changed).
    pub plan_summary: String,

    /// GitHub Actions artifact URL for full plan output.
    pub plan_full_artifact_url: Option<String>,

    /// GitHub Actions run ID for traceability.
    pub github_run_id: String,
}

/// A drift finding row (maps 1:1 to D1 `terraform_drift_findings`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFinding {
    /// UUIDv7 primary key (sortable by time).
    pub finding_id: Uuid,

    /// Source region.
    pub region: String,

    /// Epoch ms when detected.
    pub detected_at_ms: u64,

    /// Number of resources diffed.
    pub plan_diff_count: u32,

    /// Plan summary (human readable, top 5).
    pub plan_summary: String,

    /// Full plan artifact URL.
    pub plan_full_artifact_url: Option<String>,

    /// Classified severity.
    pub severity: DriftSeverity,

    /// Current lifecycle status.
    pub status: DriftStatus,

    /// Remediation decision (set during remediation flow).
    pub remediation_decision: Option<RemediationDecision>,

    /// When remediation completed (epoch ms).
    pub remediated_at_ms: Option<u64>,

    /// Admin user_id who remediated (dual-approval gated).
    pub remediated_by_user_id: Option<Uuid>,

    /// GitHub Actions run ID.
    pub github_run_id: String,

    /// Always "RB-FM-206".
    pub runbook_ref: String,
}

impl DriftFinding {
    /// Create a new finding from a plan event and classified severity.
    #[must_use]
    pub fn from_event(event: &DriftPlanEvent, severity: DriftSeverity) -> Self {
        Self {
            finding_id: Uuid::now_v7(),
            region: event.region.clone(),
            detected_at_ms: event.detected_at_ms,
            plan_diff_count: event.plan_diff_count,
            plan_summary: event.plan_summary.clone(),
            plan_full_artifact_url: event.plan_full_artifact_url.clone(),
            severity,
            status: DriftStatus::Open,
            remediation_decision: None,
            remediated_at_ms: None,
            remediated_by_user_id: None,
            github_run_id: event.github_run_id.clone(),
            runbook_ref: "RB-FM-206".to_owned(),
        }
    }
}

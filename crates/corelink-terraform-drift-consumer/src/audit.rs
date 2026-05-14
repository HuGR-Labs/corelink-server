//! Audit sink for drift consumer — WI-S13-004.
//!
//! CloudEvent types emitted:
//! - `corelink.admin.terraform_drift.detected`   (when diff_count > 0)
//! - `corelink.admin.terraform_drift.clean_run`  (when diff_count == 0; cron health)
//! - `corelink.admin.terraform_drift.remediated` (when status → remediated)
//!
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: audit fires BEFORE state mutation.
//! Audit failure → DriftConsumerError::AuditFailed → blocks store insert.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DriftConsumerError;
use crate::event::{DriftFinding, DriftSeverity};

/// CloudEvent type strings (WI-S13-004 §6.1.7).
pub const EVT_DRIFT_DETECTED: &str = "corelink.admin.terraform_drift.detected";
/// CloudEvent type for clean cron run (drift_count == 0).
pub const EVT_DRIFT_CLEAN_RUN: &str = "corelink.admin.terraform_drift.clean_run";
/// CloudEvent type for remediation completion.
pub const EVT_DRIFT_REMEDIATED: &str = "corelink.admin.terraform_drift.remediated";

/// Audit event type taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DriftAuditEventType {
    /// Drift detected (diff_count > 0); SEV-3 alert posted.
    Detected,
    /// Clean run (diff_count == 0); cron health tracking row inserted.
    CleanRun,
    /// Finding remediated via admin API + dual-approval gate.
    Remediated,
}

impl DriftAuditEventType {
    /// CloudEvent type string.
    #[must_use]
    pub fn as_cloud_event_type(&self) -> &'static str {
        match self {
            Self::Detected => EVT_DRIFT_DETECTED,
            Self::CleanRun => EVT_DRIFT_CLEAN_RUN,
            Self::Remediated => EVT_DRIFT_REMEDIATED,
        }
    }
}

/// Audit record emitted per drift event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAuditRecord {
    /// CloudEvent spec version.
    pub specversion: String,

    /// CloudEvent type.
    pub event_type: String,

    /// Event source.
    pub source: String,

    /// Unique event ID (UUIDv7).
    pub id: Uuid,

    /// Epoch ms.
    pub time_ms: u64,

    /// Finding ID this audit record covers.
    pub finding_id: Uuid,

    /// Region.
    pub region: String,

    /// Severity.
    pub severity: DriftSeverity,

    /// Diff count.
    pub plan_diff_count: u32,

    /// GitHub Actions run ID.
    pub github_run_id: String,
}

impl DriftAuditRecord {
    /// Construct from a [`DriftFinding`] and event type.
    #[must_use]
    pub fn from_finding(finding: &DriftFinding, event_type: DriftAuditEventType) -> Self {
        Self {
            specversion: "1.0".to_owned(),
            event_type: event_type.as_cloud_event_type().to_owned(),
            source: "corelink/admin/terraform-drift-consumer".to_owned(),
            id: Uuid::now_v7(),
            time_ms: finding.detected_at_ms,
            finding_id: finding.finding_id,
            region: finding.region.clone(),
            severity: finding.severity,
            plan_diff_count: finding.plan_diff_count,
            github_run_id: finding.github_run_id.clone(),
        }
    }
}

/// Audit sink trait — fail-CLOSED per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
pub trait DriftAuditSink: std::fmt::Debug {
    /// Emit an audit record. Must complete BEFORE state mutation.
    ///
    /// # Errors
    /// Any error here blocks the downstream store insert.
    fn emit(&mut self, record: DriftAuditRecord) -> Result<(), DriftConsumerError>;
}

/// In-memory audit sink for testing.
#[derive(Debug, Default)]
pub struct InMemoryDriftAuditSink {
    /// All emitted records in order.
    pub records: Vec<DriftAuditRecord>,
}

impl DriftAuditSink for InMemoryDriftAuditSink {
    fn emit(&mut self, record: DriftAuditRecord) -> Result<(), DriftConsumerError> {
        self.records.push(record);
        Ok(())
    }
}

/// Failing audit sink — always errors; used to test fail-CLOSED behavior.
#[derive(Debug)]
pub struct FailingDriftAuditSink;

impl DriftAuditSink for FailingDriftAuditSink {
    fn emit(&mut self, _record: DriftAuditRecord) -> Result<(), DriftConsumerError> {
        Err(DriftConsumerError::AuditFailed(
            "FailingDriftAuditSink always fails".to_owned(),
        ))
    }
}

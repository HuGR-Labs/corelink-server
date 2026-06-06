//! Audit subsystem for the progressive rollout controller
//! (WI-S13-005).
//!
//! Provides [`RolloutAuditEventType`] taxonomy, [`RolloutAuditSink`]
//! trait, [`InMemoryRolloutAuditSink`] capture sink, and
//! [`FailingRolloutAuditSink`] adversarial fixture.
//!
//! Per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (S-03 herdada): every
//! rollout state-machine decision arm emits its audit record BEFORE
//! state mutation; audit failure aborts the transition.
//!
//! CloudEvent type strings follow `corelink.admin.rollout.*` namespace
//! (WI-S13-005 §6.1.8).

use std::sync::{Arc, Mutex};

use super::error::RolloutError;
use super::types::{AdminActor, AutoRollbackTrigger, RolloutStage, RolloutStatus};
use uuid::Uuid;

/// Canonical audit event types emitted by the rollout controller.
///
/// All events follow CloudEvents v1.0.2 envelope with source
/// `corelink/admin/rollout-controller/{env}`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RolloutAuditEventType {
    /// `corelink.admin.rollout.start` — rollout session initiated.
    Start,
    /// `corelink.admin.rollout.stage_advance` — stage advanced to next.
    StageAdvance,
    /// `corelink.admin.rollout.hold` — probe held at current stage.
    Hold,
    /// `corelink.admin.rollout.auto_rollback` — auto-rollback triggered.
    AutoRollback,
    /// `corelink.admin.rollout.complete` — rollout reached Stage100Pct.
    Complete,
    /// `corelink.admin.rollout.abort` — admin manually aborted.
    Abort,
    /// `corelink.admin.rollout.budget_freeze` — monthly budget exceeded.
    BudgetFreeze,
    /// `corelink.admin.rollout.bypass_attempted` — stage bypass rejected.
    BypassAttempted,
}

impl RolloutAuditEventType {
    /// CloudEvents `type` field string for this event.
    #[must_use]
    pub fn as_cloud_event_type(&self) -> &'static str {
        match self {
            Self::Start => "corelink.admin.rollout.start",
            Self::StageAdvance => "corelink.admin.rollout.stage_advance",
            Self::Hold => "corelink.admin.rollout.hold",
            Self::AutoRollback => "corelink.admin.rollout.auto_rollback",
            Self::Complete => "corelink.admin.rollout.complete",
            Self::Abort => "corelink.admin.rollout.abort",
            Self::BudgetFreeze => "corelink.admin.rollout.budget_freeze",
            Self::BypassAttempted => "corelink.admin.rollout.bypass_attempted",
        }
    }
}

/// Audit record emitted per rollout state-machine transition.
#[derive(Debug, Clone)]
pub struct RolloutAuditRecord {
    /// Monotonic event identifier (UUIDv7).
    pub event_id: Uuid,
    /// Unix timestamp (ms) when the event was emitted.
    pub emitted_at_ms: u64,
    /// CloudEvent type.
    pub event_type: RolloutAuditEventType,
    /// Rollout session handle being audited.
    pub handle_id: Uuid,
    /// Current stage at time of event.
    pub stage: RolloutStage,
    /// Status after transition (or current if Hold).
    pub status: RolloutStatus,
    /// Auto-rollback trigger, if event_type == AutoRollback.
    pub rollback_trigger: Option<AutoRollbackTrigger>,
    /// Admin actor who initiated the operation.
    pub actor_user_id: Uuid,
    /// Human-readable detail or reason string.
    pub detail: String,
}

/// Audit sink trait. Production binding: atomic CloudEvent batch
/// alongside D1 UPDATE (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
pub trait RolloutAuditSink: Send + Sync {
    /// Emit audit record. MUST be called BEFORE state mutation.
    /// On error: transition aborted; caller propagates as
    /// [`RolloutError::Audit`].
    fn emit(&self, record: RolloutAuditRecord) -> Result<(), RolloutError>;
}

/// In-memory capture sink for tests. Stores all emitted records.
///
/// Per F-001: per-instance `Arc<Mutex<>>` (no global state).
#[derive(Debug, Clone)]
pub struct InMemoryRolloutAuditSink {
    records: Arc<Mutex<Vec<RolloutAuditRecord>>>,
}

impl InMemoryRolloutAuditSink {
    /// Create a new empty in-memory audit sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Snapshot of all emitted records (cloned).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::Internal`] if the internal mutex is
    /// poisoned.
    pub fn records(&self) -> Result<Vec<RolloutAuditRecord>, RolloutError> {
        self.records
            .lock()
            .map_err(|e| RolloutError::Internal(format!("audit mutex poisoned: {e}")))
            .map(|g| g.clone())
    }
}

impl Default for InMemoryRolloutAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl RolloutAuditSink for InMemoryRolloutAuditSink {
    fn emit(&self, record: RolloutAuditRecord) -> Result<(), RolloutError> {
        self.records
            .lock()
            .map_err(|e| RolloutError::Internal(format!("audit mutex poisoned: {e}")))?
            .push(record);
        Ok(())
    }
}

/// Adversarial fixture: always fails. Used in property tests to
/// verify fail-CLOSED envelope (transition aborted on audit failure).
#[derive(Debug, Clone)]
pub struct FailingRolloutAuditSink;

impl RolloutAuditSink for FailingRolloutAuditSink {
    fn emit(&self, _record: RolloutAuditRecord) -> Result<(), RolloutError> {
        Err(RolloutError::Audit(
            "FailingRolloutAuditSink: induced failure".to_string(),
        ))
    }
}

/// Returns the canonical set of CloudEvent type strings emitted by the
/// rollout controller. Used for surface-stability tests.
///
/// Count: 8 (Start / StageAdvance / Hold / AutoRollback / Complete /
/// Abort / BudgetFreeze / BypassAttempted).
#[must_use]
pub fn canonical_rollout_audit_event_strings() -> Vec<&'static str> {
    vec![
        "corelink.admin.rollout.start",
        "corelink.admin.rollout.stage_advance",
        "corelink.admin.rollout.hold",
        "corelink.admin.rollout.auto_rollback",
        "corelink.admin.rollout.complete",
        "corelink.admin.rollout.abort",
        "corelink.admin.rollout.budget_freeze",
        "corelink.admin.rollout.bypass_attempted",
    ]
}

/// Build an [`AdminActor`] for use in tests.
///
/// `mfa_age_ms`: how many ms ago MFA was verified. Values ≤ 1_800_000
/// (30 min) are "fresh".
#[must_use]
pub fn test_actor(mfa_age_ms: u64, now_ms: u64) -> AdminActor {
    AdminActor {
        user_id: Uuid::now_v7(),
        email: "test-admin@humangr-labs.io".to_string(),
        mfa_verified_at_ms: now_ms.saturating_sub(mfa_age_ms),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests use panics as assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_audit_event_count_pinned() {
        let strings = canonical_rollout_audit_event_strings();
        assert_eq!(
            strings.len(),
            8,
            "expected 8 canonical rollout audit events"
        );
    }

    #[test]
    fn all_event_types_have_distinct_strings() {
        let strings = canonical_rollout_audit_event_strings();
        let unique: std::collections::HashSet<_> = strings.iter().collect();
        assert_eq!(
            unique.len(),
            strings.len(),
            "duplicate CloudEvent type strings"
        );
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryRolloutAuditSink::new();
        let record = RolloutAuditRecord {
            event_id: Uuid::now_v7(),
            emitted_at_ms: 1_000,
            event_type: RolloutAuditEventType::Start,
            handle_id: Uuid::now_v7(),
            stage: RolloutStage::Stage1Pct,
            status: RolloutStatus::Active,
            rollback_trigger: None,
            actor_user_id: Uuid::now_v7(),
            detail: "test".to_string(),
        };
        sink.emit(record).unwrap();
        assert_eq!(sink.records().unwrap().len(), 1);
    }

    #[test]
    fn failing_sink_returns_error() {
        let sink = FailingRolloutAuditSink;
        let record = RolloutAuditRecord {
            event_id: Uuid::now_v7(),
            emitted_at_ms: 1_000,
            event_type: RolloutAuditEventType::Start,
            handle_id: Uuid::now_v7(),
            stage: RolloutStage::Stage1Pct,
            status: RolloutStatus::Active,
            rollback_trigger: None,
            actor_user_id: Uuid::now_v7(),
            detail: "test".to_string(),
        };
        assert!(matches!(sink.emit(record), Err(RolloutError::Audit(_))));
    }
}

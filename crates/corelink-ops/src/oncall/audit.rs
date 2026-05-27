//! Oncall audit-of-audit taxonomy.
//!
//! Per Lote 10.6bis pattern + S-07 P1-1 fix: every ledger state
//! mutation emits an audit record BEFORE the mutation. Audit failure
//! returns a typed error and the mutation is aborted (fail-CLOSED).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use super::engineer::EngineerId;
use super::threshold::{FatigueThreshold, HandoffDecision};
use super::tier::Tier;

/// Canonical 5-element audit event taxonomy per WI §16.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OncallAuditEventType {
    /// `corelink.oncall.shift_started` — new shift accepted into a rotation.
    ShiftStarted,
    /// `corelink.oncall.shift_ended` — shift's end_ms reached.
    ShiftEnded,
    /// `corelink.oncall.page_recorded` — PageEvent persisted to ledger.
    PageRecorded,
    /// `corelink.oncall.fatigue_threshold_breached` — soft or HARD threshold breach.
    FatigueThresholdBreached,
    /// `corelink.oncall.handoff_executed` — automatic handoff via PagerDuty API.
    HandoffExecuted,
}

impl OncallAuditEventType {
    /// Canonical CloudEvents type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShiftStarted => "corelink.oncall.shift_started",
            Self::ShiftEnded => "corelink.oncall.shift_ended",
            Self::PageRecorded => "corelink.oncall.page_recorded",
            Self::FatigueThresholdBreached => "corelink.oncall.fatigue_threshold_breached",
            Self::HandoffExecuted => "corelink.oncall.handoff_executed",
        }
    }
}

impl core::fmt::Display for OncallAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-element list of [`OncallAuditEventType`] for
/// surface-stability regression tests.
#[must_use]
pub const fn canonical_oncall_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.oncall.shift_started",
        "corelink.oncall.shift_ended",
        "corelink.oncall.page_recorded",
        "corelink.oncall.fatigue_threshold_breached",
        "corelink.oncall.handoff_executed",
    ]
}

/// Audit record emitted on every ledger mutation arm. The `engineer`
/// and `tier` plus optional `threshold` / `decision` fields are
/// populated per the arm; unused fields remain `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OncallAuditRecord {
    /// CloudEvents type.
    pub event_type: OncallAuditEventType,
    /// Subject engineer (rostered or paged).
    pub engineer: EngineerId,
    /// Tier the record pertains to.
    pub tier: Tier,
    /// Timestamp (ms since epoch).
    pub ts_ms: u64,
    /// Correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
    /// Threshold (populated for `FatigueThresholdBreached`).
    pub threshold: Option<FatigueThreshold>,
    /// Decision (populated for `HandoffExecuted`).
    pub decision: Option<HandoffDecision>,
}

impl OncallAuditRecord {
    /// Construct a minimal record (used by `ShiftStarted` / `ShiftEnded`
    /// / `PageRecorded`).
    #[must_use]
    pub fn new(
        event_type: OncallAuditEventType,
        engineer: EngineerId,
        tier: Tier,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            engineer,
            tier,
            ts_ms,
            correlation_id: correlation_id.into(),
            threshold: None,
            decision: None,
        }
    }

    /// Attach a [`FatigueThreshold`] (used by `FatigueThresholdBreached`).
    #[must_use]
    pub fn with_threshold(mut self, threshold: FatigueThreshold) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Attach a [`HandoffDecision`] (used by `HandoffExecuted`).
    #[must_use]
    pub fn with_decision(mut self, decision: HandoffDecision) -> Self {
        self.decision = Some(decision);
        self
    }
}

/// Audit-of-audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OncallAuditEmitError {
    /// Downstream audit sink rejected the record (e.g. R2 write
    /// failure / chain-link advance failure).
    #[error("oncall audit sink rejected emit: {0}")]
    Rejected(String),
}

/// Trait every oncall audit-of-audit sink satisfies.
pub trait OncallAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit record. MUST be invoked BEFORE the ledger
    /// mutation; a returned `Err` aborts the mutation per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    fn emit(&self, record: &OncallAuditRecord) -> Result<(), OncallAuditEmitError>;
}

/// In-memory test sink — accumulates emitted records for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryOncallAuditSink {
    records: Arc<Mutex<Vec<OncallAuditRecord>>>,
}

impl InMemoryOncallAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<OncallAuditRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.records.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl OncallAuditSink for InMemoryOncallAuditSink {
    fn emit(&self, record: &OncallAuditRecord) -> Result<(), OncallAuditEmitError> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| OncallAuditEmitError::Rejected(format!("mutex poisoned: {e}")))?;
        g.push(record.clone());
        Ok(())
    }
}

/// Adversarial fixture sink that always rejects (forces ledger
/// fail-CLOSED behaviour in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingOncallAuditSink;

impl OncallAuditSink for FailingOncallAuditSink {
    fn emit(&self, _record: &OncallAuditRecord) -> Result<(), OncallAuditEmitError> {
        Err(OncallAuditEmitError::Rejected(
            "adversarial fixture: always rejects".to_string(),
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

    #[test]
    fn audit_event_strings_canonical() {
        let expected = canonical_oncall_audit_event_strings();
        let actual: Vec<&str> = [
            OncallAuditEventType::ShiftStarted,
            OncallAuditEventType::ShiftEnded,
            OncallAuditEventType::PageRecorded,
            OncallAuditEventType::FatigueThresholdBreached,
            OncallAuditEventType::HandoffExecuted,
        ]
        .iter()
        .map(|e| e.as_str())
        .collect();
        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn in_memory_sink_records_emit() {
        let sink = InMemoryOncallAuditSink::new();
        assert!(sink.is_empty());
        let rec = OncallAuditRecord::new(
            OncallAuditEventType::ShiftStarted,
            EngineerId::new("eng-a"),
            Tier::Tier1,
            100,
            "cid-1",
        );
        sink.emit(&rec).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], rec);
    }

    #[test]
    fn failing_sink_always_rejects() {
        let sink = FailingOncallAuditSink;
        let rec = OncallAuditRecord::new(
            OncallAuditEventType::ShiftStarted,
            EngineerId::new("eng-a"),
            Tier::Tier1,
            100,
            "cid-1",
        );
        assert!(sink.emit(&rec).is_err());
    }
}

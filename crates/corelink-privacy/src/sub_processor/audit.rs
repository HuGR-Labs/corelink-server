//! Audit sink trait + implementations for sub-processor CloudEvents.
//!
//! Per INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116): every CloudEvent
//! MUST be emitted to audit-`<region>` Object Lock 7y. Emit is
//! fail-CLOSED: if the sink returns an error, the operation aborts
//! and state is NOT mutated.

use std::sync::{Arc, Mutex};

use super::error::SubProcessorAuditSinkError;
use super::event::SubProcessorEventType;

/// A single audit record emitted for a sub-processor event.
#[derive(Debug, Clone)]
pub struct SubProcessorAuditRecord {
    /// CloudEvent type string.
    pub event_type: SubProcessorEventType,
    /// Event source (e.g., "corelink/cd-pipeline" or "corelink/objection-handler").
    pub source: String,
    /// Event ID (ULID recommended).
    pub event_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// JSON-serialized event payload.
    pub payload_json: String,
    /// Audit region (e.g., "weur", "wnam").
    pub region: String,
}

/// Trait for emitting sub-processor audit records (fail-CLOSED).
///
/// Production wiring: R2 `audit-<region>` Object Lock 7y.
/// Test wiring: [`InMemorySubProcessorAuditSink`] + [`FailingSubProcessorAuditSink`].
pub trait SubProcessorAuditSink: std::fmt::Debug + Send + Sync {
    /// Emit a sub-processor audit record.
    ///
    /// # Fail-CLOSED contract
    ///
    /// Returning `Err(...)` causes the calling orchestrator to abort the
    /// entire operation without mutating any state.
    fn emit(&self, record: SubProcessorAuditRecord) -> Result<(), SubProcessorAuditSinkError>;
}

/// In-memory capture sink for testing.
///
/// Per-instance `Arc<Mutex<>>` — NEVER `static LazyLock<Mutex<>>` (F-001).
#[derive(Debug, Clone)]
pub struct InMemorySubProcessorAuditSink {
    records: Arc<Mutex<Vec<SubProcessorAuditRecord>>>,
}

impl InMemorySubProcessorAuditSink {
    /// Construct a new empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return a snapshot of all captured records.
    #[must_use]
    pub fn captured(&self) -> Vec<SubProcessorAuditRecord> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Return the count of captured records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Return true if no records have been captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemorySubProcessorAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl SubProcessorAuditSink for InMemorySubProcessorAuditSink {
    fn emit(&self, record: SubProcessorAuditRecord) -> Result<(), SubProcessorAuditSinkError> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(record);
        Ok(())
    }
}

/// Always-failing audit sink for fail-CLOSED envelope testing.
#[derive(Debug, Clone)]
pub struct FailingSubProcessorAuditSink;

impl SubProcessorAuditSink for FailingSubProcessorAuditSink {
    fn emit(&self, _record: SubProcessorAuditRecord) -> Result<(), SubProcessorAuditSinkError> {
        Err(SubProcessorAuditSinkError::Unavailable(
            "FailingSubProcessorAuditSink always fails".into(),
        ))
    }
}

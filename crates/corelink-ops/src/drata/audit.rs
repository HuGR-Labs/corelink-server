//! Audit envelope: every Drata push emits a `sent` or `failed` event
//! BEFORE the outcome is propagated to the caller (fail-CLOSED per
//! INV-AUDIT-EMIT-ATOMIC).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use super::stream::EvidenceStream;

/// Outcome taxonomy. Sent = Drata accepted the record. Failed = network
/// or 4xx/5xx after retry exhaustion. Skipped = idempotency ledger
/// already had the hash (Drata never called).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SyncAuditOutcome {
    /// Drata returned 2xx.
    Sent,
    /// Drata rejected after retries OR transport exhausted.
    Failed,
    /// Idempotency ledger already had the hash; no network call made.
    Skipped,
}

impl SyncAuditOutcome {
    /// Canonical audit-chain event type.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Sent => "corelink.compliance.drata_evidence_sent",
            Self::Failed => "corelink.compliance.drata_evidence_failed",
            Self::Skipped => "corelink.compliance.drata_evidence_skipped",
        }
    }
}

/// Audit envelope for one record push.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncAuditEvent {
    /// Outcome.
    pub outcome: SyncAuditOutcome,
    /// Stream this record targets.
    pub stream: EvidenceStream,
    /// SHA-256 hex of the canonical record JSON.
    pub record_sha256: String,
    /// Redacted API key (NEVER plaintext — pre-redacted via
    /// [`super::redact::redact_api_key`]).
    pub api_key_redacted: String,
    /// Drata receipt id (Some on Sent; None on Failed/Skipped).
    pub receipt_id: Option<String>,
    /// Final HTTP status (None on transport error / Skipped).
    pub final_status: Option<u16>,
    /// Number of attempts (1 = first-shot success; 0 = Skipped).
    pub attempts: u32,
    /// Reason (filled on Failed).
    pub reason: Option<String>,
}

/// Audit sink trait. Production wiring routes to the corelink audit
/// chain (S-04); the in-memory fake here drives unit tests.
pub trait SyncAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit event. Returning `Err` is fail-CLOSED:
    /// the caller MUST propagate.
    ///
    /// # Errors
    ///
    /// Returns [`SyncAuditError::EmitFailed`] when the chain rejects.
    fn emit(&self, event: &SyncAuditEvent) -> Result<(), SyncAuditError>;
}

/// Audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncAuditError {
    /// Audit chain rejected (storage / signature failure).
    #[error("drata-sync audit emit failed: {0}")]
    EmitFailed(String),
}

/// In-memory sink — records every emitted event.
#[derive(Clone, Debug, Default)]
pub struct InMemorySyncAuditSink {
    events: Arc<Mutex<Vec<SyncAuditEvent>>>,
}

impl InMemorySyncAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SyncAuditEvent> {
        match self.events.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.events.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when no events recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SyncAuditSink for InMemorySyncAuditSink {
    fn emit(&self, event: &SyncAuditEvent) -> Result<(), SyncAuditError> {
        let mut g = self
            .events
            .lock()
            .map_err(|e| SyncAuditError::EmitFailed(format!("mutex poisoned: {e}")))?;
        g.push(event.clone());
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
    fn event_types_are_canonical() {
        assert_eq!(
            SyncAuditOutcome::Sent.event_type(),
            "corelink.compliance.drata_evidence_sent"
        );
        assert_eq!(
            SyncAuditOutcome::Failed.event_type(),
            "corelink.compliance.drata_evidence_failed"
        );
        assert_eq!(
            SyncAuditOutcome::Skipped.event_type(),
            "corelink.compliance.drata_evidence_skipped"
        );
    }

    #[test]
    fn in_memory_sink_records() {
        let sink = InMemorySyncAuditSink::new();
        let evt = SyncAuditEvent {
            outcome: SyncAuditOutcome::Sent,
            stream: EvidenceStream::AuditLogs,
            record_sha256: "deadbeef".into(),
            api_key_redacted: "drata-***1234".into(),
            receipt_id: Some("rcp_1".into()),
            final_status: Some(200),
            attempts: 1,
            reason: None,
        };
        sink.emit(&evt).unwrap();
        assert_eq!(sink.snapshot(), vec![evt]);
    }
}

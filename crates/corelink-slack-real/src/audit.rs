//! Audit envelope: every dispatch emits a `sent` or `failed` event.
//!
//! Per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (Lote 10.6bis), audit fires
//! BEFORE the final outcome is reported to the caller. The audit sink
//! is fail-CLOSED: if the audit emit returns an error the caller MUST
//! propagate it instead of treating the Slack send as successful.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::channel::SlackChannel;

/// Canonical audit event taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SlackAuditOutcome {
    /// Message was accepted by Slack (2xx).
    Sent,
    /// Send failed after retry exhaustion or on a permanent 4xx.
    Failed,
}

impl SlackAuditOutcome {
    /// Canonical event-type string emitted into the audit chain.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Sent => "corelink.notification.slack.sent",
            Self::Failed => "corelink.notification.slack.failed",
        }
    }
}

/// Audit event envelope for a single dispatch attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackAuditEvent {
    /// Outcome (`Sent` or `Failed`).
    pub outcome: SlackAuditOutcome,
    /// Channel the dispatch targeted.
    pub channel: SlackChannel,
    /// Redacted webhook URL (NEVER plaintext — already passed through
    /// [`crate::redact::redact_webhook`]).
    pub webhook_redacted: String,
    /// Final HTTP status (None on transport error or pre-flight reject).
    pub final_status: Option<u16>,
    /// Number of attempts made (1 = first-shot success).
    pub attempts: u32,
    /// Optional reason string (filled on `Failed`).
    pub reason: Option<String>,
}

/// Audit sink trait. Real production wiring routes to the corelink
/// audit chain (S-04). In-memory fakes here pin invariants for tests.
pub trait SlackAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit event.
    ///
    /// # Errors
    ///
    /// Returns [`SlackAuditError::EmitFailed`] when the audit chain
    /// rejects the event. Callers MUST treat this as fatal (fail-CLOSED).
    fn emit(&self, event: &SlackAuditEvent) -> Result<(), SlackAuditError>;
}

/// Audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlackAuditError {
    /// Audit chain rejected (storage failure, signature mismatch).
    #[error("slack audit emit failed: {0}")]
    EmitFailed(String),
}

/// In-memory sink — records every emitted event.
#[derive(Clone, Debug, Default)]
pub struct InMemorySlackAuditSink {
    events: Arc<Mutex<Vec<SlackAuditEvent>>>,
}

impl InMemorySlackAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SlackAuditEvent> {
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

    /// True when no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SlackAuditSink for InMemorySlackAuditSink {
    fn emit(&self, event: &SlackAuditEvent) -> Result<(), SlackAuditError> {
        let mut g = self
            .events
            .lock()
            .map_err(|e| SlackAuditError::EmitFailed(format!("mutex poisoned: {e}")))?;
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
    fn outcome_event_types_are_canonical() {
        assert_eq!(
            SlackAuditOutcome::Sent.event_type(),
            "corelink.notification.slack.sent"
        );
        assert_eq!(
            SlackAuditOutcome::Failed.event_type(),
            "corelink.notification.slack.failed"
        );
    }

    #[test]
    fn in_memory_sink_records() {
        let sink = InMemorySlackAuditSink::new();
        let evt = SlackAuditEvent {
            outcome: SlackAuditOutcome::Sent,
            channel: SlackChannel::AlertsSev1,
            webhook_redacted: "https://hooks.slack.com/services/T0/B0/***".into(),
            final_status: Some(200),
            attempts: 1,
            reason: None,
        };
        sink.emit(&evt).unwrap();
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0], evt);
    }
}

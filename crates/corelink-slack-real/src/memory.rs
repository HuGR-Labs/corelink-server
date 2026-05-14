//! In-memory shared Slack client (test fake).
//!
//! Records every dispatched message + emits the canonical audit event
//! through the configured [`SlackAuditSink`]. Always succeeds (returns
//! status 200) — adversarial tests should wrap or compose with a
//! failing audit sink to exercise the fail-CLOSED path.

use std::sync::{Arc, Mutex};

use crate::audit::{
    InMemorySlackAuditSink, SlackAuditEvent, SlackAuditOutcome, SlackAuditSink,
};
use crate::channel::SlackChannel;
use crate::client::{SendOutcome, SharedSlackClient, SlackClientError};
use crate::message::SlackMessage;

/// Recorded send entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedSend {
    /// Target channel.
    pub channel: SlackChannel,
    /// Wire JSON that would be POSTed.
    pub payload: serde_json::Value,
}

/// In-memory shared Slack client fake.
#[derive(Clone, Debug)]
pub struct InMemorySharedSlackClient {
    sends: Arc<Mutex<Vec<RecordedSend>>>,
    audit: Arc<dyn SlackAuditSink>,
}

impl Default for InMemorySharedSlackClient {
    fn default() -> Self {
        Self::new(Arc::new(InMemorySlackAuditSink::new()))
    }
}

impl InMemorySharedSlackClient {
    /// Construct a new in-memory client with the given audit sink.
    #[must_use]
    pub fn new(audit: Arc<dyn SlackAuditSink>) -> Self {
        Self {
            sends: Arc::new(Mutex::new(Vec::new())),
            audit,
        }
    }

    /// Snapshot recorded sends.
    #[must_use]
    pub fn snapshot(&self) -> Vec<RecordedSend> {
        match self.sends.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded sends.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.sends.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True when no sends have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SharedSlackClient for InMemorySharedSlackClient {
    fn send(&self, message: &SlackMessage) -> Result<SendOutcome, SlackClientError> {
        // Audit-emit-BEFORE-mutation (fail-CLOSED).
        let event = SlackAuditEvent {
            outcome: SlackAuditOutcome::Sent,
            channel: message.channel,
            webhook_redacted: "<in-memory-fake>".to_string(),
            final_status: Some(200),
            attempts: 1,
            reason: None,
        };
        self.audit.emit(&event)?;

        let mut g = self
            .sends
            .lock()
            .map_err(|e| SlackClientError::TransportExhausted {
                attempts: 0,
                reason: format!("mutex poisoned: {e}"),
            })?;
        g.push(RecordedSend {
            channel: message.channel,
            payload: message.to_block_kit_json(),
        });
        Ok(SendOutcome {
            channel: message.channel,
            status: 200,
            attempts: 1,
            thread_ts: None,
        })
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
    fn in_memory_records_send_and_emits_audit() {
        let audit = Arc::new(InMemorySlackAuditSink::new());
        let client = InMemorySharedSlackClient::new(audit.clone());
        let msg = SlackMessage::new(SlackChannel::AlertsSev2, "hi", "f", "t");
        let outcome = client.send(&msg).unwrap();
        assert_eq!(outcome.status, 200);
        assert_eq!(outcome.attempts, 1);
        assert_eq!(outcome.channel, SlackChannel::AlertsSev2);
        assert_eq!(client.len(), 1);
        assert_eq!(audit.len(), 1);
        assert_eq!(audit.snapshot()[0].outcome, SlackAuditOutcome::Sent);
        assert_eq!(audit.snapshot()[0].channel, SlackChannel::AlertsSev2);
    }
}

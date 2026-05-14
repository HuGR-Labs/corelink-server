//! Canonical [`SharedSlackClient`] trait.

use thiserror::Error;

use crate::audit::SlackAuditError;
use crate::channel::{SlackChannel, WebhookRegistryError};
use crate::message::SlackMessage;

/// Successful send outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendOutcome {
    /// Target channel.
    pub channel: SlackChannel,
    /// Final HTTP status (always 2xx on success).
    pub status: u16,
    /// Number of attempts taken (1 = first-shot).
    pub attempts: u32,
    /// Slack message thread_ts if returned (incoming webhooks usually
    /// echo back nothing; this field is populated only when the wire
    /// response was a JSON object including `ts`).
    pub thread_ts: Option<String>,
}

/// Top-level error type for the shared Slack client surface.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlackClientError {
    /// Channel has no configured webhook URL.
    #[error("slack client config: {0}")]
    Registry(#[from] WebhookRegistryError),
    /// Transport failure exhausted retries.
    #[error("slack transport failure after {attempts} attempts: {reason}")]
    TransportExhausted {
        /// Total attempts made.
        attempts: u32,
        /// Final reason string.
        reason: String,
    },
    /// Permanent (non-retryable) rejection.
    #[error("slack rejected (status {status}): {reason}")]
    PermanentReject {
        /// HTTP status returned by Slack.
        status: u16,
        /// Reason / body excerpt.
        reason: String,
    },
    /// Audit emit failed — caller MUST treat the dispatch as a no-op.
    #[error("slack audit emit failed: {0}")]
    AuditFailed(#[from] SlackAuditError),
}

/// Canonical trait every shared Slack client implementation satisfies.
///
/// Each consumer (alerts, enterprise inquiry, breach notification,
/// oncall handoff, lighthouse) wires through this trait via the
/// per-channel routing held by the implementation.
pub trait SharedSlackClient: core::fmt::Debug + Send + Sync {
    /// Dispatch `message` to its routing channel.
    ///
    /// # Errors
    ///
    /// Returns any [`SlackClientError`] variant: missing webhook URL
    /// for the channel, transport-exhausted retries, permanent reject,
    /// or fail-CLOSED audit emit error.
    fn send(&self, message: &SlackMessage) -> Result<SendOutcome, SlackClientError>;
}

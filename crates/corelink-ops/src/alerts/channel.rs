//! Alert channel result types, delivery envelopes, and provider boundary.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

/// Closed set of customer-alert delivery channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertChannel {
    /// Durable customer dashboard alert row/fanout.
    Dashboard,
    /// Transactional customer email.
    Email,
    /// In-app customer notification.
    InApp,
    /// Optional operator/customer Slack webhook.
    Slack,
}

impl AlertChannel {
    /// Stable provider and receipt label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Email => "email",
            Self::InApp => "in_app",
            Self::Slack => "slack",
        }
    }
}

impl std::fmt::Display for AlertChannel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// PII-minimized, closed payload sent to an alert provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlertEnvelope {
    /// Event kind (`cmk_revoked` or `cmk_recovered`).
    pub event: String,
    /// KMS provider name.
    pub provider: String,
    /// Hashed tenant correlation value; raw tenant IDs never leave this layer.
    pub tenant_id_hashed: String,
    /// Revocation detection timestamp in milliseconds.
    pub detected_at_ms: u64,
    /// Total kill-switch duration in milliseconds (zero for recovery).
    pub kill_switch_duration_ms: u64,
    /// Operator/customer recovery instructions.
    pub recovery_instructions: String,
    /// Restoration timestamp for recovery events.
    pub restored_at_ms: Option<u64>,
}

/// Provider receipt proving a channel accepted the envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReceipt {
    /// Channel that accepted the envelope.
    pub channel: AlertChannel,
    /// Provider-owned or locally generated receipt identifier.
    pub receipt_id: String,
}

/// Fail-closed transport errors; no error is interpreted as delivery.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AlertTransportError {
    /// The channel was enabled but has no endpoint/provider configuration.
    #[error("{channel} alert provider is not configured")]
    NotConfigured { channel: AlertChannel },
    /// The provider endpoint is not a valid absolute URL.
    #[error("{channel} alert provider endpoint is invalid: {detail}")]
    InvalidEndpoint {
        /// Channel whose endpoint failed validation.
        channel: AlertChannel,
        /// Validation detail.
        detail: String,
    },
    /// The bounded request transport failed before acceptance.
    #[error("{channel} alert transport failed: {detail}")]
    Transport {
        /// Channel whose request failed.
        channel: AlertChannel,
        /// Redacted transport detail.
        detail: String,
    },
    /// Provider rejected the envelope; the response body is never retained.
    #[error("{channel} alert provider rejected status {status}")]
    Rejected {
        /// Channel whose provider rejected the request.
        channel: AlertChannel,
        /// HTTP status returned by the provider.
        status: u16,
    },
}

/// Repo-owned boundary for real channel delivery.
///
/// Production uses [`crate::alerts::alerter::HttpAlertTransport`]. Tests and
/// deployments may inject a provider backed by a local queue or an owned
/// adapter; every implementation must return a receipt only after acceptance.
#[async_trait]
pub trait AlertTransport: std::fmt::Debug + Send + Sync {
    /// Deliver one closed envelope to one closed channel.
    async fn deliver(
        &self,
        channel: AlertChannel,
        envelope: AlertEnvelope,
    ) -> Result<DeliveryReceipt, AlertTransportError>;
}

/// Deterministic local transport for unit/focal harnesses.
///
/// This is not selected by production construction. It records each accepted
/// envelope and returns an explicit receipt without network or credentials.
#[derive(Debug, Clone, Default)]
pub struct RecordingAlertTransport {
    deliveries: Arc<Mutex<Vec<(AlertChannel, AlertEnvelope)>>>,
}

impl RecordingAlertTransport {
    /// Return a snapshot of accepted deliveries as operational evidence.
    #[must_use]
    pub fn deliveries(&self) -> Vec<(AlertChannel, AlertEnvelope)> {
        self.deliveries
            .lock()
            .map(|items| items.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl AlertTransport for RecordingAlertTransport {
    async fn deliver(
        &self,
        channel: AlertChannel,
        envelope: AlertEnvelope,
    ) -> Result<DeliveryReceipt, AlertTransportError> {
        let mut deliveries =
            self.deliveries
                .lock()
                .map_err(|_| AlertTransportError::Transport {
                    channel,
                    detail: "recording transport mutex poisoned".to_string(),
                })?;
        deliveries.push((channel, envelope));
        Ok(DeliveryReceipt {
            channel,
            receipt_id: format!("recording:{}:{}", channel.as_str(), deliveries.len()),
        })
    }
}

/// Outcome of a single alert channel dispatch.
#[derive(Debug, Clone)]
pub enum ChannelOutcome {
    /// Channel succeeded.
    Success(String),
    /// Channel failed; message describes the failure.
    Failed(String),
    /// Channel disabled in config.
    Disabled,
}

impl ChannelOutcome {
    /// Return true if this channel succeeded.
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// Summary of all channel outcomes for a single alert dispatch.
#[derive(Debug, Clone)]
pub struct AlertDispatchSummary {
    /// Dashboard channel outcome.
    pub dashboard: ChannelOutcome,
    /// Email channel outcome.
    pub email: ChannelOutcome,
    /// In-app notification channel outcome.
    pub in_app: ChannelOutcome,
    /// Slack webhook channel outcome.
    pub slack: ChannelOutcome,
}

impl AlertDispatchSummary {
    /// Return true if at least one channel succeeded.
    #[must_use]
    pub fn any_success(&self) -> bool {
        self.dashboard.is_success()
            || self.email.is_success()
            || self.in_app.is_success()
            || self.slack.is_success()
    }

    /// Return a comma-separated list of succeeded channels.
    #[must_use]
    pub fn succeeded_channels(&self) -> String {
        let mut channels = Vec::new();
        if self.dashboard.is_success() {
            channels.push("dashboard");
        }
        if self.email.is_success() {
            channels.push("email");
        }
        if self.in_app.is_success() {
            channels.push("in_app");
        }
        if self.slack.is_success() {
            channels.push("slack");
        }
        channels.join(",")
    }
}

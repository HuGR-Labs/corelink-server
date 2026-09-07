//! [`MultiChannelAlerter`] — production multi-channel customer alert delivery.

use std::sync::Arc;
use std::time::Duration;

use corelink_byok::revocation::alerter::RevocationAlertPayload;
use corelink_byok::revocation::error::RevocationError;
use corelink_byok::revocation::CustomerAlerter;
use corelink_byok::{KmsKeyId, KmsProviderKind};
use tracing::{info, warn};

use super::channel::{
    AlertChannel, AlertDispatchSummary, AlertEnvelope, AlertTransport, AlertTransportError,
    ChannelOutcome, DeliveryReceipt,
};
use super::config::AlerterConfig;

/// Production multi-channel [`CustomerAlerter`].
///
/// Dispatches CMK revocation alerts to all enabled channels. Returns
/// `Ok` if at least one channel succeeds; `Err` only if all fail.
///
/// # Example
///
/// ```rust
/// use corelink_ops::alerts::{MultiChannelAlerter, AlerterConfig};
/// use corelink_byok::KmsKeyId;
/// use corelink_byok::revocation::alerter::RevocationAlertPayload;
/// use corelink_byok::revocation::CustomerAlerter;
///
/// # tokio_test::block_on(async {
/// let alerter = MultiChannelAlerter::new(AlerterConfig::default());
/// let payload = RevocationAlertPayload {
///     provider: "aws".to_string(),
///     kms_key_id: KmsKeyId {
///         provider: corelink_byok::KmsProviderKind::AwsKms,
///         key_arn_or_id: "k1".to_string(),
///         region: "us-east-1".to_string(),
///     },
///     tenant_id_hashed: "h1".to_string(),
///     detected_at_ms: 0,
///     kill_switch_duration_ms: 0,
///     recovery_instructions: "Re-enable CMK in AWS console.".to_string(),
/// };
/// alerter.alert(payload).await.unwrap();
/// # });
/// ```
pub struct MultiChannelAlerter {
    config: AlerterConfig,
    transport: Arc<dyn AlertTransport>,
}

impl std::fmt::Debug for MultiChannelAlerter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MultiChannelAlerter")
            .field("config", &self.config)
            .field("transport", &self.transport)
            .finish()
    }
}

impl MultiChannelAlerter {
    /// Construct with the given configuration.
    #[must_use]
    pub fn new(config: AlerterConfig) -> Self {
        let transport = Arc::new(HttpAlertTransport::from_config(&config));
        Self { config, transport }
    }

    /// Construct with an owned transport, primarily for local adapters/tests.
    ///
    /// The transport remains responsible for provider credentials and network
    /// policy. This constructor performs no I/O.
    #[must_use]
    pub fn with_transport(config: AlerterConfig, transport: Arc<dyn AlertTransport>) -> Self {
        Self { config, transport }
    }
}

#[async_trait::async_trait]
impl CustomerAlerter for MultiChannelAlerter {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        let summary = self.dispatch_all_channels(&payload).await;

        if summary.any_success() {
            info!(
                provider = payload.provider.as_str(),
                succeeded_channels = summary.succeeded_channels().as_str(),
                kill_switch_duration_ms = payload.kill_switch_duration_ms,
                "customer alert dispatched"
            );
            Ok(())
        } else {
            warn!(
                provider = payload.provider.as_str(),
                "customer alert delivery failed on every enabled channel"
            );
            Err(RevocationError::AlertDeliveryFailed {
                kms_key_id: payload.kms_key_id.to_string(),
                detail: "all alert channels failed".to_string(),
            })
        }
    }

    async fn alert_recovery(
        &self,
        provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        tenant_id_hashed: &str,
        restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        if self.config.stub_mode {
            info!(
                provider = provider.as_str(),
                kms_key_id = kms_key_id.as_str(),
                tenant_id_hashed,
                restored_at_ms,
                "stub: recovery alert dispatched"
            );
            return Ok(());
        }

        let envelope = AlertEnvelope {
            event: "cmk_recovered".to_string(),
            provider: provider.as_str().to_string(),
            tenant_id_hashed: tenant_id_hashed.to_string(),
            detected_at_ms: restored_at_ms,
            kill_switch_duration_ms: 0,
            recovery_instructions: "CMK access restored; resume normal operation.".to_string(),
            restored_at_ms: Some(restored_at_ms),
        };
        let summary = self.dispatch_envelope(&envelope).await;
        if summary.any_success() {
            info!(
                provider = provider.as_str(),
                succeeded_channels = summary.succeeded_channels().as_str(),
                restored_at_ms,
                "customer recovery alert dispatched"
            );
            Ok(())
        } else {
            warn!(
                provider = provider.as_str(),
                "customer recovery alert delivery failed on every enabled channel"
            );
            Err(RevocationError::AlertDeliveryFailed {
                kms_key_id: kms_key_id.to_string(),
                detail: "all recovery alert channels failed".to_string(),
            })
        }
    }
}

impl MultiChannelAlerter {
    /// Dispatch to all configured channels; collect outcomes.
    async fn dispatch_all_channels(
        &self,
        payload: &RevocationAlertPayload,
    ) -> AlertDispatchSummary {
        let envelope = AlertEnvelope {
            event: "cmk_revoked".to_string(),
            provider: payload.provider.clone(),
            tenant_id_hashed: payload.tenant_id_hashed.clone(),
            detected_at_ms: payload.detected_at_ms,
            kill_switch_duration_ms: payload.kill_switch_duration_ms,
            recovery_instructions: payload.recovery_instructions.clone(),
            restored_at_ms: None,
        };
        self.dispatch_envelope(&envelope).await
    }

    async fn dispatch_envelope(&self, envelope: &AlertEnvelope) -> AlertDispatchSummary {
        let dashboard = self
            .dispatch_channel(AlertChannel::Dashboard, envelope)
            .await;
        let email = self.dispatch_channel(AlertChannel::Email, envelope).await;
        let in_app = self.dispatch_channel(AlertChannel::InApp, envelope).await;
        let slack = self.dispatch_channel(AlertChannel::Slack, envelope).await;
        AlertDispatchSummary {
            dashboard,
            email,
            in_app,
            slack,
        }
    }

    async fn dispatch_channel(
        &self,
        channel: AlertChannel,
        envelope: &AlertEnvelope,
    ) -> ChannelOutcome {
        if !self.channel_enabled(channel) {
            return ChannelOutcome::Disabled;
        }
        if self.config.stub_mode {
            return ChannelOutcome::Success(format!("stub:{}", channel.as_str()));
        }
        match self.transport.deliver(channel, envelope.clone()).await {
            Ok(receipt) => self.receipt_outcome(receipt),
            Err(error) => {
                warn!(channel = channel.as_str(), error = %error, "customer alert channel failed");
                ChannelOutcome::Failed(error.to_string())
            }
        }
    }

    fn channel_enabled(&self, channel: AlertChannel) -> bool {
        match channel {
            AlertChannel::Dashboard => self.config.dashboard_enabled,
            AlertChannel::Email => self.config.email_enabled,
            AlertChannel::InApp => self.config.in_app_enabled,
            AlertChannel::Slack => self.config.slack_enabled,
        }
    }

    fn receipt_outcome(&self, receipt: DeliveryReceipt) -> ChannelOutcome {
        info!(
            channel = receipt.channel.as_str(),
            receipt_id = receipt.receipt_id.as_str(),
            "customer alert channel accepted envelope"
        );
        ChannelOutcome::Success(format!(
            "{}:{}",
            receipt.channel.as_str(),
            receipt.receipt_id
        ))
    }
}

/// Bounded HTTP implementation of the repo-owned alert provider boundary.
///
/// Endpoints and credentials are supplied by deployment configuration. The
/// client never sends during construction; a receipt is returned only for a
/// 2xx response, and response bodies are deliberately not buffered.
#[derive(Debug)]
#[non_exhaustive]
pub struct HttpAlertTransport {
    client: Option<reqwest::Client>,
    init_error: Option<String>,
    dashboard_endpoint: Option<String>,
    email_endpoint: Option<String>,
    in_app_endpoint: Option<String>,
    slack_endpoint: Option<String>,
    sendgrid_api_key: Option<String>,
}

impl HttpAlertTransport {
    fn from_config(config: &AlerterConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .user_agent("corelink-customer-alerts/1")
            .build();
        match client {
            Ok(client) => Self {
                client: Some(client),
                init_error: None,
                dashboard_endpoint: config.dashboard_endpoint.clone(),
                email_endpoint: config.email_endpoint.clone(),
                in_app_endpoint: config.in_app_endpoint.clone(),
                slack_endpoint: config.slack_webhook_url.clone(),
                sendgrid_api_key: config.sendgrid_api_key.clone(),
            },
            Err(error) => Self {
                client: None,
                init_error: Some(error.to_string()),
                dashboard_endpoint: config.dashboard_endpoint.clone(),
                email_endpoint: config.email_endpoint.clone(),
                in_app_endpoint: config.in_app_endpoint.clone(),
                slack_endpoint: config.slack_webhook_url.clone(),
                sendgrid_api_key: None,
            },
        }
    }

    fn endpoint(&self, channel: AlertChannel) -> Option<&str> {
        match channel {
            AlertChannel::Dashboard => self.dashboard_endpoint.as_deref(),
            AlertChannel::Email => self.email_endpoint.as_deref(),
            AlertChannel::InApp => self.in_app_endpoint.as_deref(),
            AlertChannel::Slack => self.slack_endpoint.as_deref(),
        }
    }
}

#[async_trait::async_trait]
impl AlertTransport for HttpAlertTransport {
    async fn deliver(
        &self,
        channel: AlertChannel,
        envelope: AlertEnvelope,
    ) -> Result<DeliveryReceipt, AlertTransportError> {
        let endpoint = self
            .endpoint(channel)
            .filter(|endpoint| !endpoint.trim().is_empty())
            .ok_or(AlertTransportError::NotConfigured { channel })?;
        let url = reqwest::Url::parse(endpoint).map_err(|error| {
            AlertTransportError::InvalidEndpoint {
                channel,
                detail: error.to_string(),
            }
        })?;
        if url.scheme() != "https"
            && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
        {
            return Err(AlertTransportError::InvalidEndpoint {
                channel,
                detail: "production alert endpoints must use https".to_string(),
            });
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AlertTransportError::Transport {
                channel,
                detail: self
                    .init_error
                    .clone()
                    .unwrap_or_else(|| "HTTP client unavailable".to_string()),
            })?;
        let mut body =
            serde_json::to_value(&envelope).map_err(|error| AlertTransportError::Transport {
                channel,
                detail: format!("envelope serialization failed: {error}"),
            })?;
        body.as_object_mut()
            .ok_or_else(|| AlertTransportError::Transport {
                channel,
                detail: "envelope must serialize as an object".to_string(),
            })?
            .insert("channel".to_string(), serde_json::json!(channel.as_str()));
        let mut request = client.post(url).json(&body);
        if channel == AlertChannel::Email {
            if let Some(api_key) = self.sendgrid_api_key.as_deref() {
                request = request.bearer_auth(api_key);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|error| AlertTransportError::Transport {
                channel,
                detail: error.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AlertTransportError::Rejected {
                channel,
                status: status.as_u16(),
            });
        }
        let receipt_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        Ok(DeliveryReceipt {
            channel,
            receipt_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> RevocationAlertPayload {
        RevocationAlertPayload {
            provider: "aws".to_owned(),
            kms_key_id: KmsKeyId {
                provider: KmsProviderKind::AwsKms,
                key_arn_or_id: "key-1".to_owned(),
                region: "us-east-1".to_owned(),
            },
            tenant_id_hashed: "tenant-hash".to_owned(),
            detected_at_ms: 1,
            kill_switch_duration_ms: 2,
            recovery_instructions: "restore the key".to_owned(),
        }
    }

    #[tokio::test]
    async fn production_alert_fails_closed_when_no_transport_is_wired() {
        let alerter = MultiChannelAlerter::new(AlerterConfig {
            stub_mode: false,
            slack_enabled: false,
            ..AlerterConfig::default()
        });
        assert!(matches!(
            alerter.alert(payload()).await,
            Err(RevocationError::AlertDeliveryFailed { .. })
        ));
    }

    #[tokio::test]
    async fn production_recovery_does_not_claim_delivery() {
        let alerter = MultiChannelAlerter::new(AlerterConfig {
            stub_mode: false,
            ..AlerterConfig::default()
        });
        let p = payload();
        assert!(alerter
            .alert_recovery(
                KmsProviderKind::AwsKms,
                &p.kms_key_id,
                &p.tenant_id_hashed,
                3,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn configured_transport_receives_all_four_closed_channels() {
        let transport = Arc::new(super::super::channel::RecordingAlertTransport::default());
        let alerter = MultiChannelAlerter::with_transport(
            AlerterConfig {
                stub_mode: false,
                slack_enabled: true,
                slack_webhook_url: Some("https://hooks.example.test/alerts".to_string()),
                ..AlerterConfig::default()
            },
            transport.clone(),
        );

        let result = alerter.alert(payload()).await;
        assert!(
            result.is_ok(),
            "configured recording transport must accept the alert: {result:?}"
        );

        let deliveries = transport.deliveries();
        assert_eq!(deliveries.len(), 4);
        assert_eq!(
            deliveries
                .iter()
                .map(|(channel, _)| *channel)
                .collect::<Vec<_>>(),
            vec![
                AlertChannel::Dashboard,
                AlertChannel::Email,
                AlertChannel::InApp,
                AlertChannel::Slack,
            ]
        );
        assert!(deliveries
            .iter()
            .all(|(_, envelope)| envelope.event == "cmk_revoked"
                && envelope.tenant_id_hashed == "tenant-hash"
                && envelope.recovery_instructions == "restore the key"));
    }

    #[tokio::test]
    async fn configured_transport_receives_recovery_and_returns_receipts() {
        let transport = Arc::new(super::super::channel::RecordingAlertTransport::default());
        let alerter = MultiChannelAlerter::with_transport(
            AlerterConfig {
                stub_mode: false,
                slack_enabled: true,
                ..AlerterConfig::default()
            },
            transport.clone(),
        );
        let p = payload();

        let result = alerter
            .alert_recovery(
                KmsProviderKind::AwsKms,
                &p.kms_key_id,
                &p.tenant_id_hashed,
                42,
            )
            .await;
        assert!(
            result.is_ok(),
            "configured recording transport must accept recovery: {result:?}"
        );

        let deliveries = transport.deliveries();
        assert_eq!(deliveries.len(), 4);
        assert!(deliveries
            .iter()
            .all(|(_, envelope)| envelope.event == "cmk_recovered"
                && envelope.restored_at_ms == Some(42)));
    }
}

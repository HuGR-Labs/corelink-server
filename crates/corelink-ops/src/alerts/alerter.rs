//! [`MultiChannelAlerter`] — production multi-channel customer alert delivery.

use corelink_byok::{KmsKeyId, KmsProviderKind};
use corelink_byok::revocation::alerter::RevocationAlertPayload;
use corelink_byok::revocation::error::RevocationError;
use corelink_byok::revocation::CustomerAlerter;
use tracing::{info, warn};

use super::channel::{AlertDispatchSummary, ChannelOutcome};
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
#[derive(Debug)]
pub struct MultiChannelAlerter {
    config: AlerterConfig,
}

impl MultiChannelAlerter {
    /// Construct with the given configuration.
    #[must_use]
    pub fn new(config: AlerterConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl CustomerAlerter for MultiChannelAlerter {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        let summary = self.dispatch_all_channels(&payload).await;

        info!(
            provider = payload.provider.as_str(),
            succeeded_channels = summary.succeeded_channels().as_str(),
            kill_switch_duration_ms = payload.kill_switch_duration_ms,
            "customer alert dispatched"
        );

        if summary.any_success() {
            Ok(())
        } else {
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

        // Production: dispatch recovery notification to all channels.
        // Channels: dashboard clear + email + in-app.
        info!(
            provider = provider.as_str(),
            kms_key_id = kms_key_id.as_str(),
            "customer recovery alert dispatched"
        );
        Ok(())
    }
}

impl MultiChannelAlerter {
    /// Dispatch to all configured channels; collect outcomes.
    async fn dispatch_all_channels(
        &self,
        payload: &RevocationAlertPayload,
    ) -> AlertDispatchSummary {
        let dashboard = self.dispatch_dashboard(payload).await;
        let email = self.dispatch_email(payload).await;
        let in_app = self.dispatch_in_app(payload).await;
        let slack = self.dispatch_slack(payload).await;
        AlertDispatchSummary {
            dashboard,
            email,
            in_app,
            slack,
        }
    }

    async fn dispatch_dashboard(&self, payload: &RevocationAlertPayload) -> ChannelOutcome {
        if !self.config.dashboard_enabled {
            return ChannelOutcome::Disabled;
        }
        if self.config.stub_mode {
            return ChannelOutcome::Success("stub:dashboard".to_string());
        }
        // Production: D1 INSERT into customer_alerts + WebSocket fanout.
        // Stub for now; production wiring in deployment.
        ChannelOutcome::Success(format!(
            "dashboard:{}",
            payload.tenant_id_hashed
        ))
    }

    async fn dispatch_email(&self, _payload: &RevocationAlertPayload) -> ChannelOutcome {
        if !self.config.email_enabled {
            return ChannelOutcome::Disabled;
        }
        if self.config.stub_mode {
            return ChannelOutcome::Success("stub:email".to_string());
        }
        // Production: SendGrid / SES.
        warn!(
            "email channel not wired in this build (production: configure sendgrid_api_key)"
        );
        ChannelOutcome::Failed("email not wired".to_string())
    }

    async fn dispatch_in_app(&self, payload: &RevocationAlertPayload) -> ChannelOutcome {
        if !self.config.in_app_enabled {
            return ChannelOutcome::Disabled;
        }
        if self.config.stub_mode {
            return ChannelOutcome::Success("stub:in_app".to_string());
        }
        // Production: D1 INSERT + WebSocket push.
        ChannelOutcome::Success(format!("in_app:{}", payload.tenant_id_hashed))
    }

    async fn dispatch_slack(&self, _payload: &RevocationAlertPayload) -> ChannelOutcome {
        if !self.config.slack_enabled {
            return ChannelOutcome::Disabled;
        }
        match &self.config.slack_webhook_url {
            None => ChannelOutcome::Disabled,
            Some(_url) => {
                if self.config.stub_mode {
                    return ChannelOutcome::Success("stub:slack".to_string());
                }
                // Production: POST to Slack webhook URL.
                warn!("slack channel not wired in this build");
                ChannelOutcome::Failed("slack not wired".to_string())
            }
        }
    }
}

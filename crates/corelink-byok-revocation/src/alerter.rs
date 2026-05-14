//! [`CustomerAlerter`] — pluggable multi-channel customer alert delivery.

use corelink_byok::{KmsKeyId, KmsProviderKind};

use crate::error::RevocationError;

/// Alert payload for a CMK revocation event.
///
/// # Example
///
/// ```rust
/// use corelink_byok::KmsKeyId;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
///
/// let payload = RevocationAlertPayload {
///     provider: "aws".to_string(),
///     kms_key_id: KmsKeyId::new("arn:aws:kms:us-east-1:123:key/abc".to_string()),
///     tenant_id_hashed: "sha256:abc123".to_string(),
///     detected_at_ms: 1_000_000,
///     kill_switch_duration_ms: 42,
///     recovery_instructions: "Re-enable CMK in AWS KMS console; access restored on next 60s check.".to_string(),
/// };
/// assert_eq!(payload.provider, "aws");
/// ```
#[derive(Debug, Clone)]
pub struct RevocationAlertPayload {
    /// Provider kind string.
    pub provider: String,
    /// KMS key ID (raw; only logged internally; NOT sent to customer).
    pub kms_key_id: KmsKeyId,
    /// Hashed tenant ID for audit correlation.
    pub tenant_id_hashed: String,
    /// Millisecond timestamp when revocation was detected.
    pub detected_at_ms: u64,
    /// Kill switch total duration in milliseconds.
    pub kill_switch_duration_ms: u64,
    /// Human-readable recovery instructions for the customer.
    pub recovery_instructions: String,
}

/// Multi-channel customer alert delivery trait.
///
/// Channels: dashboard (D1 row + WebSocket fanout) + email (SendGrid /
/// SES) + in-app notification + optional Slack webhook.
///
/// In production, `alert` must attempt all channels and return `Ok` if
/// at least one channel succeeded. Failed channels are logged at WARN.
///
/// In tests, use [`crate::testutil::NoopAlerter`] or
/// [`crate::testutil::RecordingAlerter`].
///
/// # Example
///
/// ```rust
/// use corelink_byok::KmsProviderKind;
/// use corelink_byok_revocation::CustomerAlerter;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
/// use corelink_byok_revocation::testutil::NoopAlerter;
/// use corelink_byok::KmsKeyId;
///
/// # tokio_test::block_on(async {
/// let alerter = NoopAlerter;
/// let payload = RevocationAlertPayload {
///     provider: "aws".to_string(),
///     kms_key_id: KmsKeyId::new("k1".to_string()),
///     tenant_id_hashed: "h1".to_string(),
///     detected_at_ms: 0,
///     kill_switch_duration_ms: 0,
///     recovery_instructions: "Re-enable CMK".to_string(),
/// };
/// alerter.alert(payload).await.unwrap();
/// # });
/// ```
#[async_trait::async_trait]
pub trait CustomerAlerter: Send + Sync + std::fmt::Debug {
    /// Dispatch alert to all configured channels for the given payload.
    ///
    /// Returns `Ok` if at least one channel succeeded; `Err` only if all
    /// channels failed.
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError>;

    /// Dispatch recovery alert (CMK re-enabled, access restored).
    async fn alert_recovery(
        &self,
        provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        tenant_id_hashed: &str,
        restored_at_ms: u64,
    ) -> Result<(), RevocationError>;
}

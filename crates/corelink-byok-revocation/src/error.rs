//! [`RevocationError`] — error taxonomy for the revocation detector.

use thiserror::Error;

/// Errors produced by the revocation detector.
///
/// `#[non_exhaustive]` per CoreLink codex.
///
/// # Example
///
/// ```rust
/// use corelink_byok_revocation::RevocationError;
///
/// let err = RevocationError::AuditEmitFailed("D1 batch failed".to_string());
/// assert_eq!(
///     err.to_string(),
///     "audit emit failed: D1 batch failed"
/// );
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RevocationError {
    /// KMS access check returned an error (transient; not a kill-switch).
    #[error("KMS access check error for key {kms_key_id}: {detail}")]
    KmsCheckError {
        /// The KMS key ID being checked.
        kms_key_id: String,
        /// Error detail from the provider.
        detail: String,
    },

    /// Tenant status update to `degraded_read_only` failed.
    #[error("tenant degrade failed for key {kms_key_id}: {detail}")]
    TenantDegradeFailed {
        /// The KMS key ID whose tenants were being degraded.
        kms_key_id: String,
        /// Error detail.
        detail: String,
    },

    /// Audit CloudEvent emission failed (D1 batch error; rolled back).
    #[error("audit emit failed: {0}")]
    AuditEmitFailed(String),

    /// Customer alert delivery failed on all channels.
    #[error("customer alert delivery failed for key {kms_key_id}: {detail}")]
    AlertDeliveryFailed {
        /// The KMS key ID whose customer was being alerted.
        kms_key_id: String,
        /// Detail.
        detail: String,
    },

    /// Internal logic error (should never occur in production).
    #[error("internal error: {0}")]
    Internal(String),
}

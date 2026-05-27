//! [`TenantStatusStore`] — pluggable tenant BYOK status persistence.

use crate::KmsKeyId;

use super::error::RevocationError;

/// Tenant BYOK status.
///
/// Mirrors the D1 `tenants.byok_status` column values.
///
/// # Example
///
/// ```rust
/// use corelink_byok::revocation::store::TenantByokStatus;
///
/// let s = TenantByokStatus::DegradedReadOnly;
/// assert_eq!(s.as_str(), "degraded_read_only");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantByokStatus {
    /// CMK accessible; tenant fully operational.
    Active,
    /// CMK revoked; tenant read-only (writes rejected with BYOKError::CmkRevoked).
    DegradedReadOnly,
    /// CMK permanently revoked; tenant frozen.
    Revoked,
}

impl TenantByokStatus {
    /// Canonical string for D1 `CHECK` constraint.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::DegradedReadOnly => "degraded_read_only",
            Self::Revoked => "revoked",
        }
    }
}

impl core::fmt::Display for TenantByokStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Pluggable tenant BYOK status store.
///
/// In production this wraps D1 with an atomic batch
/// `[tenants UPDATE + audit_outbox INSERT]`
/// (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). In tests, use
/// [`crate::revocation::testutil::InMemoryTenantStore`].
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use corelink_byok::{KmsKeyId, KmsProviderKind};
/// use corelink_byok::revocation::store::{TenantStatusStore, TenantByokStatus};
/// use corelink_byok::revocation::testutil::InMemoryTenantStore;
///
/// # tokio_test::block_on(async {
/// let store = Arc::new(InMemoryTenantStore::default());
/// let key_id = KmsKeyId {
///     provider: KmsProviderKind::AwsKms,
///     key_arn_or_id: "k1".to_string(),
///     region: "us-east-1".to_string(),
/// };
/// store.mark_degraded(&key_id, "aws", 1_000_000).await.unwrap();
/// # });
/// ```
#[async_trait::async_trait]
pub trait TenantStatusStore: Send + Sync + std::fmt::Debug {
    /// Mark all tenants using `kms_key_id` as `degraded_read_only`.
    ///
    /// Must be executed atomically with the audit event INSERT in D1.
    /// Returns the number of tenants updated.
    async fn mark_degraded(
        &self,
        kms_key_id: &KmsKeyId,
        provider: &str,
        revoked_at_ms: u64,
    ) -> Result<usize, RevocationError>;

    /// Restore all tenants using `kms_key_id` to `active`.
    ///
    /// Called when next 60-second check returns `KmsAccessStatus::Ok`
    /// after a previous revocation.
    async fn restore_active(
        &self,
        kms_key_id: &KmsKeyId,
        restored_at_ms: u64,
    ) -> Result<usize, RevocationError>;

    /// Return current status for `kms_key_id` (first tenant found).
    ///
    /// Used by [`RevocationDetector`] to skip duplicate kill-switch
    /// executions when status is already `degraded_read_only`.
    ///
    /// [`RevocationDetector`]: crate::revocation::detector::RevocationDetector
    async fn current_status(
        &self,
        kms_key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError>;
}

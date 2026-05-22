//! Test utilities: stub providers, in-memory stores, recording alerters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use corelink_byok_core::{BYOKError, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek};

use crate::alerter::{CustomerAlerter, RevocationAlertPayload};
use crate::error::RevocationError;
use crate::store::{TenantByokStatus, TenantStatusStore};

// ---------------------------------------------------------------------------
// StubKmsProvider
// ---------------------------------------------------------------------------

/// A stub [`KmsProvider`] that returns a fixed [`KmsAccessStatus`].
///
/// Used in property and adversarial tests.
///
/// # Example
///
/// ```rust
/// use corelink_byok_core::{KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind};
/// use corelink_byok_revocation::testutil::StubKmsProvider;
///
/// # tokio_test::block_on(async {
/// let provider = StubKmsProvider::new_revoked();
/// let key_id = KmsKeyId {
///     provider: KmsProviderKind::AwsKms,
///     key_arn_or_id: "arn:aws:kms:us-east-1:123:key/k1".to_string(),
///     region: "us-east-1".to_string(),
/// };
/// let status = provider.check_access(&key_id).await.unwrap();
/// assert_eq!(status, KmsAccessStatus::Revoked);
/// # });
/// ```
#[derive(Debug)]
pub struct StubKmsProvider {
    kind: KmsProviderKind,
    access_status: KmsAccessStatus,
}

impl StubKmsProvider {
    /// Construct with a given provider kind and access status.
    #[must_use]
    pub fn new(kind: KmsProviderKind, access_status: KmsAccessStatus) -> Self {
        Self { kind, access_status }
    }

    /// Construct an AWS provider that returns `KmsAccessStatus::Ok`.
    #[must_use]
    pub fn new_ok() -> Self {
        Self::new(KmsProviderKind::AwsKms, KmsAccessStatus::Ok)
    }

    /// Construct an AWS provider that returns `KmsAccessStatus::Revoked`.
    #[must_use]
    pub fn new_revoked() -> Self {
        Self::new(KmsProviderKind::AwsKms, KmsAccessStatus::Revoked)
    }

    /// Construct an AWS provider that returns `KmsAccessStatus::Throttled`.
    #[must_use]
    pub fn new_throttled() -> Self {
        Self::new(KmsProviderKind::AwsKms, KmsAccessStatus::Throttled)
    }
}

#[async_trait]
impl KmsProvider for StubKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        self.kind
    }

    fn region(&self) -> &str {
        "us-east-1"
    }

    fn fips_level(&self) -> corelink_byok_core::FipsLevel {
        corelink_byok_core::FipsLevel::None
    }

    async fn wrap_dek(
        &self,
        _dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Ok(WrappedDek {
            provider: self.kind,
            key_id: key_id.clone(),
            ciphertext: vec![0u8; 32],
            encryption_context: encryption_context.cloned(),
        })
    }

    async fn unwrap_dek(&self, _wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        Ok(Dek { bytes: [0u8; 32] })
    }

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(self.access_status)
    }
}

// ---------------------------------------------------------------------------
// InMemoryTenantStore
// ---------------------------------------------------------------------------

/// In-memory implementation of [`TenantStatusStore`] for testing.
///
/// # Example
///
/// ```rust
/// use corelink_byok_core::{KmsKeyId, KmsProviderKind};
/// use corelink_byok_revocation::store::{TenantByokStatus, TenantStatusStore};
/// use corelink_byok_revocation::testutil::InMemoryTenantStore;
///
/// # tokio_test::block_on(async {
/// let store = InMemoryTenantStore::default();
/// let key_id = KmsKeyId {
///     provider: KmsProviderKind::AwsKms,
///     key_arn_or_id: "k1".to_string(),
///     region: "us-east-1".to_string(),
/// };
/// store.mark_degraded(&key_id, "aws", 1_000_000).await.unwrap();
/// # });
/// ```
#[derive(Debug, Default)]
pub struct InMemoryTenantStore {
    inner: Mutex<HashMap<String, TenantByokStatus>>,
}

#[async_trait]
impl TenantStatusStore for InMemoryTenantStore {
    async fn mark_degraded(
        &self,
        kms_key_id: &KmsKeyId,
        _provider: &str,
        _revoked_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let mut map = self.inner.lock().map_err(|e| {
            RevocationError::TenantDegradeFailed {
                kms_key_id: kms_key_id.to_string(),
                detail: e.to_string(),
            }
        })?;
        map.insert(kms_key_id.to_string(), TenantByokStatus::DegradedReadOnly);
        Ok(1)
    }

    async fn restore_active(
        &self,
        kms_key_id: &KmsKeyId,
        _restored_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let mut map = self.inner.lock().map_err(|e| {
            RevocationError::TenantDegradeFailed {
                kms_key_id: kms_key_id.to_string(),
                detail: e.to_string(),
            }
        })?;
        map.insert(kms_key_id.to_string(), TenantByokStatus::Active);
        Ok(1)
    }

    async fn current_status(
        &self,
        kms_key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        let map = self.inner.lock().map_err(|e| {
            RevocationError::Internal(e.to_string())
        })?;
        Ok(map.get(kms_key_id.as_str()).copied())
    }
}

// ---------------------------------------------------------------------------
// NoopAlerter
// ---------------------------------------------------------------------------

/// A no-op [`CustomerAlerter`] that succeeds silently (tests).
///
/// # Example
///
/// ```rust
/// use corelink_byok_core::{KmsKeyId, KmsProviderKind};
/// use corelink_byok_revocation::CustomerAlerter;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
/// use corelink_byok_revocation::testutil::NoopAlerter;
///
/// # tokio_test::block_on(async {
/// let alerter = NoopAlerter;
/// alerter.alert_recovery(
///     KmsProviderKind::AwsKms,
///     &KmsKeyId {
///         provider: KmsProviderKind::AwsKms,
///         key_arn_or_id: "k1".to_string(),
///         region: "us-east-1".to_string(),
///     },
///     "h1",
///     1_000_000,
/// ).await.unwrap();
/// # });
/// ```
#[derive(Debug)]
pub struct NoopAlerter;

#[async_trait]
impl CustomerAlerter for NoopAlerter {
    async fn alert(&self, _payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        Ok(())
    }

    async fn alert_recovery(
        &self,
        _provider: KmsProviderKind,
        _kms_key_id: &KmsKeyId,
        _tenant_id_hashed: &str,
        _restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RecordingAlerter
// ---------------------------------------------------------------------------

/// A recording [`CustomerAlerter`] that stores alert calls (tests).
///
/// # Example
///
/// ```rust
/// use corelink_byok_core::{KmsKeyId, KmsProviderKind};
/// use corelink_byok_revocation::CustomerAlerter;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
/// use corelink_byok_revocation::testutil::RecordingAlerter;
///
/// # tokio_test::block_on(async {
/// let alerter = RecordingAlerter::default();
/// let payload = RevocationAlertPayload {
///     provider: "aws".to_string(),
///     kms_key_id: KmsKeyId {
///         provider: KmsProviderKind::AwsKms,
///         key_arn_or_id: "k1".to_string(),
///         region: "us-east-1".to_string(),
///     },
///     tenant_id_hashed: "h1".to_string(),
///     detected_at_ms: 0,
///     kill_switch_duration_ms: 0,
///     recovery_instructions: "Re-enable CMK".to_string(),
/// };
/// alerter.alert(payload).await.unwrap();
/// assert_eq!(alerter.alert_count(), 1);
/// # });
/// ```
#[derive(Debug, Default)]
pub struct RecordingAlerter {
    calls: Arc<Mutex<Vec<String>>>,
}

impl RecordingAlerter {
    /// Return the number of `alert` calls received.
    #[must_use]
    pub fn alert_count(&self) -> usize {
        self.calls
            .lock()
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[async_trait]
impl CustomerAlerter for RecordingAlerter {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        let mut calls = self.calls.lock().map_err(|e| {
            RevocationError::AlertDeliveryFailed {
                kms_key_id: payload.kms_key_id.to_string(),
                detail: e.to_string(),
            }
        })?;
        calls.push(format!("alert:{}", payload.kms_key_id.as_str()));
        Ok(())
    }

    async fn alert_recovery(
        &self,
        _provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        _tenant_id_hashed: &str,
        _restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        let mut calls = self.calls.lock().map_err(|e| {
            RevocationError::AlertDeliveryFailed {
                kms_key_id: kms_key_id.to_string(),
                detail: e.to_string(),
            }
        })?;
        calls.push(format!("recovery:{}", kms_key_id.as_str()));
        Ok(())
    }
}

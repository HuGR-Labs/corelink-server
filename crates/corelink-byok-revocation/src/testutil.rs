//! Test utilities: stub providers, in-memory stores, recording alerters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use corelink_byok::types::PlaintextDek;
use corelink_byok::{BYOKError, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind};
use corelink_byok::types::WrappedDek;

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
/// use corelink_byok::{KmsAccessStatus, KmsKeyId, KmsProvider};
/// use corelink_byok_revocation::testutil::StubKmsProvider;
///
/// # tokio_test::block_on(async {
/// let provider = StubKmsProvider::new_revoked();
/// let status = provider.check_access(&KmsKeyId::new("k1".to_string())).await.unwrap();
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
        Self::new(KmsProviderKind::Aws, KmsAccessStatus::Ok)
    }

    /// Construct an AWS provider that returns `KmsAccessStatus::Revoked`.
    #[must_use]
    pub fn new_revoked() -> Self {
        Self::new(KmsProviderKind::Aws, KmsAccessStatus::Revoked)
    }

    /// Construct an AWS provider that returns `KmsAccessStatus::Throttled`.
    #[must_use]
    pub fn new_throttled() -> Self {
        Self::new(KmsProviderKind::Aws, KmsAccessStatus::Throttled)
    }
}

#[async_trait]
impl KmsProvider for StubKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        self.kind
    }

    async fn wrap_dek(
        &self,
        _plaintext_dek: &PlaintextDek,
        kms_key_id: &KmsKeyId,
    ) -> Result<WrappedDek, BYOKError> {
        Ok(WrappedDek {
            ciphertext: vec![0u8; 32],
            kms_key_id: kms_key_id.clone(),
            algorithm: "STUB".to_string(),
            created_at_ms: 0,
        })
    }

    async fn unwrap_dek(&self, _wrapped_dek: &WrappedDek) -> Result<PlaintextDek, BYOKError> {
        Ok(PlaintextDek { key_bytes: vec![0u8; 32] })
    }

    async fn check_access(&self, _kms_key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(self.access_status.clone())
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
/// use corelink_byok::KmsKeyId;
/// use corelink_byok_revocation::store::{TenantByokStatus, TenantStatusStore};
/// use corelink_byok_revocation::testutil::InMemoryTenantStore;
///
/// # tokio_test::block_on(async {
/// let store = InMemoryTenantStore::default();
/// let key_id = KmsKeyId::new("k1".to_string());
/// store.mark_degraded(&key_id, "aws", 1_000_000).await.unwrap();
/// let status = store.current_status(&key_id).await.unwrap();
/// assert_eq!(status, Some(TenantByokStatus::DegradedReadOnly));
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
/// use corelink_byok::{KmsKeyId, KmsProviderKind};
/// use corelink_byok_revocation::CustomerAlerter;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
/// use corelink_byok_revocation::testutil::NoopAlerter;
///
/// # tokio_test::block_on(async {
/// let alerter = NoopAlerter;
/// alerter.alert_recovery(
///     KmsProviderKind::Aws,
///     &KmsKeyId::new("k1".to_string()),
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
/// use corelink_byok::KmsKeyId;
/// use corelink_byok_revocation::CustomerAlerter;
/// use corelink_byok_revocation::alerter::RevocationAlertPayload;
/// use corelink_byok_revocation::testutil::RecordingAlerter;
///
/// # tokio_test::block_on(async {
/// let alerter = RecordingAlerter::default();
/// let payload = RevocationAlertPayload {
///     provider: "aws".to_string(),
///     kms_key_id: KmsKeyId::new("k1".to_string()),
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

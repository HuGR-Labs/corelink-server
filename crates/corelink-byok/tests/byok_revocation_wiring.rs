//! B-083/D03 focal coverage for the scheduled revocation population seam.
//!
//! These tests are deliberately hermetic: the provider and D1 collaborators
//! are in-memory ports, so no cloud credential or network is needed.  They
//! protect the two failure modes that a static `run_loop` call misses: an
//! empty population and a population belonging to another provider.

use std::sync::Arc;

use async_trait::async_trait;
use corelink_byok::revocation::alerter::{CustomerAlerter, RevocationAlertPayload};
use corelink_byok::revocation::store::{TenantByokStatus, TenantStatusStore};
use corelink_byok::revocation::testutil::{
    InMemoryTenantStore, NoopAlerter, StaticActiveByokKeySource, StubKmsProvider,
};
use corelink_byok::revocation::{
    ActiveByokKeySource, RevocationConfig, RevocationDetector, RevocationError,
};
use corelink_byok::{BYOKError, DekCache, KmsKeyId, KmsProviderKind};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn key(provider: KmsProviderKind, id: &str) -> KmsKeyId {
    KmsKeyId {
        provider,
        key_arn_or_id: id.to_owned(),
        region: "us-east-1".to_owned(),
    }
}

#[tokio::test]
async fn revoked_active_key_reaches_the_real_kill_switch() -> TestResult {
    let cache = Arc::new(DekCache::new(300)?);
    let key_id = key(KmsProviderKind::AwsKms, "arn:aws:kms:test:key/b083");
    let provider = Arc::new(StubKmsProvider::new_revoked());
    let store = Arc::new(InMemoryTenantStore::default());
    let source = Arc::new(StaticActiveByokKeySource::new(vec![key_id.clone()]));

    RevocationDetector::new(
        vec![provider],
        cache,
        store.clone(),
        Arc::new(NoopAlerter),
        RevocationConfig::default(),
    )
    .with_key_source(source)
    .run_one_cycle()
    .await?;

    assert_eq!(
        store.current_status(&key_id).await?,
        Some(corelink_byok::revocation::store::TenantByokStatus::DegradedReadOnly)
    );
    Ok(())
}

#[derive(Debug)]
struct FailingSource;

#[async_trait]
impl ActiveByokKeySource for FailingSource {
    async fn list_active_byok_keys(
        &self,
        _provider: KmsProviderKind,
    ) -> Result<Vec<KmsKeyId>, RevocationError> {
        Err(RevocationError::Internal("D1 unavailable".to_owned()))
    }
}

#[tokio::test]
async fn population_failure_is_not_coerced_to_no_keys() -> TestResult {
    let detector = RevocationDetector::new(
        vec![Arc::new(StubKmsProvider::new_revoked())],
        Arc::new(DekCache::new(300)?),
        Arc::new(InMemoryTenantStore::default()),
        Arc::new(NoopAlerter),
        RevocationConfig::default(),
    )
    .with_key_source(Arc::new(FailingSource));

    assert!(detector.run_one_cycle().await.is_err());
    Ok(())
}

#[tokio::test]
async fn an_unconfigured_scheduler_does_not_start_a_vacuous_loop() -> TestResult {
    let detector = RevocationDetector::new(
        vec![Arc::new(StubKmsProvider::new_revoked())],
        Arc::new(DekCache::new(300)?),
        Arc::new(InMemoryTenantStore::default()),
        Arc::new(NoopAlerter),
        RevocationConfig::default(),
    );

    assert!(!detector.has_key_source());
    detector.run_loop().await;
    Ok(())
}

#[derive(Debug)]
struct FaultStore {
    current_error: bool,
    restore_error: bool,
    zero_mark: bool,
}

#[async_trait]
impl TenantStatusStore for FaultStore {
    async fn mark_degraded(
        &self,
        _key_id: &KmsKeyId,
        _provider: &str,
        _revoked_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        if self.zero_mark {
            return Ok(0);
        }
        Ok(1)
    }

    async fn restore_active(
        &self,
        key_id: &KmsKeyId,
        _restored_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        if self.restore_error {
            return Err(RevocationError::Internal(format!(
                "restore failed for {}",
                key_id.as_str()
            )));
        }
        Ok(1)
    }

    async fn current_status(
        &self,
        key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        if self.current_error {
            return Err(RevocationError::Internal(format!(
                "status failed for {}",
                key_id.as_str()
            )));
        }
        Ok(Some(TenantByokStatus::DegradedReadOnly))
    }
}

#[derive(Debug)]
struct FaultAlerter {
    fail_alert: bool,
    fail_recovery: bool,
}

#[async_trait]
impl CustomerAlerter for FaultAlerter {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        if self.fail_alert {
            return Err(RevocationError::AlertDeliveryFailed {
                kms_key_id: payload.kms_key_id.to_string(),
                detail: "alert sink failed".to_owned(),
            });
        }
        Ok(())
    }

    async fn alert_recovery(
        &self,
        _provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        _tenant_id_hashed: &str,
        _restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        if self.fail_recovery {
            return Err(RevocationError::AuditEmitFailed(format!(
                "recovery audit failed for {}",
                kms_key_id.as_str()
            )));
        }
        Ok(())
    }
}

fn detector_with_faults(
    status: corelink_byok::KmsAccessStatus,
    store: FaultStore,
    alerter: FaultAlerter,
    key_id: &KmsKeyId,
) -> Result<RevocationDetector, BYOKError> {
    Ok(RevocationDetector::new(
        vec![Arc::new(StubKmsProvider::new(
            KmsProviderKind::AwsKms,
            status,
        ))],
        Arc::new(DekCache::new(300)?),
        Arc::new(store),
        Arc::new(alerter),
        RevocationConfig::default(),
    )
    .with_key_source(Arc::new(StaticActiveByokKeySource::new(vec![
        key_id.clone()
    ]))))
}

#[tokio::test]
async fn zero_updated_rows_fail_closed_in_the_kill_switch() -> TestResult {
    let key_id = key(KmsProviderKind::AwsKms, "zero-update");
    let detector = detector_with_faults(
        corelink_byok::KmsAccessStatus::Revoked,
        FaultStore {
            current_error: false,
            restore_error: false,
            zero_mark: true,
        },
        FaultAlerter {
            fail_alert: false,
            fail_recovery: false,
        },
        &key_id,
    )?;
    assert!(detector.run_one_cycle().await.is_err());
    Ok(())
}

#[tokio::test]
async fn revocation_alert_failure_reaches_the_cycle_result() -> TestResult {
    let key_id = key(KmsProviderKind::AwsKms, "alert-failure");
    let detector = detector_with_faults(
        corelink_byok::KmsAccessStatus::Revoked,
        FaultStore {
            current_error: false,
            restore_error: false,
            zero_mark: false,
        },
        FaultAlerter {
            fail_alert: true,
            fail_recovery: false,
        },
        &key_id,
    )?;
    assert!(detector.run_one_cycle().await.is_err());
    Ok(())
}

#[tokio::test]
async fn recovery_status_restore_and_audit_failures_reach_the_cycle_result() -> TestResult {
    let key_id = key(KmsProviderKind::AwsKms, "recovery-failure");
    for store in [
        FaultStore {
            current_error: true,
            restore_error: false,
            zero_mark: false,
        },
        FaultStore {
            current_error: false,
            restore_error: true,
            zero_mark: false,
        },
    ] {
        let detector = detector_with_faults(
            corelink_byok::KmsAccessStatus::Ok,
            store,
            FaultAlerter {
                fail_alert: false,
                fail_recovery: false,
            },
            &key_id,
        )?;
        assert!(detector.run_one_cycle().await.is_err());
    }

    let detector = detector_with_faults(
        corelink_byok::KmsAccessStatus::Ok,
        FaultStore {
            current_error: false,
            restore_error: false,
            zero_mark: false,
        },
        FaultAlerter {
            fail_alert: false,
            fail_recovery: true,
        },
        &key_id,
    )?;
    assert!(detector.run_one_cycle().await.is_err());
    Ok(())
}

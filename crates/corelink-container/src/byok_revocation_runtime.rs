//! Native production collaborators for the BYOK revocation scheduler.
//!
//! The detector lives in `corelink-byok` and deliberately knows nothing about
//! D1.  This module is the native entry-point adapter: it supplies the active
//! tenant population, the kill-switch status store, and customer-facing D1
//! notifications.  Every D1 error is propagated; an unavailable source is
//! never treated as an empty population.

use std::sync::Arc;

use async_trait::async_trait;
use corelink_byok::revocation::alerter::{CustomerAlerter, RevocationAlertPayload};
use corelink_byok::revocation::detector::ActiveByokKeySource;
use corelink_byok::revocation::error::RevocationError;
use corelink_byok::revocation::store::{TenantByokStatus, TenantStatusStore};
use corelink_byok::{KmsKeyId, KmsProviderKind};
use serde_json::{json, Value};
use tracing::warn;

use crate::byok_control_transition::{CmkIdentity, D1ByokControl};
use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Minimal D1 query seam used by the native revocation adapters.
///
/// Keeping this port explicit lets adapter tests exercise the real SQL and
/// failure handling without credentials or a network. The production impl is
/// the D1-over-HTTP client; the database trigger migration supplies the
/// status-plus-customer-audit transaction boundary.
#[async_trait]
pub trait RevocationD1Client: Send + Sync + std::fmt::Debug {
    /// Run one parameterised D1 statement and return its rows.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String>;
}

#[async_trait]
impl RevocationD1Client for D1HttpClient {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        D1HttpClient::query(self, sql, params).await
    }
}

/// D1-backed active-CMK population for the revocation detector.
#[derive(Debug)]
pub struct D1ActiveByokKeySource<C = D1HttpClient> {
    client: Arc<C>,
}

impl<C> D1ActiveByokKeySource<C> {
    /// Construct the source over the shared boot-time D1 client.
    #[must_use]
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }
}

fn provider_from_db(value: &str) -> Option<KmsProviderKind> {
    match value {
        "aws" => Some(KmsProviderKind::AwsKms),
        "gcp" => Some(KmsProviderKind::GcpKms),
        "azure" => Some(KmsProviderKind::AzureKeyVault),
        "vault" => Some(KmsProviderKind::HashicorpVault),
        _ => None,
    }
}

fn required_text(row: &D1Row, column: &str) -> Result<String, RevocationError> {
    row.get(column)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| RevocationError::Internal(format!("tenant_byok_config.{column} missing")))
}

#[async_trait]
impl<C: RevocationD1Client> ActiveByokKeySource for D1ActiveByokKeySource<C> {
    async fn list_active_byok_keys(
        &self,
        provider: KmsProviderKind,
    ) -> Result<Vec<KmsKeyId>, RevocationError> {
        let rows = self
            .client
            .query(
                "SELECT DISTINCT cmk_provider,cmk_key_id,cmk_region FROM ( \
                   SELECT c.cmk_provider,c.cmk_key_id,c.cmk_region \
                   FROM tenant_byok_config c \
                   WHERE c.state IN ('pending', 'active', 'partial') AND c.cmk_provider=?1 \
                   UNION \
                   SELECT a.source_cmk_provider,a.source_cmk_key_id,a.source_cmk_region \
                   FROM byok_activation_intent a \
                   JOIN tenant_byok_config c ON c.tenant_id=a.tenant_id \
                   JOIN tenant_byok_config_history h ON h.tenant_id=a.tenant_id \
                    AND h.config_version=a.source_config_version \
                    AND h.cmk_provider=a.source_cmk_provider \
                    AND h.cmk_key_id=a.source_cmk_key_id AND h.cmk_region=a.source_cmk_region \
                   JOIN tenant_byok_secret_history s ON s.tenant_id=a.tenant_id \
                    AND s.tcs_version=a.source_tcs_version \
                    AND s.cmk_provider=a.source_cmk_provider \
                    AND s.cmk_key_id=a.source_cmk_key_id AND s.cmk_region=a.source_cmk_region \
                   WHERE a.phase IN ('copy','published_partial','purging','ready_finalize') \
                    AND a.source_generation>0 AND a.source_cmk_provider=?1 \
                    AND c.state IN ('pending','partial') AND s.tcs_wrapped IS NOT NULL) \
                 ORDER BY cmk_provider,cmk_key_id,cmk_region",
                &[json!(provider.as_str())],
            )
            .await
            .map_err(|detail| {
                RevocationError::Internal(format!("active BYOK key query failed: {detail}"))
            })?;

        rows.into_iter()
            .map(|row| {
                let provider_name = required_text(&row, "cmk_provider")?;
                let parsed_provider = provider_from_db(&provider_name).ok_or_else(|| {
                    RevocationError::Internal(format!(
                        "unsupported tenant_byok_config.cmk_provider={provider_name:?}"
                    ))
                })?;
                if parsed_provider != provider {
                    return Err(RevocationError::Internal(
                        "active BYOK key provider changed during population read".to_owned(),
                    ));
                }
                Ok(KmsKeyId {
                    provider: parsed_provider,
                    key_arn_or_id: required_text(&row, "cmk_key_id")?,
                    region: required_text(&row, "cmk_region")?,
                })
            })
            .collect()
    }
}

/// D1-backed tenant kill-switch status store.
///
/// `RETURNING` makes the updated-row count authoritative instead of claiming
/// that a write happened when D1 returned no matching tenant.
#[derive(Debug)]
#[cfg(test)]
pub struct D1TenantStatusStore<C = D1HttpClient> {
    client: Arc<C>,
}

#[cfg(test)]
const MARK_DEGRADED_SQL: &str = "UPDATE tenant SET byok_status = 'degraded_read_only', \
                    byok_revoked_at_ms = ?1, byok_revoked_provider = ?2, \
                    byok_revoked_kms_key_id = ?3 \
                 WHERE tenant_id IN (SELECT tenant_id FROM tenant_byok_config \
                    WHERE cmk_provider = ?2 AND cmk_key_id = ?3) \
                 RETURNING tenant_id";
#[cfg(test)]
const RESTORE_ACTIVE_SQL: &str = "UPDATE tenant SET byok_status = 'active', \
                    byok_revoked_at_ms = NULL, byok_revoked_provider = NULL, \
                    byok_revoked_kms_key_id = NULL \
                 WHERE byok_status = 'degraded_read_only' AND tenant_id IN \
                   (SELECT tenant_id FROM tenant_byok_config \
                    WHERE cmk_provider = ?1 AND cmk_key_id = ?2) \
                 RETURNING tenant_id";

#[cfg(test)]
impl<C: RevocationD1Client> D1TenantStatusStore<C> {
    /// Construct the store over the shared boot-time D1 client.
    #[must_use]
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }

    async fn key_tenant_status(
        &self,
        key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        let rows = self
            .client
            .query(
                "SELECT t.byok_status FROM tenant t \
                 JOIN tenant_byok_config c ON c.tenant_id = t.tenant_id \
                 WHERE c.cmk_provider = ?1 AND c.cmk_key_id = ?2 \
                 LIMIT 1",
                &[json!(key_id.provider.as_str()), json!(key_id.as_str())],
            )
            .await
            .map_err(|detail| {
                RevocationError::Internal(format!("BYOK status query failed: {detail}"))
            })?;
        rows.first()
            .map(|row| match required_text(row, "byok_status")?.as_str() {
                "active" => Ok(TenantByokStatus::Active),
                "degraded_read_only" => Ok(TenantByokStatus::DegradedReadOnly),
                "revoked" => Ok(TenantByokStatus::Revoked),
                value => Err(RevocationError::Internal(format!(
                    "unknown tenant.byok_status={value:?}"
                ))),
            })
            .transpose()
    }
}

#[async_trait]
#[cfg(test)]
impl<C: RevocationD1Client> TenantStatusStore for D1TenantStatusStore<C> {
    async fn mark_degraded(
        &self,
        key_id: &KmsKeyId,
        provider: &str,
        revoked_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let rows = self
            .client
            .query(
                MARK_DEGRADED_SQL,
                &[
                    json!(revoked_at_ms),
                    json!(provider),
                    json!(key_id.as_str()),
                ],
            )
            .await
            .map_err(|detail| RevocationError::TenantDegradeFailed {
                kms_key_id: key_id.to_string(),
                detail,
            })?;
        if rows.is_empty() {
            return Err(RevocationError::TenantDegradeFailed {
                kms_key_id: key_id.to_string(),
                detail: "zero tenants matched active BYOK key".to_owned(),
            });
        }
        Ok(rows.len())
    }

    async fn restore_active(
        &self,
        key_id: &KmsKeyId,
        _restored_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let rows = self
            .client
            .query(
                RESTORE_ACTIVE_SQL,
                &[json!(key_id.provider.as_str()), json!(key_id.as_str())],
            )
            .await
            .map_err(|detail| {
                RevocationError::Internal(format!("BYOK recovery failed: {detail}"))
            })?;
        if rows.is_empty() {
            return Err(RevocationError::Internal(
                "BYOK recovery updated zero tenants".to_owned(),
            ));
        }
        Ok(rows.len())
    }

    async fn current_status(
        &self,
        key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        self.key_tenant_status(key_id).await
    }
}

/// Production status store using tenant-wide transition fences. The legacy
/// generic adapter above remains only as a narrow SQL seam for existing unit
/// fixtures; production never performs key-wide UPDATEs or `LIMIT 1` recovery.
#[derive(Debug)]
struct D1FencedTenantStatusStore {
    control: Arc<D1ByokControl>,
}

impl D1FencedTenantStatusStore {
    fn new(control: Arc<D1ByokControl>) -> Self {
        Self { control }
    }
}

fn complete_identity(key_id: &KmsKeyId) -> Result<CmkIdentity, RevocationError> {
    if key_id.as_str().trim().is_empty() || key_id.region.trim().is_empty() {
        return Err(RevocationError::Internal(
            "revocation requires complete provider/key/region identity".to_owned(),
        ));
    }
    Ok(CmkIdentity {
        provider: key_id.provider.as_str().to_owned(),
        key_id: key_id.as_str().to_owned(),
        region: key_id.region.clone(),
    })
}

#[async_trait]
impl TenantStatusStore for D1FencedTenantStatusStore {
    async fn mark_degraded(
        &self,
        key_id: &KmsKeyId,
        provider: &str,
        revoked_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let identity = complete_identity(key_id)?;
        if provider != identity.provider {
            return Err(RevocationError::Internal(
                "revocation provider disagrees with complete key identity".to_owned(),
            ));
        }
        let at = i64::try_from(revoked_at_ms).map_err(|_| {
            RevocationError::Internal("revocation timestamp exceeds D1 INTEGER".to_owned())
        })?;
        let bindings = self
            .control
            .resolve_active_tenants(&identity)
            .await
            .map_err(|detail| RevocationError::TenantDegradeFailed {
                kms_key_id: key_id.to_string(),
                detail,
            })?;
        let mut updated = 0_usize;
        for binding in bindings {
            if self
                .control
                .degrade_tenant(&binding, at)
                .await
                .map_err(|detail| RevocationError::TenantDegradeFailed {
                    kms_key_id: key_id.to_string(),
                    detail,
                })?
            {
                updated += 1;
            }
        }
        if updated == 0
            && self
                .control
                .has_degraded_tenant(&identity)
                .await
                .map_err(|detail| RevocationError::TenantDegradeFailed {
                    kms_key_id: key_id.to_string(),
                    detail,
                })?
        {
            return Ok(1);
        }
        if updated == 0 {
            return Err(RevocationError::TenantDegradeFailed {
                kms_key_id: key_id.to_string(),
                detail: "zero exact tenant bindings transitioned".to_owned(),
            });
        }
        Ok(updated)
    }

    async fn restore_active(
        &self,
        key_id: &KmsKeyId,
        restored_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let identity = complete_identity(key_id)?;
        let at = i64::try_from(restored_at_ms).map_err(|_| {
            RevocationError::Internal("restore timestamp exceeds D1 INTEGER".to_owned())
        })?;
        let bindings = self
            .control
            .resolve_active_tenants(&identity)
            .await
            .map_err(RevocationError::Internal)?;
        let mut updated = 0_usize;
        for binding in bindings {
            if self
                .control
                .restore_tenant(&binding, at)
                .await
                .map_err(RevocationError::Internal)?
            {
                updated += 1;
            }
        }
        if updated == 0 {
            return Err(RevocationError::Internal(
                "zero exact degraded tenant bindings restored".to_owned(),
            ));
        }
        Ok(updated)
    }

    async fn current_status(
        &self,
        key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        let identity = complete_identity(key_id)?;
        self.control
            .has_degraded_tenant(&identity)
            .await
            .map(|present| present.then_some(TenantByokStatus::DegradedReadOnly))
            .map_err(RevocationError::Internal)
    }
}

/// D1-backed customer notification adapter.
///
/// The customer-safe activity row is written by the tenant status trigger in
/// the same D1 transaction as the status mutation. This adapter validates that
/// the affected tenant population is present before reporting delivery; it
/// never issues a second audit INSERT that could drift from the status.
#[derive(Debug)]
pub struct D1RevocationAlerter<C = D1HttpClient> {
    client: Arc<C>,
}

impl<C: RevocationD1Client> D1RevocationAlerter<C> {
    /// Construct the alerter over the shared boot-time D1 client.
    #[must_use]
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }

    async fn notify(
        &self,
        payload: &RevocationAlertPayload,
        event_type: &str,
    ) -> Result<(), RevocationError> {
        let tenants = self
            .client
            .query(
                "SELECT DISTINCT o.alert_recipient AS tenant_id FROM byok_control_outcome o \
                 WHERE o.cmk_provider = ?1 AND o.cmk_key_id = ?2 AND o.cmk_region = ?3 \
                 AND o.action = ?4 AND o.outcome = 'completed' \
                 AND o.alert_outcome = 'customer_activity_recorded'",
                &[
                    json!(payload.kms_key_id.provider.as_str()),
                    json!(payload.kms_key_id.as_str()),
                    json!(payload.kms_key_id.region),
                    json!(if event_type == "byok.cmk_revoked" {
                        "degrade"
                    } else {
                        "restore"
                    }),
                ],
            )
            .await
            .map_err(|detail| RevocationError::AlertDeliveryFailed {
                kms_key_id: payload.kms_key_id.to_string(),
                detail,
            })?;
        if tenants.is_empty() {
            return Err(RevocationError::AlertDeliveryFailed {
                kms_key_id: payload.kms_key_id.to_string(),
                detail: "no tenant recipients found for CMK notification".to_owned(),
            });
        }
        for tenant in tenants {
            required_text(&tenant, "tenant_id").map_err(|error| {
                RevocationError::AlertDeliveryFailed {
                    kms_key_id: payload.kms_key_id.to_string(),
                    detail: error.to_string(),
                }
            })?;
        }
        tracing::info!(
            target: "corelink.byok.revocation.notification",
            event_type,
            tenants = payload.tenant_id_hashed.as_str(),
            "BYOK customer notification delivered via transactional D1 activity row"
        );
        Ok(())
    }
}

#[async_trait]
impl<C: RevocationD1Client> CustomerAlerter for D1RevocationAlerter<C> {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        self.notify(&payload, "byok.cmk_revoked").await
    }

    async fn alert_recovery(
        &self,
        provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        _tenant_id_hashed: &str,
        restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        let payload = RevocationAlertPayload {
            provider: provider.as_str().to_owned(),
            kms_key_id: kms_key_id.clone(),
            tenant_id_hashed: "d1".to_owned(),
            detected_at_ms: restored_at_ms,
            kill_switch_duration_ms: 0,
            recovery_instructions: "CMK access restored; tenant writes resumed.".to_owned(),
        };
        self.notify(&payload, "byok.cmk_restored").await
    }
}

/// Build the complete native revocation detector from one durable D1 client.
#[must_use = "handle the result to obtain the configured revocation detector"]
pub fn detector_for_client(
    client: Arc<D1HttpClient>,
    provider: Arc<dyn corelink_byok::KmsProvider>,
) -> Result<corelink_byok::revocation::RevocationDetector, String> {
    let cache = Arc::new(
        corelink_byok::DekCache::new(300)
            .map_err(|error| format!("BYOK revocation cache configuration failed: {error}"))?,
    );
    let control = Arc::new(D1ByokControl::new(Arc::clone(&client)));
    Ok(corelink_byok::revocation::RevocationDetector::new(
        vec![provider],
        cache,
        Arc::new(D1FencedTenantStatusStore::new(control)),
        Arc::new(D1RevocationAlerter::new(Arc::clone(&client))),
        corelink_byok::revocation::RevocationConfig::default(),
    )
    .with_key_source(Arc::new(D1ActiveByokKeySource::new(client))))
}

/// Log a source configuration error without allowing an inert scheduler.
pub fn warn_unavailable(reason: &str) {
    warn!(
        event = "byok_revocation_scheduler_unavailable",
        reason, "BYOK revocation scheduler not started (fail-closed)"
    );
}

include!("byok_revocation_runtime/part-01.rs");

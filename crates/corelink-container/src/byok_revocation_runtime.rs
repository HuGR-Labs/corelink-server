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
                "SELECT DISTINCT cmk_provider, cmk_key_id, cmk_region \
                 FROM tenant_byok_config \
                 WHERE state IN ('active', 'partial') AND cmk_provider = ?1",
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
pub struct D1TenantStatusStore<C = D1HttpClient> {
    client: Arc<C>,
}

const MARK_DEGRADED_SQL: &str = "UPDATE tenant SET byok_status = 'degraded_read_only', \
                    byok_revoked_at_ms = ?1, byok_revoked_provider = ?2, \
                    byok_revoked_kms_key_id = ?3 \
                 WHERE tenant_id IN (SELECT tenant_id FROM tenant_byok_config \
                    WHERE cmk_provider = ?2 AND cmk_key_id = ?3) \
                 RETURNING tenant_id";
const RESTORE_ACTIVE_SQL: &str = "UPDATE tenant SET byok_status = 'active', \
                    byok_revoked_at_ms = NULL, byok_revoked_provider = NULL, \
                    byok_revoked_kms_key_id = NULL \
                 WHERE byok_status = 'degraded_read_only' AND tenant_id IN \
                   (SELECT tenant_id FROM tenant_byok_config \
                    WHERE cmk_provider = ?1 AND cmk_key_id = ?2) \
                 RETURNING tenant_id";

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
                "SELECT tenant_id FROM tenant_byok_config \
                 WHERE cmk_provider = ?1 AND cmk_key_id = ?2",
                &[
                    json!(payload.kms_key_id.provider.as_str()),
                    json!(payload.kms_key_id.as_str()),
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
#[must_use]
pub fn detector_for_client(
    client: Arc<D1HttpClient>,
    provider: Arc<dyn corelink_byok::KmsProvider>,
) -> Result<corelink_byok::revocation::RevocationDetector, String> {
    let cache = Arc::new(
        corelink_byok::DekCache::new(300)
            .map_err(|error| format!("BYOK revocation cache configuration failed: {error}"))?,
    );
    Ok(corelink_byok::revocation::RevocationDetector::new(
        vec![provider],
        cache,
        Arc::new(D1TenantStatusStore::new(Arc::clone(&client))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingD1 {
        responses: Mutex<Vec<Vec<D1Row>>>,
        sql: Mutex<Vec<String>>,
    }

    impl RecordingD1 {
        fn with_responses(responses: Vec<Vec<D1Row>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                sql: Mutex::new(Vec::new()),
            }
        }

        fn row(values: &[(&str, &str)]) -> D1Row {
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), Value::String((*value).to_owned())))
                .collect()
        }

        fn statements(&self) -> Vec<String> {
            self.sql.lock().expect("sql lock").clone()
        }
    }

    #[async_trait]
    impl RevocationD1Client for RecordingD1 {
        async fn query(&self, sql: &str, _params: &[Value]) -> Result<Vec<D1Row>, String> {
            self.sql.lock().expect("sql lock").push(sql.to_owned());
            Ok(self
                .responses
                .lock()
                .expect("responses lock")
                .pop()
                .unwrap_or_default())
        }
    }

    fn key_id() -> KmsKeyId {
        KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:test:key/b083".to_owned(),
            region: "us-east-1".to_owned(),
        }
    }

    #[tokio::test]
    async fn d1_population_preserves_provider_key_and_region() {
        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                ("cmk_provider", "aws"),
                ("cmk_key_id", "arn:aws:kms:test:key/b083"),
                ("cmk_region", "us-east-1"),
            ],
        )]]));
        let keys = D1ActiveByokKeySource::new(db.clone())
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await
            .expect("valid active key row");
        assert_eq!(keys, vec![key_id()]);
        assert!(db.statements()[0].contains("state IN ('active', 'partial')"));
    }

    #[tokio::test]
    async fn d1_population_rejects_missing_key_or_region_and_unknown_provider() {
        for missing in ["cmk_key_id", "cmk_region"] {
            let row = if missing == "cmk_key_id" {
                RecordingD1::row(&[("cmk_provider", "aws"), ("cmk_region", "us-east-1")])
            } else {
                RecordingD1::row(&[("cmk_provider", "aws"), ("cmk_key_id", "k1")])
            };
            let db = Arc::new(RecordingD1::with_responses(vec![vec![row]]));
            assert!(D1ActiveByokKeySource::new(db)
                .list_active_byok_keys(KmsProviderKind::AwsKms)
                .await
                .is_err());
        }

        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                ("cmk_provider", "not-a-provider"),
                ("cmk_key_id", "k1"),
                ("cmk_region", "us-east-1"),
            ],
        )]]));
        assert!(D1ActiveByokKeySource::new(db)
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn d1_population_rejects_returned_provider_mismatch() {
        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                // The request asked for AWS, but a buggy/malicious D1 adapter
                // returned a GCP row.  The adapter must not relabel it as AWS.
                ("cmk_provider", "gcp"),
                (
                    "cmk_key_id",
                    "projects/p/locations/l/keyRings/r/cryptoKeys/k",
                ),
                ("cmk_region", "us-central1"),
            ],
        )]]));
        let result = D1ActiveByokKeySource::new(db)
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await;
        assert!(
            result.is_err(),
            "returned provider mismatch must fail closed"
        );
    }

    #[tokio::test]
    async fn d1_zero_update_and_missing_alert_recipient_fail_closed() {
        let db = Arc::new(RecordingD1::with_responses(vec![Vec::new()]));
        let store = D1TenantStatusStore::new(db);
        assert!(store.mark_degraded(&key_id(), "aws", 1).await.is_err());

        let db = Arc::new(RecordingD1::with_responses(vec![Vec::new()]));
        let alerter = D1RevocationAlerter::new(db);
        let payload = RevocationAlertPayload {
            provider: "aws".to_owned(),
            kms_key_id: key_id(),
            tenant_id_hashed: "hashed".to_owned(),
            detected_at_ms: 1,
            kill_switch_duration_ms: 1,
            recovery_instructions: "restore".to_owned(),
        };
        assert!(alerter.alert(payload).await.is_err());
    }

    #[test]
    fn status_and_customer_audit_share_one_database_transaction() {
        let sql =
            include_str!("../../../migrations/d1/0114_byok_revocation_customer_audit_atomic.sql");
        let mut connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE tenant (
                    tenant_id TEXT PRIMARY KEY, byok_status TEXT NOT NULL,
                    byok_revoked_at_ms INTEGER, byok_revoked_provider TEXT,
                    byok_revoked_kms_key_id TEXT
                 );
                 CREATE TABLE tenant_byok_config (
                    tenant_id TEXT PRIMARY KEY, cmk_provider TEXT NOT NULL,
                    cmk_key_id TEXT NOT NULL
                 );
                 CREATE TABLE customer_audit_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, tenant_id TEXT NOT NULL,
                    event_type TEXT NOT NULL, actor TEXT, target TEXT,
                    ts_ms INTEGER NOT NULL, detail TEXT
                 );",
            )
            .expect("minimal D1 schema");
        connection
            .execute_batch(sql)
            .expect("0114 trigger installs");
        connection
            .execute_batch(
                "INSERT INTO tenant VALUES ('t-b083', 'active', NULL, NULL, NULL);
                 INSERT INTO tenant_byok_config VALUES ('t-b083', 'aws', 'arn:aws:kms:test:key/b083');",
            )
            .expect("fixture");

        // The exact SQL issued by D1TenantStatusStore is executed in SQLite.
        // Commit proves the trigger is in the same transaction as the update.
        {
            let transaction = connection.transaction().expect("begin degrade");
            let returned: Vec<String> = transaction
                .prepare(MARK_DEGRADED_SQL)
                .expect("prepare degrade")
                .query_map(
                    rusqlite::params![1_i64, "aws", "arn:aws:kms:test:key/b083"],
                    |row| row.get(0),
                )
                .expect("degrade RETURNING")
                .collect::<Result<_, _>>()
                .expect("degrade row");
            assert_eq!(returned, ["t-b083"]);
            transaction.commit().expect("commit degrade");
        }
        let status: String = connection
            .query_row("SELECT byok_status FROM tenant", [], |row| row.get(0))
            .expect("degraded status");
        assert_eq!(status, "degraded_read_only");
        let revoked_events: i64 = connection
            .query_row(
                "SELECT count(*) FROM customer_audit_events WHERE event_type = 'byok.cmk_revoked'",
                [],
                |row| row.get(0),
            )
            .expect("revoked audit");
        assert_eq!(revoked_events, 1);

        // A rollback must remove both the status transition and its trigger
        // row, never leaving a customer-visible half-transition.
        {
            let transaction = connection.transaction().expect("begin restore");
            let returned: Vec<String> = transaction
                .prepare(RESTORE_ACTIVE_SQL)
                .expect("prepare restore")
                .query_map(
                    rusqlite::params!["aws", "arn:aws:kms:test:key/b083"],
                    |row| row.get(0),
                )
                .expect("restore RETURNING")
                .collect::<Result<_, _>>()
                .expect("restore row");
            assert_eq!(returned, ["t-b083"]);
            transaction.rollback().expect("rollback restore");
        }
        let status_after_rollback: String = connection
            .query_row("SELECT byok_status FROM tenant", [], |row| row.get(0))
            .expect("status after rollback");
        assert_eq!(status_after_rollback, "degraded_read_only");
        let events_after_rollback: i64 = connection
            .query_row("SELECT count(*) FROM customer_audit_events", [], |row| {
                row.get(0)
            })
            .expect("events after rollback");
        assert_eq!(events_after_rollback, 1);

        // Finally commit the restore and observe its distinct trigger event.
        let returned: Vec<String> = connection
            .prepare(RESTORE_ACTIVE_SQL)
            .expect("prepare committed restore")
            .query_map(
                rusqlite::params!["aws", "arn:aws:kms:test:key/b083"],
                |row| row.get(0),
            )
            .expect("committed restore RETURNING")
            .collect::<Result<_, _>>()
            .expect("committed restore row");
        assert_eq!(returned, ["t-b083"]);
        let restored_events: i64 = connection
            .query_row(
                "SELECT count(*) FROM customer_audit_events WHERE event_type = 'byok.cmk_restored'",
                [],
                |row| row.get(0),
            )
            .expect("restored audit");
        assert_eq!(restored_events, 1);
    }
}

//! Production D1/R2 persistence for the crash-resumable BYOK backfill.
//!
//! Every authority decision uses D1's clock. R2 writes are staged below a
//! secret-derived tenant prefix and become addressable only after the guarded
//! generation commit publishes the complete target catalog.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    byok_backfill::{
        generation_qualified_suffix, BackfillAllocation, BackfillCheckpoint, BackfillError,
        BackfillPage, BackfillPhase, BackfillRun, BackfillSourceObject, BackfillSurface,
        BeginBackfill, ByokBackfillEncryptor, ByokBackfillEngine, ByokBackfillStore,
        StagedBackfillObject,
    },
    d1_http::{D1BatchStatement, D1HttpClient, D1Row},
    r2_s3::{CappedGet, R2S3Client},
};
use crate::byok_transition_fence::MAX_BYOK_FENCE_LEASE;

const NOW_MS: &str = "(CAST(strftime('%s', 'now') AS INTEGER) * 1000)";
const R2_PUT_DISPATCH_HEADROOM_MS: i64 = 65_000;

#[derive(Debug, Clone)]
/// One typed internal statement submitted through an atomic D1 batch.
pub(crate) struct BackfillD1Statement {
    sql: String,
    params: Vec<Value>,
}

impl BackfillD1Statement {
    fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

#[async_trait]
/// Narrow D1 seam used by the production adapter and hermetic tests.
pub(crate) trait BackfillD1Client: Send + Sync + fmt::Debug {
    /// Execute one parameterized D1 statement.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String>;
    /// Execute statements sequentially in one rollback-on-error transaction.
    async fn batch(&self, statements: Vec<BackfillD1Statement>) -> Result<Vec<Vec<D1Row>>, String>;
}

#[async_trait]
impl BackfillD1Client for D1HttpClient {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        D1HttpClient::query(self, sql, params).await
    }

    async fn batch(&self, statements: Vec<BackfillD1Statement>) -> Result<Vec<Vec<D1Row>>, String> {
        D1HttpClient::batch(
            self,
            statements
                .into_iter()
                .map(|statement| D1BatchStatement::new(statement.sql, statement.params))
                .collect(),
        )
        .await
        .map_err(|error| match error.statement {
            Some(index) => format!("D1 backfill batch statement {index}: {}", error.message),
            None => format!("D1 backfill batch: {}", error.message),
        })
    }
}

#[async_trait]
/// Bounded object operations required from each CAS/AC R2 bucket.
pub(crate) trait BackfillObjectClient: Send + Sync + fmt::Debug {
    /// Read an object without exceeding the supplied memory ceiling.
    async fn get_capped(&self, key: &str, max_bytes: u64) -> Result<CappedGet, String>;
    /// Read object size without materializing its body.
    async fn head_size(&self, key: &str) -> Result<Option<u64>, String>;
    /// Atomically create a key only when absent.
    async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String>;
}

#[async_trait]
impl BackfillObjectClient for R2S3Client {
    async fn get_capped(&self, key: &str, max_bytes: u64) -> Result<CappedGet, String> {
        R2S3Client::get_capped(self, key, max_bytes).await
    }

    async fn head_size(&self, key: &str) -> Result<Option<u64>, String> {
        R2S3Client::head_size(self, key).await
    }

    async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String> {
        R2S3Client::put_if_absent(self, key, bytes).await
    }
}

/// Retired generic adapter retained only inside the crate's test build. The
/// production activation worker reserves durable allocation identity before
/// I/O; this older PUT-before-checkpoint design must never be startable there.
#[derive(Debug)]
pub(crate) struct D1ByokBackfillStore<D = D1HttpClient, R = R2S3Client> {
    d1: Arc<D>,
    cas: Arc<R>,
    ac: Arc<R>,
    tdk: TenantDerivationKey,
    cas_region: String,
    ac_region: String,
    max_object_bytes: u64,
}

impl<D, R> D1ByokBackfillStore<D, R> {
    /// Construct the production adapter over D1 and the tenant-scoped CAS/AC buckets.
    #[must_use]
    pub fn new(
        d1: Arc<D>,
        cas: Arc<R>,
        ac: Arc<R>,
        tdk: TenantDerivationKey,
        cas_region: impl Into<String>,
        ac_region: impl Into<String>,
        max_object_bytes: u64,
    ) -> Self {
        Self {
            d1,
            cas,
            ac,
            tdk,
            cas_region: cas_region.into(),
            ac_region: ac_region.into(),
            max_object_bytes,
        }
    }

    fn object_client(&self, surface: BackfillSurface) -> &R {
        match surface {
            BackfillSurface::Cas => &self.cas,
            BackfillSurface::Ac => &self.ac,
        }
    }

    fn tenant_prefix(&self, tenant_id: &str) -> Result<String, BackfillError> {
        let tenant = Uuid::parse_str(tenant_id).map_err(|_| {
            BackfillError::InvalidRequest("tenant_id must be a canonical UUID".to_owned())
        })?;
        Ok(derive_prefix(&self.tdk, tenant).to_string())
    }

    fn region(&self, surface: BackfillSurface) -> &str {
        match surface {
            BackfillSurface::Cas => &self.cas_region,
            BackfillSurface::Ac => &self.ac_region,
        }
    }

    fn checked_region(&self, surface: BackfillSurface) -> Result<&str, BackfillError> {
        let region = self.region(surface);
        if region.is_empty() || region.contains('/') || region.contains('\0') {
            return Err(BackfillError::InvalidRequest(
                "backfill region must be one non-empty path segment".to_owned(),
            ));
        }
        Ok(region)
    }
}

impl<D: BackfillD1Client, R> D1ByokBackfillStore<D, R> {
    async fn load_run(
        &self,
        tenant_id: &str,
        run_id: &str,
        token: Option<&str>,
    ) -> Result<BackfillRun, BackfillError> {
        let mut sql = String::from(
            "SELECT tenant_id, run_id, intent_token, transition_epoch, gate_epoch, \
             source_generation, target_generation, phase, cas_cursor, ac_cursor, \
             cas_complete, ac_complete FROM byok_backfill_run \
             WHERE tenant_id = ?1 AND run_id = ?2",
        );
        let mut params = vec![json!(tenant_id), json!(run_id)];
        if let Some(token) = token {
            sql.push_str(" AND intent_token = ?3");
            params.push(json!(token));
        }
        let rows = self.d1.query(&sql, &params).await.map_err(store_error)?;
        rows.first().map(run_from_row).transpose()?.ok_or_else(|| {
            BackfillError::Conflict("stable backfill run or exact token not found".to_owned())
        })
    }

    async fn assert_live(&self, run: &BackfillRun) -> Result<(), BackfillError> {
        let sql = format!(
            "SELECT 1 AS live FROM byok_backfill_run r \
             JOIN byok_transition_fence f ON f.tenant_id = r.tenant_id \
              AND f.token = r.intent_token AND f.epoch = r.transition_epoch \
             JOIN byok_tenant_gate g ON g.tenant_id = r.tenant_id \
             WHERE r.tenant_id = ?1 AND r.run_id = ?2 AND r.intent_token = ?3 \
              AND r.transition_epoch = ?4 AND r.gate_epoch = ?5 \
              AND r.source_generation = ?6 AND r.target_generation = ?7 \
              AND r.phase IN ('copying','ready_to_commit') \
              AND f.outcome = 'active' AND f.expires_at_ms > {NOW_MS} \
              AND g.gate_epoch = r.gate_epoch \
              AND g.current_generation = r.source_generation LIMIT 1"
        );
        let rows = self
            .d1
            .query(&sql, &run_identity(run))
            .await
            .map_err(store_error)?;
        if rows.is_empty() {
            Err(BackfillError::Conflict(
                "backfill transition lease is stale, expired, or source changed".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    async fn assert_put_headroom(&self, run: &BackfillRun) -> Result<(), BackfillError> {
        let sql = format!(
            "SELECT 1 AS live FROM byok_backfill_run r \
             JOIN byok_transition_fence f ON f.tenant_id = r.tenant_id \
              AND f.token = r.intent_token AND f.epoch = r.transition_epoch \
             JOIN byok_tenant_gate g ON g.tenant_id = r.tenant_id \
             WHERE r.tenant_id = ?1 AND r.run_id = ?2 AND r.intent_token = ?3 \
              AND r.transition_epoch = ?4 AND r.gate_epoch = ?5 \
              AND r.source_generation = ?6 AND r.target_generation = ?7 \
              AND r.phase = 'copying' AND f.outcome = 'active' \
              AND f.expires_at_ms > {NOW_MS} + ?8 \
              AND g.gate_epoch = r.gate_epoch \
              AND g.current_generation = r.source_generation LIMIT 1"
        );
        let mut params = run_identity(run);
        params.push(json!(R2_PUT_DISPATCH_HEADROOM_MS));
        let rows = self.d1.query(&sql, &params).await.map_err(store_error)?;
        if rows.is_empty() {
            Err(BackfillError::Conflict(
                "backfill transition lease lacks R2 PUT dispatch headroom".to_owned(),
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl<D, R> ByokBackfillStore for D1ByokBackfillStore<D, R>
where
    D: BackfillD1Client + 'static,
    R: BackfillObjectClient + 'static,
{
    async fn begin(&self, request: &BeginBackfill) -> Result<BackfillRun, BackfillError> {
        let lease_ms = lease_ms(request.lease)?;
        let run_id = opaque_id();
        let token = opaque_id();
        let acquire = format!(
            "INSERT INTO byok_transition_fence \
             (tenant_id, token, epoch, observed_gate_epoch, observed_generation, \
              observed_config_version, observed_config_state, observed_byok_status, \
              acquired_at_ms, expires_at_ms) \
             SELECT g.tenant_id, ?1, COALESCE((SELECT MAX(epoch) FROM byok_transition_fence \
              WHERE tenant_id = g.tenant_id),0)+1, g.gate_epoch, g.current_generation, \
              c.config_version, COALESCE(c.state,'absent'), t.byok_status, {NOW_MS}, {NOW_MS}+?2 \
             FROM byok_tenant_gate g JOIN tenant t ON t.tenant_id=g.tenant_id \
             LEFT JOIN tenant_byok_config c ON c.tenant_id=g.tenant_id \
             WHERE g.tenant_id=?3 AND g.gate_epoch=?4 AND g.current_generation=?5 \
              AND COALESCE(c.state,'absent') IN ('pending','partial') \
              AND t.byok_status='active' \
              AND NOT EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=g.tenant_id \
               AND i.outcome='active' AND i.expires_at_ms>{NOW_MS}) \
              AND NOT EXISTS (SELECT 1 FROM byok_transition_fence f WHERE f.tenant_id=g.tenant_id \
               AND f.outcome='active' AND f.expires_at_ms>{NOW_MS}) \
              AND NOT EXISTS (SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=g.tenant_id \
               AND r.phase IN ('copying','ready_to_commit')) \
             RETURNING epoch, observed_gate_epoch, observed_generation"
        );
        let insert_run = format!(
            "INSERT INTO byok_backfill_run \
             (tenant_id,run_id,intent_token,transition_epoch,gate_epoch,source_generation, \
              target_generation,started_at_ms,checkpointed_at_ms) \
             SELECT ?1,?2,?3,f.epoch,f.observed_gate_epoch,f.observed_generation, \
              f.observed_generation+1,{NOW_MS},{NOW_MS} FROM byok_transition_fence f \
             WHERE f.tenant_id=?1 AND f.token=?3 AND f.outcome='active' \
              AND f.expires_at_ms>{NOW_MS} RETURNING run_id"
        );
        let results = self
            .d1
            .batch(vec![
                BackfillD1Statement::new(
                    acquire,
                    vec![
                        json!(token),
                        json!(lease_ms),
                        json!(request.tenant_id),
                        json!(request.expected_gate_epoch),
                        json!(request.expected_source_generation),
                    ],
                ),
                BackfillD1Statement::new(
                    insert_run,
                    vec![json!(request.tenant_id), json!(run_id), json!(token)],
                ),
            ])
            .await
            .map_err(store_error)?;
        if results.first().is_none_or(Vec::is_empty) || results.get(1).is_none_or(Vec::is_empty)
        {
            return Err(BackfillError::Conflict(
                "tenant snapshot changed or transition is already held".to_owned(),
            ));
        }
        self.load_run(&request.tenant_id, &run_id, Some(&token))
            .await
    }

    async fn resume(
        &self,
        tenant_id: &str,
        run_id: &str,
        previous_token: &str,
        lease: Duration,
    ) -> Result<BackfillRun, BackfillError> {
        let lease_ms = lease_ms(lease)?;
        let renew = format!(
            "UPDATE byok_transition_fence SET expires_at_ms={NOW_MS}+?4 \
             WHERE tenant_id=?1 AND token=?3 AND outcome='active' AND expires_at_ms>{NOW_MS} \
              AND EXISTS (SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 \
               AND r.run_id=?2 AND r.intent_token=?3 AND r.phase IN ('copying','ready_to_commit')) \
             RETURNING epoch"
        );
        let params = [
            json!(tenant_id),
            json!(run_id),
            json!(previous_token),
            json!(lease_ms),
        ];
        let renewed = self.d1.query(&renew, &params).await.map_err(store_error)?;
        if !renewed.is_empty() {
            return self.load_run(tenant_id, run_id, Some(previous_token)).await;
        }

        let token = opaque_id();
        let expire = format!(
            "UPDATE byok_transition_fence SET outcome='expired',completed_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND token=?3 AND outcome='active' AND expires_at_ms<={NOW_MS} \
              AND EXISTS (SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 \
               AND r.run_id=?2 AND r.intent_token=?3 AND r.phase IN ('copying','ready_to_commit')) \
             RETURNING epoch"
        );
        let acquire = format!(
            "INSERT INTO byok_transition_fence \
             (tenant_id,token,epoch,observed_gate_epoch,observed_generation,observed_config_version, \
              observed_config_state,observed_byok_status,acquired_at_ms,expires_at_ms) \
             SELECT r.tenant_id,?4,COALESCE((SELECT MAX(epoch) FROM byok_transition_fence \
              WHERE tenant_id=r.tenant_id),0)+1,r.gate_epoch,r.source_generation,c.config_version, \
              COALESCE(c.state,'absent'),t.byok_status,{NOW_MS},{NOW_MS}+?5 \
             FROM byok_backfill_run r JOIN byok_tenant_gate g ON g.tenant_id=r.tenant_id \
             JOIN tenant t ON t.tenant_id=r.tenant_id LEFT JOIN tenant_byok_config c ON c.tenant_id=r.tenant_id \
             WHERE r.tenant_id=?1 AND r.run_id=?2 AND r.intent_token=?3 \
              AND r.phase IN ('copying','ready_to_commit') AND g.gate_epoch=r.gate_epoch \
              AND g.current_generation=r.source_generation AND t.byok_status='active' \
              AND COALESCE(c.state,'absent') IN ('pending','partial') \
              AND EXISTS (SELECT 1 FROM byok_transition_fence old WHERE old.tenant_id=?1 \
               AND old.token=?3 AND old.outcome='expired' \
               AND old.observed_gate_epoch=r.gate_epoch \
               AND old.observed_generation=r.source_generation \
               AND ((old.observed_config_version IS NULL AND c.config_version IS NULL) \
                    OR old.observed_config_version=c.config_version) \
               AND old.observed_config_state=COALESCE(c.state,'absent') \
               AND old.observed_byok_status=t.byok_status) \
              AND NOT EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=?1 \
               AND i.outcome='active' AND i.expires_at_ms>{NOW_MS}) \
              AND NOT EXISTS (SELECT 1 FROM byok_transition_fence live WHERE live.tenant_id=?1 \
               AND live.outcome='active' AND live.expires_at_ms>{NOW_MS}) RETURNING epoch"
        );
        let switch = format!(
            "UPDATE byok_backfill_run SET intent_token=?4,transition_epoch=(SELECT epoch \
              FROM byok_transition_fence WHERE tenant_id=?1 AND token=?4 AND outcome='active' \
               AND expires_at_ms>{NOW_MS}) WHERE tenant_id=?1 AND run_id=?2 AND intent_token=?3 \
              AND phase IN ('copying','ready_to_commit') AND EXISTS (SELECT 1 FROM byok_transition_fence \
               WHERE tenant_id=?1 AND token=?4 AND outcome='active' AND expires_at_ms>{NOW_MS}) \
             RETURNING run_id"
        );
        let common = vec![
            json!(tenant_id),
            json!(run_id),
            json!(previous_token),
            json!(token),
            json!(lease_ms),
        ];
        let results = self
            .d1
            .batch(vec![
                BackfillD1Statement::new(expire, common[..3].to_vec()),
                BackfillD1Statement::new(acquire, common.clone()),
                BackfillD1Statement::new(switch, common[..4].to_vec()),
            ])
            .await
            .map_err(store_error)?;
        if results.iter().any(Vec::is_empty) {
            return Err(BackfillError::Conflict(
                "exact token is not expired or takeover snapshot changed".to_owned(),
            ));
        }
        self.load_run(tenant_id, run_id, Some(&token)).await
    }

    async fn renew(&self, run: &BackfillRun, lease: Duration) -> Result<(), BackfillError> {
        let sql = format!(
            "UPDATE byok_transition_fence SET expires_at_ms={NOW_MS}+?8 \
             WHERE tenant_id=?1 AND token=?3 AND epoch=?4 AND outcome='active' \
              AND expires_at_ms>{NOW_MS} AND observed_gate_epoch=?5 AND observed_generation=?6 \
              AND EXISTS (SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 AND r.run_id=?2 \
               AND r.intent_token=?3 AND r.transition_epoch=?4 AND r.gate_epoch=?5 \
               AND r.source_generation=?6 AND r.target_generation=?7 \
               AND r.phase IN ('copying','ready_to_commit')) RETURNING epoch"
        );
        let mut params = run_identity(run);
        params.push(json!(lease_ms(lease)?));
        let rows = self.d1.query(&sql, &params).await.map_err(store_error)?;
        if rows.is_empty() {
            Err(BackfillError::Conflict(
                "cannot renew stale backfill lease".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    async fn enumerate(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        after: Option<&str>,
        limit: usize,
    ) -> Result<BackfillPage, BackfillError> {
        self.assert_live(run).await?;
        let prefix = self.tenant_prefix(&run.tenant_id)?;
        let (table, logical, deleted) = match surface {
            BackfillSurface::Cas => ("blob_meta", "digest", "AND m.deleted_at IS NULL".to_owned()),
            BackfillSurface::Ac => (
                "ac_meta",
                "action_digest",
                format!("AND (m.expires_at IS NULL OR m.expires_at > {NOW_MS})"),
            ),
        };
        let sql = if run.source_generation == 0 {
            format!(
                "SELECT m.{logical} AS logical_key, ?7 || '/' || ?4 || '/' || m.{logical} AS physical_key, \
                 1 AS is_raw FROM {table} m WHERE m.tenant_id=?1 {deleted} \
                 AND (?5 IS NULL OR m.{logical}>?5) ORDER BY m.{logical} ASC LIMIT ?6"
            )
        } else {
            let metadata_guard = match surface {
                BackfillSurface::Cas => {
                    "EXISTS (SELECT 1 FROM blob_meta m WHERE m.tenant_id=g.tenant_id \
                     AND m.digest=substr(g.logical_key,instr(g.logical_key,':')+1) \
                     AND m.deleted_at IS NULL)"
                }
                BackfillSurface::Ac => {
                    "EXISTS (SELECT 1 FROM ac_meta m WHERE m.tenant_id=g.tenant_id \
                     AND m.action_digest=g.logical_key AND (m.expires_at IS NULL \
                     OR m.expires_at > (CAST(strftime('%s','now') AS INTEGER)*1000)))"
                }
            };
            format!(
                "SELECT g.logical_key,g.physical_key,0 AS is_raw \
                 FROM byok_logical_object_generation g WHERE g.tenant_id=?1 \
                 AND g.object_kind=?2 AND g.generation=?3 AND g.outcome='published' \
                 AND {metadata_guard} AND (?5 IS NULL OR g.logical_key>?5) \
                 ORDER BY g.logical_key ASC LIMIT ?6"
            )
        };
        let requested = i64::try_from(limit).map_err(|_| {
            BackfillError::InvalidRequest("page limit does not fit D1 integer".to_owned())
        })?;
        let mut params = vec![
            json!(run.tenant_id),
            json!(surface.as_str()),
            json!(run.source_generation),
            json!(prefix),
            json!(after),
            json!(requested.saturating_add(1)),
        ];
        if run.source_generation == 0 {
            params.push(json!(self.checked_region(surface)?));
        }
        let rows = self.d1.query(&sql, &params).await.map_err(store_error)?;
        let has_more = rows.len() > limit;
        let mut objects = Vec::with_capacity(rows.len().min(limit));
        for row in rows.iter().take(limit) {
            let logical_key = required_string(row, "logical_key")?;
            let source_physical_key = required_string(row, "physical_key")?;
            if required_i64(row, "is_raw")? == 1 {
                if surface == BackfillSurface::Cas {
                    let sha_key = legacy_sha_key(&source_physical_key).ok_or_else(|| {
                        BackfillError::SourceChanged(
                            "legacy CAS source key is malformed".to_owned(),
                        )
                    })?;
                    let native = self
                        .object_client(surface)
                        .head_size(&source_physical_key)
                        .await
                        .map_err(store_error)?;
                    let sha = self
                        .object_client(surface)
                        .head_size(&sha_key)
                        .await
                        .map_err(store_error)?;
                    match (native, sha) {
                        (Some(native_size), None) => objects.push(BackfillSourceObject {
                            surface,
                            logical_key: format!("blake3:{logical_key}"),
                            source_physical_key,
                            source_crypto: Some(
                                super::byok_activation::SourceCryptoIdentity::Plaintext,
                            ),
                            source_blake3: None,
                            plaintext_size: Some(native_size),
                        }),
                        (None, Some(sha_size)) => objects.push(BackfillSourceObject {
                            surface,
                            logical_key: format!("sha256:{logical_key}"),
                            source_physical_key: sha_key,
                            source_crypto: Some(
                                super::byok_activation::SourceCryptoIdentity::Plaintext,
                            ),
                            source_blake3: None,
                            plaintext_size: Some(sha_size),
                        }),
                        _ => {
                            return Err(BackfillError::SourceChanged(
                                "legacy CAS digest is absent or ambiguous across keyspaces"
                                    .to_owned(),
                            ))
                        }
                    }
                } else {
                    let plaintext_size = self
                        .object_client(surface)
                        .head_size(&source_physical_key)
                        .await
                        .map_err(store_error)?
                        .ok_or_else(|| {
                            BackfillError::SourceChanged(
                                "legacy AC source object disappeared".to_owned(),
                            )
                        })?;
                    objects.push(BackfillSourceObject {
                        surface,
                        logical_key,
                        source_physical_key,
                        source_crypto: Some(
                            super::byok_activation::SourceCryptoIdentity::Plaintext,
                        ),
                        source_blake3: None,
                        plaintext_size: Some(plaintext_size),
                    });
                }
            } else {
                return Err(BackfillError::SourceChanged(
                    "legacy generation source lacks the 0121 exact crypto ledger".to_owned(),
                ));
            }
        }
        let next_cursor = if has_more {
            rows.get(limit.saturating_sub(1))
                .map(|row| required_string(row, "logical_key"))
                .transpose()?
        } else {
            None
        };
        Ok(BackfillPage {
            objects,
            next_cursor,
        })
    }

    async fn read_source(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<Vec<u8>, BackfillError> {
        self.assert_live(run).await?;
        let first = self
            .object_client(source.surface)
            .get_capped(&source.source_physical_key, self.max_object_bytes)
            .await
            .map_err(store_error)?;
        match first {
            CappedGet::Found(bytes) => Ok(bytes),
            CappedGet::Missing => Err(BackfillError::SourceChanged(
                "enumerated source object disappeared".to_owned(),
            )),
            CappedGet::TooLarge { .. } => Err(BackfillError::SourceChanged(
                "source object exceeds bounded backfill memory ceiling".to_owned(),
            )),
        }
    }

    async fn target_physical_key(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        logical_key: &str,
    ) -> Result<String, BackfillError> {
        self.assert_live(run).await?;
        let region = self.checked_region(surface)?;
        Ok(format!(
            "{}/{}/{}",
            region,
            self.tenant_prefix(&run.tenant_id)?,
            generation_qualified_suffix(surface, run.target_generation, logical_key)
        ))
    }

    async fn stage(
        &self,
        run: &BackfillRun,
        staged: &StagedBackfillObject,
    ) -> Result<BackfillAllocation, BackfillError> {
        self.assert_live(run).await?;
        let ciphertext_len = u64::try_from(staged.ciphertext.len()).map_err(|_| {
            BackfillError::InvalidRequest("ciphertext length does not fit u64".to_owned())
        })?;
        if staged.target_generation != run.target_generation
            || ciphertext_len > self.max_object_bytes
        {
            return Err(BackfillError::InvalidRequest(
                "staged object violates generation or size bound".to_owned(),
            ));
        }
        let expected_prefix = format!(
            "{}/{}/",
            self.region(staged.surface),
            self.tenant_prefix(&run.tenant_id)?
        );
        if !staged.target_physical_key.starts_with(&expected_prefix) {
            return Err(BackfillError::InvalidRequest(
                "target R2 key escaped tenant namespace".to_owned(),
            ));
        }
        // Revalidate on D1's clock immediately before dispatch. The common
        // purger waits for this immutable fence expiry plus a tail margin.
        self.assert_put_headroom(run).await?;
        let created = self
            .object_client(staged.surface)
            .put_if_absent(&staged.target_physical_key, staged.ciphertext.clone())
            .await
            .map_err(store_error)?;
        if !created {
            match self
                .object_client(staged.surface)
                .get_capped(&staged.target_physical_key, self.max_object_bytes)
                .await
                .map_err(store_error)?
            {
                CappedGet::Found(existing) if existing == staged.ciphertext => {}
                CappedGet::Found(_) => {
                    return Err(BackfillError::Conflict(
                        "target key contains unequal ciphertext".to_owned(),
                    ))
                }
                _ => {
                    return Err(BackfillError::Conflict(
                        "conditional PUT conflict could not be verified".to_owned(),
                    ))
                }
            }
        }
        self.assert_live(run).await?;
        Ok(BackfillAllocation {
            allocation_id: staged.allocation_id.clone(),
            surface: staged.surface,
            logical_key: staged.logical_key.clone(),
            target_generation: staged.target_generation,
            target_physical_key: staged.target_physical_key.clone(),
            plaintext_len: staged.plaintext_len,
            ciphertext_len,
            ciphertext_blake3: hex::encode(blake3::hash(&staged.ciphertext).as_bytes()),
        })
    }

    async fn checkpoint(
        &self,
        run: &BackfillRun,
        checkpoint: &BackfillCheckpoint,
    ) -> Result<(), BackfillError> {
        validate_checkpoint_identity(run, checkpoint)?;
        let allocation = checkpoint.allocation.as_ref();
        let insert = if allocation.is_some() {
            format!("INSERT OR IGNORE INTO byok_logical_object_generation \
              (tenant_id,object_kind,logical_key,generation,physical_key,intent_token,gate_epoch,size_bytes,outcome,allocated_at_ms,allocation_id,backfill_run_id,ciphertext_size,ciphertext_blake3) \
              SELECT ?1,?8,?9,?7,?10,?3,?5,?16,'allocated',{NOW_MS},?11,?2,?12,?13 \
              WHERE EXISTS (SELECT 1 FROM byok_transition_fence f WHERE f.tenant_id=?1 AND f.token=?3 AND f.epoch=?4 AND f.outcome='active' AND f.expires_at_ms>{NOW_MS}) RETURNING allocation_id")
        } else {
            "SELECT 1 AS allocation_id".to_owned()
        };
        let update = format!("UPDATE byok_backfill_run SET \
          cas_cursor=CASE WHEN ?8='cas' THEN COALESCE(?14,cas_cursor) ELSE cas_cursor END, \
          ac_cursor=CASE WHEN ?8='ac' THEN COALESCE(?14,ac_cursor) ELSE ac_cursor END, \
          cas_complete=CASE WHEN ?8='cas' AND ?15=1 THEN 1 ELSE cas_complete END, \
          ac_complete=CASE WHEN ?8='ac' AND ?15=1 THEN 1 ELSE ac_complete END, \
          cas_copied=(SELECT COUNT(*) FROM byok_logical_object_generation x \
           WHERE x.tenant_id=?1 AND x.backfill_run_id=?2 AND x.object_kind='cas'), \
          ac_copied=(SELECT COUNT(*) FROM byok_logical_object_generation x \
           WHERE x.tenant_id=?1 AND x.backfill_run_id=?2 AND x.object_kind='ac'), \
          phase=CASE WHEN (cas_complete=1 OR (?8='cas' AND ?15=1)) AND (ac_complete=1 OR (?8='ac' AND ?15=1)) THEN 'ready_to_commit' ELSE phase END, \
          checkpointed_at_ms={NOW_MS} WHERE tenant_id=?1 AND run_id=?2 AND intent_token=?3 \
          AND transition_epoch=?4 AND gate_epoch=?5 AND source_generation=?6 AND target_generation=?7 \
          AND phase IN ('copying','ready_to_commit') AND EXISTS (SELECT 1 FROM byok_transition_fence f \
           WHERE f.tenant_id=?1 AND f.token=?3 AND f.epoch=?4 AND f.outcome='active' AND f.expires_at_ms>{NOW_MS}) \
          AND (?14 IS NULL OR (CASE WHEN ?8='cas' THEN cas_cursor ELSE ac_cursor END) IS NULL \
               OR ?14 >= (CASE WHEN ?8='cas' THEN cas_cursor ELSE ac_cursor END)) \
          AND (?11 IS NULL OR EXISTS (SELECT 1 FROM byok_logical_object_generation g WHERE g.tenant_id=?1 \
           AND g.allocation_id=?11 AND g.backfill_run_id=?2 AND g.object_kind=?8 AND g.logical_key=?9 \
           AND g.generation=?7 AND g.physical_key=?10 AND g.size_bytes=?16 \
           AND g.ciphertext_size=?12 AND g.ciphertext_blake3=?13)) RETURNING run_id");
        let mut params = checkpoint_params(run, checkpoint);
        let insert_params = if allocation.is_some() {
            params.clone()
        } else {
            Vec::new()
        };
        let results = self
            .d1
            .batch(vec![
                BackfillD1Statement::new(insert, insert_params),
                BackfillD1Statement::new(update, std::mem::take(&mut params)),
            ])
            .await
            .map_err(store_error)?;
        if results.get(1).is_none_or(Vec::is_empty) {
            return Err(BackfillError::Conflict(
                "checkpoint guard or allocation identity failed".to_owned(),
            ));
        }
        Ok(())
    }

    async fn commit_generation(&self, run: &BackfillRun) -> Result<(), BackfillError> {
        self.assert_live(run).await?;
        let identity = run_identity(run);
        let publish_allocations = format!("UPDATE byok_logical_object_generation SET outcome='published',gate_epoch=?5+1,completed_at_ms={NOW_MS} \
          WHERE tenant_id=?1 AND backfill_run_id=?2 AND gate_epoch=?5 AND generation=?7 \
           AND outcome='allocated' AND EXISTS (SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 AND r.run_id=?2 \
            AND r.intent_token=?3 AND r.phase='ready_to_commit' AND r.cas_complete=1 AND r.ac_complete=1)");
        let publications = format!("INSERT INTO byok_logical_object_publication \
          (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms) \
          SELECT tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,{NOW_MS} \
          FROM byok_logical_object_generation WHERE tenant_id=?1 AND backfill_run_id=?2 \
           AND gate_epoch=?5+1 AND generation=?7 AND outcome='published' \
          ON CONFLICT(tenant_id,object_kind,logical_key) DO UPDATE SET generation=excluded.generation, \
           allocation_id=excluded.allocation_id,physical_key=excluded.physical_key,gate_epoch=excluded.gate_epoch, \
           size_bytes=excluded.size_bytes,published_at_ms=excluded.published_at_ms \
          WHERE byok_logical_object_publication.generation=?6");
        let gate = format!(
            "UPDATE byok_tenant_gate SET current_generation=?7,gate_epoch=gate_epoch+1 \
          WHERE tenant_id=?1 AND gate_epoch=?5 AND current_generation=?6 \
           AND EXISTS (SELECT 1 FROM byok_transition_fence f \
            JOIN tenant t ON t.tenant_id=f.tenant_id \
            LEFT JOIN tenant_byok_config c ON c.tenant_id=f.tenant_id \
            WHERE f.tenant_id=?1 AND f.token=?3 AND f.epoch=?4 \
            AND f.outcome='active' AND f.expires_at_ms>{NOW_MS} \
            AND f.observed_gate_epoch=?5 AND f.observed_generation=?6 \
            AND ((f.observed_config_version IS NULL AND c.config_version IS NULL) \
                 OR f.observed_config_version=c.config_version) \
            AND f.observed_config_state=COALESCE(c.state,'absent') \
            AND f.observed_byok_status=t.byok_status) \
           AND EXISTS (SELECT 1 FROM byok_backfill_run r \
           WHERE r.tenant_id=?1 AND r.run_id=?2 AND r.intent_token=?3 AND r.transition_epoch=?4 \
            AND r.phase='ready_to_commit' AND r.cas_complete=1 AND r.ac_complete=1) \
           AND NOT EXISTS (SELECT 1 FROM byok_logical_object_generation x \
            LEFT JOIN byok_logical_object_publication p ON p.tenant_id=x.tenant_id \
             AND p.object_kind=x.object_kind AND p.logical_key=x.logical_key \
            WHERE x.tenant_id=?1 AND x.backfill_run_id=?2 AND x.generation=?7 \
             AND (x.outcome<>'published' OR p.tenant_id IS NULL OR p.generation<>?7 \
                  OR p.allocation_id<>x.allocation_id OR p.physical_key<>x.physical_key \
                  OR p.size_bytes<>x.size_bytes OR p.gate_epoch<>?5+1)) RETURNING gate_epoch"
        );
        let config = "UPDATE tenant_byok_config SET state='active',config_version=config_version+1 WHERE tenant_id=?1 AND state IN ('pending','partial') AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.current_generation=?7 AND g.gate_epoch=?5+1) RETURNING config_version";
        let finish_run = format!("UPDATE byok_backfill_run SET phase='committed',completed_at_ms={NOW_MS},checkpointed_at_ms={NOW_MS} \
          WHERE tenant_id=?1 AND run_id=?2 AND intent_token=?3 AND transition_epoch=?4 AND phase='ready_to_commit' \
           AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.current_generation=?7 AND g.gate_epoch=?5+1) \
           AND EXISTS (SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=?1 AND c.state='active') RETURNING run_id");
        let release = format!("UPDATE byok_transition_fence SET outcome='committed',completed_at_ms={NOW_MS} \
          WHERE tenant_id=?1 AND token=?3 AND epoch=?4 AND outcome='active' AND EXISTS (SELECT 1 FROM byok_backfill_run r \
           WHERE r.tenant_id=?1 AND r.run_id=?2 AND r.phase='committed') RETURNING token");
        let results = self
            .d1
            .batch(vec![
                BackfillD1Statement::new(publish_allocations, identity.clone()),
                BackfillD1Statement::new(publications, identity.clone()),
                BackfillD1Statement::new(gate, identity.clone()),
                BackfillD1Statement::new(config, identity.clone()),
                BackfillD1Statement::new(finish_run, identity.clone()),
                BackfillD1Statement::new(release, identity[..4].to_vec()),
            ])
            .await
            .map_err(store_error)?;
        if results.iter().skip(2).any(Vec::is_empty) {
            return Err(BackfillError::Conflict(
                "guarded generation commit did not complete".to_owned(),
            ));
        }
        Ok(())
    }

    async fn abort(&self, run: &BackfillRun, reason: &str) -> Result<(), BackfillError> {
        let identity = run_identity(run);
        let abort = format!("UPDATE byok_backfill_run SET phase='aborted',failure_reason=?8,completed_at_ms={NOW_MS},checkpointed_at_ms={NOW_MS} \
          WHERE tenant_id=?1 AND run_id=?2 AND intent_token=?3 AND transition_epoch=?4 AND gate_epoch=?5 \
           AND source_generation=?6 AND target_generation=?7 AND phase IN ('copying','ready_to_commit') \
           AND EXISTS(SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=?1 AND c.state IN ('pending','partial')) \
           AND EXISTS(SELECT 1 FROM byok_transition_fence f WHERE f.tenant_id=?1 AND f.token=?3 AND f.epoch=?4 \
            AND f.outcome='active' AND f.expires_at_ms>{NOW_MS}) RETURNING run_id");
        let abandon = format!("UPDATE byok_logical_object_generation SET outcome='abandoned',completed_at_ms={NOW_MS} \
          WHERE tenant_id=?1 AND backfill_run_id=?2 AND generation=?7 AND outcome='allocated' \
           AND EXISTS(SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 AND r.run_id=?2 AND r.phase='aborted')");
        let deactivate = "UPDATE tenant_byok_config SET state='inactive',config_version=config_version+1 \
          WHERE tenant_id=?1 AND state IN ('pending','partial') AND EXISTS(SELECT 1 FROM byok_backfill_run r \
           WHERE r.tenant_id=?1 AND r.run_id=?2 AND r.phase='aborted') RETURNING config_version";
        let release = format!("UPDATE byok_transition_fence SET outcome='aborted',completed_at_ms={NOW_MS} WHERE tenant_id=?1 \
          AND token=?3 AND epoch=?4 AND outcome='active' AND EXISTS(SELECT 1 FROM byok_backfill_run r WHERE r.tenant_id=?1 \
           AND r.run_id=?2 AND r.phase='aborted') RETURNING token");
        let mut abort_params = identity.clone();
        abort_params.push(json!(reason));
        let results = self
            .d1
            .batch(vec![
                BackfillD1Statement::new(abort, abort_params),
                BackfillD1Statement::new(abandon, identity.clone()),
                BackfillD1Statement::new(deactivate, identity.clone()),
                BackfillD1Statement::new(release, identity[..4].to_vec()),
            ])
            .await
            .map_err(store_error)?;
        if results.first().is_none_or(Vec::is_empty)
            || results.get(2).is_none_or(Vec::is_empty)
            || results.get(3).is_none_or(Vec::is_empty)
        {
            return Err(BackfillError::Conflict(
                "abort capability is stale or expired".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Result of one finite scheduler invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackfillDriveOutcome {
    /// Durable progress was made but more pages remain.
    InProgress(BackfillRun),
    /// Both surfaces and the generation switch committed.
    Committed(BackfillRun),
}

/// Resume and perform at most `max_pages` pages. Returning `InProgress` is a
/// normal scheduling boundary; durable cursors are never reconstructed in memory.
pub(crate) async fn drive_to_completion<S: ByokBackfillStore, E: ByokBackfillEncryptor>(
    engine: &ByokBackfillEngine<S, E>,
    tenant_id: &str,
    run_id: &str,
    previous_token: &str,
    lease: Duration,
    page_limit: usize,
    max_pages: usize,
) -> Result<BackfillDriveOutcome, BackfillError> {
    if max_pages == 0 {
        return Err(BackfillError::InvalidRequest(
            "max_pages must be greater than zero".to_owned(),
        ));
    }
    if page_limit == 0 {
        return Err(BackfillError::InvalidRequest(
            "page_limit must be greater than zero".to_owned(),
        ));
    }
    let mut run = engine
        .resume(tenant_id, run_id, previous_token, lease)
        .await?;
    if run.phase == BackfillPhase::Committed {
        return Ok(BackfillDriveOutcome::Committed(run));
    }
    if run.phase == BackfillPhase::Aborted {
        return Err(BackfillError::Conflict(
            "aborted backfill cannot resume".to_owned(),
        ));
    }
    let mut pages = 0usize;
    for surface in [BackfillSurface::Cas, BackfillSurface::Ac] {
        while run.phase == BackfillPhase::Copying
            && !match surface {
                BackfillSurface::Cas => run.cas_complete,
                BackfillSurface::Ac => run.ac_complete,
            }
        {
            if pages >= max_pages {
                return Ok(BackfillDriveOutcome::InProgress(run));
            }
            engine
                .copy_page(&mut run, surface, page_limit, lease)
                .await?;
            pages += 1;
        }
    }
    if run.phase == BackfillPhase::ReadyToCommit {
        engine.commit(&mut run).await?;
        return Ok(BackfillDriveOutcome::Committed(run));
    }
    Ok(BackfillDriveOutcome::InProgress(run))
}

fn opaque_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn lease_ms(lease: Duration) -> Result<i64, BackfillError> {
    if lease < Duration::from_secs(1) || lease > MAX_BYOK_FENCE_LEASE {
        return Err(BackfillError::InvalidRequest(format!(
            "lease must be in 1s..={MAX_BYOK_FENCE_LEASE:?}"
        )));
    }
    i64::try_from(lease.as_millis())
        .map_err(|_| BackfillError::InvalidRequest("lease overflow".to_owned()))
}
fn store_error(error: impl ToString) -> BackfillError {
    BackfillError::Store(error.to_string())
}
fn required_string(row: &D1Row, name: &str) -> Result<String, BackfillError> {
    row.get(name)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| store_error(format!("missing text {name}")))
}
fn required_i64(row: &D1Row, name: &str) -> Result<i64, BackfillError> {
    row.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| store_error(format!("missing integer {name}")))
}
fn optional_string(row: &D1Row, name: &str) -> Option<String> {
    row.get(name).and_then(Value::as_str).map(str::to_owned)
}
fn legacy_sha_key(source_physical_key: &str) -> Option<String> {
    let segments = source_physical_key.split('/').collect::<Vec<_>>();
    if segments.len() != 3 || segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    Some(format!(
        "{}/{}/bazel/sha256/{}",
        segments[0], segments[1], segments[2]
    ))
}
fn run_from_row(row: &D1Row) -> Result<BackfillRun, BackfillError> {
    let phase = match required_string(row, "phase")?.as_str() {
        "copying" => BackfillPhase::Copying,
        "ready_to_commit" => BackfillPhase::ReadyToCommit,
        "committed" => BackfillPhase::Committed,
        "aborted" => BackfillPhase::Aborted,
        other => return Err(store_error(format!("unknown backfill phase {other:?}"))),
    };
    Ok(BackfillRun {
        tenant_id: required_string(row, "tenant_id")?,
        run_id: required_string(row, "run_id")?,
        run_token: required_string(row, "intent_token")?,
        transition_epoch: required_i64(row, "transition_epoch")?,
        gate_epoch: required_i64(row, "gate_epoch")?,
        source_generation: required_i64(row, "source_generation")?,
        target_generation: required_i64(row, "target_generation")?,
        phase,
        cas_cursor: optional_string(row, "cas_cursor"),
        ac_cursor: optional_string(row, "ac_cursor"),
        cas_complete: required_i64(row, "cas_complete")? == 1,
        ac_complete: required_i64(row, "ac_complete")? == 1,
    })
}
fn run_identity(run: &BackfillRun) -> Vec<Value> {
    vec![
        json!(run.tenant_id),
        json!(run.run_id),
        json!(run.run_token),
        json!(run.transition_epoch),
        json!(run.gate_epoch),
        json!(run.source_generation),
        json!(run.target_generation),
    ]
}
fn validate_checkpoint_identity(
    run: &BackfillRun,
    c: &BackfillCheckpoint,
) -> Result<(), BackfillError> {
    if c.tenant_id != run.tenant_id
        || c.run_token != run.run_token
        || c.transition_epoch != run.transition_epoch
        || c.gate_epoch != run.gate_epoch
        || c.target_generation != run.target_generation
    {
        return Err(BackfillError::InvalidRequest(
            "checkpoint identity differs from live run".to_owned(),
        ));
    }
    Ok(())
}
fn checkpoint_params(run: &BackfillRun, c: &BackfillCheckpoint) -> Vec<Value> {
    let a = c.allocation.as_ref();
    vec![
        json!(run.tenant_id),
        json!(run.run_id),
        json!(run.run_token),
        json!(run.transition_epoch),
        json!(run.gate_epoch),
        json!(run.source_generation),
        json!(run.target_generation),
        json!(c.surface.as_str()),
        json!(a.map(|v| v.logical_key.as_str())),
        json!(a.map(|v| v.target_physical_key.as_str())),
        json!(a.map(|v| v.allocation_id.as_str())),
        json!(a.map(|v| v.ciphertext_len)),
        json!(a.map(|v| v.ciphertext_blake3.as_str())),
        json!(c.next_cursor),
        json!(i64::from(c.surface_complete)),
        json!(a.map(|v| v.plaintext_len)),
    ]
}

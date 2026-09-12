//! Concrete object worker for the 0121 purge-before-active protocol.
//!
//! R2 has no transaction with D1. Target keys and Mode-B allocation envelope
//! identities are therefore deterministic, PUT is conditional, and every D1
//! checkpoint proves the exact source and target byte identities before moving
//! its keyset cursor. Purge verification requires DELETE followed by HEAD 404;
//! random-mode envelope cleanup is allocation-qualified and precedes `verified`.

use std::{fmt, future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use serde_json::{json, Value};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{
    byok_activation::{
        ActivationIntent, ActivationPhase, ActivationSourceObject, ActivationStep,
        PhysicalPurgeItem, SourceCryptoIdentity,
    },
    byok_activation_d1::ActivationObjectWorker,
    byok_activation_d1::D1ByokActivationStore,
    byok_backfill::{
        BackfillError, BackfillPhase, BackfillRun, BackfillSourceObject, BackfillSurface,
        ByokBackfillEncryptor,
    },
    byok_backfill_crypto::ProductionByokBackfillEncryptor,
    byok_generation_catalog::generation_qualified_digest,
    d1_http::{D1BatchStatement, D1HttpClient, D1Row},
    r2_s3::{CappedGet, R2S3Client},
    StorageEnv,
};

const NOW_MS: &str = "(CAST(strftime('%s','now') AS INTEGER)*1000)";
const MAX_PURGE_LEASE: Duration = Duration::from_secs(120);
const TARGET_WRITE_LEASE: Duration = Duration::from_secs(90);
const TARGET_WRITE_MIN_REMAINING: Duration = Duration::from_secs(25);

/// Non-secret bounded-work settings for the production activation worker.
#[derive(Debug, Clone, Copy)]
pub struct ActivationWorkerLimits {
    /// Maximum source or target object size accepted by one operation.
    pub max_object_bytes: u64,
    /// Durable lease used while deleting and HEAD-verifying a source object.
    pub purge_lease: Duration,
    /// Failed purge-attempt threshold before an item is quarantined.
    pub quarantine_after: u32,
}

/// Canonical production assembly below `main`: reads the TDK/bucket/region
/// bindings once, constructs the exact two R2 clients, and returns the lifecycle
/// store ready for the supervisor. KMS/provider-registry assembly remains in the
/// crypto factory and is passed here as one already validated encryptor.
pub async fn build_production_activation_store_from_env(
    storage: &StorageEnv,
    d1: Arc<D1HttpClient>,
    encryptor: Arc<ProductionByokBackfillEncryptor>,
    limits: ActivationWorkerLimits,
) -> Result<
    D1ByokActivationStore<
        D1HttpClient,
        ProductionActivationObjectWorker<D1HttpClient, R2S3Client, ProductionByokBackfillEncryptor>,
    >,
    BackfillError,
> {
    let tdk_hex = Zeroizing::new(super::non_empty_env("R2_TDK_HEX").ok_or_else(|| {
        BackfillError::InvalidRequest("R2_TDK_HEX is required for activation".to_owned())
    })?);
    let raw =
        Zeroizing::new(hex::decode(tdk_hex.as_str()).map_err(|_| {
            BackfillError::InvalidRequest("R2_TDK_HEX must be valid hex".to_owned())
        })?);
    if raw.len() != 32 {
        return Err(BackfillError::InvalidRequest(
            "R2_TDK_HEX must encode exactly 32 bytes".to_owned(),
        ));
    }
    let mut key = Zeroizing::new([0_u8; 32]);
    key.copy_from_slice(&raw);
    let tdk = TenantDerivationKey::from_bytes(key);
    let cas_region = super::env_or("R2_CAS_REGION", "iad");
    let ac_region = super::env_or("R2_AC_REGION", "iad");
    let cas_bucket = super::env_or("R2_CAS_BUCKET", "corelink-cas-prod");
    let ac_bucket = super::env_or("R2_AC_BUCKET", "corelink-ac-iad");
    validate_bucket("CAS", &cas_bucket, &cas_region, "cas")?;
    validate_bucket("AC", &ac_bucket, &ac_region, "ac")?;
    let cas = Arc::new(
        R2S3Client::new(storage, cas_bucket)
            .await
            .map_err(store_error)?,
    );
    let ac = Arc::new(
        R2S3Client::new(storage, ac_bucket)
            .await
            .map_err(store_error)?,
    );
    let worker = Arc::new(ProductionActivationObjectWorker::new(
        Arc::clone(&d1),
        cas,
        ac,
        encryptor,
        tdk,
        cas_region,
        ac_region,
        limits.max_object_bytes,
        limits.purge_lease,
        limits.quarantine_after,
    )?);
    Ok(D1ByokActivationStore::new(d1, worker))
}

#[derive(Debug, Clone)]
/// One parameterized statement in an atomic activation-worker D1 batch.
pub struct WorkerStatement {
    sql: String,
    params: Vec<Value>,
}

impl WorkerStatement {
    fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

/// Minimal D1 seam used by the production worker and hermetic tests.
#[async_trait]
pub trait ActivationWorkerD1: Send + Sync + fmt::Debug {
    /// Execute one parameterized read and return its rows.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String>;
    /// Execute parameterized statements atomically and return rows per statement.
    async fn batch(&self, statements: Vec<WorkerStatement>) -> Result<Vec<Vec<D1Row>>, String>;
}

#[async_trait]
impl ActivationWorkerD1 for D1HttpClient {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        D1HttpClient::query(self, sql, params).await
    }

    async fn batch(&self, statements: Vec<WorkerStatement>) -> Result<Vec<Vec<D1Row>>, String> {
        D1HttpClient::batch(
            self,
            statements
                .into_iter()
                .map(|statement| D1BatchStatement::new(statement.sql, statement.params))
                .collect(),
        )
        .await
        .map_err(|error| match error.statement {
            Some(index) => format!(
                "D1 activation object batch statement {index}: {}",
                error.message
            ),
            None => format!("D1 activation object batch: {}", error.message),
        })
    }
}

/// R2 operations required by activation copy and purge.
#[async_trait]
pub trait ActivationR2: Send + Sync + fmt::Debug {
    /// Read an exact key while rejecting bodies beyond `max_bytes`.
    async fn get_capped(&self, key: &str, max_bytes: u64) -> Result<CappedGet, String>;
    /// Return the stored byte length, or `None` when the exact key is absent.
    async fn head_size(&self, key: &str) -> Result<Option<u64>, String>;
    /// Conditionally create an exact key; return whether this call created it.
    async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String>;
    /// Delete the exact key idempotently.
    async fn delete(&self, key: &str) -> Result<(), String>;
}

#[async_trait]
impl ActivationR2 for R2S3Client {
    async fn get_capped(&self, key: &str, max_bytes: u64) -> Result<CappedGet, String> {
        R2S3Client::get_capped(self, key, max_bytes).await
    }

    async fn head_size(&self, key: &str) -> Result<Option<u64>, String> {
        R2S3Client::head_size(self, key).await
    }

    async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String> {
        R2S3Client::put_if_absent(self, key, bytes).await
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        R2S3Client::delete(self, key).await
    }
}

/// Real bounded activation worker. Generic parameters are test seams; the
/// defaults are the production D1, R2 and BYOK crypto boundary.
pub struct ProductionActivationObjectWorker<
    D = D1HttpClient,
    R = R2S3Client,
    E = ProductionByokBackfillEncryptor,
> {
    d1: Arc<D>,
    cas: Arc<R>,
    ac: Arc<R>,
    encryptor: Arc<E>,
    tdk: TenantDerivationKey,
    cas_region: String,
    ac_region: String,
    max_object_bytes: u64,
    purge_lease_ms: i64,
    operation_timeout: Duration,
    quarantine_after: i64,
}

impl<D, R, E> fmt::Debug for ProductionActivationObjectWorker<D, R, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionActivationObjectWorker")
            .field("cas_region", &self.cas_region)
            .field("ac_region", &self.ac_region)
            .field("max_object_bytes", &self.max_object_bytes)
            .field("purge_lease_ms", &self.purge_lease_ms)
            .field("operation_timeout", &self.operation_timeout)
            .field("quarantine_after", &self.quarantine_after)
            .finish_non_exhaustive()
    }
}

impl<D, R, E> ProductionActivationObjectWorker<D, R, E> {
    /// Build the worker from already validated production collaborators.
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit security collaborators and independent bounds must remain visible at the construction boundary"
    )]
    pub fn new(
        d1: Arc<D>,
        cas: Arc<R>,
        ac: Arc<R>,
        encryptor: Arc<E>,
        tdk: TenantDerivationKey,
        cas_region: impl Into<String>,
        ac_region: impl Into<String>,
        max_object_bytes: u64,
        purge_lease: Duration,
        quarantine_after: u32,
    ) -> Result<Self, BackfillError> {
        let cas_region = checked_region(cas_region.into())?;
        let ac_region = checked_region(ac_region.into())?;
        if max_object_bytes == 0
            || purge_lease.is_zero()
            || purge_lease > MAX_PURGE_LEASE
            || quarantine_after == 0
        {
            return Err(BackfillError::InvalidRequest(
                "positive object bound, purge lease <=120s and quarantine threshold are required"
                    .to_owned(),
            ));
        }
        let purge_lease_ms = i64::try_from(purge_lease.as_millis()).map_err(|_| {
            BackfillError::InvalidRequest("purge lease does not fit D1 integer".to_owned())
        })?;
        Ok(Self {
            d1,
            cas,
            ac,
            encryptor,
            tdk,
            cas_region,
            ac_region,
            max_object_bytes,
            purge_lease_ms,
            operation_timeout: purge_lease.min(Duration::from_secs(20)),
            quarantine_after: i64::from(quarantine_after),
        })
    }

    fn client(&self, surface: BackfillSurface) -> &R {
        match surface {
            BackfillSurface::Cas => &self.cas,
            BackfillSurface::Ac => &self.ac,
        }
    }

    fn region(&self, surface: BackfillSurface) -> &str {
        match surface {
            BackfillSurface::Cas => &self.cas_region,
            BackfillSurface::Ac => &self.ac_region,
        }
    }

    fn tenant_prefix(&self, tenant_id: &str) -> Result<String, BackfillError> {
        let tenant = Uuid::parse_str(tenant_id).map_err(|_| {
            BackfillError::InvalidRequest(
                "activation tenant_id must be a canonical UUID".to_owned(),
            )
        })?;
        Ok(derive_prefix(&self.tdk, tenant).to_string())
    }

    fn run(&self, intent: &ActivationIntent, transition: &TransitionIdentity) -> BackfillRun {
        BackfillRun {
            tenant_id: intent.tenant_id.clone(),
            run_id: intent.intent_id.clone(),
            run_token: transition.token.clone(),
            transition_epoch: transition.epoch,
            gate_epoch: intent.observed_gate_epoch,
            source_generation: intent.source_generation,
            target_generation: intent.target_generation,
            phase: BackfillPhase::Copying,
            cas_cursor: None,
            ac_cursor: None,
            cas_complete: intent.cas_complete,
            ac_complete: intent.ac_complete,
        }
    }

    async fn validate_purge_physical(
        &self,
        intent: &ActivationIntent,
        item: &PhysicalPurgeItem,
    ) -> Result<(), BackfillError>
    where
        D: ActivationWorkerD1,
        R: ActivationR2,
        E: ByokBackfillEncryptor,
    {
        validate_logical(item.surface, &item.logical_key, false)?;
        let prefix = self.tenant_prefix(&item.tenant_id)?;
        let expected = if item.generation > 0 {
            let rows = self
                .d1
                .query(
                    "SELECT source_crypto_mode,source_config_version,source_cmk_provider,source_cmk_key_id,source_cmk_region,source_tcs_version FROM byok_activation_source_object WHERE intent_id=?1 AND tenant_id=?2 AND object_kind=?3 AND logical_key=?4 AND source_generation=?5 AND source_allocation_id=?6 AND source_physical_key=?7 LIMIT 1",
                    &[json!(intent.intent_id),json!(item.tenant_id),json!(item.surface.as_str()),json!(item.logical_key),json!(item.generation),json!(item.allocation_id),json!(item.physical_key)],
                )
                .await
                .map_err(store_error)?;
            let row = rows.first().ok_or_else(|| {
                BackfillError::Conflict("purge source custody identity is absent".to_owned())
            })?;
            let source = BackfillSourceObject {
                surface: item.surface,
                logical_key: item.logical_key.clone(),
                source_physical_key: item.physical_key.clone(),
                source_crypto: Some(SourceCryptoIdentity::Encrypted {
                    generation: item.generation,
                    allocation_id: item.allocation_id.clone().ok_or_else(|| {
                        BackfillError::Store("generation purge lacks allocation id".to_owned())
                    })?,
                    crypto_mode: text(row, "source_crypto_mode")?,
                    config_version: integer(row, "source_config_version")?,
                    cmk_provider: text(row, "source_cmk_provider")?,
                    cmk_key_id: text(row, "source_cmk_key_id")?,
                    cmk_region: text(row, "source_cmk_region")?,
                    tcs_version: integer(row, "source_tcs_version")?,
                }),
                source_blake3: None,
                plaintext_size: None,
            };
            let transition = self.transition_identity(intent).await?;
            let hardened = self
                .crypto(
                    "purge source digest hardening",
                    self.encryptor
                        .source_hardened_digest(&self.run(intent, &transition), &source),
                )
                .await?;
            generation_physical_key(
                item.surface,
                self.region(item.surface),
                &prefix,
                item.generation,
                item.allocation_id.as_deref().ok_or_else(|| {
                    BackfillError::Store("generation purge lacks allocation id".to_owned())
                })?,
                &item.logical_key,
                &hardened,
            )?
        } else {
            match item.surface {
                BackfillSurface::Cas => {
                    let (algorithm, digest) =
                        item.logical_key.split_once(':').ok_or_else(|| {
                            BackfillError::Store(
                                "legacy CAS purge identity is unqualified".to_owned(),
                            )
                        })?;
                    match algorithm {
                        "blake3" => format!("{}/{prefix}/{digest}", self.region(item.surface)),
                        "sha256" => format!(
                            "{}/{prefix}/bazel/sha256/{digest}",
                            self.region(item.surface)
                        ),
                        _ => {
                            return Err(BackfillError::Store(
                                "legacy CAS purge algorithm is invalid".to_owned(),
                            ))
                        }
                    }
                }
                BackfillSurface::Ac => format!(
                    "{}/{prefix}/{}",
                    self.region(item.surface),
                    item.logical_key
                ),
            }
        };
        if item.physical_key != expected {
            return Err(BackfillError::Conflict(
                "legacy purge physical key escaped the derived tenant/region namespace".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct TransitionIdentity {
    token: String,
    epoch: i64,
}

#[derive(Debug)]
struct CopyCandidate {
    source: ActivationSourceObject,
    source_stored_size: u64,
    cursor_key: String,
}

type ClaimedActivationPurge = (PhysicalPurgeItem, String, i64, String, String);
type SourceColumns = (
    i64,
    Option<String>,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

impl<D: ActivationWorkerD1, R: ActivationR2, E: ByokBackfillEncryptor>
    ProductionActivationObjectWorker<D, R, E>
{
    async fn io<T, F>(&self, label: &str, operation: F) -> Result<T, BackfillError>
    where
        F: Future<Output = Result<T, String>>,
    {
        tokio::time::timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| BackfillError::Store(format!("bounded {label} timed out")))?
            .map_err(store_error)
    }

    async fn crypto<T, F>(&self, label: &str, operation: F) -> Result<T, BackfillError>
    where
        F: Future<Output = Result<T, BackfillError>>,
    {
        tokio::time::timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| BackfillError::Crypto(format!("bounded {label} timed out")))?
    }

    async fn read_object(
        &self,
        surface: BackfillSurface,
        key: &str,
    ) -> Result<Vec<u8>, BackfillError> {
        match self
            .io(
                "R2 GET",
                self.client(surface).get_capped(key, self.max_object_bytes),
            )
            .await?
        {
            CappedGet::Found(bytes) => Ok(bytes),
            CappedGet::Missing => Err(BackfillError::SourceChanged(
                "exact R2 object disappeared".to_owned(),
            )),
            CappedGet::TooLarge { .. } => Err(BackfillError::SourceChanged(
                "R2 object exceeds bounded memory ceiling".to_owned(),
            )),
        }
    }
    async fn transition_identity(
        &self,
        intent: &ActivationIntent,
    ) -> Result<TransitionIdentity, BackfillError> {
        // Copy and purge both need the same live transition fence, but they
        // are distinct durable phases.  Derive the predicate from the
        // caller's claimed phase so a purge can never borrow copy authority,
        // and reject every phase where this identity is not meaningful.
        let phase = match intent.phase {
            ActivationPhase::Copy => "copy",
            ActivationPhase::Purging => "purging",
            _ => {
                return Err(BackfillError::Conflict(
                    "activation worker phase cannot use object transition identity".to_owned(),
                ))
            }
        };
        let claim = intent.claim.as_ref().ok_or_else(|| {
            BackfillError::Conflict("activation worker claim is absent".to_owned())
        })?;
        let sql = format!(
            "SELECT f.transition_token,f.transition_epoch FROM byok_activation_intent a \
             JOIN byok_activation_guard f ON f.guard_id=a.guard_id \
             JOIN byok_transition_fence tf ON tf.token=f.transition_token \
             WHERE a.intent_id=?1 AND a.tenant_id=?2 AND a.phase='{phase}' \
              AND a.claim_token=?3 AND a.claim_epoch=?4 AND a.claim_expires_at_ms>{NOW_MS} \
              AND a.state_version=?5 AND a.suspension_state='active' \
              AND f.outcome='active' AND f.expires_at_ms>{NOW_MS} \
              AND tf.tenant_id=a.tenant_id AND tf.epoch=f.transition_epoch \
              AND tf.outcome='active' AND tf.expires_at_ms>{NOW_MS} \
              AND EXISTS(SELECT 1 FROM byok_0121_capability c \
                         WHERE c.singleton=1 AND c.schema_version=2) LIMIT 1"
        );
        let rows = self
            .d1
            .query(
                &sql,
                &[
                    json!(intent.intent_id),
                    json!(intent.tenant_id),
                    json!(claim.token()),
                    json!(claim.epoch()),
                    json!(intent.state_version),
                ],
            )
            .await
            .map_err(store_error)?;
        let row = rows.first().ok_or_else(stale)?;
        Ok(TransitionIdentity {
            token: text(row, "transition_token")?,
            epoch: integer(row, "transition_epoch")?,
        })
    }

    async fn enumerate(
        &self,
        intent: &ActivationIntent,
        surface: BackfillSurface,
        limit: usize,
    ) -> Result<Vec<D1Row>, BackfillError> {
        let requested = i64::try_from(limit.saturating_add(1)).map_err(|_| {
            BackfillError::InvalidRequest("activation page limit does not fit D1".to_owned())
        })?;
        let cursor = match surface {
            BackfillSurface::Cas => "a.cas_after_logical_key",
            BackfillSurface::Ac => "a.ac_after_logical_key",
        };
        let sql = if intent.source_generation == 0 {
            let (table, key, live) = match surface {
                BackfillSurface::Cas => ("blob_meta", "digest", "m.deleted_at IS NULL"),
                BackfillSurface::Ac => (
                    "ac_meta",
                    "action_digest",
                    "(m.expires_at IS NULL OR m.expires_at > (CAST(strftime('%s','now') AS INTEGER)*1000))",
                ),
            };
            format!(
                "SELECT m.{key} logical_key FROM {table} m \
                 JOIN byok_activation_intent a ON a.intent_id=?1 AND a.tenant_id=m.tenant_id \
                 WHERE m.tenant_id=?2 AND {live} AND a.phase='copy' \
                  AND ?6 IS NULL AND ?7 IS NULL AND ?8 IS NULL AND ?9 IS NULL AND ?10 IS NULL \
                  AND (?3 IS NULL OR m.{key}>?3) ORDER BY m.{key} LIMIT ?4"
            )
        } else {
            let metadata = match surface {
                BackfillSurface::Cas => "EXISTS(SELECT 1 FROM blob_meta m WHERE m.tenant_id=p.tenant_id AND m.digest=substr(p.logical_key,instr(p.logical_key,':')+1) AND m.deleted_at IS NULL)",
                BackfillSurface::Ac => "EXISTS(SELECT 1 FROM ac_meta m WHERE m.tenant_id=p.tenant_id AND m.action_digest=p.logical_key AND (m.expires_at IS NULL OR m.expires_at>(CAST(strftime('%s','now') AS INTEGER)*1000)))",
            };
            format!(
                "SELECT p.logical_key,p.physical_key,p.allocation_id,g.size_bytes, \
                  g.ciphertext_size,g.ciphertext_blake3,a.source_config_version AS config_version, \
                  h.crypto_mode,a.source_cmk_provider AS cmk_provider, \
                  a.source_cmk_key_id AS cmk_key_id,a.source_cmk_region AS cmk_region, \
                  a.source_tcs_version AS tcs_version \
                 FROM byok_activation_intent a \
                 JOIN byok_logical_object_publication p ON p.tenant_id=a.tenant_id \
                  AND p.object_kind=?5 AND p.generation=a.source_generation \
                 JOIN byok_logical_object_generation g ON g.tenant_id=p.tenant_id \
                  AND g.object_kind=p.object_kind AND g.logical_key=p.logical_key \
                  AND g.generation=p.generation AND g.allocation_id=p.allocation_id \
                  AND g.physical_key=p.physical_key AND g.outcome='published' \
                 JOIN tenant_byok_config_history h ON h.tenant_id=a.tenant_id \
                  AND h.config_version=a.source_config_version \
                  AND h.cmk_provider=a.source_cmk_provider \
                  AND h.cmk_key_id=a.source_cmk_key_id \
                  AND h.cmk_region=a.source_cmk_region AND h.state IN ('active','partial') \
                 JOIN tenant_byok_secret_history s ON s.tenant_id=a.tenant_id \
                  AND s.tcs_version=a.source_tcs_version \
                  AND s.cmk_provider=a.source_cmk_provider \
                  AND s.cmk_key_id=a.source_cmk_key_id \
                  AND s.cmk_region=a.source_cmk_region AND s.retired_at_ms IS NULL \
                 WHERE a.intent_id=?1 AND a.tenant_id=?2 AND a.phase='copy' \
                  AND a.source_config_version=?6 AND a.source_cmk_provider=?7 \
                  AND a.source_cmk_key_id=?8 AND a.source_cmk_region=?9 \
                  AND a.source_tcs_version=?10 \
                  AND g.ciphertext_size IS NOT NULL AND g.ciphertext_blake3 IS NOT NULL \
                  AND {metadata} AND (?3 IS NULL OR p.logical_key>?3) \
                 ORDER BY p.logical_key LIMIT ?4"
            )
        };
        let after = self
            .d1
            .query(
                &format!("SELECT {cursor} cursor FROM byok_activation_intent a WHERE a.intent_id=?1 AND a.tenant_id=?2"),
                &[json!(intent.intent_id), json!(intent.tenant_id)],
            )
            .await
            .map_err(store_error)?
            .first()
            .and_then(|row| row.get("cursor"))
            .cloned()
            .unwrap_or(Value::Null);
        let source_identity = intent.source_identity.as_ref();
        self.d1
            .query(
                &sql,
                &[
                    json!(intent.intent_id),
                    json!(intent.tenant_id),
                    after,
                    json!(requested),
                    json!(surface.as_str()),
                    json!(source_identity.map(|source| source.config_version)),
                    json!(source_identity.map(|source| source.cmk_provider.as_str())),
                    json!(source_identity.map(|source| source.cmk_key_id.as_str())),
                    json!(source_identity.map(|source| source.cmk_region.as_str())),
                    json!(source_identity.map(|source| source.tcs_version)),
                ],
            )
            .await
            .map_err(store_error)
    }

    async fn materialize_candidate(
        &self,
        intent: &ActivationIntent,
        transition: &TransitionIdentity,
        surface: BackfillSurface,
        row: &D1Row,
    ) -> Result<CopyCandidate, BackfillError> {
        let logical = text(row, "logical_key")?;
        let cursor_key = logical.clone();
        validate_logical(surface, &logical, intent.source_generation == 0)?;
        if intent.source_generation == 0 {
            let prefix = self.tenant_prefix(&intent.tenant_id)?;
            let raw_key = format!("{}/{prefix}/{logical}", self.region(surface));
            let (logical_key, physical_key) = if surface == BackfillSurface::Cas {
                let sha_key = format!("{}/{prefix}/bazel/sha256/{logical}", self.region(surface));
                let native = self
                    .io("legacy CAS HEAD", self.client(surface).head_size(&raw_key))
                    .await?;
                let sha = self
                    .io(
                        "legacy SHA CAS HEAD",
                        self.client(surface).head_size(&sha_key),
                    )
                    .await?;
                match (native, sha) {
                    (Some(_), None) => (format!("blake3:{logical}"), raw_key),
                    (None, Some(_)) => (format!("sha256:{logical}"), sha_key),
                    _ => {
                        return Err(BackfillError::SourceChanged(
                            "legacy CAS object is absent or algorithm-ambiguous".to_owned(),
                        ))
                    }
                }
            } else {
                (logical, raw_key)
            };
            let ledger = self
                .d1
                .query(
                    "SELECT source_physical_key FROM byok_activation_source_object \
                     WHERE intent_id=?1 AND tenant_id=?2 AND object_kind=?3 \
                       AND logical_key=?4 LIMIT 1",
                    &[
                        json!(intent.intent_id),
                        json!(intent.tenant_id),
                        json!(surface.as_str()),
                        json!(logical_key),
                    ],
                )
                .await
                .map_err(store_error)?;
            if ledger.first().is_some_and(|row| {
                row.get("source_physical_key").and_then(Value::as_str)
                    != Some(physical_key.as_str())
            }) {
                return Err(BackfillError::Conflict(
                    "legacy source ledger differs from derived tenant/region path".to_owned(),
                ));
            }
            let bytes = Zeroizing::new(self.read_object(surface, &physical_key).await?);
            let size = u64::try_from(bytes.len())
                .map_err(|_| BackfillError::Store("source size overflow".to_owned()))?;
            Ok(CopyCandidate {
                source: ActivationSourceObject {
                    surface,
                    logical_key,
                    physical_key,
                    plaintext_size: size,
                    source_blake3: blake3::hash(&bytes).to_hex().to_string(),
                    crypto: SourceCryptoIdentity::Plaintext,
                },
                source_stored_size: size,
                cursor_key,
            })
        } else {
            let crypto_mode = text(row, "crypto_mode")?;
            let tcs_version = integer(row, "tcs_version")?;
            let stored_size = unsigned(row, "ciphertext_size")?;
            let physical_key = text(row, "physical_key")?;
            let allocation_id = text(row, "allocation_id")?;
            let plaintext_size = unsigned(row, "size_bytes")?;
            let source_blake3 = text(row, "ciphertext_blake3")?;
            let crypto = SourceCryptoIdentity::Encrypted {
                generation: intent.source_generation,
                allocation_id: allocation_id.clone(),
                crypto_mode,
                config_version: integer(row, "config_version")?,
                cmk_provider: text(row, "cmk_provider")?,
                cmk_key_id: text(row, "cmk_key_id")?,
                cmk_region: text(row, "cmk_region")?,
                tcs_version,
            };
            let source_for_hardening = BackfillSourceObject {
                surface,
                logical_key: logical.clone(),
                source_physical_key: physical_key.clone(),
                source_crypto: Some(crypto.clone()),
                source_blake3: Some(source_blake3.clone()),
                plaintext_size: Some(plaintext_size),
            };
            let hardened = self
                .crypto(
                    "source digest hardening",
                    self.encryptor.source_hardened_digest(
                        &self.run(intent, transition),
                        &source_for_hardening,
                    ),
                )
                .await?;
            let expected = generation_physical_key(
                surface,
                self.region(surface),
                &self.tenant_prefix(&intent.tenant_id)?,
                intent.source_generation,
                &allocation_id,
                &logical,
                &hardened,
            )?;
            if physical_key != expected {
                return Err(BackfillError::Conflict(
                    "published source physical key escaped the derived tenant/region namespace"
                        .to_owned(),
                ));
            }
            Ok(CopyCandidate {
                source: ActivationSourceObject {
                    surface,
                    logical_key: logical,
                    physical_key,
                    plaintext_size,
                    source_blake3,
                    crypto,
                },
                source_stored_size: stored_size,
                cursor_key,
            })
        }
    }

    async fn reserve_copy(
        &self,
        intent: &ActivationIntent,
        transition: &TransitionIdentity,
        candidate: &CopyCandidate,
        target_key: &str,
        allocation_id: &str,
        expected_version: i64,
    ) -> Result<(i64, String), BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let source = &candidate.source;
        let (
            source_generation,
            source_allocation,
            source_mode,
            source_config,
            source_provider,
            source_key,
            source_region,
            source_tcs,
        ) = source_columns(&source.crypto);
        let plaintext_size = i64::try_from(source.plaintext_size)
            .map_err(|_| BackfillError::Store("plaintext size overflow".to_owned()))?;
        let stored_size = i64::try_from(candidate.source_stored_size)
            .map_err(|_| BackfillError::Store("source stored size overflow".to_owned()))?;
        let surface = source.surface.as_str();
        let operation_token = Uuid::new_v4().to_string();
        let write_token = Uuid::new_v4().to_string();
        let write_lease_ms = i64::try_from(TARGET_WRITE_LEASE.as_millis()).unwrap_or(i64::MAX);
        let params = vec![
            json!(intent.intent_id),
            json!(intent.tenant_id),
            json!(claim.token()),
            json!(claim.epoch()),
            json!(expected_version),
            json!(surface),
            json!(source.logical_key),
            json!(source_generation),
            json!(source_allocation),
            json!(source.physical_key),
            json!(source_mode),
            json!(source_config),
            json!(source_provider),
            json!(source_key),
            json!(source_region),
            json!(source_tcs),
            json!(plaintext_size),
            json!(stored_size),
            json!(source.source_blake3),
            json!(allocation_id),
            json!(target_key),
            json!(intent.target_generation),
            json!(transition.token),
            json!(intent.observed_gate_epoch),
            json!(operation_token),
            json!(write_token),
            json!(write_lease_ms),
        ];
        let guard = format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) SELECT ?25,?1,?3,NULL,?5,'checkpoint',{NOW_MS} WHERE ?27>0 RETURNING operation_token");
        let source_insert = format!("INSERT OR IGNORE INTO byok_activation_source_object (intent_id,tenant_id,object_kind,logical_key,source_generation,source_allocation_id,source_physical_key,source_crypto_mode,source_config_version,source_cmk_provider,source_cmk_key_id,source_cmk_region,source_tcs_version,plaintext_size,source_stored_size,source_blake3,target_allocation_id,target_physical_key,copy_state,discovered_at_ms,copied_at_ms) SELECT ?1,?2,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,NULL,NULL,'discovered',{NOW_MS},NULL WHERE EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?25) AND ?26<>'' AND ?27>0");
        let live = "EXISTS(SELECT 1 FROM byok_activation_operation_guard og WHERE og.operation_token=?25 AND og.intent_id=?1 AND og.action='checkpoint' AND og.expected_state_version=?5) AND ?26<>'' AND ?27>0".to_owned();
        let target_insert = format!("INSERT OR IGNORE INTO byok_logical_object_generation (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,intent_token,gate_epoch,size_bytes,outcome,allocated_at_ms,backfill_run_id,ciphertext_size,ciphertext_blake3) SELECT ?2,?6,?7,?22,?20,?21,?23,?24,?17,'allocated',{NOW_MS},?1,NULL,NULL WHERE {live}");
        let stage_source = format!("UPDATE byok_activation_source_object SET target_allocation_id=?20,target_physical_key=?21,copy_state='staged',target_write_token=?26,target_write_expires_at_ms={NOW_MS}+?27 WHERE intent_id=?1 AND tenant_id=?2 AND object_kind=?6 AND logical_key=?7 AND source_physical_key=?10 AND source_blake3=?19 AND copy_state IN ('discovered','staged') AND (target_write_token IS NULL OR target_write_token=?26 OR target_write_expires_at_ms<={NOW_MS}) AND {live} RETURNING logical_key");
        let bump = format!("UPDATE byok_activation_intent SET state_version=state_version+1,checkpointed_at_ms={NOW_MS} WHERE intent_id=?1 AND tenant_id=?2 AND phase='copy' AND claim_token=?3 AND claim_epoch=?4 AND claim_expires_at_ms>{NOW_MS} AND state_version=?5 AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?25) AND EXISTS(SELECT 1 FROM byok_activation_source_object s JOIN byok_logical_object_generation g ON g.tenant_id=s.tenant_id AND g.object_kind=s.object_kind AND g.logical_key=s.logical_key AND g.generation=?22 AND g.allocation_id=s.target_allocation_id AND g.physical_key=s.target_physical_key AND g.backfill_run_id=s.intent_id WHERE s.intent_id=?1 AND s.object_kind=?6 AND s.logical_key=?7 AND s.copy_state='staged') RETURNING state_version");
        let assertion=format!("INSERT INTO byok_activation_worker_assertion (assertion_token,operation_token,intent_id,expected_state_version,assertion_kind,object_kind,logical_key,checkpoint_key,purge_id,checked_at_ms) SELECT ?25,?25,?1,?5,'reserve',?6,?7,NULL,NULL,{NOW_MS} WHERE ?26<>'' AND ?27>0 RETURNING assertion_token");
        let results = self
            .d1
            .batch(vec![
                WorkerStatement::new(guard, params.clone()),
                WorkerStatement::new(source_insert, params.clone()),
                WorkerStatement::new(target_insert, params.clone()),
                WorkerStatement::new(stage_source, params.clone()),
                WorkerStatement::new(bump, params.clone()),
                WorkerStatement::new(assertion, params),
            ])
            .await
            .map_err(store_error)?;
        let version = results
            .get(4)
            .and_then(|rows| rows.first())
            .map(|row| integer(row, "state_version"))
            .transpose()?
            .ok_or_else(stale)?;
        Ok((version, write_token))
    }

    async fn revalidate_put(
        &self,
        intent: &ActivationIntent,
        candidate: &CopyCandidate,
        write_token: &str,
    ) -> Result<(), BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let sql=format!("SELECT 1 ok FROM byok_activation_source_object s JOIN byok_activation_intent a ON a.intent_id=s.intent_id JOIN byok_activation_guard f ON f.guard_id=a.guard_id JOIN byok_transition_fence tf ON tf.token=f.transition_token WHERE s.intent_id=?1 AND s.tenant_id=?2 AND s.object_kind=?3 AND s.logical_key=?4 AND s.target_write_token=?5 AND s.target_write_expires_at_ms>={NOW_MS}+?9 AND a.phase='copy' AND a.claim_token=?6 AND a.claim_epoch=?7 AND a.claim_expires_at_ms>={NOW_MS}+?9 AND a.state_version=?8 AND f.outcome='active' AND f.expires_at_ms>={NOW_MS}+?9 AND tf.epoch=f.transition_epoch AND tf.outcome='active' AND tf.expires_at_ms>={NOW_MS}+?9 LIMIT 1");
        let rows =
            self.d1
                .query(
                    &sql,
                    &[
                        json!(intent.intent_id),
                        json!(intent.tenant_id),
                        json!(candidate.source.surface.as_str()),
                        json!(candidate.source.logical_key),
                        json!(write_token),
                        json!(claim.token()),
                        json!(claim.epoch()),
                        json!(intent.state_version),
                        json!(i64::try_from(TARGET_WRITE_MIN_REMAINING.as_millis())
                            .unwrap_or(i64::MAX)),
                    ],
                )
                .await
                .map_err(store_error)?;
        if rows.is_empty() {
            Err(stale())
        } else {
            Ok(())
        }
    }

    async fn checkpoint_copy(
        &self,
        intent: &ActivationIntent,
        candidate: &CopyCandidate,
        ciphertext: &[u8],
        write_token: &str,
        complete: bool,
        expected_version: i64,
    ) -> Result<(i64, bool, bool), BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let source = &candidate.source;
        let allocation = allocation_id(intent, source.surface, &source.logical_key);
        let ciphertext_size = i64::try_from(ciphertext.len())
            .map_err(|_| BackfillError::Store("target size overflow".to_owned()))?;
        let ciphertext_hash = blake3::hash(ciphertext).to_hex().to_string();
        let operation = Uuid::new_v4().to_string();
        let params = vec![
            json!(intent.intent_id),
            json!(intent.tenant_id),
            json!(claim.token()),
            json!(claim.epoch()),
            json!(expected_version),
            json!(source.surface.as_str()),
            json!(source.logical_key),
            json!(allocation),
            json!(ciphertext_size),
            json!(ciphertext_hash),
            json!(i64::from(complete)),
            json!(operation),
            json!(write_token),
            json!(candidate.cursor_key),
        ];
        let guard=format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) SELECT ?12,?1,?3,NULL,?5,'checkpoint',{NOW_MS} WHERE ?13<>'' AND ?14<>'' RETURNING operation_token");
        let generation="UPDATE byok_logical_object_generation SET ciphertext_size=?9,ciphertext_blake3=?10 WHERE tenant_id=?2 AND object_kind=?6 AND logical_key=?7 AND generation=(SELECT target_generation FROM byok_activation_intent WHERE intent_id=?1) AND allocation_id=?8 AND backfill_run_id=?1 AND outcome='allocated' AND (ciphertext_size IS NULL OR (ciphertext_size=?9 AND ciphertext_blake3=?10)) AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?12) AND ?14<>'' RETURNING allocation_id".to_owned();
        let source_update=format!("UPDATE byok_activation_source_object SET copied_at_ms=COALESCE(copied_at_ms,{NOW_MS}),target_write_token=NULL,target_write_expires_at_ms=NULL WHERE intent_id=?1 AND tenant_id=?2 AND object_kind=?6 AND logical_key=?7 AND target_allocation_id=?8 AND target_write_token=?13 AND copy_state='staged' AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?12) AND ?14<>'' AND EXISTS(SELECT 1 FROM byok_logical_object_generation g WHERE g.tenant_id=?2 AND g.object_kind=?6 AND g.logical_key=?7 AND g.allocation_id=?8 AND g.backfill_run_id=?1 AND g.ciphertext_size=?9 AND g.ciphertext_blake3=?10) RETURNING logical_key");
        let (cursor, done) = match source.surface {
            BackfillSurface::Cas => ("cas_after_logical_key", "cas_complete"),
            BackfillSurface::Ac => ("ac_after_logical_key", "ac_complete"),
        };
        let advance=format!("UPDATE byok_activation_intent SET {cursor}=?14,{done}=MAX({done},?11),state_version=state_version+1,checkpointed_at_ms={NOW_MS} WHERE intent_id=?1 AND tenant_id=?2 AND phase='copy' AND claim_token=?3 AND claim_epoch=?4 AND claim_expires_at_ms>{NOW_MS} AND state_version=?5 AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?12) AND EXISTS(SELECT 1 FROM byok_activation_source_object s WHERE s.intent_id=?1 AND s.object_kind=?6 AND s.logical_key=?7 AND s.target_allocation_id=?8 AND s.target_write_token IS NULL AND s.target_write_expires_at_ms IS NULL AND s.copy_state='staged' AND s.copied_at_ms IS NOT NULL) AND ?13<>'' RETURNING state_version,cas_complete,ac_complete");
        let assertion=format!("INSERT INTO byok_activation_worker_assertion (assertion_token,operation_token,intent_id,expected_state_version,assertion_kind,object_kind,logical_key,checkpoint_key,purge_id,checked_at_ms) VALUES (?12,?12,?1,?5,'receipt',?6,?7,?14,NULL,{NOW_MS}) RETURNING assertion_token");
        let results = self
            .d1
            .batch(vec![
                WorkerStatement::new(guard, params.clone()),
                WorkerStatement::new(generation, params.clone()),
                WorkerStatement::new(source_update, params.clone()),
                WorkerStatement::new(advance, params.clone()),
                WorkerStatement::new(assertion, params),
            ])
            .await
            .map_err(store_error)?;
        let row = results
            .get(3)
            .and_then(|rows| rows.first())
            .ok_or_else(stale)?;
        Ok((
            integer(row, "state_version")?,
            boolean(row, "cas_complete")?,
            boolean(row, "ac_complete")?,
        ))
    }

    async fn claim_purge(
        &self,
        intent: &ActivationIntent,
    ) -> Result<Option<ClaimedActivationPurge>, BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let token = Uuid::new_v4().to_string();
        let operation = Uuid::new_v4().to_string();
        let guard=format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) VALUES (?9,?1,?7,NULL,?8,'purge',{NOW_MS}) RETURNING operation_token");
        let sql = format!("UPDATE byok_object_purge_item SET state=CASE WHEN state='r2_absent' THEN 'r2_absent' ELSE 'deleting' END,claim_owner=?3,claim_token=?4,claim_epoch=claim_epoch+1,claim_expires_at_ms={NOW_MS}+?5,attempts=attempts+1,last_error=NULL WHERE purge_id=(SELECT p.purge_id FROM byok_object_purge_item p JOIN byok_object_purge_cause c ON c.purge_id=p.purge_id JOIN byok_activation_intent a ON a.intent_id=c.cause_id JOIN byok_activation_guard f ON f.guard_id=a.guard_id JOIN byok_transition_fence tf ON tf.token=f.transition_token WHERE c.cause_kind='activation_source' AND a.intent_id=?1 AND a.tenant_id=?2 AND a.phase='purging' AND a.claim_token=?7 AND a.claim_epoch=?6 AND a.claim_expires_at_ms>{NOW_MS} AND a.state_version=?8 AND a.suspension_state='active' AND f.outcome='active' AND f.expires_at_ms>{NOW_MS} AND tf.epoch=f.transition_epoch AND tf.outcome='active' AND tf.expires_at_ms>{NOW_MS} AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?9) AND ((p.state IN ('pending','retry') AND p.next_attempt_at_ms<={NOW_MS} AND p.claim_token IS NULL) OR (p.state IN ('deleting','r2_absent') AND p.claim_expires_at_ms<={NOW_MS})) ORDER BY p.purge_id LIMIT 1) RETURNING purge_id,tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,state,claim_epoch");
        let params = vec![
            json!(intent.intent_id),
            json!(intent.tenant_id),
            json!(claim.owner()),
            json!(token),
            json!(self.purge_lease_ms),
            json!(claim.epoch()),
            json!(claim.token()),
            json!(intent.state_version),
            json!(operation),
        ];
        let results = self
            .d1
            .batch(vec![
                WorkerStatement::new(guard, params.clone()),
                WorkerStatement::new(sql, params),
            ])
            .await
            .map_err(store_error)?;
        let rows = results.get(1).ok_or_else(stale)?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let surface = parse_surface(&text(row, "object_kind")?)?;
        Ok(Some((
            PhysicalPurgeItem {
                purge_id: text(row, "purge_id")?,
                tenant_id: text(row, "tenant_id")?,
                surface,
                logical_key: text(row, "logical_key")?,
                generation: integer(row, "generation")?,
                allocation_id: optional_text(row, "allocation_id"),
                physical_key: text(row, "physical_key")?,
            },
            token,
            integer(row, "claim_epoch")?,
            text(row, "crypto_mode")?,
            text(row, "state")?,
        )))
    }

    async fn fail_purge(
        &self,
        intent: &ActivationIntent,
        item: &PhysicalPurgeItem,
        purge_token: &str,
        purge_epoch: i64,
        error: &str,
        expected_version: i64,
    ) -> Result<i64, BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let error = sanitize(error);
        let operation = Uuid::new_v4().to_string();
        let guard=format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) VALUES (?11,?5,?9,NULL,?6,'purge',{NOW_MS}) RETURNING operation_token");
        let sql = format!("UPDATE byok_object_purge_item SET state=CASE WHEN attempts>=?8 THEN 'quarantined' ELSE 'retry' END,claim_owner=NULL,claim_token=NULL,claim_epoch=claim_epoch+1,claim_expires_at_ms=NULL,next_attempt_at_ms={NOW_MS}+MIN(300000,1000*(1 << MIN(attempts,8))),verified_at_ms=CASE WHEN attempts>=?8 THEN {NOW_MS} ELSE NULL END,last_error=?7 WHERE purge_id=?1 AND tenant_id=?2 AND claim_token=?3 AND claim_epoch=?4 AND state='deleting' AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) RETURNING purge_id");
        let bump = format!("UPDATE byok_activation_intent SET state_version=state_version+1,checkpointed_at_ms={NOW_MS} WHERE intent_id=?5 AND tenant_id=?2 AND phase='purging' AND claim_token=?9 AND claim_epoch=?10 AND claim_expires_at_ms>{NOW_MS} AND state_version=?6 AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) AND EXISTS(SELECT 1 FROM byok_object_purge_item p WHERE p.purge_id=?1 AND p.claim_token IS NULL AND p.state IN ('retry','quarantined')) RETURNING state_version");
        let params = vec![
            json!(item.purge_id),
            json!(item.tenant_id),
            json!(purge_token),
            json!(purge_epoch),
            json!(intent.intent_id),
            json!(expected_version),
            json!(error),
            json!(self.quarantine_after),
            json!(claim.token()),
            json!(claim.epoch()),
            json!(operation),
        ];
        let assertion=format!("INSERT INTO byok_activation_worker_assertion (assertion_token,operation_token,intent_id,expected_state_version,assertion_kind,object_kind,logical_key,checkpoint_key,purge_id,checked_at_ms) VALUES (?11,?11,?5,?6,'purge_failed',NULL,NULL,NULL,?1,{NOW_MS}) RETURNING assertion_token");
        let results = self
            .d1
            .batch(vec![
                WorkerStatement::new(guard, params.clone()),
                WorkerStatement::new(sql, params.clone()),
                WorkerStatement::new(bump, params.clone()),
                WorkerStatement::new(assertion, params),
            ])
            .await
            .map_err(store_error)?;
        results
            .get(2)
            .and_then(|rows| rows.first())
            .map(|row| integer(row, "state_version"))
            .transpose()?
            .ok_or_else(stale)
    }

    async fn verify_purge(
        &self,
        intent: &ActivationIntent,
        item: &PhysicalPurgeItem,
        purge_token: &str,
        purge_epoch: i64,
        crypto_mode: &str,
        expected_version: i64,
    ) -> Result<i64, BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(stale)?;
        let envelope = if crypto_mode == "random" {
            Some(allocation_envelope_key(item)?)
        } else {
            None
        };
        let operation = Uuid::new_v4().to_string();
        let guard=format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) VALUES (?11,?5,?9,NULL,?6,'purge',{NOW_MS}) RETURNING operation_token");
        let delete_envelope = if envelope.is_some() {
            "DELETE FROM byok_envelope WHERE tenant_id=?2 AND blob_hash=?7 AND ?11<>''"
        } else {
            "SELECT 1 WHERE ?11<>''"
        };
        let absent = "UPDATE byok_object_purge_item SET state='r2_absent' WHERE purge_id=?1 AND tenant_id=?2 AND claim_token=?3 AND claim_epoch=?4 AND state IN ('deleting','r2_absent') AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) RETURNING purge_id";
        let verified = format!("UPDATE byok_object_purge_item SET state='verified',claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL,verified_at_ms={NOW_MS},last_error=NULL WHERE purge_id=?1 AND tenant_id=?2 AND claim_token=?3 AND claim_epoch=?4 AND state='r2_absent' AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) AND (?7 IS NULL OR NOT EXISTS(SELECT 1 FROM byok_envelope e WHERE e.tenant_id=?2 AND e.blob_hash=?7)) RETURNING purge_id");
        let source = "UPDATE byok_activation_source_object SET copy_state='purged' WHERE intent_id=?5 AND tenant_id=?2 AND source_physical_key=?8 AND copy_state='purge_queued' AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) AND EXISTS(SELECT 1 FROM byok_object_purge_item p JOIN byok_object_purge_cause c ON c.purge_id=p.purge_id WHERE p.purge_id=?1 AND p.state='verified' AND c.cause_kind='activation_source' AND c.cause_id=?5) RETURNING logical_key".to_owned();
        let bump = format!("UPDATE byok_activation_intent SET state_version=state_version+1,checkpointed_at_ms={NOW_MS} WHERE intent_id=?5 AND tenant_id=?2 AND phase='purging' AND claim_token=?9 AND claim_epoch=?10 AND claim_expires_at_ms>{NOW_MS} AND state_version=?6 AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?11) AND EXISTS(SELECT 1 FROM byok_object_purge_item p WHERE p.purge_id=?1 AND p.state='verified') RETURNING state_version");
        let params = vec![
            json!(item.purge_id),
            json!(item.tenant_id),
            json!(purge_token),
            json!(purge_epoch),
            json!(intent.intent_id),
            json!(expected_version),
            json!(envelope),
            json!(item.physical_key),
            json!(claim.token()),
            json!(claim.epoch()),
            json!(operation),
        ];
        let assertion=format!("INSERT INTO byok_activation_worker_assertion (assertion_token,operation_token,intent_id,expected_state_version,assertion_kind,object_kind,logical_key,checkpoint_key,purge_id,checked_at_ms) VALUES (?11,?11,?5,?6,'purge_verified',NULL,NULL,NULL,?1,{NOW_MS}) RETURNING assertion_token");
        let results = self
            .d1
            .batch(vec![
                WorkerStatement::new(guard, params.clone()),
                WorkerStatement::new(absent, params.clone()),
                WorkerStatement::new(delete_envelope, params.clone()),
                WorkerStatement::new(verified, params.clone()),
                WorkerStatement::new(source, params.clone()),
                WorkerStatement::new(bump, params.clone()),
                WorkerStatement::new(assertion, params),
            ])
            .await
            .map_err(store_error)?;
        results
            .get(5)
            .and_then(|rows| rows.first())
            .map(|row| integer(row, "state_version"))
            .transpose()?
            .ok_or_else(stale)
    }
}

#[async_trait]
impl<D, R, E> ActivationObjectWorker for ProductionActivationObjectWorker<D, R, E>
where
    D: ActivationWorkerD1,
    R: ActivationR2,
    E: ByokBackfillEncryptor + fmt::Debug,
{
    async fn copy_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError> {
        if limit == 0 {
            return Err(BackfillError::InvalidRequest(
                "activation page limit must be positive".to_owned(),
            ));
        }
        let transition = self.transition_identity(intent).await?;
        let surface = if !intent.cas_complete {
            BackfillSurface::Cas
        } else {
            BackfillSurface::Ac
        };
        let rows = self.enumerate(intent, surface, limit).await?;
        let has_more = rows.len() > limit;
        let mut next = intent.clone();
        let mut processed = 0usize;
        for (index, row) in rows.iter().take(limit).enumerate() {
            let candidate = self
                .materialize_candidate(&next, &transition, surface, row)
                .await?;
            let stored = Zeroizing::new(
                self.read_object(surface, &candidate.source.physical_key)
                    .await?,
            );
            if u64::try_from(stored.len()).ok() != Some(candidate.source_stored_size)
                || blake3::hash(&stored).to_hex().as_str() != candidate.source.source_blake3
            {
                return Err(BackfillError::SourceChanged(
                    "source bytes differ from exact D1 ledger".to_owned(),
                ));
            }
            let source = BackfillSourceObject {
                surface,
                logical_key: candidate.source.logical_key.clone(),
                source_physical_key: candidate.source.physical_key.clone(),
                source_crypto: Some(candidate.source.crypto.clone()),
                source_blake3: Some(candidate.source.source_blake3.clone()),
                plaintext_size: Some(candidate.source.plaintext_size),
            };
            let allocation_id = allocation_id(&next, surface, &source.logical_key);
            let run = self.run(&next, &transition);
            let hardened = self
                .crypto(
                    "target digest hardening",
                    self.encryptor.target_hardened_digest(
                        &run,
                        surface,
                        &source.logical_key,
                        next.policy.config_version,
                        next.policy.tcs_version,
                    ),
                )
                .await?;
            let target_key = generation_physical_key(
                surface,
                self.region(surface),
                &self.tenant_prefix(&next.tenant_id)?,
                next.target_generation,
                &allocation_id,
                &source.logical_key,
                &hardened,
            )?;
            // This reservation is the crash/preemption hand-off. It must commit
            // before Mode-B encryption can create an envelope or R2 can receive bytes.
            let (reserved_version, write_token) = self
                .reserve_copy(
                    &next,
                    &transition,
                    &candidate,
                    &target_key,
                    &allocation_id,
                    next.state_version,
                )
                .await?;
            next.state_version = reserved_version;
            let run = self.run(&next, &transition);
            let plaintext = Zeroizing::new(
                self.crypto(
                    "source decrypt",
                    self.encryptor.decrypt_source(&run, &source, &stored),
                )
                .await?,
            );
            let ciphertext = self
                .crypto(
                    "target encrypt",
                    self.encryptor
                        .encrypt(&run, &source, &allocation_id, &plaintext),
                )
                .await?;
            if u64::try_from(ciphertext.len()).map_or(true, |size| size > self.max_object_bytes) {
                return Err(BackfillError::SourceChanged(
                    "target ciphertext exceeds activation memory ceiling".to_owned(),
                ));
            }
            self.revalidate_put(&next, &candidate, &write_token).await?;
            let created = self
                .io(
                    "R2 conditional PUT",
                    self.client(surface)
                        .put_if_absent(&target_key, ciphertext.clone()),
                )
                .await?;
            if !created {
                let existing = self.read_object(surface, &target_key).await?;
                if existing != ciphertext {
                    return Err(BackfillError::Conflict(
                        "activation target key contains unequal ciphertext".to_owned(),
                    ));
                }
            }
            let complete = !has_more && index + 1 == rows.len().min(limit);
            let (version, cas_complete, ac_complete) = self
                .checkpoint_copy(
                    &next,
                    &candidate,
                    &ciphertext,
                    &write_token,
                    complete,
                    next.state_version,
                )
                .await?;
            next.state_version = version;
            next.cas_complete = cas_complete;
            next.ac_complete = ac_complete;
            processed += 1;
        }
        if rows.is_empty() {
            let claim = next.claim.as_ref().ok_or_else(stale)?;
            let done = match surface {
                BackfillSurface::Cas => "cas_complete",
                BackfillSurface::Ac => "ac_complete",
            };
            let operation = Uuid::new_v4().to_string();
            let params = vec![
                json!(next.intent_id),
                json!(next.tenant_id),
                json!(claim.token()),
                json!(claim.epoch()),
                json!(next.state_version),
                json!(operation),
                json!(surface.as_str()),
            ];
            let guard=format!("INSERT INTO byok_activation_operation_guard (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) SELECT ?6,?1,?3,NULL,?5,'checkpoint',{NOW_MS} WHERE ?7<>'' RETURNING operation_token");
            let sql=format!("UPDATE byok_activation_intent SET {done}=1,state_version=state_version+1,checkpointed_at_ms={NOW_MS} WHERE intent_id=?1 AND tenant_id=?2 AND phase='copy' AND claim_token=?3 AND claim_epoch=?4 AND claim_expires_at_ms>{NOW_MS} AND state_version=?5 AND EXISTS(SELECT 1 FROM byok_activation_operation_guard WHERE operation_token=?6) AND ?7<>'' RETURNING state_version");
            let assertion=format!("INSERT INTO byok_activation_worker_assertion (assertion_token,operation_token,intent_id,expected_state_version,assertion_kind,object_kind,logical_key,checkpoint_key,purge_id,checked_at_ms) SELECT ?6,?6,?1,?5,'surface_complete',?7,NULL,NULL,NULL,{NOW_MS} WHERE ?7<>'' RETURNING assertion_token");
            let results = self
                .d1
                .batch(vec![
                    WorkerStatement::new(guard, params.clone()),
                    WorkerStatement::new(sql, params.clone()),
                    WorkerStatement::new(assertion, params),
                ])
                .await
                .map_err(store_error)?;
            next.state_version = integer(
                results
                    .get(1)
                    .and_then(|rows| rows.first())
                    .ok_or_else(stale)?,
                "state_version",
            )?;
            match surface {
                BackfillSurface::Cas => next.cas_complete = true,
                BackfillSurface::Ac => next.ac_complete = true,
            }
        }
        Ok(ActivationStep {
            intent: next,
            processed,
        })
    }

    async fn purge_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError> {
        if limit == 0 {
            return Err(BackfillError::InvalidRequest(
                "purge page limit must be positive".to_owned(),
            ));
        }
        let mut next = intent.clone();
        let mut attempted = 0usize;
        while attempted < limit {
            let Some((item, token, epoch, mode, purge_state)) = self.claim_purge(&next).await?
            else {
                break;
            };
            attempted += 1;
            self.validate_purge_physical(&next, &item).await?;
            let operation = if purge_state == "r2_absent" {
                Ok(())
            } else {
                let delete = self
                    .io(
                        "R2 DELETE",
                        self.client(item.surface).delete(&item.physical_key),
                    )
                    .await;
                // HEAD is mandatory even when DELETE returned an error: a lost
                // response may still have removed the object.
                let head = self
                    .io(
                        "mandatory post-DELETE R2 HEAD",
                        self.client(item.surface).head_size(&item.physical_key),
                    )
                    .await;
                match head {
                    Ok(None) => Ok(()),
                    Ok(Some(_)) => Err(BackfillError::Conflict(match delete {
                        Ok(()) => "R2 object still exists after DELETE".to_owned(),
                        Err(error) => format!("DELETE failed and object remains: {error}"),
                    })),
                    Err(error) => Err(error),
                }
            };
            next.state_version = match operation {
                Ok(()) => {
                    self.verify_purge(&next, &item, &token, epoch, &mode, next.state_version)
                        .await?
                }
                Err(error) => {
                    self.fail_purge(
                        &next,
                        &item,
                        &token,
                        epoch,
                        &error.to_string(),
                        next.state_version,
                    )
                    .await?
                }
            };
        }
        Ok(ActivationStep {
            intent: next,
            processed: attempted,
        })
    }
}

fn source_columns(crypto: &SourceCryptoIdentity) -> SourceColumns {
    match crypto {
        SourceCryptoIdentity::Plaintext => (
            0,
            None,
            "plaintext".to_owned(),
            None,
            None,
            None,
            None,
            None,
        ),
        SourceCryptoIdentity::Encrypted {
            generation,
            allocation_id,
            crypto_mode,
            config_version,
            cmk_key_id,
            cmk_provider,
            cmk_region,
            tcs_version,
        } => (
            *generation,
            Some(allocation_id.clone()),
            crypto_mode.clone(),
            Some(*config_version),
            Some(cmk_provider.clone()),
            Some(cmk_key_id.clone()),
            Some(cmk_region.clone()),
            Some(*tcs_version),
        ),
    }
}
fn allocation_id(intent: &ActivationIntent, surface: BackfillSurface, logical: &str) -> String {
    blake3::hash(
        format!(
            "{}\0{}\0{}\0{}\0{}",
            intent.tenant_id,
            intent.intent_id,
            intent.target_generation,
            surface.as_str(),
            logical
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string()
}

fn generation_physical_key(
    surface: BackfillSurface,
    region: &str,
    tenant_prefix: &str,
    generation: i64,
    allocation_id: &str,
    logical_key: &str,
    hardened_digest: &str,
) -> Result<String, BackfillError> {
    if hardened_digest.len() != 64
        || !hardened_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(BackfillError::Crypto(
            "hardened storage digest must be lowercase 64-hex".to_owned(),
        ));
    }
    let algorithm = match surface {
        BackfillSurface::Cas => {
            logical_key
                .split_once(':')
                .ok_or_else(|| {
                    BackfillError::Store("generation CAS identity is unqualified".to_owned())
                })?
                .0
        }
        BackfillSurface::Ac => "blake3",
    };
    let qualified = generation_qualified_digest(generation, allocation_id, hardened_digest)
        .map_err(BackfillError::Store)?;
    match (surface, algorithm) {
        (BackfillSurface::Cas, "blake3") | (BackfillSurface::Ac, "blake3") => {
            Ok(format!("{region}/{tenant_prefix}/{qualified}"))
        }
        (BackfillSurface::Cas, "sha256") => {
            Ok(format!("{region}/{tenant_prefix}/bazel/sha256/{qualified}"))
        }
        (BackfillSurface::Cas, _) => Err(BackfillError::Store(
            "generation CAS algorithm is invalid".to_owned(),
        )),
        (BackfillSurface::Ac, _) => unreachable!("AC uses the native layout"),
    }
}

fn allocation_envelope_key(item: &PhysicalPurgeItem) -> Result<String, BackfillError> {
    let allocation = item
        .allocation_id
        .as_deref()
        .ok_or_else(|| BackfillError::Store("random purge lacks allocation id".to_owned()))?;
    let digest = match item.surface {
        BackfillSurface::Cas => item
            .logical_key
            .split_once(':')
            .map(|(_, digest)| digest)
            .ok_or_else(|| {
                BackfillError::Store("CAS purge key is not algorithm-qualified".to_owned())
            })?,
        BackfillSurface::Ac => item.logical_key.as_str(),
    };
    Ok(format!(
        "{}:{digest}:allocation:{allocation}",
        item.surface.as_str()
    ))
}
fn checked_region(region: String) -> Result<String, BackfillError> {
    if region.is_empty() || region.contains('/') || region.contains('\0') {
        Err(BackfillError::InvalidRequest(
            "activation R2 region must be one path segment".to_owned(),
        ))
    } else {
        Ok(region)
    }
}
fn validate_bucket(
    label: &str,
    bucket: &str,
    region: &str,
    surface: &str,
) -> Result<(), BackfillError> {
    let expected = match (surface, region) {
        ("cas", "iad" | "sam" | "syd") => "corelink-cas-prod",
        ("cas", "lhr") => "corelink-cas-eu",
        ("cas", "nrt") => "corelink-cas-apac",
        ("ac", "iad") => "corelink-ac-iad",
        ("ac", "sam") => "corelink-ac-sam",
        ("ac", "lhr") => "corelink-ac-eu",
        ("ac", "nrt") => "corelink-ac-nrt",
        ("ac", "syd") => "corelink-ac-syd",
        _ => {
            return Err(BackfillError::InvalidRequest(format!(
                "unsupported {label} activation region"
            )))
        }
    };
    if bucket != expected {
        return Err(BackfillError::InvalidRequest(format!(
            "{label} activation bucket does not match its physical region"
        )));
    }
    Ok(())
}
fn validate_logical(
    surface: BackfillSurface,
    logical: &str,
    raw_legacy: bool,
) -> Result<(), BackfillError> {
    let digest = match surface {
        BackfillSurface::Cas if raw_legacy => logical,
        BackfillSurface::Cas => {
            let (algorithm, digest) = logical.split_once(':').ok_or_else(|| {
                BackfillError::Store("CAS identity must be algorithm-qualified".to_owned())
            })?;
            if !matches!(algorithm, "blake3" | "sha256") {
                return Err(BackfillError::Store(
                    "CAS identity has unknown algorithm".to_owned(),
                ));
            }
            digest
        }
        BackfillSurface::Ac => logical,
    };
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || logical.contains('/')
        || logical.contains('\0')
    {
        return Err(BackfillError::Store(
            "object logical identity is not canonical lowercase 64-hex".to_owned(),
        ));
    }
    Ok(())
}
fn parse_surface(value: &str) -> Result<BackfillSurface, BackfillError> {
    match value {
        "cas" => Ok(BackfillSurface::Cas),
        "ac" => Ok(BackfillSurface::Ac),
        _ => Err(BackfillError::Store(
            "unknown activation object surface".to_owned(),
        )),
    }
}
fn text(row: &D1Row, name: &str) -> Result<String, BackfillError> {
    row.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| BackfillError::Store(format!("activation D1 row missing text {name}")))
}
fn optional_text(row: &D1Row, name: &str) -> Option<String> {
    row.get(name).and_then(Value::as_str).map(str::to_owned)
}
fn integer(row: &D1Row, name: &str) -> Result<i64, BackfillError> {
    row.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| BackfillError::Store(format!("activation D1 row missing integer {name}")))
}
fn unsigned(row: &D1Row, name: &str) -> Result<u64, BackfillError> {
    u64::try_from(integer(row, name)?)
        .map_err(|_| BackfillError::Store(format!("activation D1 row has negative {name}")))
}
fn boolean(row: &D1Row, name: &str) -> Result<bool, BackfillError> {
    match integer(row, name)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BackfillError::Store(format!(
            "activation D1 row has invalid boolean {name}"
        ))),
    }
}
fn stale() -> BackfillError {
    BackfillError::Conflict("activation object capability is stale or expired".to_owned())
}
fn store_error(error: impl fmt::Display) -> BackfillError {
    BackfillError::Store(error.to_string())
}
fn sanitize(error: &str) -> String {
    error
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod ordering_contract_tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    use crate::storage::{
        byok_activation::{
            ActivationPhase, ActivationSuspension, PinnedActivationPolicy, WorkerClaim,
        },
        byok_backfill::ByokBackfillEncryptor,
    };

    #[derive(Debug)]
    struct FakeD1 {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_first_receipt: Mutex<bool>,
        remaining_ms: Mutex<i64>,
    }

    #[async_trait]
    impl ActivationWorkerD1 for FakeD1 {
        async fn query(&self, sql: &str, _params: &[Value]) -> Result<Vec<D1Row>, String> {
            if sql.contains("SELECT f.transition_token") {
                return Ok(vec![row(&[
                    ("transition_token", json!("transition")),
                    ("transition_epoch", json!(1)),
                ])]);
            }
            if sql.contains(" cursor FROM byok_activation_intent") {
                return Ok(vec![row(&[("cursor", Value::Null)])]);
            }
            if sql.contains("FROM blob_meta") {
                return Ok(vec![row(&[(
                    "logical_key",
                    json!(blake3::hash(b"payload").to_hex().to_string()),
                )])]);
            }
            if sql.contains("SELECT 1 ok FROM byok_activation_source_object") {
                let required = _params[8].as_i64().unwrap();
                return Ok(if *self.remaining_ms.lock().unwrap() >= required {
                    vec![row(&[("ok", json!(1))])]
                } else {
                    Vec::new()
                });
            }
            if sql.contains("FROM byok_activation_source_object") {
                return Ok(Vec::new());
            }
            Err(format!("unexpected fake query: {sql}"))
        }

        async fn batch(&self, statements: Vec<WorkerStatement>) -> Result<Vec<Vec<D1Row>>, String> {
            if statements.len() == 6 {
                self.events.lock().unwrap().push("reserve");
                let version = statements[0].params[4].as_i64().unwrap() + 1;
                let mut result = vec![Vec::new(); 6];
                result[4] = vec![row(&[("state_version", json!(version))])];
                result[5] = vec![row(&[("assertion_token", json!("reserve"))])];
                return Ok(result);
            }
            if statements.len() == 5 && statements[0].sql.contains("'checkpoint'") {
                self.events.lock().unwrap().push("receipt");
                let mut fail = self.fail_first_receipt.lock().unwrap();
                if *fail {
                    *fail = false;
                    return Err("lost checkpoint response".to_owned());
                }
                let version = statements[0].params[4].as_i64().unwrap() + 1;
                let mut result = vec![Vec::new(); 5];
                result[3] = vec![row(&[
                    ("state_version", json!(version)),
                    ("cas_complete", json!(1)),
                    ("ac_complete", json!(0)),
                ])];
                result[4] = vec![row(&[("assertion_token", json!("receipt"))])];
                return Ok(result);
            }
            Err("unexpected fake batch".to_owned())
        }
    }

    #[derive(Debug)]
    struct FakeR2 {
        events: Arc<Mutex<Vec<&'static str>>>,
        objects: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[async_trait]
    impl ActivationR2 for FakeR2 {
        async fn get_capped(&self, key: &str, _max: u64) -> Result<CappedGet, String> {
            Ok(self
                .objects
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map_or(CappedGet::Missing, CappedGet::Found))
        }
        async fn head_size(&self, key: &str) -> Result<Option<u64>, String> {
            self.events.lock().unwrap().push("head");
            if key.contains("bazel/sha256") {
                return Ok(None);
            }
            Ok(self
                .objects
                .lock()
                .unwrap()
                .get(key)
                .map(|bytes| bytes.len() as u64))
        }
        async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String> {
            self.events.lock().unwrap().push("put");
            let mut objects = self.objects.lock().unwrap();
            if objects.contains_key(key) {
                Ok(false)
            } else {
                objects.insert(key.to_owned(), bytes);
                Ok(true)
            }
        }
        async fn delete(&self, _key: &str) -> Result<(), String> {
            self.events.lock().unwrap().push("delete");
            Err("lost DELETE response".to_owned())
        }
    }

    #[derive(Debug)]
    struct DeterministicEncryptor {
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl ByokBackfillEncryptor for DeterministicEncryptor {
        async fn target_hardened_digest(
            &self,
            _run: &BackfillRun,
            _surface: BackfillSurface,
            logical_key: &str,
            _config_version: i64,
            _tcs_version: i64,
        ) -> Result<String, BackfillError> {
            Ok(logical_key
                .rsplit_once(':')
                .map_or(logical_key, |(_, digest)| digest)
                .to_owned())
        }

        async fn source_hardened_digest(
            &self,
            _run: &BackfillRun,
            source: &BackfillSourceObject,
        ) -> Result<String, BackfillError> {
            Ok(source
                .logical_key
                .rsplit_once(':')
                .map_or(source.logical_key.as_str(), |(_, digest)| digest)
                .to_owned())
        }

        async fn encrypt(
            &self,
            _run: &BackfillRun,
            _source: &BackfillSourceObject,
            allocation: &str,
            plaintext: &[u8],
        ) -> Result<Vec<u8>, BackfillError> {
            self.events.lock().unwrap().push("encrypt");
            let mut out = allocation.as_bytes()[..8].to_vec();
            out.extend_from_slice(plaintext);
            Ok(out)
        }
    }

    fn row(fields: &[(&str, Value)]) -> D1Row {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }
    fn intent(version: i64) -> ActivationIntent {
        ActivationIntent {
            intent_id: "intent".to_owned(),
            tenant_id: "11111111-1111-1111-1111-111111111111".to_owned(),
            guard_id: "guard".to_owned(),
            request_blake3: "a".repeat(64),
            claim: Some(WorkerClaim::new(
                "worker".to_owned(),
                "claim".to_owned(),
                1,
                i64::MAX,
            )),
            observed_gate_epoch: 1,
            publication_gate_epoch: None,
            published_config_version: None,
            source_generation: 0,
            source_identity: None,
            target_generation: 1,
            policy: PinnedActivationPolicy {
                config_version: 1,
                mode: "byok".to_owned(),
                crypto_mode: "random".to_owned(),
                cmk_provider: Some("aws".to_owned()),
                cmk_key_id: Some("key".to_owned()),
                cmk_region: Some("us-east-1".to_owned()),
                tcs_version: 1,
                wrapped_tcs_blake3: None,
                policy_blake3: "b".repeat(64),
            },
            phase: ActivationPhase::Copy,
            suspension: ActivationSuspension::Active,
            cas_complete: false,
            ac_complete: false,
            deadline_at_ms: i64::MAX,
            deadline_expired: false,
            state_version: version,
        }
    }

    #[tokio::test]
    async fn lost_receipt_retry_reuses_allocation_and_existing_target() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let digest = blake3::hash(b"payload").to_hex().to_string();
        let prefix = derive_prefix(
            &TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        )
        .to_string();
        let source_key = format!("iad/{prefix}/{digest}");
        let r2 = Arc::new(FakeR2 {
            events: Arc::clone(&events),
            objects: Mutex::new(HashMap::from([(source_key, b"payload".to_vec())])),
        });
        let worker = ProductionActivationObjectWorker::new(
            Arc::new(FakeD1 {
                events: Arc::clone(&events),
                fail_first_receipt: Mutex::new(true),
                remaining_ms: Mutex::new(90_000),
            }),
            Arc::clone(&r2),
            r2,
            Arc::new(DeterministicEncryptor {
                events: Arc::clone(&events),
            }),
            TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
            "iad",
            "iad",
            1024,
            Duration::from_secs(30),
            3,
        )
        .unwrap();
        assert!(worker.copy_page(&intent(1), 1).await.is_err());
        let retry = worker.copy_page(&intent(2), 1).await.unwrap();
        assert_eq!(retry.processed, 1);
        let log = events.lock().unwrap();
        assert_eq!(log.iter().filter(|event| **event == "reserve").count(), 2);
        assert_eq!(log.iter().filter(|event| **event == "put").count(), 2);
        assert!(
            log.iter().position(|event| *event == "reserve")
                < log.iter().position(|event| *event == "encrypt")
        );
    }

    #[derive(Debug)]
    struct PurgeD1 {
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl ActivationWorkerD1 for PurgeD1 {
        async fn query(&self, sql: &str, _params: &[Value]) -> Result<Vec<D1Row>, String> {
            if sql.contains("source_crypto_mode,source_config_version") {
                return Ok(vec![row(&[
                    ("source_crypto_mode", json!("random")),
                    ("source_config_version", json!(1)),
                    ("source_cmk_provider", json!("aws")),
                    ("source_cmk_key_id", json!("key")),
                    ("source_cmk_region", json!("us-east-1")),
                    ("source_tcs_version", json!(1)),
                ])]);
            }
            if sql.contains("SELECT f.transition_token") {
                assert!(
                    sql.contains("a.phase='purging'"),
                    "encrypted source purge must authorize its identity from purging phase"
                );
                assert!(
                    !sql.contains("a.phase='copy'"),
                    "purge identity must not reuse copy-only authorization"
                );
                return Ok(vec![row(&[
                    ("transition_token", json!("transition")),
                    ("transition_epoch", json!(1)),
                ])]);
            }
            Err("unexpected query".to_owned())
        }
        async fn batch(&self, statements: Vec<WorkerStatement>) -> Result<Vec<Vec<D1Row>>, String> {
            if statements.len() == 2 {
                let prefix = derive_prefix(
                    &TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
                    Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                )
                .to_string();
                let logical = format!("blake3:{}", "c".repeat(64));
                let physical = generation_physical_key(
                    BackfillSurface::Cas,
                    "iad",
                    &prefix,
                    1,
                    "allocation",
                    &logical,
                    &"c".repeat(64),
                )
                .unwrap();
                let mut result = vec![Vec::new(); 2];
                result[1] = vec![row(&[
                    ("purge_id", json!("purge")),
                    ("tenant_id", json!("11111111-1111-1111-1111-111111111111")),
                    ("object_kind", json!("cas")),
                    ("logical_key", json!(logical)),
                    ("generation", json!(1)),
                    ("allocation_id", json!("allocation")),
                    ("physical_key", json!(physical)),
                    ("crypto_mode", json!("random")),
                    ("state", json!("deleting")),
                    ("claim_epoch", json!(1)),
                ])];
                return Ok(result);
            }
            assert_eq!(statements.len(), 7);
            assert!(statements[1].sql.contains("state='r2_absent'"));
            assert!(statements[2].sql.contains("DELETE FROM byok_envelope"));
            self.events.lock().unwrap().push("envelope_reclaim");
            let mut result = vec![Vec::new(); 7];
            result[5] = vec![row(&[("state_version", json!(2))])];
            result[6] = vec![row(&[("assertion_token", json!("verified"))])];
            Ok(result)
        }
    }

    #[tokio::test]
    async fn encrypted_source_purge_reaches_delete_and_verified_terminal_state() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let r2 = Arc::new(FakeR2 {
            events: Arc::clone(&events),
            objects: Mutex::new(HashMap::new()),
        });
        let worker = ProductionActivationObjectWorker::new(
            Arc::new(PurgeD1 {
                events: Arc::clone(&events),
            }),
            Arc::clone(&r2),
            r2,
            Arc::new(DeterministicEncryptor {
                events: Arc::clone(&events),
            }),
            TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
            "iad",
            "iad",
            1024,
            Duration::from_secs(30),
            3,
        )
        .unwrap();
        let mut purge_intent = intent(1);
        purge_intent.phase = ActivationPhase::Purging;
        purge_intent.publication_gate_epoch = Some(2);
        purge_intent.published_config_version = Some(2);
        let step = worker.purge_page(&purge_intent, 1).await.unwrap();
        assert_eq!(step.processed, 1);
        let log = events.lock().unwrap();
        let delete = log.iter().position(|event| *event == "delete").unwrap();
        let head = log.iter().position(|event| *event == "head").unwrap();
        let reclaim = log
            .iter()
            .position(|event| *event == "envelope_reclaim")
            .unwrap();
        assert!(delete < head && head < reclaim);
    }

    #[derive(Debug, Default)]
    struct TargetLeaseRaceFake {
        now_ms: i64,
        lease_expires_ms: i64,
        phase: &'static str,
        r2_exists: bool,
        receipt: bool,
    }
    impl TargetLeaseRaceFake {
        fn reserve(&mut self) {
            self.lease_expires_ms = self.now_ms + 90_000;
            self.phase = "copy";
        }
        fn revalidate_put(&self) -> bool {
            self.phase == "copy" && self.lease_expires_ms - self.now_ms >= 25_000
        }
        fn preempt(&mut self) {
            self.phase = "preempted";
        }
        fn purge(&mut self) -> bool {
            if self.now_ms < self.lease_expires_ms {
                return false;
            }
            self.r2_exists = false;
            true
        }
        fn stale_put_completion(&mut self) {
            self.r2_exists = true;
        }
        fn stale_checkpoint(&mut self) -> bool {
            if self.phase != "copy" {
                return false;
            }
            self.receipt = true;
            true
        }
    }

    #[test]
    fn preempt_waits_for_write_tail_then_deletes_late_put_and_rejects_receipt() {
        let mut race = TargetLeaseRaceFake::default();
        race.reserve();
        assert!(race.revalidate_put());
        race.preempt();
        race.now_ms = 1_000;
        assert!(
            !race.purge(),
            "purge must not start under live target lease"
        );
        race.now_ms = 20_000;
        race.stale_put_completion();
        assert!(race.r2_exists);
        race.now_ms = 90_000;
        assert!(race.purge());
        assert!(!race.r2_exists, "DELETE+HEAD absence follows tail expiry");
        assert!(!race.stale_checkpoint());
        assert!(!race.receipt);
    }

    #[test]
    fn pre_put_margin_accepts_25_seconds_and_rejects_24() {
        let mut race = TargetLeaseRaceFake::default();
        race.reserve();
        race.now_ms = 66_000;
        assert!(!race.revalidate_put());
        race.now_ms = 65_000;
        assert!(race.revalidate_put());
    }

    #[test]
    fn generation_target_keys_are_allocation_qualified() {
        let logical = format!("blake3:{}", "a".repeat(64));
        let hardened = crate::storage::byok_cas::harden_digest(
            &corelink_byok::Tcs::from_bytes([9; 32]),
            &"a".repeat(64),
        );
        let first = generation_physical_key(
            BackfillSurface::Cas,
            "iad",
            "tenant",
            7,
            "a",
            &logical,
            &hardened,
        )
        .unwrap();
        let second = generation_physical_key(
            BackfillSurface::Cas,
            "iad",
            "tenant",
            7,
            "b",
            &logical,
            &hardened,
        )
        .unwrap();
        assert_ne!(first, second);
        assert!(first.contains("/generation/7/a/"));
        assert!(second.contains("/generation/7/b/"));
    }

    #[test]
    fn live_generation_cas_and_ac_layouts_are_accepted() {
        let blake3 = format!("blake3:{}", "b".repeat(64));
        let sha256 = format!("sha256:{}", "c".repeat(64));
        let ac = "d".repeat(64);
        let tcs = corelink_byok::Tcs::from_bytes([4; 32]);
        let hardened_b3 = crate::storage::byok_cas::harden_digest(&tcs, &"b".repeat(64));
        let hardened_sha = crate::storage::byok_cas::harden_digest(&tcs, &"c".repeat(64));
        let hardened_ac = crate::storage::byok_cas::harden_digest(&tcs, &ac);
        assert_eq!(
            generation_physical_key(
                BackfillSurface::Cas,
                "iad",
                "tenant",
                2,
                "alloc",
                &blake3,
                &hardened_b3
            )
            .unwrap(),
            format!("iad/tenant/generation/2/alloc/{hardened_b3}")
        );
        assert_eq!(
            generation_physical_key(
                BackfillSurface::Cas,
                "iad",
                "tenant",
                2,
                "alloc",
                &sha256,
                &hardened_sha
            )
            .unwrap(),
            format!("iad/tenant/bazel/sha256/generation/2/alloc/{hardened_sha}")
        );
        assert_eq!(
            generation_physical_key(
                BackfillSurface::Ac,
                "iad",
                "tenant",
                2,
                "alloc",
                &ac,
                &hardened_ac
            )
            .unwrap(),
            format!("iad/tenant/generation/2/alloc/{hardened_ac}")
        );
    }

    #[test]
    fn old_allocation_purge_cannot_address_new_allocation() {
        let logical = format!("blake3:{}", "e".repeat(64));
        let hardened = crate::storage::byok_cas::harden_digest(
            &corelink_byok::Tcs::from_bytes([5; 32]),
            &"e".repeat(64),
        );
        let old = generation_physical_key(
            BackfillSurface::Cas,
            "iad",
            "tenant",
            3,
            "old",
            &logical,
            &hardened,
        )
        .unwrap();
        let new = generation_physical_key(
            BackfillSurface::Cas,
            "iad",
            "tenant",
            3,
            "new",
            &logical,
            &hardened,
        )
        .unwrap();
        assert_ne!(old, new);
        assert!(!old.contains("/new/"));
    }

    #[derive(Debug, Default)]
    struct ExecutableRaceState {
        now_ms: i64,
        lease_expires_ms: i64,
        preempted: bool,
        publish_loser_queued: bool,
        target_key: Option<String>,
        target_exists: bool,
        events: Vec<&'static str>,
    }

    #[derive(Debug)]
    struct ExecutableRaceD1 {
        state: Arc<Mutex<ExecutableRaceState>>,
        checkpoint_entered: Arc<tokio::sync::Notify>,
        allow_checkpoint: Arc<tokio::sync::Notify>,
    }

    impl ExecutableRaceD1 {
        // Faithful seam for the common reconciler claim plus the 0121
        // publish_loser target-write drain trigger. It intentionally has no
        // activation purging phase or activation_source cause dependency.
        fn claim_common_publish_loser(&self) -> Option<String> {
            let mut state = self.state.lock().unwrap();
            assert!(state.preempted && state.publish_loser_queued);
            if state.now_ms < state.lease_expires_ms {
                state.events.push("common_purge_blocked");
                return None;
            }
            state.events.push("common_purge_claimed");
            state.target_key.clone()
        }

        fn finish_common_publish_loser(&self) {
            let mut state = self.state.lock().unwrap();
            assert!(
                !state.target_exists,
                "common completion requires HEAD absence"
            );
            state.events.push("common_purge_verified");
        }
    }

    #[async_trait]
    impl ActivationWorkerD1 for ExecutableRaceD1 {
        async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
            if sql.contains("SELECT f.transition_token") {
                return Ok(vec![row(&[
                    ("transition_token", json!("transition")),
                    ("transition_epoch", json!(1)),
                ])]);
            }
            if sql.contains(" cursor FROM byok_activation_intent") {
                return Ok(vec![row(&[("cursor", Value::Null)])]);
            }
            if sql.contains("FROM blob_meta") {
                return Ok(vec![row(&[(
                    "logical_key",
                    json!(blake3::hash(b"payload").to_hex().to_string()),
                )])]);
            }
            if sql.contains("SELECT 1 ok FROM byok_activation_source_object") {
                let state = self.state.lock().unwrap();
                let required = params[8].as_i64().unwrap();
                return Ok(
                    if !state.preempted && state.lease_expires_ms - state.now_ms >= required {
                        vec![row(&[("ok", json!(1))])]
                    } else {
                        Vec::new()
                    },
                );
            }
            if sql.contains("FROM byok_activation_source_object") {
                return Ok(Vec::new());
            }
            Err(format!("unexpected race query: {sql}"))
        }

        async fn batch(&self, statements: Vec<WorkerStatement>) -> Result<Vec<Vec<D1Row>>, String> {
            if statements.len() == 6 {
                let mut state = self.state.lock().unwrap();
                state.lease_expires_ms = state.now_ms + 90_000;
                state.preempted = false;
                state.publish_loser_queued = false;
                state.events.push("reserve");
                let mut result = vec![Vec::new(); 6];
                result[4] = vec![row(&[("state_version", json!(2))])];
                result[5] = vec![row(&[("assertion_token", json!("reserve"))])];
                return Ok(result);
            }
            if statements.len() == 5 && statements[0].sql.contains("'checkpoint'") {
                self.checkpoint_entered.notify_one();
                self.allow_checkpoint.notified().await;
                let mut state = self.state.lock().unwrap();
                state.events.push("stale_checkpoint_rejected");
                if state.preempted {
                    return Err("preempted checkpoint".to_owned());
                }
            }
            Err("unexpected race batch".to_owned())
        }
    }

    #[derive(Debug)]
    struct ExecutableRaceR2 {
        state: Arc<Mutex<ExecutableRaceState>>,
        source_key: String,
        put_entered: Arc<tokio::sync::Notify>,
        allow_put: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl ActivationR2 for ExecutableRaceR2 {
        async fn get_capped(&self, key: &str, _max: u64) -> Result<CappedGet, String> {
            if key == self.source_key {
                Ok(CappedGet::Found(b"payload".to_vec()))
            } else {
                Ok(CappedGet::Missing)
            }
        }
        async fn head_size(&self, key: &str) -> Result<Option<u64>, String> {
            if key == self.source_key {
                return Ok(Some(7));
            }
            if key.contains("/bazel/sha256/") {
                return Ok(None);
            }
            let mut state = self.state.lock().unwrap();
            state.events.push("head");
            Ok(state.target_exists.then_some(1))
        }
        async fn put_if_absent(&self, key: &str, _bytes: Vec<u8>) -> Result<bool, String> {
            self.put_entered.notify_one();
            self.allow_put.notified().await;
            let mut state = self.state.lock().unwrap();
            state.target_key = Some(key.to_owned());
            state.target_exists = true;
            state.events.push("late_put");
            Ok(true)
        }
        async fn delete(&self, _key: &str) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            state.target_exists = false;
            state.events.push("delete");
            Ok(())
        }
    }

    #[tokio::test]
    async fn actual_worker_hands_late_put_to_common_publish_loser_reaper() {
        let state = Arc::new(Mutex::new(ExecutableRaceState::default()));
        let checkpoint_entered = Arc::new(tokio::sync::Notify::new());
        let allow_checkpoint = Arc::new(tokio::sync::Notify::new());
        let put_entered = Arc::new(tokio::sync::Notify::new());
        let allow_put = Arc::new(tokio::sync::Notify::new());
        let digest = blake3::hash(b"payload").to_hex().to_string();
        let prefix = derive_prefix(
            &TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        )
        .to_string();
        let source_key = format!("iad/{prefix}/{digest}");
        let r2 = Arc::new(ExecutableRaceR2 {
            state: Arc::clone(&state),
            source_key,
            put_entered: Arc::clone(&put_entered),
            allow_put: Arc::clone(&allow_put),
        });
        let race_d1 = Arc::new(ExecutableRaceD1 {
            state: Arc::clone(&state),
            checkpoint_entered: Arc::clone(&checkpoint_entered),
            allow_checkpoint: Arc::clone(&allow_checkpoint),
        });
        let worker = Arc::new(
            ProductionActivationObjectWorker::new(
                Arc::clone(&race_d1),
                Arc::clone(&r2),
                Arc::clone(&r2),
                Arc::new(DeterministicEncryptor {
                    events: Arc::new(Mutex::new(Vec::new())),
                }),
                TenantDerivationKey::from_bytes(Zeroizing::new([7; 32])),
                "iad",
                "iad",
                1024,
                Duration::from_secs(30),
                3,
            )
            .unwrap(),
        );

        let copy_worker = Arc::clone(&worker);
        let copy = tokio::spawn(async move { copy_worker.copy_page(&intent(1), 1).await });
        put_entered.notified().await;
        {
            let mut locked = state.lock().unwrap();
            locked.preempted = true;
            locked.publish_loser_queued = true;
            assert!(!locked.target_exists);
        }

        assert!(race_d1.claim_common_publish_loser().is_none());
        allow_put.notify_one();
        checkpoint_entered.notified().await;
        {
            let mut locked = state.lock().unwrap();
            assert!(locked.target_exists);
            locked.now_ms = locked.lease_expires_ms;
        }
        let target = race_d1
            .claim_common_publish_loser()
            .expect("expired target write is claimable by common reaper");
        r2.delete(&target).await.unwrap();
        assert_eq!(r2.head_size(&target).await.unwrap(), None);
        race_d1.finish_common_publish_loser();
        allow_checkpoint.notify_one();
        assert!(copy.await.unwrap().is_err());
        let locked = state.lock().unwrap();
        assert!(!locked.target_exists);
        assert!(locked.events.contains(&"stale_checkpoint_rejected"));
        let blocked = locked
            .events
            .iter()
            .position(|event| *event == "common_purge_blocked")
            .unwrap();
        let late_put = locked
            .events
            .iter()
            .position(|event| *event == "late_put")
            .unwrap();
        let delete = locked
            .events
            .iter()
            .position(|event| *event == "delete")
            .unwrap();
        let head = locked
            .events
            .iter()
            .position(|event| *event == "head")
            .unwrap();
        let rejected = locked
            .events
            .iter()
            .position(|event| *event == "stale_checkpoint_rejected")
            .unwrap();
        let verified = locked
            .events
            .iter()
            .position(|event| *event == "common_purge_verified")
            .unwrap();
        assert!(
            blocked < late_put
                && late_put < delete
                && delete < head
                && head < verified
                && verified < rejected
        );
    }
}

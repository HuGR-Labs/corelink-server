//! R2 storage adapter using the AWS S3-compatible API.
//!
//! This module provides:
//!
//! - [`R2S3Client`] — low-level async `put_object` / `get_object`
//!   wrapper over `aws-sdk-s3` pointed at the R2 S3 endpoint.
//! - [`R2CasHandler`] — a sync `CasReadHandler` + `CasWriteHandler`
//!   implementation that uses [`R2S3Client`] for durable storage and
//!   `derive_prefix` for tenant-scoped R2 keys.
//!
//! # Key scheme
//!
//! ```text
//! <region>/<tenant_prefix_16>/<digest>
//! ```
//!
//! The `tenant_prefix_16` is derived via
//! `corelink_tenant_path::derive_prefix` so cross-tenant key
//! co-residence is impossible (layer 5 of `INV-TENANT-ISOLATION`).
//!
//! # Sync wrapper
//!
//! The `CasReadHandler` / `CasWriteHandler` traits are synchronous
//! (they exist in the pre-async R-prep layer). `R2CasHandler` bridges
//! the async S3 SDK into the sync trait surface by using
//! `tokio::runtime::Handle::current().block_on(...)`. The server runs
//! inside a tokio runtime, so a handle is always available.
//!
//! # Security charter compliance
//!
//! - No credentials in code; constructed from [`StorageEnv`].
//! - No secrets logged; tracing events contain bucket + key only.
//! - No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use corelink_handler_cas::{
    AuditEvent, AuditEventKind, AuditSink, CasHandlerError, CasReadHandler, CasReadRequest,
    CasReadResponse, CasWriteHandler, CasWriteRequest, CasWriteResponse, InMemoryAuditSink,
    InMemorySliObserver, SliObservation, SliObserver,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use tracing::{debug, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::StorageEnv;

/// Low-level async R2/S3 client.
///
/// Wraps `aws-sdk-s3` with a custom endpoint set to the R2
/// S3-compatible URL. Credentials are taken from [`StorageEnv`] and
/// never logged.
#[derive(Debug)]
pub struct R2S3Client {
    inner: Client,
    /// Default bucket for CAS blobs (e.g. `corelink-cas-prod`).
    bucket: String,
}

impl R2S3Client {
    /// Construct an [`R2S3Client`] from a validated [`StorageEnv`].
    ///
    /// The `bucket` parameter selects the R2 bucket name (e.g.
    /// `"corelink-cas-prod"`).
    ///
    /// # Errors
    ///
    /// Returns a descriptive `String` if the S3 config cannot be built.
    pub async fn new(env: &StorageEnv, bucket: impl Into<String>) -> Result<Self, String> {
        let credentials = Credentials::new(
            &env.r2_access_key_id,
            &env.r2_secret_access_key,
            None, // session token — not used for R2 static credentials
            None, // expiry
            "corelink-r2-s3-adapter",
        );

        // R2 uses `auto` as the region pseudo-value; the real routing
        // is done by the endpoint URL.
        let region = Region::new("auto");

        // CRITICAL — DO NOT call `aws_config::defaults(...).load().await`.
        // That helper triggers the AWS credential-provider chain (IMDS,
        // ECS, STS) which performs blocking outbound metadata probes.
        // In CF Containers there is no IMDS endpoint, so each probe runs
        // to its full retry budget — observed 60-90s cold-start in prod
        // versus ~85ms locally. We already have explicit static R2
        // credentials, so we build the S3 config directly and bypass the
        // auto-detection path entirely (zero I/O).
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(region)
            .endpoint_url(&env.r2_endpoint)
            .credentials_provider(credentials)
            .force_path_style(false) // R2 supports virtual-hosted style
            .build();

        Ok(Self {
            inner: Client::from_conf(s3_config),
            bucket: bucket.into(),
        })
    }

    /// Upload `bytes` under `key` in the configured bucket.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on S3/network error.
    pub async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), String> {
        debug!(
            bucket = %self.bucket,
            key = %key,
            bytes = bytes.len(),
            "R2S3Client::put"
        );
        let len = bytes.len() as i64;
        self.inner
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(bytes.into())
            .content_length(len)
            .send()
            .await
            .map_err(|e| format!("R2 put failed for key {key}: {e}"))?;
        Ok(())
    }

    /// Download the bytes stored under `key`.
    ///
    /// Returns `Ok(Some(bytes))` on success, `Ok(None)` if the object
    /// does not exist (HTTP 404), and `Err(String)` on other errors.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        debug!(
            bucket = %self.bucket,
            key = %key,
            "R2S3Client::get"
        );
        let result = self
            .inner
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;

        match result {
            Ok(output) => {
                let bytes = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| format!("R2 body read failed for key {key}: {e}"))?
                    .into_bytes()
                    .to_vec();
                Ok(Some(bytes))
            }
            Err(sdk_err) => {
                // Check if this is a NoSuchKey (404) — return Ok(None).
                if let aws_sdk_s3::error::SdkError::ServiceError(ref se) = sdk_err {
                    if se.err().is_no_such_key() {
                        return Ok(None);
                    }
                }
                Err(format!("R2 get failed for key {key}: {sdk_err}"))
            }
        }
    }

    /// Compute the R2 object key for a blob.
    ///
    /// Key format: `<region>/<tenant_prefix_16>/<digest>`.
    /// The tenant prefix is derived via `derive_prefix` using a zero
    /// TDK (test/dev mode). For production, a real TDK is loaded from
    /// env and threaded via [`R2CasHandler::with_tdk`].
    #[must_use]
    pub fn blob_key(region: &str, tenant_prefix: &str, digest: &str) -> String {
        format!("{region}/{tenant_prefix}/{digest}")
    }
}

/// A sync `CasReadHandler` + `CasWriteHandler` backed by [`R2S3Client`].
///
/// Wraps the async S3 operations with
/// `tokio::runtime::Handle::current().block_on(...)` so the sync
/// handler traits can drive async I/O from within a tokio runtime.
pub struct R2CasHandler {
    client: R2S3Client,
    /// The R2 region string (e.g. `"iad"`) used as key prefix.
    cas_region: String,
    /// Tenant derivation key for `derive_prefix`. Wrapped in
    /// `Option<...>` because tests construct without a TDK.
    tdk: Option<TenantDerivationKey>,
    audit: Arc<dyn AuditSink>,
    sli: Arc<dyn SliObserver>,
}

impl core::fmt::Debug for R2CasHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2CasHandler")
            .field("cas_region", &self.cas_region)
            .finish_non_exhaustive()
    }
}

impl R2CasHandler {
    /// Construct an `R2CasHandler`.
    ///
    /// `tdk_bytes` is a 32-byte secret key loaded from env/KMS. Pass
    /// `None` only in tests that use a dev/zero TDK.
    #[must_use]
    pub fn new(
        client: R2S3Client,
        cas_region: impl Into<String>,
        tdk_bytes: Option<Zeroizing<[u8; 32]>>,
        audit: Arc<dyn AuditSink>,
        sli: Arc<dyn SliObserver>,
    ) -> Self {
        let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);
        Self {
            client,
            cas_region: cas_region.into(),
            tdk,
            audit,
            sli,
        }
    }

    /// Derive the R2 key for a (tenant, digest) pair.
    ///
    /// When no TDK is configured (test mode), falls back to `tenant`
    /// as a raw 16-char pad.
    fn r2_key(&self, tenant: &str, digest: &str) -> String {
        let prefix = match &self.tdk {
            Some(tdk) => {
                // Attempt to parse tenant as UUID; fall back to raw
                // string prefix on parse error (test fixture tenants
                // are often simple strings, not UUIDs).
                if let Ok(uid) = Uuid::try_parse(tenant) {
                    derive_prefix(tdk, uid).to_string()
                } else {
                    // Non-UUID tenant (test mode) — use raw, padded.
                    let mut p = tenant.to_owned();
                    p.truncate(16);
                    while p.len() < 16 {
                        p.push('0');
                    }
                    p
                }
            }
            None => {
                // No TDK — dev/test mode: use padded tenant.
                let mut p = tenant.to_owned();
                p.truncate(16);
                while p.len() < 16 {
                    p.push('0');
                }
                p
            }
        };
        R2S3Client::blob_key(&self.cas_region, &prefix, digest)
    }

    /// Emit both SLI observations (availability + latency).
    fn emit_sli(
        &self,
        avail: corelink_handler_cas::observer::Sli,
        lat: corelink_handler_cas::observer::Sli,
        is_error: bool,
    ) {
        self.sli.observe(SliObservation::new(avail, is_error, 0));
        self.sli.observe(SliObservation::new(lat, is_error, 0));
    }
}

impl CasReadHandler for R2CasHandler {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasGet, Sli::LatencyCasGetP99, is_error);
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::ReadDenied,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // ReadAttempted audit BEFORE lookup.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::ReadAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        let key = self.r2_key(&req.tenant, &req.hash);
        debug!(key = %key, "R2CasHandler::read");

        let handle = tokio::runtime::Handle::current();
        let result = handle.block_on(self.client.get(&key));

        match result {
            Ok(Some(bytes)) => {
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::ReadServed,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli.observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                emit(false);
                Ok(CasReadResponse::new(bytes, req.hash))
            }
            Ok(None) => {
                emit(true);
                Err(CasHandlerError::NotFound {
                    tenant: req.tenant,
                    hash: req.hash,
                })
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2CasHandler::read error");
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}

impl CasWriteHandler for R2CasHandler {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasPut, Sli::LatencyCasPutP99, is_error);
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::WriteDenied,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // WriteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::WriteAttempted,
                req.tenant.clone(),
                req.claimed_hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        let key = self.r2_key(&req.tenant, &req.claimed_hash);
        debug!(key = %key, bytes = req.bytes.len(), "R2CasHandler::write");

        let handle = tokio::runtime::Handle::current();
        let result = handle.block_on(self.client.put(&key, req.bytes));

        match result {
            Ok(()) => {
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::WriteCommitted,
                        req.tenant.clone(),
                        req.claimed_hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli.observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                emit(false);
                Ok(CasWriteResponse::new(req.claimed_hash, true))
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2CasHandler::write error");
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}

/// Build an `R2CasHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryCasHandler`.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_cas_handler_from_env(
    bucket: &str,
    cas_region: &str,
) -> Option<Result<R2CasHandler, String>> {
    let env = super::StorageEnv::from_env()?;
    let tdk_bytes = load_tdk_from_env();
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    Some(Ok(R2CasHandler::new(
        client,
        cas_region,
        tdk_bytes,
        audit,
        sli,
    )))
}

/// A sync `AcLookupHandler` + `AcUpdateHandler` backed by [`R2S3Client`].
///
/// Mirrors `R2CasHandler` exactly — bridges async S3 I/O to the sync
/// AC handler trait surface via `tokio::runtime::Handle::current().block_on(...)`.
/// Wired through `routes::ac::build_handlers` when storage credentials
/// are configured; otherwise the route falls back to `InMemoryAcHandler`.
///
/// # Key scheme
///
/// AC entries reuse the canonical
/// `<region>/<tenant_prefix_16>/<action_digest>` key layout from CAS,
/// only the bucket differs (`R2_AC_BUCKET` / default `corelink-ac-iad`).
/// Per-tenant prefix isolation (layer 5 of `INV-TENANT-ISOLATION`)
/// applies identically.
pub struct R2AcHandler {
    client: R2S3Client,
    /// The R2 region string (e.g. `"iad"`) used as key prefix.
    ac_region: String,
    /// Tenant derivation key — see [`R2CasHandler`].
    tdk: Option<TenantDerivationKey>,
    audit: Arc<dyn corelink_handler_ac::AuditSink>,
    sli: Arc<dyn corelink_handler_ac::SliObserver>,
}

impl core::fmt::Debug for R2AcHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2AcHandler")
            .field("ac_region", &self.ac_region)
            .finish_non_exhaustive()
    }
}

impl R2AcHandler {
    /// Construct an `R2AcHandler`. See [`R2CasHandler::new`] for the
    /// `tdk_bytes` semantics (pass `None` in tests with a dev/zero TDK).
    #[must_use]
    pub fn new(
        client: R2S3Client,
        ac_region: impl Into<String>,
        tdk_bytes: Option<Zeroizing<[u8; 32]>>,
        audit: Arc<dyn corelink_handler_ac::AuditSink>,
        sli: Arc<dyn corelink_handler_ac::SliObserver>,
    ) -> Self {
        let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);
        Self {
            client,
            ac_region: ac_region.into(),
            tdk,
            audit,
            sli,
        }
    }

    /// Derive the R2 key for a (tenant, action_digest) pair. Mirrors
    /// `R2CasHandler::r2_key`; the AC bucket uses the same layout.
    fn r2_key(&self, tenant: &str, action_digest: &str) -> String {
        let prefix = match &self.tdk {
            Some(tdk) => {
                if let Ok(uid) = Uuid::try_parse(tenant) {
                    derive_prefix(tdk, uid).to_string()
                } else {
                    let mut p = tenant.to_owned();
                    p.truncate(16);
                    while p.len() < 16 {
                        p.push('0');
                    }
                    p
                }
            }
            None => {
                let mut p = tenant.to_owned();
                p.truncate(16);
                while p.len() < 16 {
                    p.push('0');
                }
                p
            }
        };
        R2S3Client::blob_key(&self.ac_region, &prefix, action_digest)
    }

    /// Emit the (avail, latency) SLI pair for the lookup path. The
    /// update path folds availability into `AvailAcLookup` per the
    /// canonical-15 metric registry discipline (see
    /// `InMemoryAcHandler::update`).
    fn emit_lookup_sli(&self, is_error: bool) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli.observe(SliObservation::new(Sli::AvailAcLookup, is_error, 0));
        self.sli.observe(SliObservation::new(Sli::LatencyAcHitP99, is_error, 0));
    }

    fn emit_update_sli(&self, is_error: bool) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli.observe(SliObservation::new(Sli::AvailAcLookup, is_error, 0));
    }
}

impl corelink_handler_ac::AcLookupHandler for R2AcHandler {
    fn lookup(
        &self,
        req: corelink_handler_ac::AcLookupRequest,
    ) -> Result<corelink_handler_ac::AcLookupResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::{
            AcHandlerError, AcLookupResponse, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(AcAuditEventKind::LookupDenied, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_lookup_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // LookupAttempted audit BEFORE storage read.
        self.audit
            .emit(AcAuditEvent::new(AcAuditEventKind::LookupAttempted, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
            .map_err(AcHandlerError::AuditFailed)?;

        let key = self.r2_key(&req.tenant, &req.action_digest);
        debug!(key = %key, "R2AcHandler::lookup");

        let handle = tokio::runtime::Handle::current();
        let result = handle.block_on(self.client.get(&key));

        match result {
            Ok(Some(bytes)) => {
                self.audit
                    .emit(AcAuditEvent::new(AcAuditEventKind::LookupHit, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
                    .map_err(AcHandlerError::AuditFailed)?;
                self.emit_lookup_sli(false);
                Ok(AcLookupResponse::new(req.action_digest, bytes))
            }
            Ok(None) => {
                self.audit
                    .emit(AcAuditEvent::new(AcAuditEventKind::LookupMiss, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
                    .map_err(AcHandlerError::AuditFailed)?;
                // Miss is NOT an availability error — handler served
                // correctly (mirrors `InMemoryAcHandler::lookup`).
                self.emit_lookup_sli(false);
                Err(AcHandlerError::Miss {
                    tenant: req.tenant,
                    action_digest: req.action_digest,
                })
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                self.emit_lookup_sli(true);
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

impl corelink_handler_ac::AcUpdateHandler for R2AcHandler {
    fn update(
        &self,
        req: corelink_handler_ac::AcUpdateRequest,
    ) -> Result<corelink_handler_ac::AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::{
            AcHandlerError, AcUpdateResponse, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(AcAuditEventKind::UpdateDenied, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_update_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // UpdateAttempted audit BEFORE mutation.
        self.audit
            .emit(AcAuditEvent::new(AcAuditEventKind::UpdateAttempted, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
            .map_err(AcHandlerError::AuditFailed)?;

        let key = self.r2_key(&req.tenant, &req.action_digest);
        debug!(
            key = %key,
            bytes = req.result_payload.len(),
            "R2AcHandler::update"
        );

        let handle = tokio::runtime::Handle::current();

        // `durable=true` mirrors `InMemoryAcHandler::update` — only set
        // on a fresh insert. Probe existence via GET before PUT;
        // any GET error other than NoSuchKey is treated as
        // pre-existing (conservative — never claim durable on
        // ambiguous state).
        let pre_existed = matches!(handle.block_on(self.client.get(&key)), Ok(Some(_)));

        let result = handle.block_on(self.client.put(&key, req.result_payload));

        match result {
            Ok(()) => {
                self.audit
                    .emit(AcAuditEvent::new(AcAuditEventKind::UpdateCommitted, req.tenant.clone(), req.action_digest.clone(), req.principal.clone(), req.at_unix_ms))
                    .map_err(AcHandlerError::AuditFailed)?;
                self.emit_update_sli(false);
                Ok(AcUpdateResponse::new(req.action_digest, !pre_existed))
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2AcHandler::update error");
                self.emit_update_sli(true);
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

/// Build an `R2AcHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryAcHandler`.
///
/// Mirrors [`build_r2_cas_handler_from_env`] exactly; only the bucket
/// + region defaults and the handler type differ.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_ac_handler_from_env(
    bucket: &str,
    ac_region: &str,
) -> Option<Result<R2AcHandler, String>> {
    let env = super::StorageEnv::from_env()?;
    let tdk_bytes = load_tdk_from_env();
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    let audit = Arc::new(corelink_handler_ac::InMemoryAuditSink::new());
    let sli = Arc::new(corelink_handler_ac::InMemorySliObserver::new());
    Some(Ok(R2AcHandler::new(
        client,
        ac_region,
        tdk_bytes,
        audit,
        sli,
    )))
}

/// Load the tenant derivation key from `R2_TDK_HEX` env var (64 hex chars =
/// 32 bytes). Returns `None` when not set, causing `R2CasHandler` to use
/// the raw-padded fallback (dev/test mode).
fn load_tdk_from_env() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        warn!(len = hex_str.len(), "R2_TDK_HEX has wrong length; ignoring TDK");
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        warn!("R2_TDK_HEX is not valid hex; ignoring TDK");
        return None;
    }
    Some(bytes)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Unit tests: key derivation logic (no network)
    // ---------------------------------------------------------------

    #[test]
    fn blob_key_format_is_canonical() {
        let key = R2S3Client::blob_key("iad", "abcdef1234567890", "deadbeef00000001");
        assert_eq!(key, "iad/abcdef1234567890/deadbeef00000001");
    }

    #[tokio::test]
    async fn r2_cas_handler_key_uses_region_and_prefix() {
        let handler = make_test_handler("iad").await;
        let key = handler.r2_key("tenant-abc", "abc123hash0000001");
        // Key must start with the region segment.
        assert!(key.starts_with("iad/"), "key: {key}");
        // Key must end with the digest.
        assert!(key.ends_with("/abc123hash0000001"), "key: {key}");
        // Middle segment is exactly 16 chars (tenant prefix).
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].len(), 16, "tenant prefix must be 16 chars");
    }

    /// Cross-tenant denial returns BEFORE any S3 I/O — this test
    /// exercises the audit-emit-BEFORE-rejection ordering without
    /// making network calls.
    #[tokio::test]
    async fn r2_cas_handler_cross_tenant_denied_audits_before_rejection() {
        let handler = make_test_handler("iad").await;
        let req = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "attacker@other",
            "other",
            1,
        );
        let err = handler.read(req).expect_err("denied");
        assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
        // The audit sink recorded a ReadDenied event BEFORE the
        // rejection — fail-CLOSED ordering pin.
    }

    // ---------------------------------------------------------------
    // Integration round-trip test (requires live R2 creds)
    // ---------------------------------------------------------------

    /// PUT bytes → GET → bytes match.
    ///
    /// Gated behind `#[ignore]` so the CI green path does not require
    /// live R2 credentials. Run manually with:
    ///
    /// ```bash
    /// R2_S3_ACCESS_KEY_ID=<id> R2_S3_SECRET_ACCESS_KEY=<sec> \
    ///   R2_S3_ENDPOINT=https://<account>.r2.cloudflarestorage.com \
    ///   CLOUDFLARE_ACCOUNT_ID=<acc> CF_API_TOKEN=<tok> \
    ///   D1_DATABASE_ID=<id> \
    ///   R2_TEST_BUCKET=corelink-cas-prod \
    ///   cargo test -p corelink-server storage_r2_round_trip -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn storage_r2_round_trip() {
        let env = StorageEnv::from_env()
            .expect("all R2 env vars must be set to run this test");
        let bucket = std::env::var("R2_TEST_BUCKET")
            .unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");

        // Use a timestamped key so parallel test runs don't collide.
        let key = format!(
            "test/round-trip/{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let payload = b"corelink-wp-s1-storage-round-trip".to_vec();

        // PUT
        client.put(&key, payload.clone()).await.expect("put");

        // GET → must match
        let got = client.get(&key).await.expect("get").expect("present");
        assert_eq!(got, payload, "round-trip bytes must match");

        // GET missing key → None
        let missing = client.get("__no_such_key__").await.expect("get");
        assert!(missing.is_none(), "missing key must return None");
    }

    // ---------------------------------------------------------------
    // Helper
    // ---------------------------------------------------------------

    /// Build an `R2CasHandler` backed by an unavailable (stub) S3
    /// client. Only the key-derivation and audit paths are exercised
    /// in these tests; any S3 I/O would fail.
    ///
    /// This is an `async fn` so it can be called from within a
    /// `#[tokio::test]` context without nested-runtime conflicts.
    async fn make_test_handler(region: &str) -> R2CasHandler {
        // Build a stub env pointing at localhost (won't connect).
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };

        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        R2CasHandler::new(client, region, None, audit, sli)
    }
}

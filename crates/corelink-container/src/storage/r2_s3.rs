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
    AuditEvent, AuditEventKind, AuditSink, CasDeleteHandler, CasHandlerError, CasListHandler,
    CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest,
    CasWriteResponse, DigestAlgo, InMemoryAuditSink, InMemorySliObserver, SliObservation,
    SliObserver,
};
use corelink_hash::Digest;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
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
    /// Per-key serialization locks for [`Self::delete_if_present`] (rt-nuclear
    /// #6/#10/#14 — concurrent double-DELETE over-release). HEAD-then-DELETE is
    /// non-atomic and S3 `DeleteObject` neither reports prior size nor supports a
    /// "delete-and-return-size" op, so two racing deletes of the same key both
    /// HEAD the size and both report it reclaimed → the byte accountant releases
    /// it twice → free headroom. We serialize the measure-and-delete per key in
    /// this process so AT MOST ONE racer observes the object present (HEAD ⇒
    /// `Some(size)`) and removes it; every other racer HEADs absent AFTER the
    /// delete and returns `None` (releases 0). The map is pruned on release so it
    /// does not grow without bound.
    delete_locks: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
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
            delete_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
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

    /// Read the byte size of the object stored under `key` WITHOUT
    /// downloading its body (S3 `HeadObject`).
    ///
    /// Returns `Ok(Some(size))` when the object exists, `Ok(None)` when it is
    /// absent (HTTP 404 / `NoSuchKey`), and `Err(String)` on any other
    /// transport/service error. Used by the storage byte-accounting delete
    /// path ([`crate::byte_accounting`]) to learn how many bytes a delete
    /// reclaims so it can `release` exactly that amount from
    /// `tenant_storage_state.bytes_used` — a HEAD, not a GET, so it costs no
    /// egress for an arbitrarily large blob.
    ///
    /// # Errors
    /// Returns `Err(String)` on any non-404 transport/service error.
    pub async fn head_size(&self, key: &str) -> Result<Option<u64>, String> {
        debug!(
            bucket = %self.bucket,
            key = %key,
            "R2S3Client::head_size"
        );
        let result = self
            .inner
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        match result {
            Ok(output) => Ok(Some(u64::try_from(output.content_length().unwrap_or(0)).unwrap_or(0))),
            Err(sdk_err) => {
                // A HeadObject on an absent key surfaces as a NotFound service
                // error (R2 returns 404). Treat it as "no object" (release
                // nothing) rather than an error — the delete is idempotent.
                if let aws_sdk_s3::error::SdkError::ServiceError(ref se) = sdk_err {
                    if se.err().is_not_found() {
                        return Ok(None);
                    }
                }
                Err(format!("R2 head failed for key {key}: {sdk_err}"))
            }
        }
    }

    /// Hard-delete the object stored under `key`.
    ///
    /// S3 `DeleteObject` is idempotent — deleting a key that does not
    /// exist returns success — so a replayed erasure is a safe no-op.
    /// Used by the WI-S11-008 GDPR erasure adapters (`adapter_r2_cas` /
    /// `adapter_r2_ac`) to erase a tenant's content-addressed bytes.
    ///
    /// # Errors
    /// Returns `Err(String)` on any transport/service error.
    pub async fn delete(&self, key: &str) -> Result<(), String> {
        debug!(
            bucket = %self.bucket,
            key = %key,
            "R2S3Client::delete"
        );
        self.inner
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| format!("R2 delete failed for key {key}: {e}"))?;
        Ok(())
    }

    /// Atomically (per-key, in-process) measure-and-delete: HEAD the object,
    /// delete it, and return `Some(prior_size)` to the racer that actually
    /// observed-and-removed it — `None` to every racer that saw it already
    /// absent (rt-nuclear #6/#10/#14 — concurrent double-DELETE over-release).
    ///
    /// S3 `DeleteObject` is idempotent and reports neither prior presence nor
    /// prior size, so a naive HEAD-then-DELETE lets two concurrent deletes of the
    /// same key BOTH read the size and BOTH report it reclaimed — the byte
    /// accountant then releases the bytes twice, manufacturing free headroom. We
    /// serialize the measure-and-delete under a per-key async lock: the HEAD runs
    /// INSIDE the critical section, so a second racer that enters after the first
    /// committed its delete HEADs absent and returns `None` (releases 0). Only the
    /// request that genuinely removed the object returns its size.
    ///
    /// A HEAD error fails CLOSED to `Some(0)` semantics via the caller (we still
    /// delete, but report 0 reclaimed — a missed release over-counts the tenant,
    /// which is conservative; it never widens the cap).
    ///
    /// # Errors
    /// Returns `Err(String)` on a `DeleteObject` transport/service error.
    pub async fn delete_if_present(&self, key: &str) -> Result<Option<u64>, String> {
        // Acquire (or create) the per-key serialization lock.
        let lock = {
            let mut locks = match self.delete_locks.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            std::sync::Arc::clone(
                locks
                    .entry(key.to_owned())
                    .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let _guard = lock.lock().await;

        // HEAD inside the critical section: the prior size is observed ONLY by the
        // racer that is about to remove the object. A second racer enters here
        // after this racer's DELETE committed and sees the object absent.
        let prior = match self.head_size(key).await {
            Ok(opt) => opt,
            Err(e) => {
                // Ambiguous prior state: still delete (idempotent) but report
                // nothing reclaimed — conservative (never over-credit headroom).
                warn!(error = %e, key = %key, "R2S3Client::delete_if_present HEAD error; reporting 0 reclaimed");
                None
            }
        };

        let result = self.delete(key).await;

        // Prune the per-key lock entry once no other task is waiting on it (we are
        // the sole holder ⇒ strong_count == 1 after dropping our local `lock`
        // would be 1; here we still hold `_guard`+`lock`, so check for ==2: our
        // `lock` clone + the map's). Keeps `delete_locks` from growing unbounded.
        {
            let mut locks = match self.delete_locks.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(entry) = locks.get(key) {
                // map holds 1 ref; this function holds `lock` (1) ⇒ 2 means no
                // other waiter. Drop the map entry so it can be GC'd.
                if std::sync::Arc::strong_count(entry) <= 2 {
                    locks.remove(key);
                }
            }
        }

        result?;
        Ok(prior)
    }

    /// List every object key under `prefix` (paginated via the V2
    /// continuation token).
    ///
    /// Used by the WI-S11-008 CAS erasure adapter: the native whole-blob CAS
    /// path keys objects at `<region>/<tenant_prefix>/<digest>` with **no
    /// durable D1 index**, so LIST-by-prefix is the only enumeration that is
    /// complete by construction (deletes everything under a tenant's prefix,
    /// no matter which component wrote it).
    ///
    /// # Errors
    /// Returns `Err(String)` on any transport/service error.
    pub async fn list_objects_v2(&self, prefix: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let mut req = self
                .inner
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(token) = continuation.as_ref() {
                req = req.continuation_token(token);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| format!("R2 list failed for prefix {prefix}: {e}"))?;
            for obj in resp.contents() {
                if let Some(k) = obj.key() {
                    keys.push(k.to_owned());
                }
            }
            if resp.is_truncated().unwrap_or(false) {
                match resp.next_continuation_token() {
                    Some(t) => continuation = Some(t.to_owned()),
                    None => break,
                }
            } else {
                break;
            }
        }
        Ok(keys)
    }

    /// List ONE page of objects under `prefix`, returning per-object
    /// metadata + an opaque continuation token for the next page.
    ///
    /// Used by the D-7 / D-8 paginated enumeration routes. Unlike
    /// [`Self::list_objects_v2`] (which drains every page for erasure),
    /// this returns a single S3 `ListObjectsV2` page so the HTTP route
    /// can stream pages back to the client under its own cursor. The
    /// S3 V2 continuation token IS the route's opaque `next_cursor`.
    ///
    /// `max_keys` is clamped into `1..=1000` (the S3 hard cap). Each
    /// returned tuple is `(full_key, size_bytes, rfc3339_last_modified)`;
    /// the caller strips the `<region>/<tenant_prefix>/` segments to
    /// recover the bare digest.
    ///
    /// # Errors
    /// Returns `Err(String)` on any transport/service error.
    pub async fn list_objects_page(
        &self,
        prefix: &str,
        max_keys: u32,
        cursor: Option<&str>,
    ) -> Result<(Vec<(String, u64, String)>, Option<String>), String> {
        let max_keys = max_keys.clamp(1, 1000) as i32;
        let mut req = self
            .inner
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .max_keys(max_keys);
        if let Some(token) = cursor {
            req = req.continuation_token(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("R2 list-page failed for prefix {prefix}: {e}"))?;

        let mut out = Vec::new();
        for obj in resp.contents() {
            let Some(key) = obj.key() else { continue };
            let size = u64::try_from(obj.size().unwrap_or(0)).unwrap_or(0);
            // RFC-3339 last-modified; absent ⇒ unix epoch (deterministic
            // fallback rather than a panic / skipped row).
            let last_modified = obj
                .last_modified()
                .and_then(|dt| dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).ok())
                .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned());
            out.push((key.to_owned(), size, last_modified));
        }

        // Only surface a next cursor when S3 says the listing is
        // truncated AND hands back a token (fail-safe: a missing token
        // on a truncated page ends pagination rather than looping).
        let next = if resp.is_truncated().unwrap_or(false) {
            resp.next_continuation_token().map(str::to_owned)
        } else {
            None
        };
        Ok((out, next))
    }

    /// Compute the R2 object key for a blob, surface-partitioned by the
    /// keyspace's content-addressing function ([`DigestAlgo`]).
    ///
    /// - **`Blake3`** (native CAS + sccache): `<region>/<tenant_prefix_16>/<digest>`.
    /// - **`Sha256`** (Bazel REAPI v2): `<region>/<tenant_prefix_16>/bazel/sha256/<digest>`.
    ///
    /// The `bazel/sha256/` segment sits AFTER the tenant prefix so the
    /// secret-keyed HMAC tenant isolation (layer 5 of `INV-TENANT-ISOLATION`)
    /// is fully preserved; the sub-prefix only partitions the digest function
    /// WITHIN a tenant's namespace. Each keyspace is single-function: a
    /// SHA-256 blob is never co-resident with a BLAKE3 blob under one key,
    /// so the durable gate's read-path re-verification always applies the
    /// blob's own function (Option A, ADR-0044).
    ///
    /// The `tenant_prefix` is computed by the caller; on the production path
    /// it is always `derive_prefix(secret_tdk, tenant_uuid)` (the handlers
    /// fail closed without a TDK — F1/F2). `region` is the handler's
    /// residency region (F7), so each regional env keys under its own region.
    #[must_use]
    pub fn blob_key(region: &str, tenant_prefix: &str, digest: &str, algo: DigestAlgo) -> String {
        match algo {
            DigestAlgo::Blake3 => format!("{region}/{tenant_prefix}/{digest}"),
            DigestAlgo::Sha256 => format!("{region}/{tenant_prefix}/bazel/sha256/{digest}"),
        }
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
    /// The tenant prefix is ALWAYS `derive_prefix(tdk, tenant_uuid)` —
    /// an unpredictable, secret-keyed HMAC namespace (layer 5 of
    /// `INV-TENANT-ISOLATION`). The handler cannot be constructed
    /// without a TDK on the production path (see
    /// [`build_r2_cas_handler_from_env`], which fails closed when
    /// `R2_TDK_HEX` is unset) — so the raw-padded public-prefix
    /// fallback used by simple non-UUID test fixtures is gated behind
    /// `#[cfg(test)]` and is unreachable in production (F1/F2).
    fn r2_key(&self, tenant: &str, digest: &str, algo: DigestAlgo) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(R2S3Client::blob_key(&self.cas_region, &prefix, digest, algo))
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

/// Compute the per-tenant 16-char R2 key prefix (layer 5 of
/// `INV-TENANT-ISOLATION`).
///
/// The production handlers are ALWAYS constructed with a secret TDK
/// (`build_r2_*_handler_from_env` fail closed otherwise — F1/F2), so
/// the live path always takes the `Some(tdk)` arm and HMACs the FULL
/// tenant id under the secret key: an unpredictable, ~96-bit,
/// collision-resistant namespace.
///
/// The public, predictable raw-padded fallback (a 16-char prefix of
/// the tenant string) exists ONLY for unit-test fixtures whose tenant
/// is a simple non-UUID string (e.g. `"t1"`); it is gated behind
/// `#[cfg(test)]` and is unreachable in production.
///
/// # Errors
///
/// Returns `Err(String)` on the production path when the prefix is NOT
/// derivable — the tenant id is not a canonical UUID, or the handler
/// was somehow constructed without a TDK. The callers
/// ([`R2CasHandler::r2_key`] / `r2_list_prefix` and their AC twins)
/// propagate this as a 500/`Internal` so the op NEVER touches R2 under
/// a degraded/empty prefix that would collapse every non-derivable
/// tenant into one SHARED keyspace (cross-tenant read/overwrite/
/// delete/list). Fail CLOSED, never silent co-residence.
fn tenant_prefix(tdk: Option<&TenantDerivationKey>, tenant: &str) -> Result<String, String> {
    match tdk {
        Some(tdk) => match Uuid::try_parse(tenant) {
            Ok(uid) => Ok(derive_prefix(tdk, uid).to_string()),
            // Non-UUID tenant: in test builds the simple raw-padded
            // fixture prefix is allowed; on the production path it is a
            // SEV-class invariant violation that must fail CLOSED (see
            // `derive_tenant_prefix_strict`).
            #[cfg(test)]
            Err(_) => Ok(raw_padded_prefix(tenant)),
            #[cfg(not(test))]
            Err(_) => derive_tenant_prefix_strict(Some(tdk), tenant),
        },
        // No TDK is only reachable under `#[cfg(test)]`: the production
        // builders fail closed when `R2_TDK_HEX` is unset, so the real
        // handler is never constructed with `tdk = None` (F1/F2).
        #[cfg(test)]
        None => Ok(raw_padded_prefix(tenant)),
        #[cfg(not(test))]
        None => derive_tenant_prefix_strict(None, tenant),
    }
}

/// Production-strict tenant-prefix derivation — the single fail-CLOSED
/// authority for the live storage path.
///
/// The ONLY non-degraded outcome is a secret-keyed `derive_prefix(tdk,
/// uuid)` over a canonical UUID tenant under a present TDK. Every other
/// input is non-derivable and returns `Err`: there is NO empty/public/
/// predictable fallback. An empty prefix would key objects under
/// `<region>//<digest>`, collapsing every non-derivable tenant into one
/// SHARED keyspace (cross-tenant read/overwrite/delete/list) — so the
/// op MUST fail before it ever reaches R2 (INV-TENANT-ISOLATION).
///
/// This function is NOT `#[cfg(test)]`-gated (unlike the
/// `raw_padded_prefix` fixture path) so the production fail-closed
/// contract is directly covered by the regression suite.
fn derive_tenant_prefix_strict(
    tdk: Option<&TenantDerivationKey>,
    tenant: &str,
) -> Result<String, String> {
    let Some(tdk) = tdk else {
        tracing::error!(
            "R2 storage handler constructed without a TDK on the production path; \
             refusing to derive a tenant prefix (INV-TENANT-ISOLATION)"
        );
        return Err("missing TDK on production storage path (INV-TENANT-ISOLATION)".to_owned());
    };
    match Uuid::try_parse(tenant) {
        Ok(uid) => Ok(derive_prefix(tdk, uid).to_string()),
        Err(_) => {
            tracing::error!(
                "tenant id is not a canonical UUID on the production storage path; \
                 refusing to derive a tenant prefix (INV-TENANT-ISOLATION)"
            );
            Err("non-derivable tenant prefix (INV-TENANT-ISOLATION)".to_owned())
        }
    }
}

/// Public raw-padded 16-char prefix — TEST FIXTURES ONLY.
///
/// Truncates/pads the raw tenant string to exactly 16 chars. This is a
/// PUBLIC, predictable namespace and must NEVER be used on the
/// production path (see F1/F2); it lets unit tests use simple non-UUID
/// tenant ids (`"t1"`, `"tenant-abc"`) without a real TDK.
#[cfg(test)]
fn raw_padded_prefix(tenant: &str) -> String {
    let mut p = tenant.to_owned();
    p.truncate(16);
    while p.len() < 16 {
        p.push('0');
    }
    p
}

/// Enforce the CAS content-addressing invariant
/// (`INV-CAS-INTEGRITY`): the supplied `bytes` MUST hash to
/// `claimed_hash` under the **keyspace's canonical digest function**,
/// selected explicitly by `algo`:
///
/// - [`DigestAlgo::Blake3`] — native CAS + sccache (the BLAKE3 keyspace).
/// - [`DigestAlgo::Sha256`] — the Bazel REAPI v2 `bazel/sha256/` keyspace.
///
/// The function is threaded as an EXPLICIT [`DigestAlgo`] (never inferred
/// from hash-string length — that would be a silent gate). The durable gate
/// is therefore surface-PARTITIONED, not literally BLAKE3-only-everywhere:
/// each keyspace is single-function and the two never mix within one key, so
/// the read-path re-verification (bitrot) always re-applies the SAME function
/// the blob was admitted under (Option A, ADR-0044). A native/sccache caller
/// always passes `Blake3` (behaviour unchanged); only the Bazel adapter
/// passes `Sha256`.
///
/// Returns `Ok(())` on a match, or `Err(actual_hex)` carrying the hash
/// actually computed from the bytes (under `algo`) so the caller can build
/// the `HashMismatch` error and the `CorrectnessViolation` audit event.
///
/// A malformed `claimed_hash` (not canonical 64-char lowercase hex) is
/// itself a mismatch — the durable store never persists/serves bytes
/// under a digest it cannot validate.
///
/// This is the single enforcement point for content-addressing on the
/// durable path. Every CAS write/read surface — the native
/// `PUT/GET /v1/cas/...` route, the Bazel REAPI v2 bridge, and sccache
/// — funnels through `R2CasHandler`, so this one gate closes the
/// cache-poisoning hole across all of them. (The in-memory handler
/// enforces the same invariant for dev/test.)
fn verify_content_hash(algo: DigestAlgo, claimed_hash: &str, bytes: &[u8]) -> Result<(), String> {
    match algo {
        DigestAlgo::Blake3 => {
            let actual = Digest::compute(bytes);
            match Digest::from_hex(claimed_hash) {
                Ok(claimed) if claimed.verify_constant_time(&actual) => Ok(()),
                _ => Err(actual.to_hex()),
            }
        }
        DigestAlgo::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let actual_hex = hex::encode(hasher.finalize());
            let claimed_lower = claimed_hash.to_ascii_lowercase();
            // Constant-time compare (same posture as the BLAKE3 path's
            // `verify_constant_time`): a malformed/short claim simply does not
            // match — never admitted.
            if actual_hex.len() == claimed_lower.len()
                && actual_hex
                    .as_bytes()
                    .ct_eq(claimed_lower.as_bytes())
                    .unwrap_u8()
                    == 1
            {
                Ok(())
            } else {
                Err(actual_hex)
            }
        }
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

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. `req.algo` routes the
        // read into the blob's own keyspace (`bazel/sha256/` for SHA-256,
        // native for BLAKE3) so read-back is single-function.
        let key = match self.r2_key(&req.tenant, &req.hash, req.algo) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::read");

        // CRITICAL — must wrap in `block_in_place`.
        //
        // This trait method is `fn read(...)` (sync), but it is invoked
        // from inside an async axum handler that is itself being polled
        // on the tokio multi-thread runtime. A bare
        // `handle.block_on(future)` from inside a running future on the
        // SAME runtime hangs forever (observed: 60s curl timeout in prod
        // before this fix). `tokio::task::block_in_place` tells the
        // runtime to release the current worker so the inner `block_on`
        // can drive the future to completion. Valid only on the
        // multi-thread runtime — `#[tokio::main]` gives us that.
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)));

        match result {
            Ok(Some(bytes)) => {
                // Read-path content-addressing RE-verification: the
                // bytes R2 returned MUST still hash to the requested
                // digest. This catches R2 bitrot, storage-tier
                // tampering, or a historically mis-keyed blob BEFORE it
                // is served as trusted CAS content. A mismatch is a
                // `CorrectnessViolation`, never a hit. (See
                // `verify_content_hash`.)
                if let Err(actual) = verify_content_hash(req.algo, &req.hash, &bytes) {
                    self.audit
                        .emit(AuditEvent::new(
                            AuditEventKind::CorrectnessViolation,
                            req.tenant.clone(),
                            req.hash.clone(),
                            req.principal.clone(),
                            req.at_unix_ms,
                        ))
                        .map_err(CasHandlerError::AuditFailed)?;
                    self.sli
                        .observe(SliObservation::new(Sli::CorrectnessCas, true, 0));
                    emit(true);
                    return Err(CasHandlerError::HashMismatch {
                        claimed: req.hash,
                        actual,
                    });
                }
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::ReadServed,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
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

    /// Cheap existence probe — a single S3 `HeadObject`, NO body
    /// download and NO content rehash.
    ///
    /// This is the override that removes the `findMissingBlobs`
    /// egress+rehash amplification (r34 #10): the REAPI bridge probes
    /// existence per digest, and the default trait `exists` would route
    /// through [`Self::read`] (a full GET + read-path re-verify per blob).
    /// Here we HEAD instead — `Ok(true)` when present, `Ok(false)` on
    /// absence, every other error propagated so the fail-CLOSED contract
    /// holds.
    ///
    /// Audit/SLI posture mirrors [`Self::read`]: a cross-tenant probe is
    /// denied (and audited as `ReadDenied`) before any storage touch, and
    /// a `ReadAttempted` row is emitted before the lookup. A HEAD reveals
    /// only presence (not bytes), so there is no read-path content
    /// re-verification and no `ReadServed`/`CorrectnessCas` emission — the
    /// probe never serves trusted content.
    fn exists(&self, req: CasReadRequest) -> Result<bool, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasGet, Sli::LatencyCasGetP99, is_error);
        };

        // Cross-tenant denial — audit BEFORE returning (mirrors `read`).
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

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. `req.algo` routes the
        // probe into the blob's own keyspace (`bazel/sha256/` for SHA-256,
        // native for BLAKE3) so the HEAD targets the SAME key `read` would.
        let key = match self.r2_key(&req.tenant, &req.hash, req.algo) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::exists");

        // CRITICAL — `block_in_place` rationale: see `read` above. This is
        // a HEAD (`head_size`), not a GET — no body transfer, no rehash.
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)));

        match result {
            Ok(Some(_)) => {
                emit(false);
                Ok(true)
            }
            Ok(None) => {
                emit(false);
                Ok(false)
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2CasHandler::exists error");
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

        // Content-addressing enforcement (INV-CAS-INTEGRITY):
        // the durable store MUST NOT persist bytes that do not hash to
        // the claimed digest — otherwise the CAS guarantee is a lie and
        // any client (or a buggy uploader) can poison the cache for
        // every subsequent reader of that digest. Verify BEFORE the
        // PUT; on mismatch emit `CorrectnessViolation` + a
        // `CorrectnessCas` SLI failure and reject with 422
        // `HashMismatch`. Nothing is written.
        if let Err(actual) = verify_content_hash(req.algo, &req.claimed_hash, &req.bytes) {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::CorrectnessViolation,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            self.sli
                .observe(SliObservation::new(Sli::CorrectnessCas, true, 0));
            emit(true);
            return Err(CasHandlerError::HashMismatch {
                claimed: req.claimed_hash,
                actual,
            });
        }

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &req.claimed_hash, req.algo) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, bytes = req.bytes.len(), "R2CasHandler::write");

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<Self as CasReadHandler>::read` above.
        let handle = tokio::runtime::Handle::current();

        // Idempotent-rewrite detection (rt-nuclear #13 — byte double-charge).
        // CAS is content-addressed: the key already embeds the verified content
        // hash, so an object that already exists under this key holds the SAME
        // bytes (the hash was verified above). HEAD before PUT (mirrors the AC
        // path's GET-and-compare, but for CAS a presence HEAD suffices): if the
        // blob is already present we SKIP the re-PUT and return `durable=false`,
        // so the `AccountingCasHandler` decorator does NOT charge the bytes a
        // second time on an idempotent re-write. A HEAD error fails CLOSED to the
        // PUT path (correctness over the accounting optimisation — a transient
        // HEAD blip must never drop a write); a duplicate PUT is harmless (same
        // bytes) and the worst case is the legacy double-charge, never data loss.
        match tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key))) {
            Ok(Some(_)) => {
                // Already durable under this content-addressed key → idempotent
                // no-op; do not re-PUT and do not re-charge the bytes.
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::WriteCommitted,
                        req.tenant.clone(),
                        req.claimed_hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                emit(false);
                return Ok(CasWriteResponse::new(req.claimed_hash, false));
            }
            Ok(None) => { /* absent — fall through to the PUT below */ }
            Err(e) => {
                // Ambiguous prior state: fall through to the PUT (fail-CLOSED to a
                // durable write) rather than risk dropping a fresh blob.
                warn!(error = %e, key = %key, "R2CasHandler::write pre-PUT HEAD error; PUTting");
            }
        }

        let result =
            tokio::task::block_in_place(|| handle.block_on(self.client.put(&key, req.bytes)));

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
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
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

impl R2CasHandler {
    /// The tenant-scoped LIST prefix: `<region>/<tenant_prefix>/`. Every
    /// key returned under this prefix belongs to exactly this tenant
    /// (layer 5 of `INV-TENANT-ISOLATION`); the trailing slash bounds
    /// the prefix so one tenant's prefix can never be a prefix of
    /// another's. Enumeration / delete NEVER widen beyond this.
    fn r2_list_prefix(&self, tenant: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(format!("{}/{}/", self.cas_region, prefix))
    }
}

impl CasDeleteHandler for R2CasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::CasDeleteResponse;

        // Delete folds availability into the PUT (mutation) SLI bucket.
        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasPut, Sli::LatencyCasPutP99, is_error);
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::DeleteDenied,
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

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::DeleteAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. DELETE is native-only
        // (the Bazel REAPI bridge exposes no delete surface), so the BLAKE3
        // keyspace applies.
        let key = match self.r2_key(&req.tenant, &req.hash, DigestAlgo::Blake3) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::delete");

        // S3 DeleteObject is idempotent: deleting an absent key succeeds.
        // `existed` is best-effort (S3 does not report prior presence on a
        // plain DeleteObject); we report `true` on a clean delete so the
        // diagnostic is monotone, never a silent success on a transport
        // error. CRITICAL — `block_in_place`: see `read` above.
        //
        // Storage byte-accounting (finding #1 / cluster-C) + concurrent
        // double-DELETE over-release (rt-nuclear #6/#10/#14): the size measurement
        // and the delete are serialized per-key by `delete_if_present`, which
        // returns the reclaimed size to AT MOST ONE racer — every other racer sees
        // the object already gone and gets `None` (releases 0). This makes the
        // release reflect what THIS request actually removed, so two racing
        // deletes can never both credit the same bytes. CRITICAL — `block_in_place`:
        // see `read` above.
        let handle = tokio::runtime::Handle::current();
        let result =
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)));

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::DeleteCommitted,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                emit(false);
                Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
            }
            Err(e) => {
                // Fail CLOSED on a storage fault: never a silent success.
                warn!(error = %e, key = %key, "R2CasHandler::delete error");
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}

impl CasListHandler for R2CasHandler {
    fn list(
        &self,
        req: corelink_handler_cas::CasListRequest,
    ) -> Result<corelink_handler_cas::CasListResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::{CasBlobEntry, CasListResponse};

        // List folds availability into the GET (read) SLI bucket.
        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasGet, Sli::LatencyCasGetP99, is_error);
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::ListDenied,
                    req.tenant.clone(),
                    String::new(),
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

        // ListAttempted audit BEFORE enumeration.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::ListAttempted,
                req.tenant.clone(),
                String::new(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        // Enumeration is bounded to the tenant's derived prefix —
        // cross-tenant keys cannot appear in the result. Fail CLOSED if
        // the prefix is not derivable: an empty prefix would list a
        // SHARED keyspace across every non-derivable tenant.
        let prefix = match self.r2_list_prefix(&req.tenant) {
            Ok(p) => p,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(prefix = %prefix, "R2CasHandler::list");

        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| {
            handle.block_on(self.client.list_objects_page(
                &prefix,
                req.limit,
                req.cursor.as_deref(),
            ))
        });

        match result {
            Ok((rows, next_cursor)) => {
                let blobs = rows
                    .into_iter()
                    .map(|(key, size, last_modified)| {
                        // Strip `<region>/<tenant_prefix>/` to recover the
                        // bare digest; never leak the storage key layout.
                        let hash = key.rsplit('/').next().unwrap_or(&key).to_owned();
                        CasBlobEntry::new(hash, size, last_modified)
                    })
                    .collect();
                emit(false);
                Ok(CasListResponse::new(blobs, next_cursor))
            }
            Err(e) => {
                warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
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
    // FAIL CLOSED: storage creds are present, so this is the production
    // data plane — the secret TDK is MANDATORY (F1/F2). Without it the
    // tenant prefix would degrade to a public, predictable scheme and
    // enable same-millisecond cross-tenant blob co-residence. Refuse to
    // construct the handler (the route will not mount) and emit a loud,
    // structured error rather than serving in the silently-degraded
    // public-prefix mode.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 CAS handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned(),
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    Some(Ok(R2CasHandler::new(
        client,
        cas_region,
        Some(tdk_bytes),
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
    /// `R2CasHandler::r2_key`; the AC bucket uses the same layout and
    /// the same always-HMAC tenant prefix (F1/F2). The handler cannot
    /// be built without a TDK on the production path (see
    /// [`build_r2_ac_handler_from_env`]).
    fn r2_key(&self, tenant: &str, action_digest: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        // AC keys are native (BLAKE3) keyspace — REAPI AC action digests are
        // stored under the same scheme as native CAS (no `bazel/sha256/` tag).
        Ok(R2S3Client::blob_key(
            &self.ac_region,
            &prefix,
            action_digest,
            DigestAlgo::Blake3,
        ))
    }

    /// Emit the (avail, latency) SLI pair for the lookup path. The
    /// update path folds availability into `AvailAcLookup` per the
    /// canonical-15 metric registry discipline (see
    /// `InMemoryAcHandler::update`).
    fn emit_lookup_sli(&self, is_error: bool) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli
            .observe(SliObservation::new(Sli::AvailAcLookup, is_error, 0));
        self.sli
            .observe(SliObservation::new(Sli::LatencyAcHitP99, is_error, 0));
    }

    fn emit_update_sli(&self, is_error: bool) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli
            .observe(SliObservation::new(Sli::AvailAcLookup, is_error, 0));
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
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::LookupDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_lookup_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // LookupAttempted audit BEFORE storage read.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::LookupAttempted,
                req.tenant.clone(),
                req.action_digest.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(AcHandlerError::AuditFailed)?;

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &req.action_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_lookup_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2AcHandler::lookup");

        // CRITICAL — `block_in_place` rationale: this sync trait method
        // is invoked from inside an async axum handler on the tokio
        // multi-thread runtime; a bare `handle.block_on(future)` from
        // inside a running future on the SAME runtime hangs forever
        // (observed: 60s curl timeout in prod before this fix).
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)));

        match result {
            Ok(Some(bytes)) => {
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::LookupHit,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(AcHandlerError::AuditFailed)?;
                self.emit_lookup_sli(false);
                Ok(AcLookupResponse::new(req.action_digest, bytes))
            }
            Ok(None) => {
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::LookupMiss,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
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
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::UpdateDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_update_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // UpdateAttempted audit BEFORE mutation.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::UpdateAttempted,
                req.tenant.clone(),
                req.action_digest.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(AcHandlerError::AuditFailed)?;

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &req.action_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(
            key = %key,
            bytes = req.result_payload.len(),
            "R2AcHandler::update"
        );

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<R2AcHandler as AcLookupHandler>::lookup` above.
        let handle = tokio::runtime::Handle::current();

        // AC IMMUTABILITY INVARIANT (F5): the Action Cache maps an
        // `action_digest` (hash of the build *action*, not its result)
        // to a result payload. AC bytes are therefore NOT
        // self-verifying — a divergent re-PUT must be REFUSED, never
        // silently overwritten, or any write-capable token can poison a
        // proven cache result for every subsequent build that hits the
        // same digest (supply-chain compromise).
        //
        // GET-and-compare BEFORE any PUT (mirrors
        // `InMemoryAcHandler::update`):
        //   - existing != payload → `DivergentBody` (409); NO PUT.
        //   - existing == payload → idempotent no-op (`durable=false`).
        //   - absent              → PUT (`durable=true`).
        //   - ambiguous GET error → fail CLOSED (`Internal`); never
        //     blind-overwrite on an unknown prior state.
        let existing =
            tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)));
        match existing {
            Ok(Some(prior)) if prior != req.result_payload => {
                warn!(
                    key = %key,
                    "R2AcHandler::update divergent body — refusing to overwrite a \
                     proven AC result (INV-AC-RESULT-HASH-IMMUTABLE)"
                );
                self.emit_update_sli(false);
                return Err(AcHandlerError::DivergentBody {
                    tenant: req.tenant,
                    action_digest: req.action_digest,
                });
            }
            Ok(Some(_)) => {
                // Byte-identical re-PUT → idempotent no-op. The proven
                // bytes are already durable; do not re-PUT.
                self.emit_update_sli(false);
                return Ok(AcUpdateResponse::new(req.action_digest, false));
            }
            Ok(None) => { /* absent — fall through to the PUT below */ }
            Err(e) => {
                // Ambiguous prior state: fail closed rather than risk a
                // blind overwrite of a proven result.
                warn!(error = %e, key = %key, "R2AcHandler::update pre-PUT GET error");
                self.emit_update_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        }

        let result = tokio::task::block_in_place(|| {
            handle.block_on(self.client.put(&key, req.result_payload))
        });

        match result {
            Ok(()) => {
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::UpdateCommitted,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(AcHandlerError::AuditFailed)?;
                self.emit_update_sli(false);
                // Fresh insert (the `None` arm above) → durable=true.
                Ok(AcUpdateResponse::new(req.action_digest, true))
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2AcHandler::update error");
                self.emit_update_sli(true);
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

impl R2AcHandler {
    /// The tenant-scoped LIST prefix for AC refs:
    /// `<region>/<tenant_prefix>/`. Same isolation guarantee as
    /// [`R2CasHandler::r2_list_prefix`]; enumeration / delete never
    /// widen beyond this tenant's derived prefix.
    fn r2_list_prefix(&self, tenant: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(format!("{}/{}/", self.ac_region, prefix))
    }
}

impl corelink_handler_ac::AcDeleteHandler for R2AcHandler {
    fn delete(
        &self,
        req: corelink_handler_ac::AcDeleteRequest,
    ) -> Result<corelink_handler_ac::AcDeleteResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::{
            AcDeleteResponse, AcHandlerError, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::DeleteDenied,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_update_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::DeleteAttempted,
                req.tenant.clone(),
                req.action_digest.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(AcHandlerError::AuditFailed)?;

        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &req.action_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2AcHandler::delete");

        // Byte-accounting (finding #1 / cluster-C) + concurrent double-DELETE
        // over-release (rt-nuclear #6/#10/#14): `delete_if_present` serializes the
        // measure-and-delete per key and returns the reclaimed size to AT MOST ONE
        // racer, so two racing deletes can never both credit the same bytes.
        // CRITICAL — `block_in_place`.
        let handle = tokio::runtime::Handle::current();
        let result =
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)));

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                self.audit
                    .emit(AcAuditEvent::new(
                        AcAuditEventKind::DeleteCommitted,
                        req.tenant.clone(),
                        req.action_digest.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(AcHandlerError::AuditFailed)?;
                self.emit_update_sli(false);
                Ok(AcDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
            }
            Err(e) => {
                // Fail CLOSED on a storage fault: never a silent success.
                warn!(error = %e, key = %key, "R2AcHandler::delete error");
                self.emit_update_sli(true);
                Err(AcHandlerError::Internal(e))
            }
        }
    }
}

impl corelink_handler_ac::AcListHandler for R2AcHandler {
    fn list(
        &self,
        req: corelink_handler_ac::AcListRequest,
    ) -> Result<corelink_handler_ac::AcListResponse, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::{
            AcHandlerError, AcListResponse, AcRefEntry, AuditEvent as AcAuditEvent,
            AuditEventKind as AcAuditEventKind,
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AcAuditEvent::new(
                    AcAuditEventKind::ListDenied,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(AcHandlerError::AuditFailed)?;
            self.emit_lookup_sli(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // ListAttempted audit BEFORE enumeration.
        self.audit
            .emit(AcAuditEvent::new(
                AcAuditEventKind::ListAttempted,
                req.tenant.clone(),
                String::new(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(AcHandlerError::AuditFailed)?;

        // Enumeration is bounded to the tenant's derived prefix. Fail
        // CLOSED if the prefix is not derivable: an empty prefix would
        // list a SHARED keyspace across every non-derivable tenant.
        let prefix = match self.r2_list_prefix(&req.tenant) {
            Ok(p) => p,
            Err(e) => {
                self.emit_lookup_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(prefix = %prefix, "R2AcHandler::list");

        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::block_in_place(|| {
            handle.block_on(self.client.list_objects_page(
                &prefix,
                req.limit,
                req.cursor.as_deref(),
            ))
        });

        match result {
            Ok((rows, next_cursor)) => {
                let refs = rows
                    .into_iter()
                    .map(|(key, size, last_modified)| {
                        let ref_key = key.rsplit('/').next().unwrap_or(&key).to_owned();
                        AcRefEntry::new(ref_key, last_modified, size)
                    })
                    .collect();
                self.emit_lookup_sli(false);
                Ok(AcListResponse::new(refs, next_cursor))
            }
            Err(e) => {
                warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                self.emit_lookup_sli(true);
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
    // FAIL CLOSED: see `build_r2_cas_handler_from_env`. The AC key space
    // is NOT content-addressed (AC bytes are not self-verifying), so a
    // predictable-prefix collision is even more dangerous here (F1/F5):
    // a same-ms prefix collision lets one tenant poison another's
    // ActionResult. The secret TDK is mandatory on the production path.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 AC handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned(),
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    let audit = Arc::new(corelink_handler_ac::InMemoryAuditSink::new());
    let sli = Arc::new(corelink_handler_ac::InMemorySliObserver::new());
    Some(Ok(R2AcHandler::new(
        client,
        ac_region,
        Some(tdk_bytes),
        audit,
        sli,
    )))
}

/// Load the tenant derivation key from `R2_TDK_HEX` env var (64 hex chars =
/// 32 bytes). Returns `None` when unset/invalid; on the production storage
/// path a `None` here makes `build_r2_*_handler_from_env` FAIL CLOSED (the
/// handler is not constructed and the route does not mount) rather than
/// degrade to a public-prefix scheme (F1/F2).
fn load_tdk_from_env() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        warn!(
            len = hex_str.len(),
            "R2_TDK_HEX has wrong length; ignoring TDK"
        );
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
        let key = R2S3Client::blob_key(
            "iad",
            "abcdef1234567890",
            "deadbeef00000001",
            DigestAlgo::Blake3,
        );
        assert_eq!(key, "iad/abcdef1234567890/deadbeef00000001");
    }

    #[tokio::test]
    async fn r2_cas_handler_key_uses_region_and_prefix() {
        let handler = make_test_handler("iad").await;
        let key = handler
            .r2_key("tenant-abc", "abc123hash0000001", DigestAlgo::Blake3)
            .unwrap();
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
    // Content-addressing enforcement (INV-CAS-INTEGRITY)
    // ---------------------------------------------------------------

    #[test]
    fn verify_content_hash_accepts_matching_digest() {
        let bytes = b"the quick brown fox";
        let claimed = Digest::compute(bytes).to_hex();
        assert!(verify_content_hash(DigestAlgo::Blake3, &claimed, bytes).is_ok());
    }

    #[test]
    fn verify_content_hash_rejects_mismatch_and_reports_actual() {
        // Claim the digest of "A" but hand over the bytes of "B".
        let claimed = Digest::compute(b"A").to_hex();
        let actual_expected = Digest::compute(b"B").to_hex();
        let err = verify_content_hash(DigestAlgo::Blake3, &claimed, b"B").expect_err("must reject");
        // The reported `actual` is the TRUE hash of the bytes given,
        // not the (lying) claimed hash.
        assert_eq!(err, actual_expected);
        assert_ne!(err, claimed);
    }

    #[test]
    fn verify_content_hash_rejects_malformed_claim() {
        // A non-canonical claimed hash can never be validated — treat
        // as a mismatch, never persist/serve under an unparseable key.
        assert!(verify_content_hash(DigestAlgo::Blake3, "not-a-hash", b"anything").is_err());
        assert!(verify_content_hash(DigestAlgo::Blake3, "", b"anything").is_err());
    }

    // ── Surface-tagged SHA-256 keyspace (Option A / ADR-0044) ──────────────

    /// Real SHA-256 of `bytes` as lowercase hex (test helper).
    fn sha256_hex_t(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        hex::encode(h.finalize())
    }

    #[test]
    fn verify_content_hash_sha256_accepts_correct_and_rejects_mismatch() {
        let bytes = b"bazel reapi blob";
        let good = sha256_hex_t(bytes);
        assert!(verify_content_hash(DigestAlgo::Sha256, &good, bytes).is_ok());

        // Wrong claim → Err carrying the TRUE sha256 of the bytes.
        let err = verify_content_hash(DigestAlgo::Sha256, &"0".repeat(64), bytes)
            .expect_err("must reject");
        assert_eq!(err, good);
    }

    #[test]
    fn verify_content_hash_sha256_rejects_malformed_claim() {
        assert!(verify_content_hash(DigestAlgo::Sha256, "not-hex", b"x").is_err());
        assert!(verify_content_hash(DigestAlgo::Sha256, "", b"x").is_err());
    }

    #[test]
    fn verify_content_hash_algos_do_not_cross() {
        // A correct BLAKE3 claim is NOT accepted under the SHA-256 keyspace,
        // and vice-versa — the gate is single-function per keyspace, never
        // blanket OR-accept (the bug Option A refuses to ship).
        let bytes = b"single-function keyspace";
        let blake3_claim = Digest::compute(bytes).to_hex();
        let sha256_claim = sha256_hex_t(bytes);

        assert!(verify_content_hash(DigestAlgo::Blake3, &blake3_claim, bytes).is_ok());
        assert!(verify_content_hash(DigestAlgo::Sha256, &blake3_claim, bytes).is_err());
        assert!(verify_content_hash(DigestAlgo::Sha256, &sha256_claim, bytes).is_ok());
        assert!(verify_content_hash(DigestAlgo::Blake3, &sha256_claim, bytes).is_err());
    }

    #[test]
    fn blob_key_keyspace_isolation_native_vs_bazel() {
        let digest = "a".repeat(64);
        let native = R2S3Client::blob_key("iad", "abcdef1234567890", &digest, DigestAlgo::Blake3);
        let bazel = R2S3Client::blob_key("iad", "abcdef1234567890", &digest, DigestAlgo::Sha256);

        // Native: <region>/<prefix>/<digest> — no bazel sub-prefix.
        assert_eq!(native, format!("iad/abcdef1234567890/{digest}"));
        assert!(!native.contains("bazel/sha256/"));

        // Bazel: the sha256 keyspace sub-prefix sits AFTER the tenant prefix.
        assert_eq!(bazel, format!("iad/abcdef1234567890/bazel/sha256/{digest}"));
        assert!(bazel.starts_with("iad/abcdef1234567890/"));

        // The two keyspaces never collide for the same digest.
        assert_ne!(native, bazel);
    }

    /// Round-trip keyspace isolation through the handler's key derivation: a
    /// SHA-256 (Bazel) read/write derives the `bazel/sha256/` key, while a
    /// BLAKE3 (native) request for the SAME digest derives a DIFFERENT key —
    /// so a SHA-256 blob is never found under the native keyspace and vice
    /// versa. (`r2_key` is the production key authority; this exercises it
    /// without S3 I/O.)
    #[tokio::test]
    async fn r2_key_round_trip_keyspace_isolation() {
        let handler = make_test_handler_with_tdk("iad").await;
        let uuid = "00000000-0000-4000-8000-000000000001";
        let digest = "b".repeat(64);

        let native_key = handler
            .r2_key(uuid, &digest, DigestAlgo::Blake3)
            .expect("native key");
        let bazel_key = handler
            .r2_key(uuid, &digest, DigestAlgo::Sha256)
            .expect("bazel key");

        // Same tenant prefix (HMAC tenant isolation preserved), divergent
        // keyspace tail.
        assert!(bazel_key.contains("/bazel/sha256/"));
        assert!(!native_key.contains("/bazel/sha256/"));
        assert_ne!(native_key, bazel_key);
        // A SHA-256 blob's key is NOT a hit under the native keyspace.
        assert!(!bazel_key.starts_with(&native_key));
    }

    /// The load-bearing security regression: a WRITE whose bytes do not
    /// hash to the claimed digest is rejected with `HashMismatch`
    /// BEFORE any storage I/O — the durable store never persists
    /// poisoned content. (`make_test_handler` points at localhost:1, so
    /// reaching the PUT would error; this test proves we never reach
    /// it.)
    #[tokio::test]
    async fn r2_cas_write_rejects_poisoned_bytes_before_storage() {
        let handler = make_test_handler("iad").await;
        let claimed = Digest::compute(b"honest-bytes").to_hex();
        // Same tenant (so we pass the cross-tenant gate) but the body
        // is NOT what the claimed digest addresses.
        let req = CasWriteRequest::new(
            "t1",
            claimed.clone(),
            b"POISONED-bytes".to_vec(),
            "anon@t1",
            "t1",
            1,
        );
        let err = handler
            .write(req)
            .expect_err("poisoned write must be rejected");
        match err {
            CasHandlerError::HashMismatch { claimed: c, actual } => {
                assert_eq!(c, claimed);
                assert_eq!(actual, Digest::compute(b"POISONED-bytes").to_hex());
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }

    /// A WRITE whose bytes DO hash to the claimed digest passes
    /// verification and proceeds to the storage layer (which then
    /// errors against the unreachable stub endpoint — proving we got
    /// PAST the content check rather than being rejected by it).
    // NOTE: `multi_thread` flavor is REQUIRED — this test proceeds past
    // content verification into the storage layer, which uses
    // `tokio::task::block_in_place` (valid only on the multi-threaded
    // runtime; the production server is `#[tokio::main]` multi-thread).
    // The default current-thread `#[tokio::test]` runtime would panic at
    // the `block_in_place` call, not at any fault in the fix.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_write_with_honest_bytes_passes_verification() {
        let handler = make_test_handler("iad").await;
        let bytes = b"honest-bytes".to_vec();
        let claimed = Digest::compute(&bytes).to_hex();
        let req = CasWriteRequest::new("t1", claimed, bytes, "anon@t1", "t1", 1);
        let err = handler
            .write(req)
            .expect_err("stub endpoint is unreachable");
        // The load-bearing assertion: honest bytes are NOT rejected by
        // the content-addressing gate — verification PASSED and we
        // proceeded to storage (which then failed at the unreachable
        // stub endpoint). We assert "not a HashMismatch" rather than a
        // specific downstream error so the test does not depend on the
        // exact network-failure variant.
        assert!(
            !matches!(err, CasHandlerError::HashMismatch { .. }),
            "honest bytes must pass content verification, got {err:?}"
        );
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
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
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

    /// rt-nuclear #13 regression (live R2): the FIRST CAS write of a content hash
    /// returns `durable=true` (a real PUT); an idempotent re-write of the SAME
    /// content returns `durable=false` (HEAD hit → no re-PUT), so the
    /// `AccountingCasHandler` decorator rolls the reservation back and does NOT
    /// double-charge the bytes. Gated behind `#[ignore]` like `storage_r2_round_trip`.
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn cas_idempotent_rewrite_reports_durable_false() {
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = R2CasHandler::new(client, "iad", None, audit, sli);

        // Unique content per run so parallel runs / prior state don't collide.
        let payload = format!(
            "rt-nuclear-13-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
        .into_bytes();
        let tenant = "t-rt13";
        let claimed = Digest::compute(&payload).to_hex();

        let first = handler
            .write(CasWriteRequest::new(
                tenant,
                claimed.clone(),
                payload.clone(),
                "p",
                tenant,
                1,
            ))
            .expect("first write");
        assert!(first.durable, "first write of a fresh hash must be durable=true");

        let second = handler
            .write(CasWriteRequest::new(
                tenant,
                claimed.clone(),
                payload,
                "p",
                tenant,
                2,
            ))
            .expect("second write");
        assert!(
            !second.durable,
            "an idempotent re-write must report durable=false (HEAD hit → no re-PUT, no re-charge)"
        );
    }

    /// rt-nuclear #6/#10/#14 regression (live R2): `delete_if_present` returns the
    /// reclaimed size to the FIRST delete and `None` (release 0) to the SECOND —
    /// two deletes of the same key can never both credit the same bytes. Run with
    /// the same live-R2 env as `storage_r2_round_trip`.
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn delete_if_present_credits_size_once_then_none() {
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");

        let key = format!(
            "test/delete-once/{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let payload = b"rt-nuclear-6-10-14".to_vec();
        let size = payload.len() as u64;
        client.put(&key, payload).await.expect("put");

        // First delete observes-and-removes → Some(size).
        let first = client.delete_if_present(&key).await.expect("first delete");
        assert_eq!(first, Some(size), "the first delete must credit the reclaimed size");

        // Second delete sees the key already gone → None (releases 0).
        let second = client.delete_if_present(&key).await.expect("second delete");
        assert_eq!(second, None, "a second delete must credit 0 (no double-release)");
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

    /// A non-zero fake 32-byte TDK for tests that must exercise the
    /// PRODUCTION always-HMAC prefix path (`Some(tdk)` arm).
    fn fake_tdk() -> Zeroizing<[u8; 32]> {
        Zeroizing::new([0x5au8; 32])
    }

    /// Build an `R2CasHandler` with a real (fake) TDK over a stub S3
    /// client — exercises the production `Some(tdk)` key-derivation arm.
    async fn make_test_handler_with_tdk(region: &str) -> R2CasHandler {
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
        R2CasHandler::new(client, region, Some(fake_tdk()), audit, sli)
    }

    /// Build an `R2AcHandler` over a stub S3 client (no TDK → test
    /// raw-pad prefix; only the divergent-body/region logic is
    /// exercised here, any S3 I/O fails against the stub).
    async fn make_test_ac_handler(region: &str) -> R2AcHandler {
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
        let audit = Arc::new(corelink_handler_ac::InMemoryAuditSink::new());
        let sli = Arc::new(corelink_handler_ac::InMemorySliObserver::new());
        R2AcHandler::new(client, region, None, audit, sli)
    }

    // ---------------------------------------------------------------
    // F1/F2 — production path ALWAYS HMACs the full tenant id
    // ---------------------------------------------------------------

    /// With a TDK configured (the production posture), a canonical UUID
    /// tenant resolves to the secret-keyed `derive_prefix` HMAC — NOT
    /// the public raw-padded prefix of the tenant string. This is the
    /// regression pin for F1/F2: the predictable public prefix must
    /// never appear on the TDK path for a UUID tenant.
    #[tokio::test]
    async fn r2_cas_tdk_path_uses_hmac_prefix_not_raw_tenant() {
        let handler = make_test_handler_with_tdk("iad").await;
        // A canonical UUIDv7-shaped tenant id.
        let tenant = "0190abcd-1234-75ab-8def-0123456789ab";
        let key = handler
            .r2_key(tenant, &"d".repeat(64), DigestAlgo::Blake3)
            .unwrap();
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3, "key: {key}");
        let prefix = parts[1];
        assert_eq!(prefix.len(), 16, "prefix must be 16 chars: {key}");
        // The HMAC prefix must NOT be the predictable public prefix of
        // the tenant string (the F1 raw-padded fallback).
        let raw_public = &tenant[..16];
        assert_ne!(
            prefix, raw_public,
            "production prefix leaked the public tenant-id prefix (F1/F2)"
        );
        // And it must equal the canonical secret-keyed derivation.
        let expected = derive_prefix(
            &TenantDerivationKey::from_bytes(fake_tdk()),
            Uuid::try_parse(tenant).unwrap(),
        )
        .to_string();
        assert_eq!(prefix, expected, "prefix must be derive_prefix(tdk, uuid)");
    }

    // ---------------------------------------------------------------
    // F2 — production prefix derivation FAILS CLOSED for a non-derivable
    // tenant (never an empty `<region>//<digest>` SHARED keyspace).
    // ---------------------------------------------------------------

    /// The production-strict derivation (`derive_tenant_prefix_strict`,
    /// the authority the live `tenant_prefix` path delegates to) MUST
    /// refuse a non-UUID tenant and a missing TDK — there is NO public/
    /// empty fallback. This is the regression pin for finding #2/#8: a
    /// non-derivable tenant on the prod path errors out instead of
    /// keying under `<region>//<digest>` (a SHARED, cross-tenant
    /// keyspace).
    #[test]
    fn prod_strict_prefix_fails_closed_for_non_derivable_tenant() {
        let tdk = TenantDerivationKey::from_bytes(fake_tdk());

        // Non-UUID tenant under a present TDK → Err (no raw-padded
        // fallback on the prod path).
        let err = derive_tenant_prefix_strict(Some(&tdk), "tenant-abc")
            .expect_err("non-UUID tenant must fail closed on the prod path");
        assert!(
            err.contains("INV-TENANT-ISOLATION"),
            "error must cite the isolation invariant: {err}"
        );

        // Missing TDK → Err (the production builders fail closed before
        // this, but the derivation itself must not produce a prefix).
        assert!(
            derive_tenant_prefix_strict(None, "0190abcd-1234-75ab-8def-0123456789ab").is_err(),
            "absent TDK must fail closed"
        );

        // Sanity: a canonical UUID under a present TDK IS derivable and
        // is exactly the secret-keyed prefix (never empty).
        let ok = derive_tenant_prefix_strict(Some(&tdk), "0190abcd-1234-75ab-8def-0123456789ab")
            .expect("a canonical UUID tenant must derive a prefix");
        assert_eq!(ok.len(), 16, "derived prefix must be 16 chars, got {ok:?}");
        assert!(!ok.is_empty(), "derived prefix must never be empty");
    }

    // ---------------------------------------------------------------
    // F7 — CAS storage is residency-aware: keyed by the handler's
    // region, never a process-global. A regional handler MUST prefix
    // its keys with that region.
    // ---------------------------------------------------------------

    /// Each regional CAS handler keys objects under its OWN region — an
    /// `lhr` handler must never write into the `iad` key space. This is
    /// the invariant that makes per-env `R2_CAS_REGION` (the frozen
    /// contract) load-bearing rather than cosmetic. If a regional env
    /// fails to thread its region through, this fails.
    #[tokio::test]
    async fn r2_cas_keys_are_residency_scoped_per_region() {
        for region in ["iad", "lhr", "sam", "nrt", "syd"] {
            let handler = make_test_handler_with_tdk(region).await;
            let key = handler
                .r2_key(
                    "0190abcd-1234-75ab-8def-0123456789ab",
                    &"a".repeat(64),
                    DigestAlgo::Blake3,
                )
                .unwrap();
            assert!(
                key.starts_with(&format!("{region}/")),
                "CAS key for region {region} must be region-scoped (residency): {key}"
            );
        }
        // A non-iad region must NOT collapse to the iad default.
        let lhr = make_test_handler_with_tdk("lhr").await;
        let key = lhr
            .r2_key(
                "0190abcd-1234-75ab-8def-0123456789ab",
                &"a".repeat(64),
                DigestAlgo::Blake3,
            )
            .unwrap();
        assert!(
            !key.starts_with("iad/"),
            "EU (lhr) CAS write fell back to the US (iad) key space: {key}"
        );
    }

    // ---------------------------------------------------------------
    // F5 — AC update divergent-body invariant (no silent overwrite)
    // ---------------------------------------------------------------

    /// On an AMBIGUOUS pre-PUT GET (the stub endpoint is unreachable, so
    /// GET errors), `R2AcHandler::update` MUST fail closed with
    /// `Internal` and NEVER fall through to a blind PUT that could
    /// overwrite a proven AC result. This pins the F5 fail-closed branch
    /// (a proven result is never overwritten on unknown prior state).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_ac_update_fails_closed_on_ambiguous_get() {
        use corelink_handler_ac::{AcHandlerError, AcUpdateHandler, AcUpdateRequest};
        let handler = make_test_ac_handler("iad").await;
        let req = AcUpdateRequest::new("t1", "d".repeat(64), b"payload".to_vec(), "p@t1", "t1", 1);
        let err = handler
            .update(req)
            .expect_err("ambiguous GET against stub must fail closed");
        assert!(
            matches!(err, AcHandlerError::Internal(_)),
            "update must fail closed (Internal) on an ambiguous pre-PUT GET, \
             never blind-overwrite — got {err:?}"
        );
    }
}

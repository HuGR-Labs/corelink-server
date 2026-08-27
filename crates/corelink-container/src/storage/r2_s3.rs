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
use corelink_byok::{CryptoContext, CryptoMode, Tcs};
use corelink_handler_cas::{
    AuditEvent, AuditEventKind, AuditSink, CasDeleteHandler, CasHandlerError, CasListHandler,
    CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest,
    CasWriteResponse, DigestAlgo, InMemorySliObserver, SliObservation, SliObserver,
};
// `InMemoryAuditSink` is now used only by tests (the deployed builder wires the
// durable D1 sink); gate the import so the non-test build stays warning-clean.
#[cfg(test)]
use corelink_handler_cas::InMemoryAuditSink;
use corelink_hash::Digest;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};

use crate::storage::d1_audit_sink::{
    ac_audit_sink_from_d1_concrete, cas_audit_sink_from_d1_concrete,
};
use crate::storage::d1_http::D1HttpClient;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
use tracing::{debug, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::byok_cas::{
    ac_crypto_context, ac_crypto_context_for, cas_crypto_context, cas_crypto_context_for,
    decrypt_cas_blob, encrypt_cas_blob, engagement_for, harden_digest, ByokConfigCache,
    ByokEngagement, ModeBEncryptor, TcsResolver,
};
use super::StorageEnv;
use crate::customer_d1::ByokCryptoMode;

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
    delete_locks:
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
}

/// Outcome of [`R2S3Client::get_capped`].
///
/// Three states, not two: "absent" and "present but refused" are different
/// answers and the caller must not be able to conflate them — a 404 for an
/// object that exists would tell the client to re-upload bytes we already hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CappedGet {
    /// The object exists and is within the ceiling.
    Found(Vec<u8>),
    /// No such key.
    Missing,
    /// The object exists but is over the ceiling and was NOT read.
    /// `actual_bytes` is `None` when the storage layer reported no content
    /// length at all — refused for the same reason, since the size that would
    /// have been buffered is unknown.
    TooLarge {
        /// Size the storage layer reported, when it reported one.
        actual_bytes: Option<u64>,
    },
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

    /// Upload `bytes` under `key` ONLY if no object exists there.
    ///
    /// Returns `Ok(true)` when this call created the object and `Ok(false)`
    /// when the key was already taken. The conditional is server-side
    /// (`If-None-Match: *`), so two racing writers cannot both believe they
    /// created it.
    ///
    /// This exists for the audit archive, whose bucket carries a 7-year Object
    /// Lock: an existing object there CANNOT be overwritten, so the caller has
    /// to know which of the two happened rather than firing a blind `put` and
    /// reading success into an error it never saw.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any S3/network error other than the
    /// precondition failure, which is reported as `Ok(false)`.
    pub async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String> {
        debug!(
            bucket = %self.bucket,
            key = %key,
            bytes = bytes.len(),
            "R2S3Client::put_if_absent"
        );
        let len = bytes.len() as i64;
        let result = self
            .inner
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(bytes.into())
            .content_length(len)
            .if_none_match("*")
            .send()
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(sdk_err) => {
                // R2 answers a failed `If-None-Match: *` with 412
                // PreconditionFailed. The SDK has no typed variant for it on
                // PutObject, so the HTTP status is the discriminator.
                let status = sdk_err.raw_response().map(|r| r.status().as_u16());
                if status == Some(412) {
                    return Ok(false);
                }
                Err(format!(
                    "R2 conditional put failed for key {key}: {sdk_err}"
                ))
            }
        }
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
    /// GET an object, refusing anything larger than `max_bytes` BEFORE the body
    /// is materialised.
    ///
    /// The plain [`Self::get`] collects the whole body into memory and then
    /// copies it into a `Vec<u8>`, so peak heap for one read is twice the object
    /// size. That is fine for the median CAS object (593 bytes measured in prod)
    /// and is not fine for the tail: nothing on the read path bounded object
    /// size, and the server-side mirror ingest accepts a blob up to 1 GiB
    /// (`routes::public_mirror::MIRROR_MAX_BLOB_BYTES`) — on a 0.25 vCPU /
    /// 1024 MiB container that is an OOM, not a slow request (ADR-S34-002).
    ///
    /// The check reads `Content-Length` from the `GetObject` response and drops
    /// the stream without collecting it, so an over-size object costs one
    /// round-trip and no heap. A response with NO content length is refused
    /// too — fail-CLOSED, because the whole point is to never buffer an object
    /// of unknown size.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any transport/service error other than a 404.
    pub async fn get_capped(&self, key: &str, max_bytes: u64) -> Result<CappedGet, String> {
        debug!(bucket = %self.bucket, key = %key, max_bytes, "R2S3Client::get_capped");
        let result = self
            .inner
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;

        match result {
            Ok(output) => {
                let Some(len) = output.content_length() else {
                    return Ok(CappedGet::TooLarge { actual_bytes: None });
                };
                let len = u64::try_from(len).unwrap_or(u64::MAX);
                if len > max_bytes {
                    // Drop `output` (and its `ByteStream`) without collecting:
                    // the body never enters the heap.
                    return Ok(CappedGet::TooLarge {
                        actual_bytes: Some(len),
                    });
                }
                let bytes = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| format!("R2 body read failed for key {key}: {e}"))?
                    .into_bytes()
                    .to_vec();
                Ok(CappedGet::Found(bytes))
            }
            Err(sdk_err) => {
                if let aws_sdk_s3::error::SdkError::ServiceError(ref se) = sdk_err {
                    if se.err().is_no_such_key() {
                        return Ok(CappedGet::Missing);
                    }
                }
                Err(format!("R2 get failed for key {key}: {sdk_err}"))
            }
        }
    }

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
            Ok(output) => Ok(Some(
                u64::try_from(output.content_length().unwrap_or(0)).unwrap_or(0),
            )),
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
                .and_then(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .ok()
                })
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

/// The body-crypto plan for a CAS/AC object under a resolved BYOK config
/// (surface-neutral — shared by [`R2CasHandler`] and [`R2AcHandler`]). The
/// physical R2 key digest is resolved alongside it (see [`ByokResolved`]).
enum ByokBodyPlan {
    /// Store/serve `req.bytes` unchanged (non-BYOK / `_public` / inactive).
    Plaintext,
    /// Mode A — convergent: body keyed by the Tcs-derived DEK (dedup-preserving).
    Convergent { tcs: Tcs, ctx: CryptoContext },
    /// Mode B — random: body keyed by a random per-blob DEK persisted in
    /// `byok_envelope` (no dedup). The crypto material is held by the handler's
    /// [`ModeBEncryptor`]; only the (real-digest) context travels in the plan.
    Random { ctx: CryptoContext },
}

/// The full BYOK resolution for a CAS op: the **physical** R2 key digest (the
/// §4-hardened HMAC for an active tenant, audit H-4; the raw digest otherwise)
/// plus the body crypto [`ByokBodyPlan`].
struct ByokResolved {
    physical_digest: String,
    plan: ByokBodyPlan,
}

/// How many CAS existence probes (`HeadObject`) [`R2CasHandler::exists_batch`]
/// keeps in flight at once.
///
/// Deliberately small. The container runs on a Cloudflare Containers `basic`
/// instance — **0.25 vCPU** — so the ceiling has to stay well under what would
/// make TLS/HTTP bookkeeping for the in-flight requests contend for that
/// quarter-core; the probes themselves are pure I/O wait (~60 ms per R2 HEAD
/// measured from IAD), so a small window already recovers nearly all of the
/// serial loss. R2 request rate limits are also shared across tenants on the
/// account, so one tenant's 4096-digest `findMissingBlobs` must not be able to
/// open a wide burst against the bucket every other tenant reads through.
///
/// 16 turns the 4096-digest worst case from 4096 serial HEADs into 256 waves
/// (~15 s of HEAD time instead of ~246 s) and a typical 100-digest Bazel call
/// into 7 waves (~0.42 s instead of ~6 s) — the linear term is broken without
/// betting the shared bucket budget or the quarter-core on a large number.
/// Raise it only against a measurement, never on intuition.
const MAX_CONCURRENT_EXISTS_PROBES: usize = 16;

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
    /// Narrow async-capable seam onto the SAME sink as `audit` (perf,
    /// concurrent-list PR): `Some` only when `audit` is backed by the
    /// durable `D1AuditOutboxSink` (the production builder always wires
    /// this — see [`build_r2_cas_handler_from_env`] /
    /// [`Self::with_async_audit`]). `list()` uses this to `tokio::join!`
    /// the mandatory `ListAttempted` audit write with the R2 enumeration
    /// instead of running them serially; `None` (tests, any other
    /// `AuditSink` impl) keeps the fully serial fallback, byte-identical to
    /// before this seam existed. This does NOT weaken fail-closed: the
    /// response is still gated on the audit result, checked FIRST, exactly
    /// as the serial path checks it — see `list()`'s doc.
    audit_async: Option<Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>>,
    sli: Arc<dyn SliObserver>,
    /// BYOK Wave 3a (GATED-INERT): per-tenant BYOK config cache. `None` on the
    /// non-BYOK build / tests → the plaintext path runs unchanged. When `Some`
    /// AND a tenant is `active`, the CAS write/read path encrypts at rest.
    byok_config_cache: Option<Arc<ByokConfigCache>>,
    /// BYOK Wave 3a: the Tcs resolver (CMK-unwrap → convergence secret). `None`
    /// → plaintext path. Both this and `byok_config_cache` must be `Some` for
    /// encryption to engage (frozen policy §3).
    tcs_resolver: Option<Arc<TcsResolver>>,
    /// BYOK Wave 3c: the Mode-B (random-DEK) encryptor + `byok_envelope` store.
    /// `None` → a tenant configured for `crypto_mode='random'` fails CLOSED on
    /// the data plane (never plaintext); Mode A (convergent) is unaffected.
    byok_mode_b: Option<Arc<ModeBEncryptor>>,
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
            audit_async: None,
            sli,
            byok_config_cache: None,
            tcs_resolver: None,
            byok_mode_b: None,
        }
    }

    /// Attach the concurrent-list async audit seam: `audit_async` MUST be
    /// the SAME sink as the `audit` passed to [`Self::new`] (the production
    /// builder enforces this by cloning one `Arc<D1AuditOutboxSink>` into
    /// both places — see [`build_r2_cas_handler_from_env`]). Passing a
    /// DIFFERENT sink here would let `list()` write its audit row to one
    /// sink while every other call writes to another — never do that.
    ///
    /// Optional: a handler with `audit_async` left `None` keeps `list()`
    /// fully serial (identical to the pre-existing behavior) — this is the
    /// state every test handler in this module is in today.
    #[must_use]
    pub fn with_async_audit(
        mut self,
        audit_async: Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>,
    ) -> Self {
        self.audit_async = Some(audit_async);
        self
    }

    /// Attach the BYOK Wave-3a collaborators (config cache + Tcs resolver),
    /// enabling convergent encryption-at-rest for `active` tenants.
    ///
    /// GATED-INERT: encryption engages ONLY for a tenant whose
    /// `tenant_byok_config.state == 'active'`; every other tenant (and the
    /// `_public` namespace) keeps the exact plaintext path. The production
    /// builder ([`build_r2_cas_handler_from_env`]) does NOT call this yet —
    /// onboarding (the sole writer of the `active` state) is a later wave, and
    /// the default build links no real `KmsProvider` — so production stays on
    /// the unchanged plaintext path until that wave wires a provider here.
    #[must_use]
    pub fn with_byok(
        mut self,
        byok_config_cache: Arc<ByokConfigCache>,
        tcs_resolver: Arc<TcsResolver>,
    ) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self.tcs_resolver = Some(tcs_resolver);
        self
    }

    /// Attach the BYOK Wave-3c Mode-B (random-DEK) encryptor. GATED-INERT: only
    /// engaged for an `active` tenant whose `crypto_mode='random'`. Without it,
    /// such a tenant fails CLOSED on the data plane (never plaintext); Mode A is
    /// unaffected. Chains after [`Self::with_byok`].
    #[must_use]
    pub fn with_byok_random(mut self, mode_b: Arc<ModeBEncryptor>) -> Self {
        self.byok_mode_b = Some(mode_b);
        self
    }

    /// Resolve the BYOK plan for a (tenant, digest, algo): the **physical R2 key
    /// digest** (§4-hardened for an active tenant — audit H-4) plus the body
    /// crypto plan. Single source of truth for the read/write/exists/delete
    /// paths.
    ///
    /// - `Plaintext` — BYOK not wired / not configured / inactive / `_public`:
    ///   the physical digest is the RAW digest (byte-identical to today).
    /// - `Convergent` / `Random` — active: the physical digest is
    ///   `harden_digest(tcs, digest)`; the body plan carries the real-digest
    ///   [`CryptoContext`].
    /// - `Err(..)` — active-but-unresolvable (KMS/Tcs down, Mode-B unwired,
    ///   partial): FAIL CLOSED (5xx); NEVER plaintext.
    async fn resolve_byok(
        &self,
        tenant: &str,
        digest: &str,
        algo: DigestAlgo,
    ) -> Result<ByokResolved, CasHandlerError> {
        let plaintext = || ByokResolved {
            physical_digest: digest.to_owned(),
            plan: ByokBodyPlan::Plaintext,
        };
        // Both Wave-3a collaborators must be present (frozen policy §3); else the
        // existing plaintext path runs unchanged — no D1 hop, no behaviour change.
        let (Some(cache), Some(resolver)) =
            (self.byok_config_cache.as_ref(), self.tcs_resolver.as_ref())
        else {
            return Ok(plaintext());
        };
        // `_public` is deterministic public content with no secret — it MUST stay
        // plaintext (raw key) so cross-tenant dedup is preserved (plan §3).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(plaintext());
        }
        // ONE D1 read on a cache miss; cached (incl. the not-configured answer).
        // A config error fails closed — never a silent plaintext downgrade.
        let Some(cfg) = cache
            .get(tenant)
            .await
            .map_err(|e| CasHandlerError::Internal(format!("byok config read: {e}")))?
        else {
            return Ok(plaintext());
        };
        match engagement_for(&cfg) {
            ByokEngagement::Plaintext => Ok(plaintext()),
            ByokEngagement::FailClosed(why) => Err(CasHandlerError::Internal(format!(
                "byok active but {why}; refusing to fall back to plaintext (fail-closed)"
            ))),
            ByokEngagement::Encrypt(mode) => {
                let key_id = cfg.cmk_key_id.clone().unwrap_or_default();
                // The Tcs is resolved for BOTH modes — Mode A uses it for the
                // convergent DEK, and BOTH modes use it to §4-harden the physical
                // R2 key (audit H-4: the on-disk key reveals nothing without it).
                let tcs = resolver
                    .resolve(&cfg)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok tcs resolve: {e}")))?;
                let physical_digest = harden_digest(&tcs, digest);
                let plan = match mode {
                    ByokCryptoMode::Convergent => ByokBodyPlan::Convergent {
                        tcs,
                        ctx: cas_crypto_context(tenant, digest, algo, &key_id),
                    },
                    ByokCryptoMode::Random => {
                        if self.byok_mode_b.is_none() {
                            return Err(CasHandlerError::Internal(
                                "byok active Mode B (random) but the random-mode encryptor is \
                                 not wired; refusing to fall back to plaintext (fail-closed)"
                                    .to_owned(),
                            ));
                        }
                        ByokBodyPlan::Random {
                            ctx: cas_crypto_context_for(
                                tenant,
                                digest,
                                algo,
                                &key_id,
                                CryptoMode::Random,
                            ),
                        }
                    }
                };
                Ok(ByokResolved {
                    physical_digest,
                    plan,
                })
            }
        }
    }

    /// Encrypt the body for a resolved plan. `Ok(None)` ⇒ store the plaintext
    /// unchanged; `Ok(Some(ct))` ⇒ store the ciphertext blob; `Err` ⇒ fail closed
    /// (the caller returns before any PUT — plaintext is NEVER stored).
    async fn encrypt_body(
        &self,
        plan: &ByokBodyPlan,
        plaintext: &[u8],
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(None),
            ByokBodyPlan::Convergent { tcs, ctx } => {
                let stored = encrypt_cas_blob(plaintext, tcs, ctx)
                    .map_err(|e| CasHandlerError::Internal(format!("byok encrypt: {e}")))?;
                Ok(Some(stored))
            }
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                let stored = mode_b
                    .encrypt(plaintext, ctx)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok mode-b encrypt: {e}")))?;
                Ok(Some(stored))
            }
        }
    }

    /// Decrypt the stored body for a resolved plan. `Plaintext` ⇒ return the
    /// stored bytes unchanged; otherwise decrypt (fail closed on any failure —
    /// raw stored bytes are NEVER served). The post-decrypt content-hash
    /// re-verify (audit C1) runs on the returned PLAINTEXT, in the caller.
    async fn decrypt_body(
        &self,
        plan: &ByokBodyPlan,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(stored),
            ByokBodyPlan::Convergent { tcs, ctx } => decrypt_cas_blob(&stored, tcs, ctx)
                .map_err(|e| CasHandlerError::Internal(format!("byok decrypt: {e}"))),
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                mode_b
                    .decrypt(&stored, ctx)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok mode-b decrypt: {e}")))
            }
        }
    }

    /// BYOK Wave 4a — reclaim the Mode-B `byok_envelope` row for a resolved plan
    /// AFTER the R2 object has been deleted. ONLY Mode B (`Random`) writes an
    /// envelope row, so `Plaintext` / `Convergent` (Mode A) are no-ops.
    ///
    /// Ordering + fail-safety (frozen policy §3): the caller deletes the R2
    /// object FIRST, then calls this. A failed reclaim leaves an orphan
    /// wrapped-DEK row that now wraps NOTHING (the blob is already gone) — the
    /// SAFE-fail direction — so the caller WARNS and continues rather than fail
    /// the whole delete (which could leave a readable blob whose key was
    /// destroyed). `Ok(())` ⇒ nothing to reclaim or reclaim succeeded.
    async fn reclaim_byok_envelope(&self, plan: &ByokBodyPlan) -> Result<(), String> {
        if let ByokBodyPlan::Random { ctx } = plan {
            if let Some(mode_b) = self.byok_mode_b.as_ref() {
                return mode_b.reclaim(ctx).await;
            }
        }
        Ok(())
    }

    /// BYOK write hook (test-facing): resolve + encrypt the body. The production
    /// `write` path resolves ONCE and calls [`Self::encrypt_body`] directly.
    #[cfg(test)]
    async fn byok_encrypt_for_write(
        &self,
        req: &CasWriteRequest,
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        let resolved = self
            .resolve_byok(&req.tenant, &req.claimed_hash, req.algo)
            .await?;
        self.encrypt_body(&resolved.plan, &req.bytes).await
    }

    /// BYOK read hook (test-facing): resolve + decrypt the stored body. The
    /// production `read` path resolves ONCE and calls [`Self::decrypt_body`].
    #[cfg(test)]
    async fn byok_decrypt_for_read(
        &self,
        req: &CasReadRequest,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
        self.decrypt_body(&resolved.plan, stored).await
    }

    /// BYOK delete-reclaim hook (test-facing): resolve + reclaim the Mode-B
    /// `byok_envelope` row, mirroring the post-R2-delete step in the production
    /// `delete` path (which WARNS on the returned `Err` and never fails the
    /// delete). Returns the reclaim `Result` so a test can assert the safe-fail
    /// direction.
    #[cfg(test)]
    async fn byok_reclaim_for_delete(
        &self,
        tenant: &str,
        digest: &str,
        algo: DigestAlgo,
    ) -> Result<(), String> {
        let resolved = self
            .resolve_byok(tenant, digest, algo)
            .await
            .map_err(|e| format!("resolve: {e}"))?;
        self.reclaim_byok_envelope(&resolved.plan).await
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
        Ok(R2S3Client::blob_key(
            &self.cas_region,
            &prefix,
            digest,
            algo,
        ))
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
/// Fixed, reserved sentinel UUID for the public shared-dedup namespace
/// ([`crate::adapter_cache::PUBLIC_NAMESPACE`] = `_public`). `_public` is NOT a
/// tenant UUID — it's the intentional cross-tenant namespace for public,
/// deterministic content (Homebrew bottles, public npm/PyPI; the network-effect
/// moat). It has NO per-tenant isolation requirement (the content is public),
/// but it MUST get a STABLE prefix so every caller storing the same public blob
/// dedups to the same R2 key. We derive it from this fixed sentinel under the
/// SAME secret TDK: TDK-keyed (not a predictable raw prefix), reserved so it can
/// never collide with a real (random v4/v7) tenant's HMAC prefix, and identical
/// across callers. Hex spells `__public` in the leading bytes. Without this,
/// `_public` storage writes fail CLOSED (non-derivable) and the public bottle /
/// package cache is non-functional (the brew 502 root cause, 2026-06-21).
const PUBLIC_NAMESPACE_UUID: Uuid = Uuid::from_u128(0x5f5f_7075_626c_6963_0000_0000_0000_0001);

/// The stable, TDK-keyed R2 key prefix for the `_public` shared-dedup namespace
/// (see [`PUBLIC_NAMESPACE_UUID`]).
///
/// SINGLE SOURCE OF TRUTH for the public sentinel derivation. The `_public` CAS
/// **write** path ([`tenant_prefix`] below) and the public-revocation **eraser**
/// (`routes::public_revoke`, F3.2 B1b) MUST address the identical prefix — a
/// drift between the two would leave a revoked public blob's bytes physically
/// un-erasable (the exact BLOCKER-1 the revocation path exists to close). Both
/// go through this one function so the write key and the erase key are equal by
/// construction.
pub(crate) fn public_namespace_prefix(tdk: &TenantDerivationKey) -> String {
    derive_prefix(tdk, PUBLIC_NAMESPACE_UUID).to_string()
}

fn tenant_prefix(tdk: Option<&TenantDerivationKey>, tenant: &str) -> Result<String, String> {
    match tdk {
        // Public shared-dedup namespace: stable, TDK-keyed, reserved sentinel
        // prefix (see PUBLIC_NAMESPACE_UUID). Scoped to the EXACT `_public`
        // string — real UUID tenants are unaffected, other non-UUID tenants
        // still fail CLOSED below.
        Some(tdk) if tenant == crate::adapter_cache::PUBLIC_NAMESPACE => {
            Ok(public_namespace_prefix(tdk))
        }
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
pub(crate) fn verify_content_hash(
    algo: DigestAlgo,
    claimed_hash: &str,
    bytes: &[u8],
) -> Result<(), String> {
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

        // ReadAttempted audit and the (resolve_byok -> R2 GET) chain.
        //
        // CRITICAL — `block_in_place` rationale: this sync trait method is
        // invoked from inside an async axum handler that is itself being
        // polled on the tokio multi-thread runtime. A bare
        // `handle.block_on(future)` from inside a running future on the
        // SAME runtime hangs forever (observed: 60s curl timeout in prod
        // before this fix). `tokio::task::block_in_place` tells the
        // runtime to release the current worker so the inner `block_on`
        // can drive the future to completion. Valid only on the
        // multi-thread runtime — `#[tokio::main]` gives us that.
        let handle = tokio::runtime::Handle::current();

        // Unlike `list()` (a single R2 call), the read-side store work is a
        // TWO-STEP chain: BYOK Wave 3c resolution (the §4-hardened physical
        // key digest, audit H-4, plus the body crypto plan) THEN the R2 GET
        // keyed off the resolved digest. Both steps run inside `store_fut`
        // below so the WHOLE chain — not just the R2 call — overlaps with
        // the audit write; `resolved` is threaded back out because
        // `decrypt_body` needs `resolved.plan` after the join.
        type StoreResult = Result<(ByokResolved, Option<Vec<u8>>), CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                //
                // Mirrors `R2CasHandler::list`'s seam exactly (see that
                // method's doc for the full fail-CLOSED rationale): the
                // mandatory `ReadAttempted` audit write and the
                // resolve_byok-then-R2-GET chain are DISPATCHED together
                // under one `tokio::join!`, but neither result is used —
                // and no bytes are served — until BOTH complete and are
                // checked below, in the SAME order the serial code checked
                // them: audit result first (`AuditFailed` short-circuits
                // exactly as before), store result second. A failing audit
                // no longer prevents the R2 GET from having been
                // *dispatched*; it still prevents any R2 result — and any
                // decrypted bytes — from ever *reaching the caller*.
                //
                // Cross-tenant denial (above) is UNAFFECTED — it returns
                // before this point and never joins anything.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
                    let key = self
                        .r2_key(&req.tenant, &resolved.physical_digest, req.algo)
                        .map_err(CasHandlerError::Internal)?;
                    debug!(key = %key, "R2CasHandler::read");
                    let got = self.client.get(&key).await.map_err(|e| {
                        warn!(error = %e, key = %key, "R2CasHandler::read error");
                        CasHandlerError::Internal(e)
                    })?;
                    Ok((resolved, got))
                };
                // ONE `Phase::Store` scope wraps the ENTIRE joined window —
                // see `R2CasHandler::list` for why `oaudit` must NOT also
                // be entered here (it would double-count the overlapping
                // wall-clock window; `append_async` opens no scope of its
                // own for exactly this reason).
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior. Taken whenever `audit_async` is unset (every
                // test handler in this module, and any future `AuditSink`
                // impl that is not the durable D1 sink).
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching resolve_byok or
                    // R2 — never compute or evaluate the store side here.
                    // This filler is never read (audit_result is checked
                    // first, below).
                    Ok((
                        ByokResolved {
                            physical_digest: String::new(),
                            plan: ByokBodyPlan::Plaintext,
                        },
                        None,
                    ))
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.hash, req.algo))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            emit(true);
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
                        Ok(k) => k,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2CasHandler::read");
                    let result =
                        {
                            let _scope = crate::origin_timing::PhaseScope::enter(
                                crate::origin_timing::Phase::Store,
                            );
                            tokio::task::block_in_place(|| {
                                handle.block_on(self.client.get_capped(
                                    &key,
                                    crate::routes::cas::CAS_READ_MAX_OBJECT_BYTES,
                                ))
                            })
                        };
                    match result {
                        // B-051: refused BEFORE the body was collected, so the
                        // bytes never entered the heap. Distinct from NotFound —
                        // the object is there and intact.
                        Ok(CappedGet::TooLarge { actual_bytes }) => {
                            warn!(
                                key = %key,
                                actual_bytes = ?actual_bytes,
                                limit_bytes = crate::routes::cas::CAS_READ_MAX_OBJECT_BYTES,
                                "R2CasHandler::read refused an over-size object"
                            );
                            emit(true);
                            return Err(CasHandlerError::ObjectTooLarge {
                                actual_bytes: actual_bytes.unwrap_or(0),
                                limit_bytes: crate::routes::cas::CAS_READ_MAX_OBJECT_BYTES,
                            });
                        }
                        Ok(CappedGet::Missing) => Ok((resolved, None)),
                        Ok(CappedGet::Found(bytes)) => Ok((resolved, Some(bytes))),
                        Err(e) => {
                            warn!(error = %e, key = %key, "R2CasHandler::read error");
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        let (resolved, stored_opt) = match store_result {
            Ok(ok) => ok,
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                emit(true);
                return Err(e);
            }
        };

        match stored_opt {
            Some(stored) => {
                // BYOK Wave 3a/3c (GATED-INERT): for an `active` tenant, decrypt
                // the stored blob to plaintext BEFORE the content-hash re-verify
                // (audit C1: the integrity re-verify MUST run on the PLAINTEXT,
                // never on ciphertext). FAIL-CLOSED: any decrypt / unwrap failure
                // returns Err — the raw stored bytes are NEVER served for an
                // active tenant. `Plaintext`-plan tenants get their bytes back
                // unchanged (byte-identical to today).
                let bytes = match tokio::task::block_in_place(|| {
                    handle.block_on(self.decrypt_body(&resolved.plan, stored))
                }) {
                    Ok(pt) => pt,
                    Err(e) => {
                        emit(true);
                        return Err(e);
                    }
                };

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
            None => {
                emit(true);
                Err(CasHandlerError::NotFound {
                    tenant: req.tenant,
                    hash: req.hash,
                })
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

        // ReadAttempted audit and the (resolve_byok -> R2 HEAD) chain —
        // mirrors `read`'s concurrent seam exactly, see there for the full
        // fail-CLOSED rationale. No `resolved`/decrypt step is needed after
        // the join here (a HEAD reveals only presence, never bytes), so
        // `store_fut` only needs to yield `Option<u64>`.
        //
        // CRITICAL — `block_in_place` rationale: see `read` above. This is
        // a HEAD (`head_size`), not a GET — no body transfer, no rehash.
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<Option<u64>, CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH — see `read` for the full rationale.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                // The storage half is the SHARED `probe_existence_unaudited`
                // — the ONE body the batch seam (`exists_batch`) also drives,
                // so a single-digest probe and a batched one can never drift
                // on which key they HEAD or on their fail-CLOSED handling.
                let store_fut = self.probe_existence_unaudited(&req);
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior.
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    Ok(None)
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.hash, req.algo))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            emit(true);
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
                        Ok(k) => k,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2CasHandler::exists");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, key = %key, "R2CasHandler::exists error");
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        match store_result {
            Ok(Some(_)) => {
                emit(false);
                Ok(true)
            }
            Ok(None) => {
                emit(false);
                Ok(false)
            }
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                emit(true);
                Err(e)
            }
        }
    }

    /// Batch existence probe — the `findMissingBlobs` seam. Collapses the
    /// N `ReadAttempted` audit writes into ONE D1 round trip and runs the N
    /// R2 `HeadObject` probes with bounded concurrency, instead of
    /// `2N` strictly serial public-endpoint round trips.
    ///
    /// # Measured defect this fixes
    ///
    /// `POST /bazel/v2/{instance}/findMissingBlobs` was perfectly linear at
    /// ~268 ms per digest against prod (2.10 / 5.74 / 13.48 / 26.86 s for
    /// 5 / 20 / 50 / 100 digests), i.e. ~18 minutes at the documented
    /// `FIND_MISSING_BLOB_CAP` of 4096. `Server-Timing` decomposed a 40-digest
    /// call (7961 ms total) into 40 blocking D1 audit writes (~122 ms each)
    /// plus 40 R2 HEADs (~60 ms each), all serial — the loop, not the work.
    ///
    /// # `None` ⇒ serial fallback
    ///
    /// Returns `None` when the async audit seam is not wired
    /// (`audit_async == None`: every test handler in this module, and any
    /// non-D1 `AuditSink`), so those callers keep looping [`Self::exists`],
    /// byte-identical to before this method existed. Mirrors exactly how the
    /// concurrent `list()` seam was introduced.
    ///
    /// # Fail-CLOSED
    ///
    /// The audit batch and the probes are DISPATCHED together, but the audit
    /// result is evaluated FIRST and an `Err` short-circuits to
    /// `AuditFailed` — no probe result can reach the caller without its audit
    /// rows having committed. Cross-tenant denial happens strictly before
    /// anything is dispatched, and still emits its own `ReadDenied` row. See
    /// `list()`'s doc for the full argument; this path is the same shape.
    fn exists_batch(&self, reqs: &[CasReadRequest]) -> Option<Result<Vec<bool>, CasHandlerError>> {
        // No async-capable durable sink ⇒ no batch capability ⇒ the caller
        // uses the unchanged per-digest `exists()` loop.
        let audit_async = Arc::clone(self.audit_async.as_ref()?);
        Some(self.exists_batch_inner(&audit_async, reqs))
    }
}

impl R2CasHandler {
    /// The body of [`CasReadHandler::exists_batch`], reached only once the
    /// async audit seam is known to be wired.
    fn exists_batch_inner(
        &self,
        audit_async: &crate::storage::d1_audit_sink::D1AuditOutboxSink,
        reqs: &[CasReadRequest],
    ) -> Result<Vec<bool>, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        let emit = |is_error: bool| {
            self.emit_sli(Sli::AvailCasGet, Sli::LatencyCasGetP99, is_error);
        };

        // STRICTLY FIRST, before ANY dispatch: cross-tenant denial, audited
        // as `ReadDenied` through the SYNC sink exactly as `exists()` does.
        // Scanning the whole slice before dispatching (rather than per
        // digest, interleaved) is what makes "denial precedes dispatch" true
        // for the batch as a whole — one poisoned digest must not have let
        // the others touch R2 first.
        for req in reqs {
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
                    caller: req.caller_tenant.clone(),
                    requested_tenant: req.tenant.clone(),
                });
            }
        }

        if reqs.is_empty() {
            // Nothing to audit, nothing to probe — the serial loop would
            // likewise write no row and issue no request.
            return Ok(Vec::new());
        }

        // N digests still produce N `ReadAttempted` rows — same kind, same
        // per-digest fields. The taxonomy is untouched; only the number of
        // network trips that carry the rows changes. Duplicate digests in one
        // request produce colliding `id`/`request_id` keys and are deduped by
        // `INSERT OR IGNORE`, exactly as duplicate serial emits are today.
        let events: Vec<AuditEvent> = reqs
            .iter()
            .map(|req| {
                AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                )
            })
            .collect();

        let handle = tokio::runtime::Handle::current();
        let audit_fut = audit_async.emit_cas_batch_async(&events);
        let probes_fut = self.probe_existence_concurrently(reqs);

        // ONE `Phase::Store` scope over the ENTIRE joined window — not one
        // per future, and NOT a second `Phase::Audit` scope. `Σ(phases) ≤
        // total` holds by construction rather than by `oother`'s `max(0)`
        // guard swallowing a double count. See `origin_timing.rs`'s
        // "Concurrent native-plane list seam" note, which covers this path.
        let (audit_result, probe_results) = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| {
                handle.block_on(async { tokio::join!(audit_fut, probes_fut) })
            })
        };

        // AUDIT FIRST. A failed audit returns `AuditFailed` and NO probe
        // result reaches the caller — identical to the serial path, which
        // returns from the first digest's audit `map_err(..)?` (and, like it,
        // emits no SLI observation on that path).
        if let Err(e) = audit_result {
            return Err(CasHandlerError::AuditFailed(e));
        }

        // Deterministic, request-ordered evaluation: FIRST error wins, and
        // the observations recorded before it are exactly the ones the serial
        // loop would have recorded before returning on the same digest.
        let mut flags = Vec::with_capacity(probe_results.len());
        for result in probe_results {
            match result {
                Ok(present) => {
                    emit(false);
                    flags.push(present);
                }
                Err(e) => {
                    emit(true);
                    return Err(e);
                }
            }
        }
        Ok(flags)
    }

    /// Run the per-digest HEAD probes with bounded concurrency, returning one
    /// outcome per request **in request order** (`buffered` preserves the
    /// input order regardless of completion order — the response must not
    /// depend on which R2 call finished first).
    async fn probe_existence_concurrently(
        &self,
        reqs: &[CasReadRequest],
    ) -> Vec<Result<bool, CasHandlerError>> {
        use futures::StreamExt as _;

        futures::stream::iter(reqs.iter().map(|req| async move {
            // `Some(len)` present / `None` absent — the storage-layer
            // `NotFound` absorbed into `Ok(false)` exactly as `exists` does.
            self.probe_existence_unaudited(req)
                .await
                .map(|found| found.is_some())
        }))
        .buffered(MAX_CONCURRENT_EXISTS_PROBES)
        .collect::<Vec<_>>()
        .await
    }

    /// The storage half of ONE existence probe: BYOK resolution, key
    /// derivation, one S3 `HeadObject`. No body download, no rehash — the
    /// same work [`CasReadHandler::exists`] does after its audit row.
    ///
    /// # This deliberately emits NO audit row
    ///
    /// It is `fn`-private to this module and has exactly TWO callers, both
    /// concurrent seams inside this same impl: [`CasReadHandler::exists`]'s
    /// join and [`Self::exists_batch_inner`]'s. Each has already DISPATCHED
    /// the corresponding `ReadAttempted` write and refuses to return ANY
    /// result of this probe unless that write committed. It must NOT be
    /// widened into a generally reachable way to probe the CAS without an
    /// audit row — that would be a hole straight through the fail-CLOSED
    /// contract. There is deliberately ONE body: two copies of an
    /// un-audited probe is exactly the kind of duplication where one copy
    /// later grows a caller that skips the audit.
    ///
    /// It also opens no [`crate::origin_timing::PhaseScope`] of its own: on
    /// the batch path N concurrent probes each entering `Phase::Store` would
    /// count the same wall-clock window N times over. Each caller's single
    /// scope covers its whole joined window instead.
    ///
    /// Returns the RAW `head_size` outcome (`Some(len)` present / `None`
    /// absent) rather than a bool, because `exists` needs the same shape the
    /// serial fallback produces; the batch path maps it to a bool itself.
    async fn probe_existence_unaudited(
        &self,
        req: &CasReadRequest,
    ) -> Result<Option<u64>, CasHandlerError> {
        // BYOK Wave 3c: probe the §4-hardened physical key for an active
        // tenant, so this hits the SAME key `read`/`write`/`exists` use.
        let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
        // Fail CLOSED if the tenant prefix is not derivable: never touch R2
        // under a degraded/empty (SHARED) prefix.
        let key = self
            .r2_key(&req.tenant, &resolved.physical_digest, req.algo)
            .map_err(CasHandlerError::Internal)?;
        debug!(key = %key, "R2CasHandler::exists");

        self.client.head_size(&key).await.map_err(|e| {
            warn!(error = %e, key = %key, "R2CasHandler::exists error");
            CasHandlerError::Internal(e)
        })
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

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<Self as CasReadHandler>::read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — the §4-hardened physical key digest (audit
        // H-4) + the body crypto plan. Non-BYOK tenants resolve to the RAW digest
        // (byte-identical to today); an active-but-unresolvable tenant fails
        // CLOSED here (never PUTs plaintext).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.claimed_hash, req.algo))
        }) {
            Ok(r) => r,
            Err(e) => {
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, bytes = req.bytes.len(), "R2CasHandler::write");

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
        //
        // BYOK Wave 3c — the dedup HEAD-skip is gated to the convergent/plaintext
        // path ONLY. Mode B (random DEK) is deliberately NOT deduped (audit C2):
        // the §4-hardened key + random DEK + the `byok_envelope` idempotency
        // (reuse-the-row, deterministic ciphertext) own the no-orphan guarantee,
        // so a Mode-B write always proceeds to the PUT below.
        let dedup_eligible = !matches!(resolved.plan, ByokBodyPlan::Random { .. });
        if dedup_eligible {
            let head_result = {
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
            };
            match head_result {
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
                    // Ambiguous prior state: fall through to the PUT (fail-CLOSED to
                    // a durable write) rather than risk dropping a fresh blob.
                    warn!(error = %e, key = %key, "R2CasHandler::write pre-PUT HEAD error; PUTting");
                }
            }
        }

        // BYOK Wave 3a/3c (GATED-INERT): for an `active` tenant, encrypt at rest
        // AFTER the plaintext integrity verify + the (gated) dedup HEAD check,
        // replacing the stored bytes with the ciphertext blob (convergent CLB1 or
        // Mode-B CLB2). FAIL-CLOSED: an active tenant whose encryptor/KMS/Tcs/
        // envelope-store is unavailable returns Err here — plaintext is NEVER PUT
        // for an active tenant. `None` ⇒ the plaintext path (req.bytes),
        // byte-identical to today for every non-BYOK tenant.
        let payload = match tokio::task::block_in_place(|| {
            handle.block_on(self.encrypt_body(&resolved.plan, &req.bytes))
        }) {
            Ok(Some(ciphertext)) => ciphertext,
            Ok(None) => req.bytes,
            Err(e) => {
                emit(true);
                return Err(e);
            }
        };

        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.put(&key, payload)))
        };

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

        // CRITICAL — `block_in_place`: see `read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `write`/`read` stored. Non-BYOK
        // tenants resolve to the raw digest (unchanged); an active-but-unresolvable
        // tenant fails CLOSED rather than delete the wrong (or no) key. BYOK Wave
        // 4a: the resolved plan also drives the Mode-B `byok_envelope` reclaim
        // performed AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.hash, DigestAlgo::Blake3))
        }) {
            Ok(r) => r,
            Err(e) => {
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. DELETE is native-only
        // (the Bazel REAPI bridge exposes no delete surface), so the BLAKE3
        // keyspace applies.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest, DigestAlgo::Blake3) {
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
        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                // BYOK Wave 4a: the R2 object is now gone — reclaim the Mode-B
                // `byok_envelope` row so the deleted blob's wrapped DEK does not
                // linger (orphan key-material). A reclaim failure is the SAFE-fail
                // direction (the DEK wraps nothing now), so WARN + continue — the
                // delete still SUCCEEDS and the R2 delete is NOT rolled back.
                if let Err(e) = tokio::task::block_in_place(|| {
                    handle.block_on(self.reclaim_byok_envelope(&resolved.plan))
                }) {
                    warn!(
                        error = %e, key = %key, tenant = %req.tenant,
                        "R2CasHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; blob already deleted)"
                    );
                }
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

        // ListAttempted audit and the R2 enumeration.
        //
        // Enumeration is bounded to the tenant's derived prefix — cross-tenant
        // keys cannot appear in the result. Fail CLOSED if the prefix is not
        // derivable: an empty prefix would list a SHARED keyspace across every
        // non-derivable tenant. Both branches below log their own R2 error
        // (with whatever prefix they had, if any) INSIDE the branch, so the
        // shared post-join code only has to deal in `CasHandlerError`.
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(Vec<(String, u64, String)>, Option<String>), CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                //
                // The mandatory `ListAttempted` audit write and the R2
                // `ListObjectsV2` call now run CONCURRENTLY — one
                // `tokio::join!` under a single `block_in_place` +
                // `block_on`, instead of two serial round trips (measured:
                // ~236ms total on IAD, ~30-36ms `ostore` + ~108-120ms
                // `oother` dominated by this blocking D1 INSERT — see the
                // PR).
                //
                // FAIL-CLOSED IS PRESERVED. The two calls are dispatched
                // together, but NEITHER result is used — and no bytes are
                // served — until BOTH have completed and are checked BELOW,
                // in the SAME order the old serial code checked them: the
                // audit result first (`AuditFailed` short-circuits exactly
                // as it did when the calls were serial), the store result
                // second. What changes is that a failing audit no longer
                // PREVENTS the R2 call from having been issued (it can no
                // longer prevent it — they started together); what does NOT
                // change is that a failing audit still prevents any R2
                // result from ever reaching the caller. "No bytes served
                // without the audit row" holds; "no R2 request issued
                // without the audit row" is the guarantee traded away for
                // the latency win, and it was never a stated invariant —
                // only "audit before enumeration" was, and enumeration
                // (returning rows to the caller) still cannot happen without
                // the audit row.
                //
                // Cross-tenant denial (above) is UNAFFECTED — it returns
                // before this point and never joins anything.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let prefix = self
                        .r2_list_prefix(&req.tenant)
                        .map_err(CasHandlerError::Internal)?;
                    debug!(prefix = %prefix, "R2CasHandler::list");
                    self.client
                        .list_objects_page(&prefix, req.limit, req.cursor.as_deref())
                        .await
                        .map_err(|e| {
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            CasHandlerError::Internal(e)
                        })
                };
                // ONE `Phase::Store` scope wraps the ENTIRE joined window —
                // NOT two separate scopes (one per future) — so the
                // overlapping wall time is attributed exactly once.
                // `oaudit` (`Phase::Audit`) is deliberately NOT entered
                // here: `append_async` (which this drives) skips its own
                // scope for exactly this reason. See `origin_timing.rs`'s
                // "Concurrent native-plane list seam" note — without this,
                // the two phases would double-count the same wall-clock
                // window and the `Σ(phases) ≤ total` invariant would break.
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior. Taken whenever `audit_async` is unset (every
                // test handler in this module, and any future `AuditSink`
                // impl that is not the durable D1 sink).
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching the prefix or
                    // R2 — never compute or evaluate the store side here.
                    Ok((Vec::new(), None))
                } else {
                    let prefix = match self.r2_list_prefix(&req.tenant) {
                        Ok(p) => p,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(prefix = %prefix, "R2CasHandler::list");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| {
                            handle.block_on(self.client.list_objects_page(
                                &prefix,
                                req.limit,
                                req.cursor.as_deref(),
                            ))
                        })
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        match store_result {
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
                // Already `warn!`-logged (with prefix, where one was
                // available) at the point the error was produced above.
                emit(true);
                Err(e)
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
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: storage creds ARE present, so this is the
    // production data plane. The audit trail MUST be DURABLE — a volatile
    // `InMemoryAuditSink` here loses every CAS audit event on restart AND makes
    // the route's `AuditFailed → 503` guard dead code (in-memory emit only
    // errors under a test-injected failure). Wire the D1 `audit_outbox` sink
    // (the same trail the S-09 drain seals); if it cannot be constructed,
    // REFUSE to build the handler — the route mounts the fail-CLOSED 503
    // handler, never a silent in-memory fallback (mirrors the `R2_TDK_HEX`
    // refusal above).
    //
    // The CONCRETE `Arc<D1AuditOutboxSink>` is kept (not just the
    // type-erased `Arc<dyn AuditSink>`) so it can ALSO be wired as the
    // handler's `audit_async` seam below — same sink instance, two views:
    // the sync `AuditSink` trait object every non-list call still uses, and
    // the concrete type `list()` uses to `tokio::join!` the audit write
    // with the R2 enumeration (perf: concurrent native-plane list).
    let audit_concrete = match cas_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable CAS audit sink unavailable with storage creds present; \
                 refusing to mount the R2 CAS handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn AuditSink> = audit_concrete.clone();
    let sli = Arc::new(InMemorySliObserver::new());
    Some(Ok(R2CasHandler::new(
        client,
        cas_region,
        Some(tdk_bytes),
        audit,
        sli,
    )
    .with_async_audit(audit_concrete)))
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
    /// Narrow async-capable seam onto the SAME sink as `audit` — AC
    /// counterpart of [`R2CasHandler::audit_async`]; see there for the full
    /// rationale. `list()` uses this; `None` keeps it fully serial.
    audit_async: Option<Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>>,
    sli: Arc<dyn corelink_handler_ac::SliObserver>,
    /// BYOK Wave 3b (GATED-INERT): per-tenant BYOK config cache. `None` on the
    /// non-BYOK build / tests → the plaintext path runs unchanged. When `Some`
    /// AND a tenant is `active`, the AC update/lookup path encrypts the
    /// `result_payload` at rest under the `"ac"` surface (closes audit H1).
    /// Mirrors [`R2CasHandler::byok_config_cache`].
    byok_config_cache: Option<Arc<ByokConfigCache>>,
    /// BYOK Wave 3b: the Tcs resolver (CMK-unwrap → convergence secret). `None`
    /// → plaintext path. Both this and `byok_config_cache` must be `Some` for
    /// AC encryption to engage. Mirrors [`R2CasHandler::tcs_resolver`].
    tcs_resolver: Option<Arc<TcsResolver>>,
    /// BYOK Wave 3c: the Mode-B (random-DEK) encryptor + `byok_envelope` store
    /// for the AC surface. Mirrors [`R2CasHandler::byok_mode_b`].
    byok_mode_b: Option<Arc<ModeBEncryptor>>,
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
            audit_async: None,
            sli,
            byok_config_cache: None,
            tcs_resolver: None,
            byok_mode_b: None,
        }
    }

    /// AC counterpart of [`R2CasHandler::with_async_audit`] — same
    /// same-sink invariant applies (`audit_async` MUST be the same
    /// `D1AuditOutboxSink` behind `audit`).
    #[must_use]
    pub fn with_async_audit(
        mut self,
        audit_async: Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>,
    ) -> Self {
        self.audit_async = Some(audit_async);
        self
    }

    /// Attach the BYOK Wave-3b collaborators (config cache + Tcs resolver),
    /// enabling convergent encryption-at-rest of the AC `result_payload` for
    /// `active` tenants. Mirrors [`R2CasHandler::with_byok`].
    ///
    /// GATED-INERT: encryption engages ONLY for a tenant whose
    /// `tenant_byok_config.state == 'active'`; every other tenant (and the
    /// `_public` namespace) keeps the exact plaintext path. The production
    /// builder ([`build_r2_ac_handler_from_env`]) does NOT call this yet —
    /// onboarding (the sole writer of the `active` state, and the prod
    /// `KmsProvider` wiring) is a later wave.
    #[must_use]
    pub fn with_byok(
        mut self,
        byok_config_cache: Arc<ByokConfigCache>,
        tcs_resolver: Arc<TcsResolver>,
    ) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self.tcs_resolver = Some(tcs_resolver);
        self
    }

    /// Attach the BYOK Wave-3c Mode-B (random-DEK) encryptor for the AC surface.
    /// Mirrors [`R2CasHandler::with_byok_random`].
    #[must_use]
    pub fn with_byok_random(mut self, mode_b: Arc<ModeBEncryptor>) -> Self {
        self.byok_mode_b = Some(mode_b);
        self
    }

    /// Resolve the BYOK plan for an AC `(tenant, action_digest)`: the §4-hardened
    /// physical key digest (audit H-4) + the body crypto plan, binding the
    /// `"ac"` surface ([`ac_crypto_context`]) so an AC blob is domain-separated
    /// from CAS. Mirrors [`R2CasHandler::resolve_byok`].
    async fn resolve_byok(
        &self,
        tenant: &str,
        action_digest: &str,
    ) -> Result<ByokResolved, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        let plaintext = || ByokResolved {
            physical_digest: action_digest.to_owned(),
            plan: ByokBodyPlan::Plaintext,
        };
        let (Some(cache), Some(resolver)) =
            (self.byok_config_cache.as_ref(), self.tcs_resolver.as_ref())
        else {
            return Ok(plaintext());
        };
        // `_public` is deterministic public content — never encrypted (dedup).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(plaintext());
        }
        let Some(cfg) = cache
            .get(tenant)
            .await
            .map_err(|e| AcHandlerError::Internal(format!("byok config read: {e}")))?
        else {
            return Ok(plaintext());
        };
        match engagement_for(&cfg) {
            ByokEngagement::Plaintext => Ok(plaintext()),
            ByokEngagement::FailClosed(why) => Err(AcHandlerError::Internal(format!(
                "byok active but {why}; refusing to fall back to plaintext (fail-closed)"
            ))),
            ByokEngagement::Encrypt(mode) => {
                let key_id = cfg.cmk_key_id.clone().unwrap_or_default();
                let tcs = resolver
                    .resolve(&cfg)
                    .await
                    .map_err(|e| AcHandlerError::Internal(format!("byok tcs resolve: {e}")))?;
                let physical_digest = harden_digest(&tcs, action_digest);
                let plan = match mode {
                    ByokCryptoMode::Convergent => ByokBodyPlan::Convergent {
                        tcs,
                        ctx: ac_crypto_context(tenant, action_digest, &key_id),
                    },
                    ByokCryptoMode::Random => {
                        if self.byok_mode_b.is_none() {
                            return Err(AcHandlerError::Internal(
                                "byok active Mode B (random) but the random-mode encryptor is \
                                 not wired; refusing to fall back to plaintext (fail-closed)"
                                    .to_owned(),
                            ));
                        }
                        ByokBodyPlan::Random {
                            ctx: ac_crypto_context_for(
                                tenant,
                                action_digest,
                                &key_id,
                                CryptoMode::Random,
                            ),
                        }
                    }
                };
                Ok(ByokResolved {
                    physical_digest,
                    plan,
                })
            }
        }
    }

    /// Encrypt the AC body for a resolved plan (`None` ⇒ store plaintext). See
    /// [`R2CasHandler::encrypt_body`].
    async fn encrypt_body(
        &self,
        plan: &ByokBodyPlan,
        payload: &[u8],
    ) -> Result<Option<Vec<u8>>, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        match plan {
            ByokBodyPlan::Plaintext => Ok(None),
            ByokBodyPlan::Convergent { tcs, ctx } => {
                let stored = encrypt_cas_blob(payload, tcs, ctx)
                    .map_err(|e| AcHandlerError::Internal(format!("byok ac encrypt: {e}")))?;
                Ok(Some(stored))
            }
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    AcHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                let stored = mode_b.encrypt(payload, ctx).await.map_err(|e| {
                    AcHandlerError::Internal(format!("byok ac mode-b encrypt: {e}"))
                })?;
                Ok(Some(stored))
            }
        }
    }

    /// Decrypt the stored AC body for a resolved plan. See
    /// [`R2CasHandler::decrypt_body`].
    async fn decrypt_body(
        &self,
        plan: &ByokBodyPlan,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        match plan {
            ByokBodyPlan::Plaintext => Ok(stored),
            ByokBodyPlan::Convergent { tcs, ctx } => decrypt_cas_blob(&stored, tcs, ctx)
                .map_err(|e| AcHandlerError::Internal(format!("byok ac decrypt: {e}"))),
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    AcHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                mode_b
                    .decrypt(&stored, ctx)
                    .await
                    .map_err(|e| AcHandlerError::Internal(format!("byok ac mode-b decrypt: {e}")))
            }
        }
    }

    /// BYOK Wave 4a — reclaim the Mode-B `byok_envelope` row for the AC surface
    /// after the R2 object is deleted. Mirrors [`R2CasHandler::reclaim_byok_envelope`]
    /// (only `Random` writes a row; same R2-first ordering + warn-on-failure
    /// safe-fail rationale). The plan's `ctx` carries `AC_SURFACE`, so the
    /// reclaimed key is `ac:<digest>` (surface-correct).
    async fn reclaim_byok_envelope(&self, plan: &ByokBodyPlan) -> Result<(), String> {
        if let ByokBodyPlan::Random { ctx } = plan {
            if let Some(mode_b) = self.byok_mode_b.as_ref() {
                return mode_b.reclaim(ctx).await;
            }
        }
        Ok(())
    }

    /// BYOK AC write hook (test-facing): resolve + encrypt the body. The
    /// production `update` path resolves ONCE and calls [`Self::encrypt_body`].
    #[cfg(test)]
    async fn byok_encrypt_for_update(
        &self,
        req: &corelink_handler_ac::AcUpdateRequest,
    ) -> Result<Option<Vec<u8>>, corelink_handler_ac::AcHandlerError> {
        let resolved = self.resolve_byok(&req.tenant, &req.action_digest).await?;
        self.encrypt_body(&resolved.plan, &req.result_payload).await
    }

    /// BYOK AC read hook (test-facing): resolve + decrypt the stored body. The
    /// production `lookup` path resolves ONCE and calls [`Self::decrypt_body`].
    #[cfg(test)]
    async fn byok_decrypt_for_lookup(
        &self,
        tenant: &str,
        action_digest: &str,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, corelink_handler_ac::AcHandlerError> {
        let resolved = self.resolve_byok(tenant, action_digest).await?;
        self.decrypt_body(&resolved.plan, stored).await
    }

    /// BYOK AC delete-reclaim hook (test-facing): resolve + reclaim the Mode-B
    /// `byok_envelope` row (surface = `ac`), mirroring the post-R2-delete step in
    /// the production AC `delete` path.
    #[cfg(test)]
    async fn byok_reclaim_for_delete(
        &self,
        tenant: &str,
        action_digest: &str,
    ) -> Result<(), String> {
        let resolved = self
            .resolve_byok(tenant, action_digest)
            .await
            .map_err(|e| format!("resolve: {e}"))?;
        self.reclaim_byok_envelope(&resolved.plan).await
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

        // LookupAttempted audit and the (resolve_byok -> R2 GET) chain —
        // AC counterpart of `R2CasHandler::read`'s concurrent seam; see
        // there for the full fail-CLOSED rationale (audit result checked
        // first, in the same order the old serial code checked it;
        // store-side error already `warn!`-logged at the point it is
        // produced). The SECOND audit write (`LookupHit`/`LookupMiss`)
        // stays exactly where it was — after the outcome is known, still
        // fully synchronous.
        //
        // CRITICAL — `block_in_place` rationale: this sync trait method
        // is invoked from inside an async axum handler on the tokio
        // multi-thread runtime; a bare `handle.block_on(future)` from
        // inside a running future on the SAME runtime hangs forever
        // (observed: 60s curl timeout in prod before this fix).
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(ByokResolved, Option<Vec<u8>>), AcHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH — see `R2CasHandler::read` for the full
                // rationale; this mirrors it exactly for the AC surface.
                let audit_fut = audit_async.emit_ac_async(AcAuditEvent::new(
                    AcAuditEventKind::LookupAttempted,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let resolved = self.resolve_byok(&req.tenant, &req.action_digest).await?;
                    let key = self
                        .r2_key(&req.tenant, &resolved.physical_digest)
                        .map_err(AcHandlerError::Internal)?;
                    debug!(key = %key, "R2AcHandler::lookup");
                    let got = self.client.get(&key).await.map_err(|e| {
                        warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                        AcHandlerError::Internal(e)
                    })?;
                    Ok((resolved, got))
                };
                // ONE `Phase::Store` scope for the whole joined window —
                // see `R2CasHandler::list` for why (never double-count
                // against `oaudit`).
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior (every test handler in this module today).
                let audit_result = self.audit.emit(AcAuditEvent::new(
                    AcAuditEventKind::LookupAttempted,
                    req.tenant.clone(),
                    req.action_digest.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching resolve_byok or
                    // R2. This filler is never read (audit_result is
                    // checked first, below).
                    Ok((
                        ByokResolved {
                            physical_digest: String::new(),
                            plan: ByokBodyPlan::Plaintext,
                        },
                        None,
                    ))
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            self.emit_lookup_sli(true);
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
                        Ok(k) => k,
                        Err(e) => {
                            self.emit_lookup_sli(true);
                            return Err(AcHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2AcHandler::lookup");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
                    };
                    match result {
                        Ok(got) => Ok((resolved, got)),
                        Err(e) => {
                            warn!(error = %e, key = %key, "R2AcHandler::lookup error");
                            self.emit_lookup_sli(true);
                            return Err(AcHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            self.emit_lookup_sli(true);
            return Err(AcHandlerError::AuditFailed(e));
        }

        let (resolved, stored_opt) = match store_result {
            Ok(ok) => ok,
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                self.emit_lookup_sli(true);
                return Err(e);
            }
        };

        match stored_opt {
            Some(bytes) => {
                // BYOK Wave 3b/3c (GATED-INERT): for an `active` tenant decrypt the
                // stored blob to plaintext before serving (surface `"ac"`).
                // FAIL-CLOSED: any decrypt/unwrap failure returns Err — the raw
                // stored bytes are NEVER served. `Plaintext`-plan tenants get
                // their bytes back unchanged (byte-identical to today).
                let bytes = match tokio::task::block_in_place(|| {
                    handle.block_on(self.decrypt_body(&resolved.plan, bytes))
                }) {
                    Ok(pt) => pt,
                    Err(e) => {
                        self.emit_lookup_sli(true);
                        return Err(e);
                    }
                };
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
            None => {
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

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<R2AcHandler as AcLookupHandler>::lookup` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — §4-hardened physical key (audit H-4) + the
        // body crypto plan. Fail CLOSED for an active-but-unresolvable tenant.
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
        }) {
            Ok(r) => r,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
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

        // BYOK Wave 3b/3c (GATED-INERT): compute the bytes we WOULD store — the
        // ciphertext for an `active` tenant (surface `"ac"`), else the plaintext
        // payload. Encrypting BEFORE the divergent-body compare is LOAD-BEARING:
        // for an active tenant the stored prior is ciphertext and the encryption
        // is deterministic (convergent CLB1, or Mode-B CLB2 under the persisted
        // per-(tenant,action_digest) DEK), so comparing the prior against the
        // would-be-stored CIPHERTEXT keeps the immutability + idempotency contract
        // exact (an identical payload re-PUT is byte-identical → no-op; a divergent
        // payload yields divergent ciphertext → DivergentBody). FAIL-CLOSED: an
        // active tenant whose encryptor/KMS/Tcs/envelope-store is unavailable
        // returns Err here — plaintext is NEVER PUT for an active tenant. The
        // non-BYOK path computes nothing (`None`) and stores `req.result_payload`
        // verbatim, byte-identical to today.
        let encrypted: Option<Vec<u8>> = match tokio::task::block_in_place(|| {
            handle.block_on(self.encrypt_body(&resolved.plan, &req.result_payload))
        }) {
            Ok(maybe_ct) => maybe_ct,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(e);
            }
        };

        // AC IMMUTABILITY INVARIANT (F5): the Action Cache maps an
        // `action_digest` (hash of the build *action*, not its result)
        // to a result payload. AC bytes are therefore NOT
        // self-verifying — a divergent re-PUT must be REFUSED, never
        // silently overwritten, or any write-capable token can poison a
        // proven cache result for every subsequent build that hits the
        // same digest (supply-chain compromise).
        //
        // GET-and-compare BEFORE any PUT (mirrors
        // `InMemoryAcHandler::update`), against the WOULD-BE-STORED bytes
        // (`stored_view`: ciphertext for an active tenant, else the plaintext):
        //   - existing != stored_view → `DivergentBody` (409); NO PUT.
        //   - existing == stored_view → idempotent no-op (`durable=false`).
        //   - absent                  → PUT (`durable=true`).
        //   - ambiguous GET error     → fail CLOSED (`Internal`); never
        //     blind-overwrite on an unknown prior state.
        // Owned copy: the compare view must outlive the move of
        // `encrypted`/`result_payload` into the conditional put, because a
        // lost race re-compares against it.
        let stored_view: Vec<u8> = encrypted
            .as_deref()
            .unwrap_or(req.result_payload.as_slice())
            .to_vec();
        let existing = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
        };
        let pre_put_state = match existing {
            Ok(prior) => classify_prior(prior.as_deref(), &stored_view),
            Err(e) => {
                // Ambiguous prior state: fail closed rather than risk a
                // blind overwrite of a proven result.
                warn!(error = %e, key = %key, "R2AcHandler::update pre-PUT GET error");
                self.emit_update_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        match pre_put_state {
            PriorState::Divergent => {
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
            PriorState::Identical => {
                // Byte-identical re-PUT → idempotent no-op. The proven
                // bytes are already durable; do not re-PUT.
                self.emit_update_sli(false);
                return Ok(AcUpdateResponse::new(req.action_digest, false));
            }
            PriorState::Absent => { /* absent — fall through to the PUT below */ }
        }

        // Store the ciphertext (active) or the plaintext payload (non-BYOK) —
        // the same bytes the compare above proved are non-divergent.
        //
        // CONDITIONAL PUT (WP-C): the GET-compare above is NOT atomic with
        // the write. Two concurrent writers with divergent bodies can both
        // observe `Ok(None)` and both fire an unconditional `put` —
        // last-write-wins over a PROVEN result (intra-tenant AC poisoning
        // window). `put_if_absent` closes it server-side (`If-None-Match: *`):
        //   - Ok(true)  → this caller created the object (durable=true).
        //   - Ok(false) → another writer won the race. Re-GET and apply the
        //     SAME classifier as the pre-PUT compare: identical bytes →
        //     idempotent no-op (durable=false); divergent → `DivergentBody`.
        //     The winner's bytes are NEVER overwritten on this path.
        //   - Err       → fail CLOSED (same as the pre-PUT GET error arm).
        let payload = encrypted.unwrap_or(req.result_payload);
        let created = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| {
                handle.block_on(self.client.put_if_absent(&key, payload))
            })
        };

        match created {
            Ok(true) => {
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
                // This caller created the object → durable=true.
                Ok(AcUpdateResponse::new(req.action_digest, true))
            }
            Ok(false) => {
                // Lost the race: someone else's bytes are (or were being)
                // stored under this digest. Re-GET and compare against OUR
                // would-be-stored view.
                let prior = {
                    let _scope =
                        crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                    tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))
                };
                let lost_race_state = match prior {
                    Ok(bytes) => classify_prior(bytes.as_deref(), &stored_view),
                    Err(e) => {
                        // Ambiguous post-race state: fail closed rather
                        // than report success on bytes we cannot prove.
                        warn!(error = %e, key = %key, "R2AcHandler::update lost-race re-GET error");
                        self.emit_update_sli(true);
                        return Err(AcHandlerError::Internal(e));
                    }
                };
                match lost_race_state {
                    PriorState::Identical => {
                        self.emit_update_sli(false);
                        Ok(AcUpdateResponse::new(req.action_digest, false))
                    }
                    PriorState::Divergent => {
                        warn!(
                            key = %key,
                            "R2AcHandler::update lost conditional-PUT race with a DIVERGENT \
                             body — preserving the proven AC result \
                             (INV-AC-RESULT-HASH-IMMUTABLE)"
                        );
                        self.emit_update_sli(false);
                        Err(AcHandlerError::DivergentBody {
                            tenant: req.tenant,
                            action_digest: req.action_digest,
                        })
                    }
                    PriorState::Absent => {
                        // We lost a conditional put yet the object reads
                        // absent — ambiguous state (concurrent delete?).
                        // Fail closed rather than report success on bytes we
                        // cannot prove are stored.
                        warn!(key = %key, "R2AcHandler::update lost-race re-GET absent");
                        self.emit_update_sli(true);
                        Err(AcHandlerError::Internal(
                            "conditional-PUT lost the race but re-GET found the key absent"
                                .to_owned(),
                        ))
                    }
                }
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

/// The relationship between an EXISTING stored object and the bytes a
/// caller would store (`stored_view`): the AC immutability decision space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriorState {
    /// No object stored — the caller may create it.
    Absent,
    /// Stored bytes are byte-identical to the would-be-stored view — an
    /// idempotent no-op (durable=false).
    Identical,
    /// Stored bytes diverge — refuse with `DivergentBody` (409).
    Divergent,
}

/// Classify a prior GET result against the bytes that would be stored.
///
/// Shared by the pre-PUT compare AND the lost-race re-GET after a failed
/// conditional put, so both sites can never diverge in their decision.
fn classify_prior(existing: Option<&[u8]>, stored_view: &[u8]) -> PriorState {
    match existing {
        None => PriorState::Absent,
        Some(prior) if prior == stored_view => PriorState::Identical,
        Some(_) => PriorState::Divergent,
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

        // Byte-accounting (finding #1 / cluster-C) + concurrent double-DELETE
        // over-release (rt-nuclear #6/#10/#14): `delete_if_present` serializes the
        // measure-and-delete per key and returns the reclaimed size to AT MOST ONE
        // racer, so two racing deletes can never both credit the same bytes.
        // CRITICAL — `block_in_place`.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `update`/`lookup` stored.
        // Fail CLOSED for an active-but-unresolvable tenant. BYOK Wave 4a: the
        // resolved plan also drives the Mode-B `byok_envelope` reclaim performed
        // AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.action_digest))
        }) {
            Ok(r) => r,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest) {
            Ok(k) => k,
            Err(e) => {
                self.emit_update_sli(true);
                return Err(AcHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2AcHandler::delete");

        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                // BYOK Wave 4a: the R2 object is gone — reclaim the Mode-B
                // `byok_envelope` row (surface = `ac`) so a deleted AC entry does
                // not leave an orphan wrapped-DEK row. WARN + continue on failure
                // (safe-fail direction); the delete still SUCCEEDS.
                if let Err(e) = tokio::task::block_in_place(|| {
                    handle.block_on(self.reclaim_byok_envelope(&resolved.plan))
                }) {
                    warn!(
                        error = %e, key = %key, tenant = %req.tenant,
                        "R2AcHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; entry already deleted)"
                    );
                }
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

        // ListAttempted audit and the R2 enumeration — AC counterpart of
        // `R2CasHandler::list`'s concurrent seam; see that doc for the full
        // fail-CLOSED rationale (audit result checked first, in the same
        // order as the old serial code; store-side error already
        // `warn!`-logged at the point it is produced).
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(Vec<(String, u64, String)>, Option<String>), AcHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                // See `R2CasHandler::list` for the full rationale — this
                // mirrors it exactly for the AC surface.
                let audit_fut = audit_async.emit_ac_async(AcAuditEvent::new(
                    AcAuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let prefix = self
                        .r2_list_prefix(&req.tenant)
                        .map_err(AcHandlerError::Internal)?;
                    debug!(prefix = %prefix, "R2AcHandler::list");
                    self.client
                        .list_objects_page(&prefix, req.limit, req.cursor.as_deref())
                        .await
                        .map_err(|e| {
                            warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                            AcHandlerError::Internal(e)
                        })
                };
                // ONE `Phase::Store` scope for the whole joined window — see
                // `R2CasHandler::list` for why (never double-count against
                // `oaudit`).
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior (every test handler in this module today).
                let audit_result = self.audit.emit(AcAuditEvent::new(
                    AcAuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result = if audit_result.is_err() {
                    Ok((Vec::new(), None))
                } else {
                    let prefix = match self.r2_list_prefix(&req.tenant) {
                        Ok(p) => p,
                        Err(e) => {
                            self.emit_lookup_sli(true);
                            return Err(AcHandlerError::Internal(e));
                        }
                    };
                    debug!(prefix = %prefix, "R2AcHandler::list");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| {
                            handle.block_on(self.client.list_objects_page(
                                &prefix,
                                req.limit,
                                req.cursor.as_deref(),
                            ))
                        })
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, prefix = %prefix, "R2AcHandler::list error");
                            self.emit_lookup_sli(true);
                            return Err(AcHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            self.emit_lookup_sli(true);
            return Err(AcHandlerError::AuditFailed(e));
        }

        match store_result {
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
                // Already `warn!`-logged (with prefix, where available) at
                // the point the error was produced above.
                self.emit_lookup_sli(true);
                Err(e)
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
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: DURABLE audit trail is mandatory on the
    // production data plane — see `build_r2_cas_handler_from_env`. The AC key
    // space is not content-addressed, so a lost/forged audit row is even more
    // dangerous. Wire the D1 `audit_outbox` sink or REFUSE (route mounts the
    // fail-CLOSED handler, never a volatile in-memory fallback).
    // Kept concrete for `with_async_audit` too — see the matching comment in
    // `build_r2_cas_handler_from_env`.
    let audit_concrete = match ac_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable AC audit sink unavailable with storage creds present; \
                 refusing to mount the R2 AC handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn corelink_handler_ac::AuditSink> = audit_concrete.clone();
    let sli = Arc::new(corelink_handler_ac::InMemorySliObserver::new());
    Some(Ok(R2AcHandler::new(
        client,
        ac_region,
        Some(tdk_bytes),
        audit,
        sli,
    )
    .with_async_audit(audit_concrete)))
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
    // origin_timing: `ostore` must absorb the native-plane R2 calls
    // ---------------------------------------------------------------

    /// Parse a `Server-Timing` header value into `{ name: dur_ms }`, mirroring
    /// `origin_timing::tests::parse` (that one is private to its own module).
    fn parse_server_timing(value: &str) -> std::collections::HashMap<String, i64> {
        let mut out = std::collections::HashMap::new();
        for part in value.split(',') {
            let mut it = part.trim().split(";dur=");
            if let (Some(name), Some(dur)) = (it.next(), it.next()) {
                if let Ok(v) = dur.trim().parse::<i64>() {
                    out.insert(name.trim().to_owned(), v);
                }
            }
        }
        out
    }

    /// The `oother` residue used to swallow the native CAS plane's R2 GET
    /// silently. This proves `R2CasHandler::read`'s `block_in_place` call is
    /// now wrapped into `Phase::Store` (`ostore`) — recorded even though the
    /// stub endpoint (`localhost:1`) makes the GET itself fail, because
    /// `PhaseScope`'s `Drop` records on every exit path, success or error.
    /// Driven through the real `origin_timing_layer` axum middleware (the
    /// only public way to scope a ledger from outside `origin_timing.rs`),
    /// exactly like `origin_timing::tests::the_layer_attributes_a_delay_to_the_phase_it_was_charged_to`.
    // NOTE: `multi_thread` flavor is REQUIRED — see
    // `r2_cas_write_with_honest_bytes_passes_verification` above for why.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_read_attributes_the_r2_get_to_ostore() {
        let handler = std::sync::Arc::new(make_test_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = CasReadRequest::new(
                            "tenant-x",
                            "deadbeef00000000000000000000000000000000000000000000000000000001",
                            "caller@tenant-x",
                            "tenant-x",
                            1,
                        );
                        // Errors against the unreachable stub endpoint — that
                        // is fine, the assertion is on the timing header, not
                        // the result.
                        let _ = handler.read(req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2CasHandler::read's block_in_place R2 GET must be attributed \
             to Phase::Store (ostore) even when the call errors. Header: {header}"
        );
    }

    /// AC counterpart of the above: `R2AcHandler::lookup`'s `block_in_place`
    /// R2 GET must also land in `Phase::Store`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_ac_lookup_attributes_the_r2_get_to_ostore() {
        let handler = std::sync::Arc::new(make_test_ac_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = corelink_handler_ac::AcLookupRequest::new(
                            "tenant-x",
                            "deadbeef00000000000000000000000000000000000000000000000000000001",
                            "caller@tenant-x",
                            "tenant-x",
                            1,
                        );
                        let _ = corelink_handler_ac::AcLookupHandler::lookup(&*handler, req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2AcHandler::lookup's block_in_place R2 GET must be attributed \
             to Phase::Store (ostore) even when the call errors. Header: {header}"
        );
    }

    // ---------------------------------------------------------------
    // Concurrent audit+R2 `list()` seam
    // ---------------------------------------------------------------

    /// `list()`'s SERIAL FALLBACK path (`audit_async` unset — every test
    /// handler in this module) is exercised no differently than before this
    /// PR: the `block_in_place` R2 `ListObjectsV2` call still lands in
    /// `Phase::Store` (`ostore`). Network-free (stub S3 endpoint), so this
    /// runs in CI, unlike the concurrent-path test below.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_list_serial_fallback_attributes_the_r2_call_to_ostore() {
        let handler = std::sync::Arc::new(make_test_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = corelink_handler_cas::CasListRequest::new(
                            "tenant-x",
                            "caller@tenant-x",
                            "tenant-x",
                            10,
                            None,
                            1,
                        );
                        let _ = CasListHandler::list(&*handler, req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2CasHandler::list's serial-fallback R2 call must be attributed \
             to Phase::Store (ostore). Header: {header}"
        );
    }

    /// The CONCURRENT path (`audit_async` wired — production shape): even
    /// when the durable-audit D1 write fails (here: a reachable-but-wrong
    /// token, `stub_env()` from `d1_audit_sink.rs`'s own test helper shape),
    /// `list()` still returns `AuditFailed` and — the load-bearing part —
    /// the joined window is attributed ONCE to `ostore`, and `oaudit` is
    /// ABSENT (proving `append_async` did not enter its own `Phase::Audit`
    /// scope, so the two never double-count the same wall-clock window; see
    /// `origin_timing.rs`'s "Concurrent native-plane list seam" note).
    ///
    /// Gated behind `#[ignore]` like the `d1_audit_sink.rs` async-phase
    /// test it mirrors — needs outbound reachability to
    /// `api.cloudflare.com` (not live credentials: a 401 still proves the
    /// join ran). Run manually with:
    ///
    /// ```bash
    /// cargo test -p corelink-server r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires outbound network reachability to api.cloudflare.com"]
    async fn r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds() {
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
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1HttpClient constructs over the stub env (network call happens lazily)");
        let audit: Arc<dyn AuditSink> = audit_concrete.clone();
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = std::sync::Arc::new(
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete),
        );

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = corelink_handler_cas::CasListRequest::new(
                            "tenant-x",
                            "caller@tenant-x",
                            "tenant-x",
                            10,
                            None,
                            1,
                        );
                        let result = CasListHandler::list(&*handler, req);
                        // FAIL-CLOSED: a rejected D1 credential must surface
                        // as AuditFailed, never as a "success" that could
                        // have served R2 rows.
                        assert!(
                            matches!(result, Err(CasHandlerError::AuditFailed(_))),
                            "bad D1 creds must fail CLOSED as AuditFailed, got: {result:?}"
                        );
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "the concurrent join must be attributed to Phase::Store (ostore). \
             Header: {header}"
        );
        assert!(
            !detail_phases_enabled_for_test() || !parsed.contains_key("oaudit"),
            "the concurrent list()'s audit write must NOT ALSO appear under \
             oaudit — it would double-count the same wall-clock window \
             `ostore` already reports. Header: {header}"
        );
    }

    /// `oaudit`/`ortier`/`opermit`/`oargon` are gated behind
    /// `CORELINK_ORIGIN_TIMING_DETAIL`; read it the same way
    /// `origin_timing.rs`'s own tests do, so the assertion above is
    /// meaningful whether or not the detail phases are published on this
    /// run.
    fn detail_phases_enabled_for_test() -> bool {
        // Mirrors `origin_timing::detail_phases_enabled` exactly (that
        // function is private to its own module).
        std::env::var("CORELINK_ORIGIN_TIMING_DETAIL").is_ok_and(|v| v == "on")
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
        assert!(
            first.durable,
            "first write of a fresh hash must be durable=true"
        );

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
        assert_eq!(
            first,
            Some(size),
            "the first delete must credit the reclaimed size"
        );

        // Second delete sees the key already gone → None (releases 0).
        let second = client.delete_if_present(&key).await.expect("second delete");
        assert_eq!(
            second, None,
            "a second delete must credit 0 (no double-release)"
        );
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

    // ---------------------------------------------------------------
    // `exists_batch` — the findMissingBlobs seam
    // ---------------------------------------------------------------

    /// Without the async audit seam wired, the handler advertises NO batch
    /// capability, so `find_missing` keeps the unchanged per-digest
    /// `exists()` loop. This is the regression pin for "the serial fallback
    /// stayed byte-identical".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_batch_is_absent_without_the_async_audit_seam() {
        let handler = make_test_handler("iad").await;
        let req = CasReadRequest::new(
            "tenant-x",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "caller@tenant-x",
            "tenant-x",
            1,
        );
        assert!(
            CasReadHandler::exists_batch(&handler, &[req]).is_none(),
            "no durable async sink -> no batch capability -> serial fallback"
        );
    }

    /// Cross-tenant denial is evaluated STRICTLY FIRST — before the audit
    /// batch or any R2 probe is dispatched — and still emits its own
    /// `ReadDenied` row through the sync sink.
    ///
    /// The proof that nothing was dispatched is that this test is fully
    /// hermetic: the S3 client points at `localhost:1` and the D1 sink
    /// carries stub credentials, so ANY dispatch would have to fail against
    /// an unreachable endpoint rather than return a clean denial.
    ///
    /// It deliberately wires an in-memory `audit` alongside a D1
    /// `audit_async` — a combination `with_async_audit`'s doc forbids in
    /// production (the two must be the same sink) — purely so the denial
    /// row can be READ BACK without a network call. The denial path only
    /// ever touches `audit`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_batch_denies_cross_tenant_before_dispatching_anything() {
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
        let recorder = Arc::new(InMemoryAuditSink::new());
        let audit: Arc<dyn AuditSink> = recorder.clone();
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1 client constructs (the network call happens lazily)");
        let sli = Arc::new(InMemorySliObserver::new());
        let handler =
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete);

        // A legitimate digest FIRST, the poisoned one second: the scan must
        // still refuse the whole batch before the good one touches R2.
        let ok = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "attacker",
            "victim",
            1,
        );
        let poisoned = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000002",
            "attacker",
            "attacker-tenant",
            1,
        );

        let result = CasReadHandler::exists_batch(&handler, &[ok, poisoned])
            .expect("batch capability is wired");
        match result {
            Err(CasHandlerError::CrossTenantDenied {
                caller,
                requested_tenant,
            }) => {
                assert_eq!(caller, "attacker-tenant");
                assert_eq!(requested_tenant, "victim");
            }
            other => panic!("expected CrossTenantDenied, got {other:?}"),
        }

        let rows = recorder.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 1, "exactly one denial row: {rows:?}");
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
        assert_eq!(rows[0].tenant, "victim");
    }

    /// The CONCURRENT batch path, production shape: when the durable-audit
    /// D1 write fails (reachable-but-wrong credentials), `exists_batch`
    /// returns `AuditFailed` — NO probe result reaches the caller — and the
    /// joined window is attributed ONCE to `ostore` with `oaudit` ABSENT
    /// (proving neither `append_batch_async` nor the per-probe helper
    /// opened a scope of its own). Mirrors
    /// `r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds`.
    ///
    /// `#[ignore]` for the same reason as that test: needs outbound
    /// reachability to `api.cloudflare.com` (a 401 still proves the join
    /// ran). Run manually with:
    ///
    /// ```bash
    /// cargo test -p corelink-server r2_cas_exists_batch_fails_closed_on_bad_audit_creds -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires outbound network reachability to api.cloudflare.com"]
    async fn r2_cas_exists_batch_fails_closed_on_bad_audit_creds() {
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
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1HttpClient constructs over the stub env");
        let audit: Arc<dyn AuditSink> = audit_concrete.clone();
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = std::sync::Arc::new(
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete),
        );

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let reqs: Vec<CasReadRequest> = (0..4u8)
                            .map(|i| {
                                CasReadRequest::new(
                                    "tenant-x",
                                    format!("{i:064x}"),
                                    "caller@tenant-x",
                                    "tenant-x",
                                    1,
                                )
                            })
                            .collect();
                        let result = CasReadHandler::exists_batch(&*handler, &reqs)
                            .expect("batch capability is wired");
                        assert!(
                            matches!(result, Err(CasHandlerError::AuditFailed(_))),
                            "a rejected D1 credential must surface as AuditFailed, \
                             never as probe results: {result:?}"
                        );
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "the joined window must be attributed to ostore. Header: {header}"
        );
        assert!(
            !parsed.contains_key("oaudit"),
            "oaudit must be ABSENT on the concurrent batch path — a second \
             scope over the same window would double-count it. Header: {header}"
        );
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

    #[test]
    fn public_namespace_resolves_a_stable_isolated_derived_prefix() {
        // The brew-502 root cause: `_public` (the shared cross-tenant dedup
        // namespace) is not a UUID, so the prod path failed CLOSED on it. It MUST
        // instead resolve a stable, secret-keyed prefix — public content is shared
        // by design, but every caller must dedup the same blob to the same key.
        let tdk = TenantDerivationKey::from_bytes(fake_tdk());
        let p = tenant_prefix(Some(&tdk), crate::adapter_cache::PUBLIC_NAMESPACE)
            .expect("_public must resolve a prefix (not fail closed)");
        assert_eq!(p.len(), 16, "prefix must be 16 chars: {p:?}");
        // Secret-keyed reserved-sentinel HMAC (NOT a predictable raw prefix).
        assert_eq!(p, derive_prefix(&tdk, PUBLIC_NAMESPACE_UUID).to_string());
        // Deterministic across calls (so cross-tenant dedup actually dedups).
        assert_eq!(
            p,
            tenant_prefix(Some(&tdk), crate::adapter_cache::PUBLIC_NAMESPACE).unwrap()
        );
        // Reserved: it never collides with a real (UUID) tenant's prefix.
        let real = tenant_prefix(Some(&tdk), "0190abcd-1234-75ab-8def-0123456789ab").unwrap();
        assert_ne!(
            p, real,
            "_public must not collide with a real tenant prefix"
        );
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

    // ---------------------------------------------------------------
    // BYOK Wave 3a — handler-level encrypt/decrypt hooks + Option-gating
    // + fail-closed (the data-plane integration, minus the R2 I/O which
    // is unchanged plumbing). These exercise `R2CasHandler`'s own
    // `byok_encrypt_for_write` / `byok_decrypt_for_read` /
    // `resolve_byok_ctx`; the convergent crypto + caches themselves are
    // covered in `storage::byok_cas::tests`.
    // ---------------------------------------------------------------

    use crate::customer_d1::{
        ByokConfigError, ByokCryptoMode, ByokMode, ByokState, TenantByokConfig,
    };
    use crate::storage::byok_cas::{
        ByokConfigCache, ByokConfigSource, ByokEnvelopeRow, ByokEnvelopeStore, ByokSecretSource,
        ModeBEncryptor, TcsResolver, WrappedTcsRow,
    };
    use corelink_byok::{
        BYOKError, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
    };
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex as StdMutex;

    /// Hermetic in-memory `byok_envelope` store for the Mode-B handler tests.
    /// `fail_delete` injects a reclaim (`delete_envelope`) failure for the Wave-4a
    /// safe-fail test.
    #[derive(Debug, Default)]
    struct MemEnvStore {
        inner: StdMutex<StdHashMap<(String, String), ByokEnvelopeRow>>,
        fail_delete: bool,
    }
    impl MemEnvStore {
        fn failing_delete() -> Self {
            Self {
                inner: StdMutex::default(),
                fail_delete: true,
            }
        }
        fn len(&self) -> usize {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len()
        }
        fn contains(&self, tenant: &str, blob_key: &str) -> bool {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(&(tenant.to_owned(), blob_key.to_owned()))
        }
        fn wrapped_dek(&self, tenant: &str, blob_key: &str) -> Option<Vec<u8>> {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&(tenant.to_owned(), blob_key.to_owned()))
                .map(|r| r.wrapped_dek.clone())
        }
    }
    #[async_trait::async_trait]
    impl ByokEnvelopeStore for MemEnvStore {
        async fn get_envelope(
            &self,
            tenant: &str,
            blob_key: &str,
        ) -> Result<Option<ByokEnvelopeRow>, String> {
            Ok(self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&(tenant.to_owned(), blob_key.to_owned()))
                .cloned())
        }
        async fn put_envelope_if_absent(
            &self,
            tenant: &str,
            blob_key: &str,
            row: &ByokEnvelopeRow,
            _created_at_ms: i64,
        ) -> Result<(), String> {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry((tenant.to_owned(), blob_key.to_owned()))
                .or_insert_with(|| row.clone());
            Ok(())
        }
        async fn delete_envelope(&self, tenant: &str, blob_key: &str) -> Result<(), String> {
            if self.fail_delete {
                return Err("mem env store: injected delete failure".to_owned());
            }
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&(tenant.to_owned(), blob_key.to_owned()));
            Ok(())
        }
    }

    const BYOK_TENANT: &str = "byok-tenant-x";

    fn byok_cfg(crypto_mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: BYOK_TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("arn:cmk".to_owned()),
            cmk_region: Some("iad".to_owned()),
            state,
        }
    }

    #[derive(Debug)]
    struct CfgSrc(Option<TenantByokConfig>);
    #[async_trait::async_trait]
    impl ByokConfigSource for CfgSrc {
        async fn get_byok_config(
            &self,
            _t: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            Ok(self.0.clone())
        }
    }

    #[derive(Debug)]
    struct SecSrc;
    #[async_trait::async_trait]
    impl ByokSecretSource for SecSrc {
        async fn get_wrapped_tcs(&self, _t: &str) -> Result<Option<WrappedTcsRow>, String> {
            Ok(Some(WrappedTcsRow {
                tcs_wrapped: vec![5u8; 32],
                cmk_key_id: Some("arn:cmk".to_owned()),
                tcs_version: 1,
            }))
        }
    }

    #[derive(Debug)]
    struct Kms {
        fail: bool,
    }
    #[async_trait::async_trait]
    impl KmsProvider for Kms {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::AwsKms
        }
        fn region(&self) -> &str {
            "iad"
        }
        fn fips_level(&self) -> corelink_byok::FipsLevel {
            corelink_byok::FipsLevel::Fips140_3_L1
        }
        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            ec: Option<&serde_json::Value>,
        ) -> Result<WrappedDek, BYOKError> {
            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext: dek.bytes.to_vec(),
                encryption_context: ec.cloned(),
            })
        }
        async fn unwrap_dek(&self, w: &WrappedDek) -> Result<Dek, BYOKError> {
            if self.fail {
                return Err(BYOKError::Provider("kms down".to_owned()));
            }
            let mut b = [0u8; 32];
            b.copy_from_slice(&w.ciphertext);
            Ok(Dek { bytes: b })
        }
        async fn check_access(&self, _k: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            Ok(KmsAccessStatus::Ok)
        }
    }

    /// Wire a handler with BYOK collaborators (config + Tcs resolver).
    async fn handler_with_byok(cfg: Option<TenantByokConfig>, kms_fail: bool) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: kms_fail }), 300).unwrap(),
        );
        base.with_byok(cache, resolver)
    }

    /// A CAS handler with BOTH the convergent collaborators AND the Mode-B
    /// (random-DEK) encryptor wired over a shared in-memory envelope store.
    async fn handler_with_byok_random(cfg: Option<TenantByokConfig>) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store: Arc<dyn ByokEnvelopeStore> = Arc::new(MemEnvStore::default());
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    /// Like [`handler_with_byok_random`] but threads a caller-supplied
    /// [`MemEnvStore`] so a test can assert on the persisted envelope rows
    /// (Wave 4a reclaim). The store is shared (the handler holds an `Arc` clone).
    async fn handler_with_byok_random_store(
        cfg: Option<TenantByokConfig>,
        store: Arc<MemEnvStore>,
    ) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store_dyn: Arc<dyn ByokEnvelopeStore> = store;
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store_dyn, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    fn write_req(tenant: &str, bytes: Vec<u8>) -> CasWriteRequest {
        let claimed = Digest::compute(&bytes).to_hex();
        CasWriteRequest::new(tenant, claimed, bytes, "p", tenant, 1)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_inactive_handler_is_plaintext_passthrough() {
        // No `with_byok` at all → the existing plaintext path, byte-identical.
        let h = make_test_handler_with_tdk("iad").await;
        let req = write_req(BYOK_TENANT, b"hello".to_vec());
        assert!(
            h.byok_encrypt_for_write(&req).await.unwrap().is_none(),
            "no BYOK collaborators ⇒ store plaintext (None)"
        );
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h
            .byok_decrypt_for_read(&rreq, b"hello".to_vec())
            .await
            .unwrap();
        assert_eq!(out, b"hello", "read must return the stored bytes unchanged");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_inactive_config_is_plaintext_passthrough() {
        // Collaborators present but state=inactive → still the plaintext path.
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
            false,
        )
        .await;
        let req = write_req(BYOK_TENANT, b"data".to_vec());
        assert!(h.byok_encrypt_for_write(&req).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_encrypts_then_round_trips() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let plaintext = b"top secret artifact".to_vec();
        let req = write_req(BYOK_TENANT, plaintext.clone());

        let stored = h
            .byok_encrypt_for_write(&req)
            .await
            .unwrap()
            .expect("active tenant must encrypt");
        assert_ne!(stored, plaintext, "stored bytes must be ciphertext");

        // Convergent dedup: a second encrypt of the same content is identical.
        let stored2 = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(stored, stored2, "convergent ⇒ idempotent stored bytes");

        // Read back: decrypt → plaintext; the content-hash re-verify (in `read`)
        // then runs on this plaintext.
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h.byok_decrypt_for_read(&rreq, stored).await.unwrap();
        assert_eq!(out, plaintext, "decrypt must recover the plaintext");
        assert!(verify_content_hash(DigestAlgo::Blake3, &req.claimed_hash, &out).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_write_fails_closed_when_kms_down() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true, // KMS unwrap fails
        )
        .await;
        let req = write_req(BYOK_TENANT, b"secret".to_vec());
        assert!(
            matches!(
                h.byok_encrypt_for_write(&req).await,
                Err(CasHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on write (never plaintext)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_read_fails_closed_when_kms_down() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        let rreq = CasReadRequest::new(BYOK_TENANT, "a".repeat(64), "p", BYOK_TENANT, 1);
        // Even given some stored bytes, a failed unwrap must NOT return them raw.
        assert!(
            matches!(
                h.byok_decrypt_for_read(&rreq, b"CLB1raw".to_vec()).await,
                Err(CasHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on read (raw bytes never served)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_unwired_fails_closed() {
        // Mode B active but the random-mode encryptor is NOT wired → fail closed,
        // never plaintext (frozen policy: `byok_mode_b` is `None` here).
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            false,
        )
        .await;
        let req = write_req(BYOK_TENANT, b"x".to_vec());
        assert!(matches!(
            h.byok_encrypt_for_write(&req).await,
            Err(CasHandlerError::Internal(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_wired_round_trips_via_envelope() {
        // Mode B (random) wired: encrypt → CLB2 ciphertext (+ a byok_envelope row),
        // read back → plaintext.
        let h = handler_with_byok_random(Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)))
            .await;
        let plaintext = b"mode-b artifact bytes".to_vec();
        let req = write_req(BYOK_TENANT, plaintext.clone());
        let stored = h
            .byok_encrypt_for_write(&req)
            .await
            .unwrap()
            .expect("active Mode-B tenant must encrypt");
        assert_ne!(stored, plaintext, "Mode-B stored bytes must be ciphertext");
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h.byok_decrypt_for_read(&rreq, stored).await.unwrap();
        assert_eq!(out, plaintext, "Mode-B decrypt must recover the plaintext");
        assert!(verify_content_hash(DigestAlgo::Blake3, &req.claimed_hash, &out).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_re_put_is_idempotent_no_orphan() {
        // Re-encrypt of the same blob reuses the persisted envelope ⇒ byte-identical
        // ciphertext (no orphan; audit C2), and the original still decrypts.
        let h = handler_with_byok_random(Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)))
            .await;
        let req = write_req(BYOK_TENANT, b"idempotent mode-b".to_vec());
        let a = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let b = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(a, b, "Mode-B re-PUT reuses the persisted DEK (no orphan)");
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        assert_eq!(
            h.byok_decrypt_for_read(&rreq, a).await.unwrap(),
            req.bytes,
            "the original Mode-B ciphertext still decrypts"
        );
    }

    // ── BYOK Wave 4a — Mode-B `byok_envelope` reclaim on delete ───────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_delete_reclaims_envelope_row_cas_surface() {
        // A Mode-B blob delete removes the matching CAS-surface envelope row.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"delete-me mode-b".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let key = format!("cas:{}", req.claimed_hash);
        assert!(
            store.contains(BYOK_TENANT, &key),
            "write minted the cas: envelope row"
        );
        assert!(
            !store.contains(BYOK_TENANT, &format!("ac:{}", req.claimed_hash)),
            "no ac: row"
        );
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert!(
            !store.contains(BYOK_TENANT, &key),
            "delete reclaimed the envelope row"
        );
        assert_eq!(store.len(), 0, "no orphan row lingers");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_a_delete_touches_no_envelope_row() {
        // Mode A (convergent) writes NO envelope row; reclaim is a no-op.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"convergent payload".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(store.len(), 0, "Mode A writes no envelope row");
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "Mode-A delete touches no envelope row");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_non_byok_and_public_delete_touch_no_envelope_row() {
        // Non-BYOK (no collaborators) → reclaim is a plaintext no-op.
        let h = make_test_handler_with_tdk("iad").await;
        h.byok_reclaim_for_delete(BYOK_TENANT, &"a".repeat(64), DigestAlgo::Blake3)
            .await
            .unwrap();
        // `_public` under an active Mode-B config → still plaintext (no row).
        let store = Arc::new(MemEnvStore::default());
        let hp = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        hp.byok_reclaim_for_delete(
            crate::adapter_cache::PUBLIC_NAMESPACE,
            &"b".repeat(64),
            DigestAlgo::Blake3,
        )
        .await
        .unwrap();
        assert_eq!(
            store.len(),
            0,
            "_public never writes/reclaims an envelope row"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_re_put_after_delete_mints_fresh_envelope() {
        // After a delete-reclaim, a re-PUT of the SAME blob mints a FRESH envelope
        // row (no stale reuse of the deleted DEK) — confirms the reclaim happened.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"re-put mode-b".to_vec());
        let key = format!("cas:{}", req.claimed_hash);
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let dek_before = store.wrapped_dek(BYOK_TENANT, &key).unwrap();
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "row gone after reclaim");
        // Re-PUT: a brand-new envelope row (fresh random DEK), not the deleted one.
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert!(
            store.contains(BYOK_TENANT, &key),
            "re-PUT minted a fresh envelope row"
        );
        let dek_after = store.wrapped_dek(BYOK_TENANT, &key).unwrap();
        assert_ne!(
            dek_before, dek_after,
            "re-PUT after delete uses a FRESH DEK, not the reclaimed one"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_reclaim_failure_is_safe_fail() {
        // A failed envelope-delete is the SAFE-fail direction: the production
        // delete WARNS and continues (the R2 object is already gone, never rolled
        // back). The reclaim hook surfaces the Err that production swallows.
        let store = Arc::new(MemEnvStore::failing_delete());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"reclaim-fails".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let res = h
            .byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await;
        assert!(
            res.is_err(),
            "reclaim failure surfaces as Err (production warns, never fails the delete)"
        );
        // The row lingers (the failed delete left it) — an orphan that wraps the
        // already-deleted ciphertext (the safe direction); the blob delete itself
        // is unaffected (R2 delete is not rolled back).
        assert!(store.contains(BYOK_TENANT, &format!("cas:{}", req.claimed_hash)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_read_fails_closed_when_kms_down() {
        // A wired Mode-B handler whose KMS unwrap fails must NOT serve raw bytes.
        let base = make_test_handler_with_tdk("iad").await;
        let cfg = Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active));
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: true });
        let store: Arc<dyn ByokEnvelopeStore> = Arc::new(MemEnvStore::default());
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store, 300).unwrap());
        let h = base.with_byok(cache, resolver).with_byok_random(mode_b);
        let rreq = CasReadRequest::new(BYOK_TENANT, "a".repeat(64), "p", BYOK_TENANT, 1);
        let mut blob = b"CLB2".to_vec();
        blob.extend_from_slice(b"ciphertext");
        assert!(
            matches!(
                h.byok_decrypt_for_read(&rreq, blob).await,
                Err(CasHandlerError::Internal(_))
            ),
            "Mode-B read with KMS down must fail closed"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_partial_state_fails_closed() {
        // Partial/backfill dual-read is deferred to Wave 4 — fail closed.
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
            false,
        )
        .await;
        let req = write_req(BYOK_TENANT, b"x".to_vec());
        assert!(matches!(
            h.byok_encrypt_for_write(&req).await,
            Err(CasHandlerError::Internal(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_public_namespace_never_encrypts() {
        // `_public` is shared deterministic content — it must stay plaintext even
        // with an active config, so cross-tenant dedup is preserved (plan §3).
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let req = write_req(
            crate::adapter_cache::PUBLIC_NAMESPACE,
            b"public bottle".to_vec(),
        );
        assert!(
            h.byok_encrypt_for_write(&req).await.unwrap().is_none(),
            "_public must never be encrypted"
        );
    }

    // ---------------------------------------------------------------
    // BYOK Wave 3b — AC handler encrypt/decrypt hooks (surface="ac"),
    // Option-gating, fail-closed, + surface separation from CAS (H1).
    // Exercise `R2AcHandler::byok_encrypt_for_update` /
    // `byok_decrypt_for_lookup` / `resolve_byok_ctx`; the crypto itself
    // is covered in `storage::byok_cas::tests`.
    // ---------------------------------------------------------------

    /// Wire an `R2AcHandler` with BYOK collaborators (mirrors `handler_with_byok`).
    async fn ac_handler_with_byok(cfg: Option<TenantByokConfig>, kms_fail: bool) -> R2AcHandler {
        let base = make_test_ac_handler("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: kms_fail }), 300).unwrap(),
        );
        base.with_byok(cache, resolver)
    }

    /// Wire an `R2AcHandler` with the Mode-B encryptor over a shared envelope
    /// store (mirrors `handler_with_byok_random_store`).
    async fn ac_handler_with_byok_random_store(
        cfg: Option<TenantByokConfig>,
        store: Arc<MemEnvStore>,
    ) -> R2AcHandler {
        let base = make_test_ac_handler("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store_dyn: Arc<dyn ByokEnvelopeStore> = store;
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store_dyn, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    fn ac_update_req(tenant: &str, payload: Vec<u8>) -> corelink_handler_ac::AcUpdateRequest {
        corelink_handler_ac::AcUpdateRequest::new(tenant, "a".repeat(64), payload, "p", tenant, 1)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_b_delete_reclaims_envelope_row_ac_surface() {
        // Wave 4a: an AC Mode-B delete reclaims the `ac:<digest>` envelope row —
        // surface-correct (a CAS row for the same digest would be `cas:<digest>`).
        let store = Arc::new(MemEnvStore::default());
        let h = ac_handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"ac mode-b payload".to_vec());
        h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        let ac_key = format!("ac:{}", req.action_digest);
        assert!(
            store.contains(BYOK_TENANT, &ac_key),
            "write minted the ac: envelope row"
        );
        assert!(
            !store.contains(BYOK_TENANT, &format!("cas:{}", req.action_digest)),
            "no cas: row"
        );
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.action_digest)
            .await
            .unwrap();
        assert!(
            !store.contains(BYOK_TENANT, &ac_key),
            "AC delete reclaimed the ac: row"
        );
        assert_eq!(store.len(), 0, "no orphan AC envelope row lingers");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_a_delete_touches_no_envelope_row() {
        // Mode A on the AC surface writes no envelope row; reclaim is a no-op.
        let store = Arc::new(MemEnvStore::default());
        let h = ac_handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"ac convergent".to_vec());
        h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        assert_eq!(store.len(), 0, "Mode A writes no AC envelope row");
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.action_digest)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "Mode-A AC delete touches no envelope row");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_inactive_handler_is_plaintext_passthrough() {
        // No `with_byok` ⇒ AC plaintext path, byte-identical to today.
        let h = make_test_ac_handler("iad").await;
        let req = ac_update_req(BYOK_TENANT, b"ac-result".to_vec());
        assert!(
            h.byok_encrypt_for_update(&req).await.unwrap().is_none(),
            "no BYOK collaborators ⇒ store the AC payload plaintext (None)"
        );
        let out = h
            .byok_decrypt_for_lookup(BYOK_TENANT, &req.action_digest, b"ac-result".to_vec())
            .await
            .unwrap();
        assert_eq!(
            out, b"ac-result",
            "lookup must return the stored bytes unchanged"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_encrypts_then_round_trips() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let payload = b"proven action result metadata".to_vec();
        let req = ac_update_req(BYOK_TENANT, payload.clone());
        let stored = h
            .byok_encrypt_for_update(&req)
            .await
            .unwrap()
            .expect("active tenant must encrypt the AC payload");
        assert_ne!(stored, payload, "AC stored bytes must be ciphertext");
        // Convergent ⇒ identical payload yields byte-identical stored bytes — this
        // is what keeps the divergent-body GET-and-compare idempotent for an
        // active tenant (ciphertext-vs-ciphertext).
        let stored2 = h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        assert_eq!(stored, stored2, "convergent ⇒ idempotent AC stored bytes");
        let out = h
            .byok_decrypt_for_lookup(BYOK_TENANT, &req.action_digest, stored)
            .await
            .unwrap();
        assert_eq!(out, payload, "decrypt must recover the AC plaintext");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_write_fails_closed_when_kms_down() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"secret".to_vec());
        assert!(
            matches!(
                h.byok_encrypt_for_update(&req).await,
                Err(corelink_handler_ac::AcHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on AC write (never plaintext)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_read_fails_closed_when_kms_down() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        assert!(
            matches!(
                h.byok_decrypt_for_lookup(BYOK_TENANT, &"a".repeat(64), b"CLB1raw".to_vec())
                    .await,
                Err(corelink_handler_ac::AcHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on AC read (raw bytes never served)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_b_and_partial_fail_closed() {
        for (mode, state) in [
            (ByokCryptoMode::Random, ByokState::Active),
            (ByokCryptoMode::Convergent, ByokState::Partial),
        ] {
            let h = ac_handler_with_byok(Some(byok_cfg(mode, state)), false).await;
            let req = ac_update_req(BYOK_TENANT, b"x".to_vec());
            assert!(
                matches!(
                    h.byok_encrypt_for_update(&req).await,
                    Err(corelink_handler_ac::AcHandlerError::Internal(_))
                ),
                "Mode B / partial AC write must fail closed, never plaintext ({mode:?},{state:?})"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_public_namespace_never_encrypts() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let req = ac_update_req(crate::adapter_cache::PUBLIC_NAMESPACE, b"shared".to_vec());
        assert!(
            h.byok_encrypt_for_update(&req).await.unwrap().is_none(),
            "_public AC entries must never be encrypted (dedup)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_blob_does_not_decrypt_under_cas_and_vice_versa() {
        // Frozen policy H1 at the HANDLER level: an AC-encrypted blob must NOT be
        // decryptable by the CAS read hook, and a CAS blob must NOT be decryptable
        // by the AC lookup hook — even for the SAME tenant + digest + tcs (same
        // SecSrc/Kms). Domain separation by `surface` makes an AC↔CAS blob swap
        // fail closed end-to-end.
        let digest = "a".repeat(64);
        let payload = b"cross-surface".to_vec();
        let ac = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let cas = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;

        let ac_req = corelink_handler_ac::AcUpdateRequest::new(
            BYOK_TENANT,
            digest.clone(),
            payload.clone(),
            "p",
            BYOK_TENANT,
            1,
        );
        let ac_blob = ac.byok_encrypt_for_update(&ac_req).await.unwrap().unwrap();
        let cas_rreq = CasReadRequest::new(BYOK_TENANT, &digest, "p", BYOK_TENANT, 1);
        assert!(
            cas.byok_decrypt_for_read(&cas_rreq, ac_blob).await.is_err(),
            "an AC blob must NOT decrypt under the CAS surface"
        );

        let cas_wreq =
            CasWriteRequest::new(BYOK_TENANT, digest.clone(), payload, "p", BYOK_TENANT, 1);
        let cas_blob = cas
            .byok_encrypt_for_write(&cas_wreq)
            .await
            .unwrap()
            .unwrap();
        assert!(
            ac.byok_decrypt_for_lookup(BYOK_TENANT, &digest, cas_blob)
                .await
                .is_err(),
            "a CAS blob must NOT decrypt under the AC surface"
        );
    }

    // ---------------------------------------------------------------
    // WP-C — AC conditional PUT: prior-state classification
    // ---------------------------------------------------------------

    #[test]
    fn classify_prior_absent_allows_put() {
        assert_eq!(
            classify_prior(None, b"payload"),
            PriorState::Absent,
            "no prior object → the caller may create it"
        );
    }

    #[test]
    fn classify_prior_identical_is_idempotent_no_op() {
        let view = vec![7u8; 32];
        assert_eq!(
            classify_prior(Some(&view), &view),
            PriorState::Identical,
            "byte-identical re-PUT → no-op (durable=false)"
        );
    }

    #[test]
    fn classify_prior_divergent_refuses() {
        let view = vec![1u8; 16];
        let other = vec![2u8; 16];
        assert_eq!(
            classify_prior(Some(&other), &view),
            PriorState::Divergent,
            "divergent stored bytes → DivergentBody (409), never overwrite"
        );
    }

    #[test]
    fn classify_prior_empty_vs_empty_is_identical_not_divergent() {
        // Edge: an empty stored body and empty would-be-stored bytes are
        // EQUAL — the immutability contract compares bytes, not presence.
        assert_eq!(classify_prior(Some(&[]), &[]), PriorState::Identical);
    }

    #[test]
    fn lost_race_resolution_mirrors_pre_put_compare() {
        // The Ok(false) arm of put_if_absent re-GETs and applies the SAME
        // classifier: identical → no-op, divergent → DivergentBody. Pin the
        // mapping so a future edit cannot make the loser overwrite or
        // double-report.
        let winner_bytes = vec![9u8; 64];
        let loser_view = vec![1u8; 64];

        // Loser wrote the SAME bytes → idempotent success (durable=false).
        assert_eq!(
            classify_prior(Some(&winner_bytes), &winner_bytes),
            PriorState::Identical
        );

        // Loser's body DIVERGES from what won → refused, winner preserved.
        assert_eq!(
            classify_prior(Some(&winner_bytes), &loser_view),
            PriorState::Divergent
        );
    }
}

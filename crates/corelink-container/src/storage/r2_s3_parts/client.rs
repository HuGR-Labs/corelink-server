// R2 storage adapter using the AWS S3-compatible API.
//
// This module provides:
//
// - [`R2S3Client`] — low-level async `put_object` / `get_object`
//   wrapper over `aws-sdk-s3` pointed at the R2 S3 endpoint.
// - [`R2CasHandler`] — a sync `CasReadHandler` + `CasWriteHandler`
//   implementation that uses [`R2S3Client`] for durable storage and
//   `derive_prefix` for tenant-scoped R2 keys.
//
// # Key scheme
//
// ```text
// <region>/<tenant_prefix_16>/<digest>
// ```
//
// The `tenant_prefix_16` is derived via
// `corelink_tenant_path::derive_prefix` so cross-tenant key
// co-residence is impossible (layer 5 of `INV-TENANT-ISOLATION`).
//
// # Sync wrapper
//
// The `CasReadHandler` / `CasWriteHandler` traits are synchronous
// (they exist in the pre-async R-prep layer). `R2CasHandler` bridges
// the async S3 SDK into the sync trait surface by using
// `tokio::runtime::Handle::current().block_on(...)`. The server runs
// inside a tokio runtime, so a handle is always available.
//
// # Security charter compliance
//
// - No credentials in code; constructed from [`StorageEnv`].
// - No secrets logged; tracing events contain bucket + key only.
// - No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.

use std::sync::Arc;
use std::time::Instant;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use corelink_byok::{CryptoContext, CryptoMode, Tcs};
use corelink_handler_cas::{
    AuditEvent, AuditEventKind, AuditSink, CasDeleteHandler, CasHandlerError, CasListHandler,
    CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest,
    CasWriteResponse, DigestAlgo, SliObservation, SliObserver,
};
// `InMemoryAuditSink` is now used only by tests (the deployed builder wires the
// durable D1 sink); gate the import so the non-test build stays warning-clean.
#[cfg(test)]
use corelink_handler_cas::InMemoryAuditSink;
// Same reason: the capture-everything SLI observer is a TEST fixture now that
// the deployed builders wire the constant-memory `CountingSliObserver` (B-057).
#[cfg(test)]
use corelink_handler_cas::InMemorySliObserver;
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

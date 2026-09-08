/// The narrow body interface keeps the bounded collector testable with a
/// deterministic stream while the production adapter wraps AWS's
/// `ByteStream` below.
trait R2BodyChunkStream {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Option<Result<bytes::Bytes, String>>,
                > + Send
                + 'a,
        >,
    >;
}

struct R2SdkBodyStream(aws_sdk_s3::primitives::ByteStream);

impl R2BodyChunkStream for R2SdkBodyStream {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Option<Result<bytes::Bytes, String>>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.0
                .next()
                .await
                .map(|chunk| chunk.map_err(|error| error.to_string()))
        })
    }
}

impl R2S3Client {
    /// Bound the three phases that can otherwise leave a synchronous CAS
    /// reader parked forever when R2 stops making progress. The operation
    /// timeout is deliberately longer than the per-read timeout because a
    /// large object may legitimately need several read windows, while still
    /// giving cancellation a finite unwind point.
    pub(crate) const R2_CONNECT_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(2);
    pub(crate) const R2_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    pub(crate) const R2_OPERATION_ATTEMPT_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(30);
    pub(crate) const R2_OPERATION_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(60);
    /// Body-level deadlines remain necessary after `GetObject` headers arrive:
    /// the SDK operation timeout does not reliably cover a stalled stream.
    pub(crate) const R2_BODY_IDLE_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(30);
    pub(crate) const R2_BODY_TOTAL_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(60);

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
            .timeout_config(
                aws_sdk_s3::config::timeout::TimeoutConfig::builder()
                    .connect_timeout(Self::R2_CONNECT_TIMEOUT)
                    .read_timeout(Self::R2_READ_TIMEOUT)
                    .operation_attempt_timeout(Self::R2_OPERATION_ATTEMPT_TIMEOUT)
                    .operation_timeout(Self::R2_OPERATION_TIMEOUT)
                    .build(),
            )
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
                // Do not use `ByteStream::collect()` here. Content-Length is
                // useful admission metadata but is not a safe bound for a
                // malformed/chunked response. Consume incrementally and stop
                // as soon as the actual stream crosses the ceiling, keeping
                // peak allocation bounded even when R2 lies about its length.
                let body = R2SdkBodyStream(output.body);
                Self::collect_capped_body(key, max_bytes, body).await
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

    async fn collect_capped_body<S: R2BodyChunkStream>(
        key: &str,
        max_bytes: u64,
        body: S,
    ) -> Result<CappedGet, String> {
        Self::collect_capped_body_with_deadlines(
            key,
            max_bytes,
            body,
            Self::R2_BODY_IDLE_TIMEOUT,
            Self::R2_BODY_TOTAL_TIMEOUT,
        )
        .await
    }

    async fn collect_capped_body_with_deadlines<S: R2BodyChunkStream>(
        key: &str,
        max_bytes: u64,
        mut body: S,
        idle_timeout: std::time::Duration,
        total_timeout: std::time::Duration,
    ) -> Result<CappedGet, String> {
        let collect = async {
            let mut bytes = Vec::new();
            let mut actual_bytes = 0_u64;
            loop {
                let next = tokio::time::timeout(idle_timeout, body.next_chunk())
                    .await
                    .map_err(|_| format!("R2 body idle timeout for key {key}"))?;
                let Some(chunk) = next else {
                    return Ok(CappedGet::Found(bytes));
                };
                let chunk = chunk.map_err(|error| format!("R2 body read failed for key {key}: {error}"))?;
                actual_bytes = actual_bytes.saturating_add(chunk.len() as u64);
                if actual_bytes > max_bytes {
                    return Ok(CappedGet::TooLarge {
                        actual_bytes: Some(actual_bytes),
                    });
                }
                bytes.extend_from_slice(&chunk);
            }
        };
        tokio::time::timeout(total_timeout, collect)
            .await
            .map_err(|_| format!("R2 body total timeout for key {key}"))?
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

//! R2 single-blob adapter (WI-S01-003).
//!
//! Implements [`R2Writer`] / [`R2Reader`] for blobs ≤ 5 MiB plus the in-memory
//! [`InMemoryR2`] backend used by the test suite. Both writers route every
//! call through the canonical `super::key::canonical_key`
//! constructor and the [`R2Backend::put_if_none_match`] / [`R2Backend::get`]
//! contract; nothing in this crate ever touches a tenant_id outside of the
//! ctx + key path.
//!
//! ## Cross-tenant isolation by construction
//!
//! [`R2Writer::put`] and [`R2Reader::get`] take a `&TenantCtx`. The ctx
//! carries a [`corelink_tenant_path::TenantPrefix`] (the HMAC16 of
//! `tenant_id` under the per-region TDK). Two distinct tenant UUIDs produce
//! distinct prefixes with overwhelming probability (≈ 2^-96 per pair, see
//! the `corelink_tenant_path` crate's rustdoc), so cross-tenant key
//! collisions are vanishingly rare. Even if a collision did occur it would
//! grant only namespace co-residence: ACL is enforced upstream by the auth
//! middleware (S-03), and read-after-write semantics are content-addressed.
//!
//! ## Idempotent duplicate semantics (INV-CAS-IDEMPOTENCY)
//!
//! Every PUT carries `If-None-Match: *`; if the key already exists, R2
//! returns 412 Precondition Failed and the writer surfaces
//! [`PutOutcome::Duplicate`]. The [`corelink_hash::BlobStoreWrite`] trait
//! then maps Duplicate → `Ok(())` (the trait API has no concept of an
//! "already there" return because, semantically, the write *succeeded*: the
//! body is durably stored under the digest). Callers that need to distinguish
//! Fresh vs. Duplicate (e.g. for the `corelink.storage.r2.put_total{result}`
//! metric) call [`R2Writer::put`] directly.
//!
//! ## Backend trait abstraction
//!
//! [`R2Backend`] models the minimal R2 surface this crate touches:
//! `put_if_none_match`, `get`, `head`. The real Cloudflare binding adapter
//! lands in WI-S01-005 alongside the REAPI handler (it requires the Workers
//! runtime context). For host-side test/CI we use [`InMemoryR2`] which
//! preserves the same idempotent semantics.

use core::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` for InMemoryR2 fake
// (infallible lock; no poisoning). `R2Error::Backend` mutex-poisoned
// path is structurally unreachable on this backend but retained for
// transport-class failures on the production R2 binding.
use parking_lot::Mutex;

use bytes::Bytes;
use corelink_hash::{BlobStoreWrite, Digest, VerifiedBody};

use super::error::R2Error;
use super::key::canonical_key;
use super::metrics::{GetResultLabel, MetricsObserver, NoopMetrics, PutResultLabel, PutSizeBucket};
use crate::tenant::TenantCtx;
use crate::Region;

/// Single-blob upper bound for [`R2Writer::put`] (WI-S01-003 title:
/// "≤ 5 MiB"). Multipart upload for blobs > 5 MiB is WI-S05-003.
pub const SINGLE_BLOB_LIMIT_BYTES: usize = 5 * 1024 * 1024;

/// Outcome of [`R2Writer::put`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutOutcome {
    /// The PUT succeeded for the first time at this key (R2 200/204).
    Fresh,
    /// The PUT was rejected by `If-None-Match: *` because the key already
    /// existed (R2 412 Precondition Failed). Per INV-CAS-IDEMPOTENCY this is
    /// **not** an error: the body is byte-identical (CAS guarantees this) and
    /// the write is semantically successful. The caller emits the
    /// `result="conflict_duplicate"` metric here (WI-S01-003 §6.1.5).
    Duplicate,
}

/// Outcome of an [`R2Backend::put_if_none_match`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendPutOutcome {
    /// Object newly written.
    Stored,
    /// Object existed; `If-None-Match: *` matched. No write performed.
    AlreadyExists,
}

/// Minimal R2 surface used by [`R2Writer`] / [`R2Reader`].
///
/// All methods are async; the real Cloudflare binding adapter (WI-S01-005)
/// implements them by calling `env.R2_CAS_<REGION>.put(...)` etc. The
/// [`InMemoryR2`] fake provided in this module implements them with a
/// `HashMap<String, Bytes>` plus a `Mutex` for interior mutability.
///
/// The trait deliberately uses `impl Future` rather than `async fn in trait`
/// to mirror the [`corelink_hash::BlobStoreWrite`] contract and to keep the
/// crate buildable without `async-trait` macro pulls.
pub trait R2Backend: Send + Sync {
    /// PUT `body` at `key` only if no object currently lives there
    /// (`If-None-Match: *`). Returns [`BackendPutOutcome::Stored`] on first
    /// write, [`BackendPutOutcome::AlreadyExists`] on collision.
    fn put_if_none_match<'a>(
        &'a self,
        key: &'a str,
        body: Bytes,
    ) -> impl Future<Output = Result<BackendPutOutcome, R2Error>> + Send + 'a;

    /// GET `key`. Returns [`R2Error::NotFound`] if the key does not exist.
    fn get<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Bytes, R2Error>> + Send + 'a;

    /// HEAD `key`. Returns true iff an object lives at `key`. Used for
    /// idempotency probes / GC sweeps; not load-bearing for S-01 PUT/GET hot
    /// paths but exposed here so the production CF binding adapter (S-01-005)
    /// can surface the cheap existence check directly.
    fn head<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<bool, R2Error>> + Send + 'a;
}

/// CAS write adapter: takes a `&TenantCtx` + `&VerifiedBody` and lands the
/// body at the canonical R2 key.
///
/// Construction wires a [`Region`] (residency anchor) and a backend. Every
/// `put` call **enforces** that `ctx.region() == writer.region()`; a
/// mismatched dispatcher (e.g. a WEUR tenant accidentally routed to the
/// WNAM writer) is rejected with [`R2Error::RegionMismatch`] rather than
/// silently writing to the wrong bucket. This protects INV-DATA-RESIDENCY
/// at the storage seam.
pub struct R2Writer<B: R2Backend> {
    region: Region,
    backend: Arc<B>,
    metrics: Arc<dyn MetricsObserver>,
}

impl<B: R2Backend> core::fmt::Debug for R2Writer<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2Writer")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl<B: R2Backend> R2Writer<B> {
    /// Construct a writer pinned to `region` over `backend`.
    ///
    /// Uses [`NoopMetrics`] for the metrics observer; tests + production
    /// deploys that need real observability call [`Self::with_metrics`].
    #[must_use]
    pub fn new(region: Region, backend: Arc<B>) -> Self {
        Self {
            region,
            backend,
            metrics: Arc::new(NoopMetrics),
        }
    }

    /// Construct a writer with an explicit metrics observer.
    ///
    /// Production binders (S-09 observability) use this to plug in a
    /// Workers Analytics-backed observer. The observer is called exactly
    /// once per `put` call, in the success or failure tail.
    #[must_use]
    pub fn with_metrics(
        region: Region,
        backend: Arc<B>,
        metrics: Arc<dyn MetricsObserver>,
    ) -> Self {
        Self {
            region,
            backend,
            metrics,
        }
    }

    /// The [`Region`] this writer is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Persist `vb` under `ctx`'s tenant prefix in this writer's region.
    ///
    /// Enforces the WI-S01-003 §6.1.1 invariants:
    /// 1. Path derivation via the canonical key constructor
    ///    (REG-NAMESPACE-001/-002).
    /// 2. Body size ≤ [`SINGLE_BLOB_LIMIT_BYTES`] (WI title constraint).
    /// 3. `If-None-Match: *` semantics (INV-CAS-IMMUTABILITY).
    ///
    /// Returns [`PutOutcome::Fresh`] for the first writer of a digest and
    /// [`PutOutcome::Duplicate`] for any subsequent attempt (callers emit
    /// the `result="conflict_duplicate"` metric on Duplicate).
    pub async fn put(&self, ctx: &TenantCtx, vb: &VerifiedBody) -> Result<PutOutcome, R2Error> {
        let started = std::time::Instant::now();
        if ctx.region() != self.region {
            self.metrics.record_put(self.region, PutResultLabel::Error);
            self.metrics.record_put_duration(
                self.region,
                PutSizeBucket::from_bytes(vb.body().len()),
                started.elapsed(),
            );
            return Err(R2Error::RegionMismatch {
                adapter: self.region,
                ctx: ctx.region(),
            });
        }
        let body = vb.body();
        let size_bucket = PutSizeBucket::from_bytes(body.len());
        if body.len() > SINGLE_BLOB_LIMIT_BYTES {
            self.metrics.record_put(self.region, PutResultLabel::Error);
            self.metrics
                .record_put_duration(self.region, size_bucket, started.elapsed());
            return Err(R2Error::BlobTooLarge {
                size: body.len(),
                limit: SINGLE_BLOB_LIMIT_BYTES,
            });
        }
        let key = canonical_key(self.region, ctx.prefix(), vb.digest());
        let outcome = match self.backend.put_if_none_match(&key, body.clone()).await {
            Ok(o) => o,
            Err(e) => {
                self.metrics.record_put(self.region, PutResultLabel::Error);
                self.metrics
                    .record_put_duration(self.region, size_bucket, started.elapsed());
                return Err(e);
            }
        };
        let label = match outcome {
            BackendPutOutcome::Stored => PutResultLabel::Ok,
            BackendPutOutcome::AlreadyExists => PutResultLabel::ConflictDuplicate,
        };
        self.metrics.record_put(self.region, label);
        self.metrics
            .record_put_duration(self.region, size_bucket, started.elapsed());
        Ok(match outcome {
            BackendPutOutcome::Stored => PutOutcome::Fresh,
            BackendPutOutcome::AlreadyExists => PutOutcome::Duplicate,
        })
    }

    /// Yield a per-tenant view that implements [`BlobStoreWrite`].
    ///
    /// The returned [`ScopedR2Writer`] captures `ctx` and exposes the
    /// canonical type-driven trait surface from `corelink-hash`: any code
    /// path holding a `BlobStoreWrite` cannot construct a write without first
    /// running `VerifiedBody::new`, and the trait return is `Result<(), _>`
    /// (Duplicate maps to `Ok(())` — the body is durably stored either way).
    #[must_use]
    pub fn for_tenant<'a>(&'a self, ctx: &'a TenantCtx) -> ScopedR2Writer<'a, B> {
        ScopedR2Writer { writer: self, ctx }
    }
}

/// A per-tenant adapter over [`R2Writer`] that implements
/// [`BlobStoreWrite`].
///
/// This is the seam that wires WI-S01-002's `BlobStoreWrite` trait to the R2
/// backend at the type level: the trait method takes `&VerifiedBody`, so any
/// caller who wants to write to R2 *must* go through `VerifiedBody::new` —
/// the BLAKE3 verify is a compile-time obligation, not a code-review one.
#[derive(Debug)]
pub struct ScopedR2Writer<'a, B: R2Backend> {
    writer: &'a R2Writer<B>,
    ctx: &'a TenantCtx,
}

impl<B: R2Backend> BlobStoreWrite for ScopedR2Writer<'_, B> {
    type Error = R2Error;

    async fn put_verified(&self, vb: &VerifiedBody) -> Result<(), Self::Error> {
        // Duplicate is a successful idempotent write per
        // INV-CAS-IDEMPOTENCY; the trait surface returns `Ok(())` for
        // both Fresh and Duplicate. Callers that need the distinction
        // (e.g. for metrics) call `writer.put` directly.
        self.writer.put(self.ctx, vb).await.map(|_| ())
    }
}

/// CAS read adapter: takes a `&TenantCtx` + `&Digest` and returns the body.
pub struct R2Reader<B: R2Backend> {
    region: Region,
    backend: Arc<B>,
    metrics: Arc<dyn MetricsObserver>,
}

impl<B: R2Backend> core::fmt::Debug for R2Reader<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2Reader")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl<B: R2Backend> R2Reader<B> {
    /// Construct a reader pinned to `region` over `backend` with a no-op
    /// metrics observer.
    #[must_use]
    pub fn new(region: Region, backend: Arc<B>) -> Self {
        Self {
            region,
            backend,
            metrics: Arc::new(NoopMetrics),
        }
    }

    /// Construct a reader with an explicit metrics observer.
    #[must_use]
    pub fn with_metrics(
        region: Region,
        backend: Arc<B>,
        metrics: Arc<dyn MetricsObserver>,
    ) -> Self {
        Self {
            region,
            backend,
            metrics,
        }
    }

    /// The [`Region`] this reader is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Fetch the blob at `(ctx.prefix(), digest)` in this reader's region.
    ///
    /// Like [`R2Writer::put`], this method enforces
    /// `ctx.region() == reader.region()` — a misrouted reader is rejected
    /// with [`R2Error::RegionMismatch`] (residency invariant
    /// INV-DATA-RESIDENCY).
    ///
    /// On miss returns [`R2Error::NotFound`] which the REAPI handler maps to
    /// `COR_CAS_BLOB_NOT_FOUND`. Cross-tenant reads (Tenant B requesting a
    /// digest stored under Tenant A's prefix) hit a different key and
    /// therefore see the same NotFound — by construction, ADR-0028 closes
    /// the cross-tenant enumeration oracle here.
    pub async fn get(&self, ctx: &TenantCtx, digest: &Digest) -> Result<Bytes, R2Error> {
        let started = std::time::Instant::now();
        if ctx.region() != self.region {
            self.metrics.record_get(self.region, GetResultLabel::Error);
            self.metrics
                .record_get_duration(self.region, started.elapsed());
            return Err(R2Error::RegionMismatch {
                adapter: self.region,
                ctx: ctx.region(),
            });
        }
        let key = canonical_key(self.region, ctx.prefix(), digest);
        let result = self.backend.get(&key).await;
        let label = match &result {
            Ok(_) => GetResultLabel::Ok,
            Err(R2Error::NotFound) => GetResultLabel::NotFound,
            Err(_) => GetResultLabel::Error,
        };
        self.metrics.record_get(self.region, label);
        self.metrics
            .record_get_duration(self.region, started.elapsed());
        result
    }
}

// ---------------------------------------------------------------------------
// In-memory test backend
// ---------------------------------------------------------------------------

/// Host-side test fake for [`R2Backend`].
///
/// Backed by a `Mutex<HashMap<String, Bytes>>`. Implements idempotent
/// `put_if_none_match` semantics identically to R2 (REG-NAMESPACE-002 +
/// INV-CAS-IMMUTABILITY): a second PUT to the same key returns
/// [`BackendPutOutcome::AlreadyExists`] without overwriting.
///
/// The fake is not benchmarked or hardened for high concurrency — it exists
/// to exercise the same code paths as the production binding adapter under
/// `cargo test`. Production deployments wire the real Cloudflare binding in
/// WI-S01-005.
#[derive(Debug, Default)]
pub struct InMemoryR2 {
    inner: Mutex<HashMap<String, Bytes>>,
}

impl InMemoryR2 {
    /// Construct a fresh, empty in-memory R2 fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of objects currently stored. Test-only helper.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// True iff no objects are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot the current set of keys. Test-only helper used by integration
    /// tests to assert key-construction grammar.
    #[must_use]
    pub fn keys_snapshot(&self) -> Vec<String> {
        let guard = self.inner.lock();
        guard.keys().cloned().collect()
    }

    /// Test-only helper: forcibly delete the R2 object whose key
    /// EXACTLY matches `key` while leaving any external `blob_meta`
    /// row untouched. Used by integration tests to simulate the
    /// canonical "R2 orphan window" — an alive `blob_meta` row
    /// whose R2 object disappeared (e.g. R2 GC race; backup-restore
    /// with stale R2; force-delete via R2 API). Production code MUST
    /// never call this; the GC reconciler in S-06 closes the orphan
    /// window via the supported deletion API.
    ///
    /// Returns `true` iff a key was removed (i.e. the orphan was
    /// actually staged). Tests should assert the return value to
    /// avoid false-passing scenarios where the digest never
    /// existed.
    ///
    /// Codex round-4 P2 SEAL fix: the earlier signature took
    /// `(tenant_id, region, digest)` but ignored tenant_id +
    /// region and matched by digest-hex suffix only. Two different
    /// tenants writing the same body would both be evicted by the
    /// first call — collapsing into a false-pass on
    /// isolation/orphan scenarios. The exact-key form is
    /// unambiguous; tests compute the canonical key via the
    /// `keys_snapshot()` helper paired with their own
    /// (tenant, region) state.
    pub fn evict_key_for_test(&self, key: &str) -> bool {
        let mut guard = self.inner.lock();
        guard.remove(key).is_some()
    }

    /// Test-only helper: simulate **R2 silent bit rot** (FM-051) on an
    /// existing key by overwriting the stored bytes with the supplied
    /// `corrupt_body`. Used by the WI-S02-006 bit-rot integration test
    /// to drive `corelink_client_verify::ClientVerifier::verify` into
    /// a digest mismatch on the read path. Production code MUST never
    /// call this — there is no on-Cloudflare equivalent.
    ///
    /// Returns `true` iff a value was overwritten (the key existed
    /// pre-call). Tests should assert the return value to avoid
    /// false-passing scenarios where the key never existed.
    pub fn inject_corrupt_for_test(&self, key: &str, corrupt_body: Bytes) -> bool {
        let mut guard = self.inner.lock();
        guard.insert(key.to_owned(), corrupt_body).is_some()
    }
}

impl R2Backend for InMemoryR2 {
    async fn put_if_none_match(
        &self,
        key: &str,
        body: Bytes,
    ) -> Result<BackendPutOutcome, R2Error> {
        // parking_lot lock is infallible — R2Error::Backend mutex-
        // poisoned mapping unreachable here; retained for production
        // transport-class failures.
        let mut guard = self.inner.lock();
        if guard.contains_key(key) {
            return Ok(BackendPutOutcome::AlreadyExists);
        }
        guard.insert(key.to_owned(), body);
        Ok(BackendPutOutcome::Stored)
    }

    async fn get(&self, key: &str) -> Result<Bytes, R2Error> {
        let guard = self.inner.lock();
        guard.get(key).cloned().ok_or(R2Error::NotFound)
    }

    async fn head(&self, key: &str) -> Result<bool, R2Error> {
        let guard = self.inner.lock();
        Ok(guard.contains_key(key))
    }
}

// ---------------------------------------------------------------------------
// Failure-injecting test backend
// ---------------------------------------------------------------------------

/// Test wrapper backend that counts `get` / `put_if_none_match` /
/// `head` calls per inner backend. Lets the WI-S02-001 integration
/// tests assert "no R2 GET was issued on the cross-tenant path" beyond
/// the `keys_snapshot` invariant — which only checks final state, not
/// whether the GET method was called.
#[derive(Debug)]
pub struct CountingR2<B: R2Backend> {
    inner: Arc<B>,
    get_calls: std::sync::atomic::AtomicUsize,
    put_calls: std::sync::atomic::AtomicUsize,
    head_calls: std::sync::atomic::AtomicUsize,
}

impl<B: R2Backend> CountingR2<B> {
    /// Wrap an existing backend with call counters.
    #[must_use]
    pub fn new(inner: Arc<B>) -> Self {
        Self {
            inner,
            get_calls: std::sync::atomic::AtomicUsize::new(0),
            put_calls: std::sync::atomic::AtomicUsize::new(0),
            head_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Number of `get` invocations observed.
    #[must_use]
    pub fn get_calls(&self) -> usize {
        self.get_calls.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Number of `put_if_none_match` invocations observed.
    #[must_use]
    pub fn put_calls(&self) -> usize {
        self.put_calls.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Number of `head` invocations observed.
    #[must_use]
    pub fn head_calls(&self) -> usize {
        self.head_calls.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl<B: R2Backend> R2Backend for CountingR2<B> {
    async fn put_if_none_match(
        &self,
        key: &str,
        body: Bytes,
    ) -> Result<BackendPutOutcome, R2Error> {
        self.put_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.put_if_none_match(key, body).await
    }

    async fn get(&self, key: &str) -> Result<Bytes, R2Error> {
        self.get_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.get(key).await
    }

    async fn head(&self, key: &str) -> Result<bool, R2Error> {
        self.head_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.head(key).await
    }
}

/// Test backend that injects [`R2Error::Backend`] on every call.
///
/// Used by the integration tests to verify that the writer surfaces
/// transient backend faults verbatim (mapped upstream to
/// `COR_SERVICE_DEGRADED` per `error_taxonomy.md`).
#[derive(Debug)]
pub struct AlwaysFailingR2 {
    /// Stable diagnostic carried back to the caller for assertion.
    pub diagnostic: String,
}

impl AlwaysFailingR2 {
    /// Construct a failing backend with a fixed diagnostic message.
    #[must_use]
    pub fn new(diagnostic: impl Into<String>) -> Self {
        Self {
            diagnostic: diagnostic.into(),
        }
    }
}

impl R2Backend for AlwaysFailingR2 {
    async fn put_if_none_match(
        &self,
        _key: &str,
        _body: Bytes,
    ) -> Result<BackendPutOutcome, R2Error> {
        Err(R2Error::Backend(self.diagnostic.clone()))
    }

    async fn get(&self, _key: &str) -> Result<Bytes, R2Error> {
        Err(R2Error::Backend(self.diagnostic.clone()))
    }

    async fn head(&self, _key: &str) -> Result<bool, R2Error> {
        Err(R2Error::Backend(self.diagnostic.clone()))
    }
}

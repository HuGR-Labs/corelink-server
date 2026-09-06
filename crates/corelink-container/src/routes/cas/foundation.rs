use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::wall_clock::{SystemWallClock, WallClock};
use axum::{
    body::Body,
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasListHandler,
    CasListRequest, CasListResponse, CasReadHandler, CasReadRequest, CasReadResponse,
    CasWriteHandler, CasWriteRequest, CasWriteResponse, InMemoryAuditSink, InMemoryCasHandler,
    InMemorySliObserver,
};
use http_body::Frame;
use http_body_util::StreamBody;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::container_capacity::CONTAINER_MEMORY_BYTES;

/// Canonical CAS list route path — `GET /v1/cas/:tenant` (D-8).
pub const CAS_LIST_ROUTE: &str = "/v1/cas/{tenant}";

/// Default page size for the CAS list route when `?limit` is absent.
const DEFAULT_LIST_LIMIT: u32 = 200;
/// Hard cap on the CAS list page size (contract: `1..=1000`).
const MAX_LIST_LIMIT: u32 = 1000;

/// Hard cap on the opaque continuation `?cursor` length. A legitimate cursor is
/// a short server-minted token; bounding it keeps a malformed/abusive value a
/// clean 400 reject instead of one that rides the ~16 KB edge query limit into
/// the list backend.
const MAX_CURSOR_LEN: usize = 1024;

/// Query parameters for the paginated CAS list route (`?limit=&cursor=`).
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListQuery {
    /// Requested page size; clamped to `1..=1000` (default 200).
    pub limit: Option<u32>,
    /// Opaque continuation cursor from a prior page.
    pub cursor: Option<String>,
}

/// Clamp a requested `?limit` into the `1..=1000` contract window,
/// defaulting to 200 when absent or zero.
fn clamp_limit(requested: Option<u32>) -> u32 {
    match requested {
        None | Some(0) => DEFAULT_LIST_LIMIT,
        Some(n) => n.min(MAX_LIST_LIMIT),
    }
}

/// Canonical CAS read route path (matchit-0.7 / axum-0.7 `:name` captures).
///
/// MUST use the `:name` form — matchit 0.7.3 (the version transitively
/// pinned via `axum = "0.7"`) parses `{name}` as **literal path bytes**
/// rather than a capture, which would silently route every real request
/// to a router-level 404. This was DEBT-029 (closed wave-30 stream-1)
/// on `ac.rs` + `admin.rs`; DEBT-029-cas closes the same surface here.
pub const CAS_READ_ROUTE: &str = "/v1/cas/{tenant}/{hash}";

/// Canonical CAS write route path. The write path reuses the same
/// template; axum disambiguates by HTTP method (GET vs PUT).
pub const CAS_WRITE_ROUTE: &str = "/v1/cas/{tenant}/{hash}";

/// `POST /v1/cas/:tenant/batch` — bulk write (length-framed upload).
///
/// The single-object `PUT /v1/cas/:tenant/:hash` path costs one D1 round-trip
/// per object (auth/pat-gate/quota/storage), which dominates wall-clock on bulk
/// git ingest (thousands of tiny loose objects). This route collapses N objects
/// into ONE request: ONE auth + ONE scope check + ONE pat-gate + ONE quota
/// charge + a batched storage commit.
pub const CAS_BATCH_ROUTE: &str = "/v1/cas/{tenant}/batch";

/// `POST /v1/cas/:tenant/batch-read` — bulk read (length-framed download).
pub const CAS_BATCH_READ_ROUTE: &str = "/v1/cas/{tenant}/batch-read";

/// `POST /v1/cas/:tenant/batch-exists` — bulk HEAD-class existence probe.
pub const CAS_BATCH_EXISTS_ROUTE: &str = "/v1/cas/{tenant}/batch-exists";

/// FROZEN upload content-type for all three batch routes (the manifest+bytes
/// wire format). A request that does not declare it is rejected 415 — the
/// length-framed body is NOT a generic octet-stream and must not be misparsed.
pub const BATCH_CONTENT_TYPE: &str = "application/x-hugit-cas-batch";

/// Additional content-type accepted on the two READ-side batch routes
/// (`batch-read`, `batch-exists`), whose request bodies are plain NDJSON hash
/// lists. The upload route (`batch`) accepts ONLY [`BATCH_CONTENT_TYPE`].
pub const NDJSON_CONTENT_TYPE: &str = "application/x-ndjson";

/// FROZEN per-batch object-count cap (applies to all three routes). Over ⇒ 413.
/// Bounds the per-request fan-out so one request can't enqueue an unbounded
/// number of D1 existence/storage ops behind a single quota charge.
pub const BATCH_MAX_OBJECTS: usize = 2_000;

/// FROZEN per-batch object-bytes cap (applies to `batch` + `batch-read`). Over
/// ⇒ 413. Sits UNDER the container's GLOBAL 10 MiB `DefaultBodyLimit`
/// (main.rs), so the body limit is unchanged: 8 MiB is the batch-payload
/// ceiling, the extra 2 MiB body headroom covers manifest framing; parser
/// strings/clones are reserved separately in the shared capacity envelope.
pub const BATCH_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Explicit route-side mirror of the global axum body limit. The global layer
/// normally rejects this first; keeping the check here makes the contract true
/// in focused routers/tests as well, rather than relying on deployment wiring.
const BATCH_REQUEST_BODY_LIMIT_BYTES: usize =
    crate::container_capacity::CAS_READ_BATCH_BODY_LIMIT_BYTES as usize;

/// Maximum request-line size for either NDJSON batch route. The global 10 MiB
/// body limit bounds total bytes, while this smaller line cap bounds serde's
/// per-line scan and keeps malformed metadata from consuming the parse slice.
const BATCH_MAX_LINE_BYTES: usize = 1024;

/// Maximum retained hash string in a parsed batch entry. Canonical BLAKE3
/// hashes are 64 bytes; allowing a bounded malformed value preserves the
/// per-object `absent`/error response without permitting unbounded clones.
const BATCH_MAX_HASH_BYTES: usize = 128;

/// Conservative retained-metadata bound used by the shared process-wide
/// reservation. It covers the original hash plus the queued task clone and a
/// bounded tenant/response-entry allowance for every admitted object; the
/// active task's short-lived request clones fit in the remaining margin.
const BATCH_METADATA_WORST_CASE_BYTES: u64 = (BATCH_MAX_OBJECTS as u64)
    * ((2 * BATCH_MAX_HASH_BYTES + crate::auth_tenant::MAX_TENANT_ID_BYTES + 256) as u64);

const _: () = assert!(
    BATCH_METADATA_WORST_CASE_BYTES <= crate::container_capacity::CAS_BATCH_PARSE_METADATA_BYTES,
    "batch parser metadata caps exceed the shared reservation"
);

/// FROZEN ceiling on the size of a single CAS object the read path will serve.
///
/// The read path materialises a whole object in memory (twice — the SDK's
/// `Bytes` plus the `Vec<u8>` copy the handler trait's return type forces, and
/// three times on the BYOK path where the plaintext exists while the ciphertext
/// is still held). Peak heap is therefore `concurrent_reads x object_size x
/// live_copy_count`; the process-wide weighted guard charges the worst case.
/// [`CAS_READ_CONCURRENCY_LIMIT`] bounds the first factor; NOTHING bounded the
/// second until the process-wide weighted guard was added. The guard reserves
/// three copies for BYOK before the storage read; the per-tenant guard remains
/// a separate fairness limit.
///
/// It is tempting to think the container's global 10 MiB `DefaultBodyLimit`
/// caps object size transitively. It does not: that limit bounds
/// CLIENT-SUPPLIED request bodies, and the server-side mirror ingest presents
/// no request body at all — `public_mirror::MIRROR_MAX_BLOB_BYTES` accepts a
/// blob up to **1 GiB**. On a 0.25 vCPU / 1024 MiB prod container (measured via
/// the Containers API, 2026-08-26) a single such read is an OOM.
///
/// **Why 64 MiB.** Full enumeration of `corelink-cas-prod` on 2026-08-26 —
/// 22,597 objects, 3.57 GB — put the median at 593 BYTES, 95.1% of objects
/// under 1 MiB and the largest at 52.3 MB. 64 MiB clears the observed maximum,
/// so nothing served today stops being served, and it makes the per-tenant
/// worst case `8 x 64 MiB = 512 MiB` — a number this system chose, instead of
/// the 8 GiB it had inherited from the mirror's fetch cap.
///
/// The process-wide weighted budget below bounds aggregate reads across tenants,
/// using the same deployed-container capacity truth as Argon2id and Turbo.
pub const CAS_READ_MAX_OBJECT_BYTES: u64 = corelink_hash::CACHE_ENTRY_MAX_BYTES as u64;

/// Largest object actually stored, from the full enumeration of
/// `corelink-cas-prod` on 2026-08-26 (22,597 objects, 3.57 GB).
const OBSERVED_MAX_OBJECT_BYTES: u64 = 52_341_477;

/// Process-wide CAS read budget, derived from the shared deployed-container
/// capacity. This is distinct from the per-tenant count guard below: both are
/// needed for tenant isolation and aggregate memory safety.
pub const CAS_READ_GLOBAL_BUDGET_BYTES: u64 =
    crate::container_capacity::CAS_READ_GLOBAL_BUDGET_BYTES;
const CAS_READ_BUDGET_UNIT_BYTES: u64 = crate::container_capacity::MEMORY_BUDGET_UNIT_BYTES;
const CAS_READ_GLOBAL_PERMITS: usize =
    (CAS_READ_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as usize;
const CAS_READ_SINGLE_PERMITS: u32 =
    (crate::container_capacity::CAS_READ_SINGLE_PEAK_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as u32;
const CAS_READ_BATCH_PERMITS: u32 =
    (crate::container_capacity::CAS_READ_BATCH_PEAK_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as u32;
const CAS_WRITE_GLOBAL_BUDGET_BYTES: u64 = crate::container_capacity::CAS_WRITE_GLOBAL_BUDGET_BYTES;
const CAS_WRITE_SINGLE_PERMITS: u32 =
    (crate::container_capacity::CAS_WRITE_SINGLE_PEAK_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as u32;
const CAS_WRITE_BATCH_PERMITS: u32 =
    (crate::container_capacity::CAS_WRITE_BATCH_PEAK_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as u32;
const CAS_WRITE_GLOBAL_PERMITS: usize =
    (CAS_WRITE_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES) as usize;
const CAS_READ_GLOBAL_PERMIT_WAIT: Duration = Duration::from_millis(250);

const _: () = assert!(CAS_READ_GLOBAL_BUDGET_BYTES > 0);
const _: () = assert!(CAS_READ_GLOBAL_BUDGET_BYTES % CAS_READ_BUDGET_UNIT_BYTES == 0);
const _: () = assert!(CAS_READ_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES <= u32::MAX as u64);
const _: () = assert!(CAS_READ_SINGLE_PERMITS > 0 && CAS_READ_BATCH_PERMITS > 0);
const _: () = assert!(CAS_READ_GLOBAL_BUDGET_BYTES <= CONTAINER_MEMORY_BYTES);
const _: () = assert!(
    crate::container_capacity::CAS_READ_SINGLE_PEAK_BYTES
        + crate::container_capacity::CAS_READ_BATCH_PEAK_BYTES
        <= CAS_READ_GLOBAL_BUDGET_BYTES,
    "CAS read copies plus batch request/response envelope exceed the process-wide read slice"
);
const _: () = assert!(
    CAS_WRITE_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES >= CAS_WRITE_SINGLE_PERMITS as u64
        && CAS_WRITE_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES
            >= CAS_WRITE_BATCH_PERMITS as u64
);

static GLOBAL_CAS_READ_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();
static GLOBAL_CAS_WRITE_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn global_cas_read_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_CAS_READ_BUDGET.get_or_init(|| Arc::new(Semaphore::new(CAS_READ_GLOBAL_PERMITS))),
    )
}

async fn acquire_global_cas_read_budget(
    permits: u32,
    route: &'static str,
) -> Result<OwnedSemaphorePermit, axum::response::Response> {
    match tokio::time::timeout(
        CAS_READ_GLOBAL_PERMIT_WAIT,
        global_cas_read_budget().acquire_many_owned(permits),
    )
    .await
    {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => {
            tracing::error!(route, "global CAS read budget closed; failing closed");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global read budget unavailable",
            )
                .into_response())
        }
        Err(_) => {
            tracing::warn!(
                route,
                permits,
                "global CAS read budget saturated; returning 503 before buffering"
            );
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "server busy: too many concurrent reads",
            )
                .into_response())
        }
    }
}

fn global_cas_write_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_CAS_WRITE_BUDGET.get_or_init(|| Arc::new(Semaphore::new(CAS_WRITE_GLOBAL_PERMITS))),
    )
}

async fn acquire_global_cas_write_budget(
    permits: u32,
    route: &'static str,
) -> Result<OwnedSemaphorePermit, axum::response::Response> {
    match tokio::time::timeout(
        CAS_READ_GLOBAL_PERMIT_WAIT,
        global_cas_write_budget().acquire_many_owned(permits),
    )
    .await
    {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => {
            tracing::error!(route, "global CAS write budget closed; failing closed");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global write budget unavailable",
            )
                .into_response())
        }
        Err(_) => {
            tracing::warn!(route, permits, "global CAS write budget saturated");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "server busy: too many concurrent writes",
            )
                .into_response())
        }
    }
}

/// The ceiling MUST stay above the largest object actually stored, or the
/// change stops serving content that is served today — the migration question
/// ADR-S34-002 deliberately left open. A COMPILE-TIME assert, not a test: both
/// sides are constants, so a runtime check folds to `assert!(true)` and proves
/// nothing (clippy says so). This fails the build instead.
const _: () = assert!(
    CAS_READ_MAX_OBJECT_BYTES > OBSERVED_MAX_OBJECT_BYTES,
    "the read ceiling would refuse an object that is served today; \
     lowering it needs a migration story, not just a smaller number"
);

/// A single materialised object can have three live copies (SDK bytes, the
/// handler Vec, and BYOK plaintext); that reservation must fit the process
/// slice. The per-tenant count guard remains a fairness bound, while the
/// process-wide weighted guard limits aggregate peak memory.
const _: () = assert!(
    crate::container_capacity::CAS_READ_COPY_MULTIPLIER * CAS_READ_MAX_OBJECT_BYTES
        <= CAS_READ_GLOBAL_BUDGET_BYTES,
    "one CAS read's peak copies exceed the process-wide read slice"
);

/// Bounded per-request read concurrency for `handle_batch_read`: each in-flight
/// R2 GET holds one permit, so at most this many reads run at once. Keeps a
/// 2000-object batch under the Cloudflare wall-clock deadline (the old fully
/// sequential loop @ ~80 ms/object blew past it → HTTP 500) while staying gentle
/// on R2 and on the blocking pool the sync `read()` bridges through.
pub const BATCH_READ_FANOUT: usize = 16;

/// Shared route state — distinct trait objects for read and write.
#[derive(Clone)]
pub struct CasRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see [`build_handlers`].
    pub read: Arc<dyn CasReadHandler>,
    /// Write handler (separate trait object — see crate-level docs).
    pub write: Arc<dyn CasWriteHandler>,
    /// Delete handler (D-8) — write-capable. In production this points at
    /// the same shared handler instance as `read`/`write`.
    pub delete: Arc<dyn CasDeleteHandler>,
    /// List handler (D-8) — read-capable paginated blob enumeration.
    pub list: Arc<dyn CasListHandler>,
    /// Optional 410-Gone tombstone store (hugit-P2 seam B, WP-B). When present,
    /// `GET` consults it FIRST: an erased `(tenant, hash)` short-circuits to
    /// HTTP 410 Gone (never 404, never 200) before the R2 lookup. `None` (the
    /// default — in-memory / test builds, and prod until PR #254's R2 erase
    /// adapter lands) ⇒ no tombstone gate, classic 200/404 behaviour. Backed by
    /// [`crate::routes::cas_erase::D1TombstoneStore`] in prod.
    pub tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed) — checked at the TOP of each billable
    /// handler, AFTER the scope gate, BEFORE storage. `None` in dev/CI (no D1)
    /// — the ceiling is simply not enforced. See [`crate::tenant_quota`].
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production (PAT_SIGNING_KEY + D1) — re-runs the full
    /// Argon2id Option-B verify at the TOP of each billable handler, AFTER the
    /// scope+tenant gate, BEFORE storage, so a leaked `PAT_SIGNING_KEY` cannot
    /// serve a forged tenant's PAT. `None` in dev/CI (skipped). See
    /// [`crate::native_pat_gate`].
    pub pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    /// Per-tenant in-flight CAS write concurrency counter (cluster F — the native
    /// CAS write path buffers the full body before any gate and had NO per-tenant
    /// concurrency cap, so a single authenticated tenant could open N concurrent
    /// PUTs and consume N × body-limit of heap). Mirrors the Bazel `put_inflight`
    /// guard: a `FromRequestParts` extractor ([`CasPutGuard`]) declared AHEAD of
    /// `body: Bytes` increments this BEFORE the body is buffered and rejects the
    /// over-cap PUT 429; the RAII [`CasPutSlot`] releases on return.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight CAS bulk-READ concurrency counter (brutal-fleet M2 —
    /// MED DoS). The `batch-read` path accumulates up to [`BATCH_MAX_BYTES`] of
    /// object bytes into a per-request payload buffer, and `batch-exists` fans out
    /// up to [`BATCH_MAX_OBJECTS`] storage existence probes — neither held a
    /// per-tenant concurrency cap, so one authenticated tenant could open N
    /// concurrent bulk reads and consume N × payload heap (≈ N × 8 MiB) on the
    /// shared container. A [`CasReadConcurrencyGuard`] `FromRequestParts` extractor
    /// declared AHEAD of `body: Bytes` increments this BEFORE the body is buffered
    /// and rejects the over-cap read 429; the RAII [`CasReadSlot`] releases on
    /// return. This is a SEPARATE pool from [`Self::put_inflight`] — reads and
    /// writes do not contend on one counter (sharing would over-throttle a tenant
    /// that legitimately reads and writes concurrently).
    pub(crate) read_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Display usage aggregator (usage-metering-roi). The read-HIT / read-MISS /
    /// write decision points `record` fire-and-forget into this meter — a cheap
    /// lock+increment, NEVER a DB call / `await` on the hot path. Inert (`record`
    /// is a no-op) in dev/CI; `build_with_factory` sets the ONE process-wide
    /// meter shared across the instrumented cache surfaces. See
    /// [`crate::usage_meter`].
    pub usage_meter: Arc<crate::usage_meter::UsageMeter>,
    // Storage byte accounting (red-team finding #1 / cluster B+C) is NOT a route
    // field: it is enforced INSIDE the `write`/`delete` trait objects above by
    // the [`crate::byte_accounting::AccountingCasHandler`] decorator (wired in
    // `routes::build_with_factory`), so EVERY surface that shares these handlers
    // — native CAS, Bazel, OCI, cargo/brew/npm/pip — gets identical, atomic,
    // reserve→commit→release accounting with no per-route plumbing.
}

impl CasRouteState {
    /// Construct a [`CasRouteState`] from its public collaborators, initializing
    /// the crate-private per-tenant in-flight write counter ([`Self::put_inflight`])
    /// to empty. This is the supported constructor for callers OUTSIDE the crate
    /// (e.g. integration smoke tests) that cannot name the `pub(crate)` field.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        read: Arc<dyn CasReadHandler>,
        write: Arc<dyn CasWriteHandler>,
        delete: Arc<dyn CasDeleteHandler>,
        list: Arc<dyn CasListHandler>,
        tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
        quota: Option<crate::routes::QuotaGate>,
        pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    ) -> Self {
        Self {
            read,
            write,
            delete,
            list,
            tombstones,
            quota,
            pat_gate,
            put_inflight: Arc::new(Mutex::new(HashMap::new())),
            read_inflight: Arc::new(Mutex::new(HashMap::new())),
            // Inert placeholder for external callers; the production wiring in
            // `build_with_factory` sets the shared, D1-backed meter.
            usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }
}

impl core::fmt::Debug for CasRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasRouteState").finish_non_exhaustive()
    }
}

/// Maximum concurrent in-flight native CAS writes for a single tenant. Excess
/// writes are rejected 429 BEFORE the body is buffered. Bounds peak per-tenant
/// write heap; mirrors [`super::bazel_v2::BAZEL_WRITE_CONCURRENCY_LIMIT`].
pub const CAS_WRITE_CONCURRENCY_LIMIT: usize = 8;

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `auth_tenant::AuthTenant`'s sentinel set so the pre-body write guard
/// fails CLOSED on the same non-authenticated values.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant in the
/// pre-body write guard. Does not leak which condition tripped.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// RAII release of one per-tenant in-flight CAS write slot (decrements on every
/// return path — success, error, panic). See [`CasPutGuard`].
pub(crate) struct CasPutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for CasPutSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor reserving a per-tenant in-flight CAS write slot
/// BEFORE the body is buffered (cluster F — pre-buffer OOM guard). Declared ahead
/// of `body: Bytes` in `handle_write` so axum 0.7 runs it first: a tenant already
/// at [`CAS_WRITE_CONCURRENCY_LIMIT`] is rejected 429 with no body read. Mirrors
/// `bazel_v2::BazelPutGuard` exactly.
pub(crate) struct CasPutGuard {
    _slot: CasPutSlot,
}

impl FromRequestParts<CasRouteState> for CasPutGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Reserve per AUTHENTICATED tenant (fail-CLOSED on missing/sentinel —
        // mirrors `AuthTenant`): an unauthenticated request never reserves a slot
        // and never buffers a body.
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty()
            || raw.len() > crate::auth_tenant::MAX_TENANT_ID_BYTES
            || TENANT_SENTINELS.contains(&raw)
        {
            return Err(unauthenticated_tenant());
        }
        let tenant_key = raw.to_owned();
        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(tenant_id = %tenant_key, error = %e, "cas write concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= CAS_WRITE_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = CAS_WRITE_CONCURRENCY_LIMIT, "cas write concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent uploads").into_response(),
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: CasPutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

/// Maximum concurrent in-flight native CAS READs for a single tenant — both the
/// bulk routes (`batch-read` / `batch-exists`) and, since B-052, the single-object
/// `GET /v1/cas/:tenant/:hash`. Excess reads are rejected 429 before any object is
/// buffered. Bounds peak per-tenant read-payload heap (`batch-read` accumulates up
/// to [`BATCH_MAX_BYTES`] per request; a single GET buffers one whole object).
///
/// Single and bulk reads deliberately share ONE pool. Giving the single GET its
/// own counter would let a tenant hold `2 x 8` concurrent reads and defeat the
/// ceiling this constant exists to impose — the pool is the bound, not the route.
/// Still a SEPARATE axis from the write cap [`CAS_WRITE_CONCURRENCY_LIMIT`] so
/// reads and writes do not over-throttle each other; same per-tenant ceiling (8).
pub const CAS_READ_CONCURRENCY_LIMIT: usize = 8;

/// RAII release of one per-tenant in-flight CAS bulk-READ slot (decrements on
/// every return path — success, error, panic). Mirrors [`CasPutSlot`] against the
/// SEPARATE `read_inflight` pool. See [`CasReadConcurrencyGuard`].
pub(crate) struct CasReadSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for CasReadSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor reserving a per-tenant in-flight CAS bulk-READ
/// slot BEFORE the body is buffered (brutal-fleet M2 — pre-buffer DoS guard).
/// Declared ahead of `body: Bytes` in `handle_batch_read` / `handle_batch_exists`
/// so axum 0.7 runs it first: a tenant already at [`CAS_READ_CONCURRENCY_LIMIT`]
/// concurrent bulk reads is rejected 429 with no body read. Mirrors [`CasPutGuard`]
/// exactly but against the SEPARATE [`CasRouteState::read_inflight`] pool — bulk
/// reads and writes are bounded on independent counters (sharing one pool would
/// over-throttle a tenant that legitimately reads and writes at the same time).
pub(crate) struct CasReadConcurrencyGuard {
    _slot: CasReadSlot,
}

impl FromRequestParts<CasRouteState> for CasReadConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Reserve per AUTHENTICATED tenant (fail-CLOSED on missing/sentinel —
        // mirrors `AuthTenant` / `CasPutGuard`): an unauthenticated request never
        // reserves a slot and never buffers a body.
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty()
            || raw.len() > crate::auth_tenant::MAX_TENANT_ID_BYTES
            || TENANT_SENTINELS.contains(&raw)
        {
            return Err(unauthenticated_tenant());
        }
        let tenant_key = raw.to_owned();
        {
            let mut inflight = match state.read_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(tenant_id = %tenant_key, error = %e, "cas read concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= CAS_READ_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = CAS_READ_CONCURRENCY_LIMIT, "cas bulk-read concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent reads").into_response()
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: CasReadSlot {
                inflight: Arc::clone(&state.read_inflight),
                tenant_key,
            },
        })
    }
}

/// Process-wide weighted reservation for one CAS object read. The permit is
/// held until the handler returns, covering the full response-buffer lifetime.
pub(crate) struct GlobalCasReadBudgetGuard {
    _permit: OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasReadBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_read_budget(CAS_READ_SINGLE_PERMITS, "single").await?,
        })
    }
}

/// Process-wide reservation for a `batch-read` response. It is acquired before
/// the request body extractor, so a saturated container rejects before parsing
/// or materialising a batch.
pub(crate) struct GlobalCasBatchReadBudgetGuard {
    _permit: OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasBatchReadBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_read_budget(CAS_READ_BATCH_PERMITS, "batch-read").await?,
        })
    }
}

/// Process-wide reservation for a single CAS write, acquired before the body
/// extractor buffers request bytes.
pub(crate) struct GlobalCasWriteBudgetGuard {
    _permit: OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasWriteBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_write_budget(CAS_WRITE_SINGLE_PERMITS, "single").await?,
        })
    }
}

/// Process-wide reservation for a length-framed CAS batch write.
pub(crate) struct GlobalCasBatchWriteBudgetGuard {
    _permit: OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasBatchWriteBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_write_budget(CAS_WRITE_BATCH_PERMITS, "batch").await?,
        })
    }
}

/// Sentinel carried in [`CasHandlerError::Internal`] by
/// [`UnavailableCasHandler`] so [`map_err`] can map the storage-unavailable
/// condition to **503** (rather than the generic 500 the `Internal` wildcard
/// arm yields). The handler-error enum lives in a sibling crate and is
/// `#[non_exhaustive]` without a dedicated `Unavailable` variant, so we thread
/// the distinction through `Internal` with this exact marker prefix.
const STORAGE_UNAVAILABLE_SENTINEL: &str = "storage-unavailable: ";

/// Fail-CLOSED stand-in handler mounted in place of `InMemoryCasHandler` when
/// storage credentials ARE present but the R2 handler refused to build —
/// today exactly the "R2_TDK_HEX required" case (F1, CAA-360).
///
/// A silent `InMemory` fallback there would serve a NON-durable cache with no
/// alarm (fail-OPEN-ish). Instead every read/write through this handler fails
/// CLOSED + LOUD: it returns [`CasHandlerError::Internal`] carrying the
/// [`STORAGE_UNAVAILABLE_SENTINEL`], which [`map_err`] maps to HTTP 503 — the
/// route is unavailable until `R2_TDK_HEX` is set. Distinct from the
/// creds-ABSENT path (`None` ⇒ dev/CI `InMemory`, which is fine).
#[derive(Debug)]
struct UnavailableCasHandler;

impl CasReadHandler for UnavailableCasHandler {
    fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasWriteHandler for UnavailableCasHandler {
    fn write(&self, _req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasDeleteHandler for UnavailableCasHandler {
    fn delete(&self, _req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasListHandler for UnavailableCasHandler {
    fn list(&self, _req: CasListRequest) -> Result<CasListResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

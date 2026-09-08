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

use crate::container_capacity::CONTAINER_MEMORY_BYTES;

/// Fixed native CAS handler set returned by [`build_handlers`].
pub type CasHandlers = (
    Arc<dyn CasReadHandler>,
    Arc<dyn CasWriteHandler>,
    Arc<dyn CasDeleteHandler>,
    Arc<dyn CasListHandler>,
);

type CasRequestAuth = (
    crate::auth_tenant::AuthTenant,
    crate::scope::CacheScope,
    axum::http::HeaderMap,
);

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
#[non_exhaustive]
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
pub const BATCH_MAX_BYTES: usize =
    crate::container_capacity::CAS_WRITE_BATCH_PAYLOAD_LIMIT_BYTES as usize;

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
const _: () = assert!(
    BATCH_MAX_BYTES as u64 * crate::container_capacity::CAS_READ_COPY_MULTIPLIER
        == crate::container_capacity::CAS_READ_BATCH_OBJECT_PEAK_BYTES
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
/// Weighted reservation for one in-flight batch-read object. It is three
/// copies of the 8 MiB batch object ceiling (SDK bytes, handler `Vec`, and
/// plaintext), or 24 MiB with the deployed capacity declaration.
const CAS_READ_BATCH_OBJECT_PERMITS: u32 =
    (crate::container_capacity::CAS_READ_BATCH_OBJECT_PEAK_BYTES / CAS_READ_BUDGET_UNIT_BYTES)
        as u32;
/// Structured object fanout derived from the process-wide read slice: the
/// 22 MiB batch envelope leaves 198 MiB, which admits eight 24 MiB objects.
pub const BATCH_READ_FANOUT: usize = crate::container_capacity::CAS_READ_BATCH_FANOUT;
/// Maximum number of batch envelopes admitted while reserving each envelope's
/// complete object window. The deployed arithmetic is `220 / (22 + 8*24)`.
const CAS_READ_BATCH_MAX_IN_FLIGHT: usize = crate::container_capacity::CAS_READ_BATCH_MAX_IN_FLIGHT;
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
const _: () = assert!(CAS_READ_BATCH_OBJECT_PERMITS > 0);
const _: () = assert!(BATCH_READ_FANOUT > 1);
const _: () = assert!(CAS_READ_BATCH_MAX_IN_FLIGHT > 0);
const _: () = assert!(
    CAS_READ_BATCH_PERMITS as usize + BATCH_READ_FANOUT * CAS_READ_BATCH_OBJECT_PERMITS as usize
        <= CAS_READ_GLOBAL_PERMITS
);
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

static GLOBAL_CAS_READ_BUDGET: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
/// Batch requests hold their 22 MiB envelope while their bounded object window
/// obtains eight 24 MiB reservations. The derived admission cap is one
/// envelope, keeping the maximum batch peak at 214 MiB within the 220 MiB
/// process-wide slice.
static GLOBAL_CAS_BATCH_READ_ADMISSION: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
static GLOBAL_CAS_WRITE_BUDGET: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

pub(super) fn global_cas_read_budget() -> Arc<tokio::sync::Semaphore> {
    Arc::clone(
        GLOBAL_CAS_READ_BUDGET
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(CAS_READ_GLOBAL_PERMITS))),
    )
}

pub(super) fn global_cas_batch_read_admission() -> Arc<tokio::sync::Semaphore> {
    Arc::clone(
        GLOBAL_CAS_BATCH_READ_ADMISSION
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(CAS_READ_BATCH_MAX_IN_FLIGHT))),
    )
}

async fn acquire_cas_batch_read_admission(
    admission: Arc<tokio::sync::Semaphore>,
) -> Result<tokio::sync::OwnedSemaphorePermit, axum::response::Response> {
    match tokio::time::timeout(
        CAS_READ_GLOBAL_PERMIT_WAIT,
        admission.acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => {
            tracing::error!("global CAS batch-read admission closed; failing closed");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global read budget unavailable",
            )
                .into_response())
        }
        Err(_) => {
            tracing::warn!(
                limit = CAS_READ_BATCH_MAX_IN_FLIGHT,
                "global CAS batch-read admission saturated"
            );
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "server busy: too many concurrent reads",
            )
                .into_response())
        }
    }
}

async fn acquire_cas_read_budget(
    budget: Arc<tokio::sync::Semaphore>,
    permits: u32,
    route: &'static str,
) -> Result<tokio::sync::OwnedSemaphorePermit, axum::response::Response> {
    match tokio::time::timeout(
        CAS_READ_GLOBAL_PERMIT_WAIT,
        budget.acquire_many_owned(permits),
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

fn global_cas_write_budget() -> Arc<tokio::sync::Semaphore> {
    Arc::clone(
        GLOBAL_CAS_WRITE_BUDGET
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(CAS_WRITE_GLOBAL_PERMITS))),
    )
}

async fn acquire_global_cas_write_budget(
    permits: u32,
    route: &'static str,
) -> Result<tokio::sync::OwnedSemaphorePermit, axum::response::Response> {
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

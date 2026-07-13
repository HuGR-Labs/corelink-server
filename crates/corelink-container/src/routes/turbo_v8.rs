//! `GET/PUT /v8/artifacts/:hash` + `POST /v8/artifacts/events` +
//! `POST /v8/artifacts/status` — Vercel Turborepo remote-cache protocol.
//!
//! # Why this module exists
//!
//! Turborepo (by Vercel) supports any HTTP server that implements the
//! Vercel Remote Cache `/v8/artifacts` API. Wiring CoreLink as that server
//! lets teams replace Vercel's paid remote cache with their own CAS-backed
//! store by setting:
//!
//! ```text
//! TURBO_API=https://corelink-api.humangr.com
//! TURBO_TOKEN=<their CoreLink PAT>
//! ```
//!
//! # Backing store
//!
//! The route state is backed by the **durable**, per-tenant R2-backed
//! [`R2KvStore`](crate::storage::r2_kv::R2KvStore) whenever storage
//! credentials are configured (`StorageEnv::from_env()`), so artifacts
//! persist across container restarts. Only the no-creds dev / CI path
//! falls back to the in-RAM [`InMemoryKvStore`]; if creds ARE present but
//! `R2KvStore` refuses to build, the handler fails CLOSED (503s every verb)
//! rather than silently losing durability. The backing store is a swappable
//! port trait ([`CasReadStore`] / [`CasWriteStore`]); the route handlers and
//! audit surface are identical across both backings. See `build_handlers`.
//!
//! # Tenant isolation
//!
//! The isolation tenant is the DO-injected, PAT-resolved authenticated tenant
//! (`AuthTenant` / `x-corelink-tenant-id`), threaded in as `caller_tenant`. The
//! `teamId` query parameter is required on PUT and GET (missing `teamId` → 400)
//! but is NOT a security boundary: it is a Turborepo team label demoted to a
//! logical sub-namespace WITHIN the authenticated tenant. Storage is keyed
//! `tenant = auth.0`, `key = "<teamId>/<hash>"`, so teams under one tenant stay
//! partitioned while cross-tenant access is impossible (no authenticated tenant
//! ⇒ 401 fail-CLOSED at the extractor).
//!
//! # Hash semantics
//!
//! Turbo's artifact hash is OPAQUE. It is stored verbatim as the KV key without
//! any hash-integrity verification. Hashes longer than
//! [`corelink_turbo_bridge::MAX_HASH_LEN`] (128 chars) are rejected with 400
//! as a DoS guard — see [`corelink_turbo_bridge::TurboBridgeError::HashTooLong`].
//!
//! # Route constants
//!
//! MUST use the matchit-0.7 `:name` capture syntax.  `{name}` form is silently
//! treated as a literal path segment in matchit 0.7.3 — see DEBT-029 comment in
//! `routes/cas.rs` for the full rationale.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};

use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_turbo_bridge::{
    adapter::{CasAdapterTurboHandler, InMemoryKvStore},
    audit::InMemoryTurboAuditSink,
    error::validate_team_id,
    TurboArtifactHandler, TurboBridgeError, TurboEventsRequest, TurboEventsResponse,
    TurboGetRequest, TurboGetResponse, TurboPutRequest, TurboPutResponse, TurboStatusRequest,
    TurboStatusResponse,
};

// ── Route constants ───────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash` — retrieve a Turborepo build artifact.
///
/// MUST use matchit-0.7 `:name` syntax (not `{name}`).
pub const TURBO_GET_ROUTE: &str = "/v8/artifacts/{hash}";

/// `PUT /v8/artifacts/:hash` — store a Turborepo build artifact.
///
/// Same template as GET; axum disambiguates by HTTP method.
pub const TURBO_PUT_ROUTE: &str = "/v8/artifacts/{hash}";

/// `POST /v8/artifacts/events` — accept-and-drop Turborepo telemetry.
pub const TURBO_EVENTS_ROUTE: &str = "/v8/artifacts/events";

/// `POST /v8/artifacts/status` — static remote-cache enabled check.
pub const TURBO_STATUS_ROUTE: &str = "/v8/artifacts/status";

/// Per-route request-body cap for the `/v8/artifacts/*` routes (100 MiB).
///
/// Turbo build artifacts are legitimately larger than the 10 MiB global body
/// limit applied in `main.rs`, so the Turbo router layers this larger
/// `DefaultBodyLimit` (which axum honours as the innermost limit) — while still
/// bounding the body so an authenticated PAT cannot OOM the shared container.
pub const TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024;

/// Maximum concurrent in-flight `PUT /v8/artifacts/:hash` requests for a
/// single tenant. Excess PUTs are rejected with 429 (Too Many Requests).
///
/// Rationale: each Turbo PUT buffers up to `TURBO_BODY_LIMIT_BYTES` (100 MiB)
/// in memory. Without a concurrency cap, a single authenticated tenant can open
/// N concurrent PUTs and consume N × 100 MiB of heap, OOM-ing their container.
/// Capping at 4 bounds peak per-tenant working set to ~400 MiB while still
/// allowing realistic parallel builds (Turborepo's default parallelism is 2–4
/// concurrent tasks). Excess requests get 429, not 503 — the client retries.
pub const TURBO_PUT_CONCURRENCY_LIMIT: usize = 4;

/// Maximum concurrent in-flight `GET /v8/artifacts/:hash` requests for a single
/// tenant. Excess GETs are rejected with 429 (Too Many Requests).
///
/// Rationale (H1 — read-path OOM, mirrors [`TURBO_PUT_CONCURRENCY_LIMIT`]): a
/// successful GET buffers the FULL artifact (up to [`TURBO_BODY_LIMIT_BYTES`] =
/// 100 MiB) into the heap before responding. Without a concurrency cap, an
/// authenticated tenant who has uploaded a 100 MiB artifact can burst N
/// concurrent GETs of it and consume N × 100 MiB of heap, OOM-ing the shared
/// container — the read path was left asymmetrically open while the PUT path was
/// hardened. Capping at 4 bounds peak per-tenant read working set to ~400 MiB
/// while still allowing realistic parallel cache reads. Excess requests get 429,
/// not 503 — the client retries.
pub const TURBO_GET_CONCURRENCY_LIMIT: usize = 4;

/// Per-route request-body cap for `POST /v8/artifacts/events` (64 KiB).
///
/// C4: the events route is "accept-and-drop" telemetry — it has NO storage,
/// no `PutConcurrencyGuard`, and no legitimate reason to buffer a large body.
/// Without its own cap it would inherit the router-wide
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB), letting an authenticated tenant OOM
/// the shared container by POSTing 100 MiB "telemetry" bodies. This small
/// route-specific `DefaultBodyLimit` makes axum reject (413) oversized
/// telemetry BEFORE buffering. Real Turbo `/events` payloads are a few KiB of
/// JSON, so 64 KiB is generous headroom.
pub const EVENTS_BODY_LIMIT_BYTES: usize = 64 * 1024;

/// Process-wide cap on concurrent large Turbo PUTs (C5).
///
/// [`PutConcurrencyGuard`] is PER-TENANT (4 × 100 MiB each). With N distinct
/// authenticated tenants, peak transient heap is `N × 4 × 100 MiB` — N tenants
/// can together OOM the shared container even though each is within its own
/// per-tenant cap. This is a SECOND, PROCESS-WIDE budget, reserved (like the
/// per-tenant guard) in a `FromRequestParts` extractor — which axum 0.7 runs
/// BEFORE the `body: Bytes` body extractor — so a permit is taken (or the
/// request 503s) BEFORE a single body byte is buffered.
///
/// Sizing mirrors the `adapter_pat::ARGON2_VERIFY_PERMITS` rationale: on a
/// standard-1 instance (~4 GiB) each in-flight PUT buffers up to
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB). floor(4096 MiB / 100 MiB) ≈ 40, but
/// the body buffer is not the only allocation (runtime, R2 client buffers,
/// the `body.to_vec()` clone in `handle_put` transiently DOUBLES the body),
/// so we apply a ~2.5× safety headroom and pick **16** as the conservative
/// process-wide floor — 16 × 100 MiB ≈ 1.6 GiB peak PUT working set, leaving
/// ample room for the rest of the process. Beyond 16 concurrent PUTs the
/// extractor fails CLOSED (503) after a short acquire wait rather than
/// blocking forever.
pub const GLOBAL_TURBO_PUT_PERMITS: usize = 16;

/// Process-wide cap on concurrent large Turbo GETs (H1 — read-path twin of
/// [`GLOBAL_TURBO_PUT_PERMITS`]).
///
/// [`GetConcurrencyGuard`] is PER-TENANT (4 × 100 MiB each). With N distinct
/// authenticated tenants, peak transient read heap is `N × 4 × 100 MiB` — N
/// tenants can together OOM the shared container even though each is within its
/// own per-tenant cap. This is a SECOND, PROCESS-WIDE budget, reserved (like the
/// per-tenant guard) in a `FromRequestParts` extractor — which axum 0.7 runs
/// BEFORE the response body is buffered — so a permit is taken (or the request
/// 503s) BEFORE the artifact is read into the heap.
///
/// Sizing mirrors [`GLOBAL_TURBO_PUT_PERMITS`]: each in-flight GET buffers up to
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB). 16 × 100 MiB ≈ 1.6 GiB peak read
/// working set on a standard-1 instance (~4 GiB), leaving ample room for the
/// rest of the process. Reads get a SEPARATE pool from writes: a GET flood must
/// not be able to consume PUT permits (and vice-versa), and the combined
/// PUT+GET ceiling (16 + 16 = 32 × 100 MiB ≈ 3.2 GiB) still fits with headroom.
/// Beyond 16 concurrent GETs the extractor fails CLOSED (503) after a short
/// acquire wait rather than blocking forever.
pub const GLOBAL_TURBO_GET_PERMITS: usize = 16;

/// Process-wide concurrency budget for the accept-and-drop `/v8/artifacts/events`
/// telemetry route — SEPARATE from the PUT write budget (rt-nuclear cycle-2
/// #4/#8). Events previously shared `GLOBAL_TURBO_PUT_BUDGET` (the C5 OOM guard),
/// but that let a telemetry flood / slow-body events POST hold PUT permits and
/// starve real cache writes (cross-plane DoS). A dedicated budget keeps the C5
/// OOM bound (≤ this many × `EVENTS_BODY_LIMIT_BYTES` ≈ 16 × 64 KiB = 1 MiB
/// aggregate events heap) WITHOUT letting worthless telemetry starve billable
/// writes. 16 is ample for telemetry; events are tiny + accept-and-drop.
pub const GLOBAL_TURBO_EVENTS_PERMITS: usize = 16;

/// Maximum concurrent in-flight `POST /v8/artifacts/events` requests for a
/// single tenant (rt-nuclear cycle-2 #9). Excess events POSTs are rejected with
/// 429 (Too Many Requests).
///
/// The PUT path already has a per-tenant cap ([`TURBO_PUT_CONCURRENCY_LIMIT`]),
/// but `/events` had only the PROCESS-WIDE [`GLOBAL_TURBO_EVENTS_PERMITS`]
/// budget — so one tenant could open all 16 events permits and monopolise the
/// telemetry pool, starving every OTHER tenant's `/events` (an intra-plane,
/// per-tenant fairness gap). This per-tenant cap mirrors the PUT guard so no
/// single tenant can hog more than its share of the events pool. Telemetry is
/// tiny + accept-and-drop, so 4 concurrent is ample headroom for one tenant.
pub const EVENTS_CONCURRENCY_LIMIT: usize = 4;

/// Maximum wall-time a `POST /v8/artifacts/events` body may take to stream in
/// before the handler aborts it (rt-nuclear cycle-2 #8 — slow-body slowloris).
///
/// `/events` holds one [`GLOBAL_TURBO_EVENTS_PERMITS`] permit (and now one
/// per-tenant [`EVENTS_CONCURRENCY_LIMIT`] slot) for the WHOLE request — from
/// permit-acquisition through body read. Without a server-side read deadline a
/// client can dribble its (≤ 64 KiB) body one byte at a time and pin a permit +
/// a slot indefinitely; `EVENTS_CONCURRENCY_LIMIT` such clients per tenant, or
/// `GLOBAL_TURBO_EVENTS_PERMITS` across tenants, slowloris the events pool to a
/// standstill. The handler reads the body under this timeout; a stalled body is
/// aborted with 408 (Request Timeout), which returns the handler and RAII-frees
/// the permit + slot. A real Turbo `/events` payload is a few KiB of JSON over a
/// healthy connection, so a few seconds is generous for any legitimate client.
const EVENTS_BODY_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the global-budget extractor waits for a permit before declaring
/// the container globally saturated and returning 503. Short by design (same
/// rationale as `adapter_pat::ARGON2_PERMIT_WAIT`): a caller waiting longer is
/// better served a fast fail than a stalled request holding a connection
/// hostage under a flood.
const GLOBAL_PUT_PERMIT_WAIT: Duration = Duration::from_millis(250);

/// Number of fixed per-object write locks (C1). A power of two so the shard
/// index is a cheap mask. 1024 `tokio::sync::Mutex<()>` (each is a few words)
/// is a constant, memory-BOUNDED footprint — there is no per-key map that can
/// grow or need eviction. Same-object PUTs hash to the same shard and
/// serialize; the false-sharing rate of distinct objects colliding on a shard
/// is ~1/1024, which only ever costs a little extra serialization, never
/// correctness.
const TURBO_WRITE_LOCK_SHARDS: usize = 1024;

// ── Query parameters ──────────────────────────────────────────────────────────

/// Query parameters for `GET /v8/artifacts/:hash` and
/// `PUT /v8/artifacts/:hash`.
///
/// `teamId` is **required**; missing value → 400 (axum rejects the
/// extraction before the handler is called).  `slug` is optional and
/// informational.
#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    /// Vercel team identifier — a logical sub-namespace WITHIN the authenticated
    /// tenant, NOT the isolation tenant. Forms the storage key prefix
    /// (`"<teamId>/<hash>"`); the isolation tenant is `AuthTenant`.
    #[serde(rename = "teamId")]
    pub team_id: String,
    /// Informational slug (repo name, etc.) — logged but not partitioned.
    #[serde(default)]
    pub slug: String,
}

// ── Response DTOs ─────────────────────────────────────────────────────────────

/// Serialised body for `PUT /v8/artifacts/:hash` success response.
///
/// Vercel spec: `{"urls": ["..."]}`
#[derive(Debug, Serialize)]
pub struct PutArtifactResponse {
    /// Storage URL(s) for the stored artifact.  Turbo treats 200 + this body
    /// as "uploaded successfully"; the URL is informational.
    pub urls: Vec<String>,
}

// ── Route state ───────────────────────────────────────────────────────────────

/// Shared state for `/v8/artifacts/*` routes.
///
/// Holds a single `Arc<dyn TurboArtifactHandler>` so the backing store is
/// swappable behind the port trait without changing the route layer.  See
/// [`build_handlers`] for Phase 0 wiring using `InMemoryKvStore`.
#[derive(Clone)]
pub struct TurboRouteState {
    /// Handler implementing all four Turbo API verbs.
    pub handler: Arc<dyn TurboArtifactHandler>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed); checked at the TOP of the billable
    /// artifact GET/PUT handlers, AFTER the scope gate, BEFORE storage. `None`
    /// in dev/CI (not enforced). The telemetry `events`/static `status` verbs
    /// are NOT billable and are not gated. Set by `routes::build_with_factory`.
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production; re-runs the full Argon2id Option-B verify
    /// at the TOP of the billable artifact handlers, AFTER scope, BEFORE storage.
    /// `None` in dev/CI (skipped). See [`crate::native_pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// Optional per-tenant storage byte accountant (red-team finding #1).
    /// `Some` in production — Turbo writes to R2 directly (via the `R2KvStore`
    /// handler), so a successful PUT `accrue`s its body bytes (over-cap ⇒ 402 /
    /// fault ⇒ 503). `None` in dev/CI. See [`crate::byte_accounting`].
    pub bytes: Option<Arc<crate::byte_accounting::ByteAccountant>>,
    /// Per-tenant in-flight PUT concurrency counter (finding #2 — pre-buffer
    /// self-DoS guard).
    ///
    /// Maps `tenant_id → count` of PUT requests currently in-flight (body
    /// buffered in memory). The reservation is taken by the
    /// [`PutConcurrencyGuard`] `FromRequestParts` extractor — which axum runs
    /// BEFORE the `body: Bytes` extractor — so a 6th concurrent PUT is rejected
    /// 429 BEFORE its (up to 100 MiB) body is read into the heap. The RAII guard
    /// the extractor yields releases the slot when the handler returns.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight GET concurrency counter (H1 — pre-buffer read-path
    /// self-DoS guard, the read twin of [`put_inflight`](Self::put_inflight)).
    ///
    /// Maps `tenant_id → count` of GET requests currently in-flight (artifact
    /// buffered in memory before the response is written). The reservation is
    /// taken by the [`GetConcurrencyGuard`] `FromRequestParts` extractor — which
    /// axum runs BEFORE the handler buffers the response body — so a 5th
    /// concurrent GET is rejected 429 BEFORE its (up to 100 MiB) artifact is read
    /// into the heap. The RAII guard the extractor yields releases the slot when
    /// the handler returns.
    pub(crate) get_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight `/events` concurrency counter (rt-nuclear cycle-2
    /// #9 — events fairness).
    ///
    /// Maps `tenant_id → count` of `POST /v8/artifacts/events` requests
    /// currently in-flight. The PUT plane already has a per-tenant cap
    /// ([`put_inflight`](Self::put_inflight)); `/events` had only the
    /// process-wide [`GLOBAL_TURBO_EVENTS_PERMITS`] budget, so one tenant could
    /// take every events permit and starve all other tenants' telemetry. The
    /// reservation is taken by the [`EventsConcurrencyGuard`] `FromRequestParts`
    /// extractor (which axum runs BEFORE the body is read), capped at
    /// [`EVENTS_CONCURRENCY_LIMIT`] per tenant (429 over it). The RAII
    /// [`EventsSlot`] the extractor yields releases the slot on every return
    /// path (success, error, panic, body-read timeout).
    pub(crate) events_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Fixed array of per-object async write locks (C1 — same-key write
    /// serialization).
    ///
    /// `handle_put` accrues the full new body, then `R2KvStore::write` does a
    /// NON-serialized GET-presence probe to capture `prior_len`, then the route
    /// `release`s that `prior_len`. Two concurrent PUTs to the SAME stored
    /// object both probe the same prior size `L` and both release `L` —
    /// double-releasing `bytes_used` and underflowing the tenant's accounting
    /// while real storage is unchanged (re-grow + repeat ⇒ unbounded free
    /// storage). The fix serializes the whole probe→put→release sequence per
    /// stored object. The lock IDENTITY is `(caller_tenant, team_id, hash)` —
    /// exactly the tuple that maps to ONE R2 object (`tenant = caller_tenant`,
    /// storage key = `"<team_id>/<hash>"` per `corelink_turbo_bridge::adapter`,
    /// HMAC-prefixed per tenant in `R2KvStore::object_key`). We index a FIXED
    /// `TURBO_WRITE_LOCK_SHARDS`-wide array by a stable hash of that tuple, so
    /// the footprint is constant (no growing/evicting map) yet same-object
    /// writes always serialize. Distinct objects rarely collide on a shard
    /// (~1/1024) and a collision only adds a little serialization, never a
    /// correctness defect.
    pub(crate) write_locks: Arc<Vec<AsyncMutex<()>>>,
    /// Display usage aggregator (usage-metering-roi). The Turbo artifact GET-HIT /
    /// GET-MISS / PUT decision points `record` fire-and-forget into this meter — a
    /// cheap lock+increment, NEVER a DB call / `await` on the hot path. Inert in
    /// dev/CI; `build_with_factory` sets the ONE shared, D1-backed meter.
    pub usage_meter: Arc<crate::usage_meter::UsageMeter>,
}

/// Process-wide Turbo-PUT byte/concurrency budget (C5). A single
/// [`Semaphore`] shared by EVERY [`TurboRouteState`] (and thus every tenant)
/// in the process, sized to [`GLOBAL_TURBO_PUT_PERMITS`]. Lazily initialised
/// so it is a true singleton regardless of how many routers are built (prod
/// builds one; tests build many).
static GLOBAL_TURBO_PUT_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide PUT budget (C5).
fn global_turbo_put_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_PUT_BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_PUT_PERMITS))),
    )
}

/// Process-wide Turbo-GET read budget (H1) — the read-path twin of
/// [`GLOBAL_TURBO_PUT_BUDGET`]. A single [`Semaphore`] shared by EVERY
/// [`TurboRouteState`] (and thus every tenant) in the process, sized to
/// [`GLOBAL_TURBO_GET_PERMITS`]. SEPARATE from the PUT budget so reads and writes
/// don't share one pool. Lazily initialised so it is a true singleton regardless
/// of how many routers are built.
static GLOBAL_TURBO_GET_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide GET budget (H1).
fn global_turbo_get_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_GET_BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_GET_PERMITS))),
    )
}

/// Process-wide `/events` telemetry budget — SEPARATE from the PUT budget so a
/// telemetry flood cannot starve real cache writes (rt-nuclear cycle-2 #4/#8).
static GLOBAL_TURBO_EVENTS_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide `/events` budget.
fn global_turbo_events_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_EVENTS_BUDGET
            .get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_EVENTS_PERMITS))),
    )
}

/// Build a fresh fixed-size array of per-object write locks (C1).
fn new_write_locks() -> Arc<Vec<AsyncMutex<()>>> {
    let mut v = Vec::with_capacity(TURBO_WRITE_LOCK_SHARDS);
    for _ in 0..TURBO_WRITE_LOCK_SHARDS {
        v.push(AsyncMutex::new(()));
    }
    Arc::new(v)
}

/// Stable shard index for an object's write lock (C1). Hashes the lock
/// identity tuple `(caller_tenant, team_id, hash)` — the tuple that maps to a
/// single stored R2 object — into `[0, TURBO_WRITE_LOCK_SHARDS)`.
fn write_lock_shard(tenant: &str, team_id: &str, hash: &str) -> usize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tenant.hash(&mut h);
    0u8.hash(&mut h); // domain separator so ("a","b") ≠ ("ab","")
    team_id.hash(&mut h);
    0u8.hash(&mut h);
    hash.hash(&mut h);
    // TURBO_WRITE_LOCK_SHARDS is a power of two ⇒ the mask is exact. Mask in
    // u64 first; the result is `< TURBO_WRITE_LOCK_SHARDS` (1024) so it always
    // fits a usize on every target without a truncating cast.
    let mask = (TURBO_WRITE_LOCK_SHARDS as u64) - 1;
    usize::try_from(h.finish() & mask).unwrap_or(0)
}

impl core::fmt::Debug for TurboRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TurboRouteState")
            .field("put_inflight", &"Arc<Mutex<HashMap<..>>>")
            .finish_non_exhaustive()
    }
}

// ── Pre-buffer PUT concurrency guard (finding #2) ──────────────────────────────

/// RAII release of one per-tenant in-flight PUT slot.
///
/// Decrements the tenant's `put_inflight` count on `Drop`, so the slot is freed
/// on EVERY return path (success, handler error, panic). Carried out of the
/// [`PutConcurrencyGuard`] extractor into the handler so the slot stays held for
/// the whole request lifetime (including the body buffer + storage write).
pub(crate) struct PutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for PutSlot {
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

/// `FromRequestParts` extractor that reserves a per-tenant in-flight PUT slot.
///
/// # Why an extractor (finding #2 — pre-buffer OOM guard)
///
/// The slot reservation USED to live inside `handle_put`, AFTER the
/// `body: axum::body::Bytes` extractor had already buffered up to
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB) into the heap. So a burst of concurrent
/// PUTs each buffered ~100 MiB BEFORE the cap-check ran — `burst × 100 MiB` of
/// transient heap could OOM the shared container regardless of the cap.
///
/// As a [`FromRequestParts`] extractor this runs while only request **parts**
/// (headers/method/uri) are available — axum 0.7 runs every `FromRequestParts`
/// extractor BEFORE the single `FromRequest` body extractor (`Bytes`). Declaring
/// it AHEAD of `body: Bytes` in the handler signature therefore does the
/// lock+count+increment BEFORE a single body byte is read: an over-cap request
/// is rejected 429 with no buffering. The yielded [`PutSlot`] RAII-releases the
/// slot when the handler returns.
pub(crate) struct PutConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: PutSlot,
}

impl axum::extract::FromRequestParts<TurboRouteState> for PutConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The isolation tenant is the DO-injected, PAT-resolved authenticated
        // tenant. A missing/empty header fails CLOSED (401) — the same gate the
        // `AuthTenant` extractor enforces; we mirror it here so the reservation
        // is per AUTHENTICATED tenant (an unauthenticated request never reserves
        // a slot, and never buffers a body).
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        // Sentinels the Worker/DO use for non-tenant traffic — never a real
        // tenant (mirrors `auth_tenant::AuthTenant`'s set).
        const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];
        if tenant.is_empty() || TENANT_SENTINELS.contains(&tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo PUT concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= TURBO_PUT_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = TURBO_PUT_CONCURRENCY_LIMIT,
                    "turbo PUT concurrency limit reached; returning 429 BEFORE body buffering"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent artifact uploads",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: PutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

// ── Global process-wide PUT budget guard (C5) ──────────────────────────────────

/// `FromRequestParts` extractor that reserves ONE permit from the
/// process-wide [`GLOBAL_TURBO_PUT_BUDGET`] semaphore (C5).
///
/// # Why an extractor (same reason as [`PutConcurrencyGuard`])
///
/// The per-tenant [`PutConcurrencyGuard`] bounds ONE tenant to
/// `TURBO_PUT_CONCURRENCY_LIMIT × TURBO_BODY_LIMIT_BYTES`, but with N tenants
/// the aggregate transient heap is `N × that` — N tenants can together OOM the
/// shared container. This extractor adds a SECOND, process-wide bound. As a
/// `FromRequestParts` extractor axum runs it BEFORE the `body: Bytes`
/// extractor, so the permit is reserved (or the request 503s) BEFORE any body
/// byte is buffered. The held [`OwnedSemaphorePermit`] RAII-releases the
/// permit when the handler returns (success, error, or panic).
///
/// Under global saturation we wait at most [`GLOBAL_PUT_PERMIT_WAIT`] for a
/// permit, then fail CLOSED with 503 (Service Unavailable) rather than block
/// the request forever — the client retries.
pub(crate) struct GlobalPutBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GlobalPutBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_put_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — we never
            // close it, so this is unreachable, but fail CLOSED if it happens.
            Ok(Err(_)) => Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global upload budget unavailable",
            )
                .into_response()),
            // Timed out waiting: the container is globally saturated.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_PUT_PERMITS,
                    "turbo PUT global budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent uploads",
                )
                    .into_response())
            }
        }
    }
}

// ── Pre-buffer GET concurrency guard (H1 — read-path twin of PUT) ──────────────

/// RAII release of one per-tenant in-flight GET slot.
///
/// Decrements the tenant's `get_inflight` count on `Drop`, so the slot is freed
/// on EVERY return path (success, handler error, panic). Carried out of the
/// [`GetConcurrencyGuard`] extractor into the handler so the slot stays held for
/// the whole request lifetime (including the response-body buffer). Mirrors
/// [`PutSlot`].
pub(crate) struct GetSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for GetSlot {
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

/// `FromRequestParts` extractor that reserves a per-tenant in-flight GET slot
/// (H1 — pre-buffer read-path OOM guard).
///
/// # Why an extractor (mirrors [`PutConcurrencyGuard`])
///
/// `handle_get` buffers the FULL artifact (up to [`TURBO_BODY_LIMIT_BYTES`] =
/// 100 MiB) into the heap before responding. Without a cap, a burst of
/// concurrent GETs each buffers ~100 MiB — `burst × 100 MiB` of transient heap
/// could OOM the shared container. The PUT path was hardened against exactly
/// this; the GET path was left asymmetrically open.
///
/// As a [`FromRequestParts`] extractor this runs while only request **parts**
/// (headers/method/uri) are available — BEFORE the handler ever touches storage
/// or buffers a response body. Declaring it AHEAD of any body work in the handler
/// signature therefore does the lock+count+increment BEFORE a single artifact
/// byte is read: an over-cap request is rejected 429 with no buffering. The
/// yielded [`GetSlot`] RAII-releases the slot when the handler returns. Like the
/// PUT guard it fails CLOSED (401) on a missing/sentinel tenant.
pub(crate) struct GetConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: GetSlot,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GetConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The isolation tenant is the DO-injected, PAT-resolved authenticated
        // tenant. A missing/empty/sentinel header fails CLOSED (401) — the same
        // gate `AuthTenant` and `PutConcurrencyGuard` enforce; we mirror it so the
        // reservation is per AUTHENTICATED tenant (an unauthenticated request
        // never reserves a slot, and never buffers an artifact).
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];
        if tenant.is_empty() || TENANT_SENTINELS.contains(&tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.get_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo GET concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= TURBO_GET_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = TURBO_GET_CONCURRENCY_LIMIT,
                    "turbo GET concurrency limit reached; returning 429 BEFORE body buffering"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent artifact downloads",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: GetSlot {
                inflight: Arc::clone(&state.get_inflight),
                tenant_key,
            },
        })
    }
}

// ── Global process-wide GET budget guard (H1) ──────────────────────────────────

/// `FromRequestParts` extractor that reserves ONE permit from the process-wide
/// [`GLOBAL_TURBO_GET_BUDGET`] semaphore (H1 — read-path twin of
/// [`GlobalPutBudgetGuard`]).
///
/// The per-tenant [`GetConcurrencyGuard`] bounds ONE tenant to
/// `TURBO_GET_CONCURRENCY_LIMIT × TURBO_BODY_LIMIT_BYTES`, but with N tenants the
/// aggregate transient read heap is `N × that` — N tenants can together OOM the
/// shared container. This extractor adds a SECOND, process-wide bound on a pool
/// SEPARATE from writes. As a `FromRequestParts` extractor axum runs it BEFORE
/// the handler buffers the response, so the permit is reserved (or the request
/// 503s) BEFORE any artifact byte is buffered. The held [`OwnedSemaphorePermit`]
/// RAII-releases when the handler returns.
///
/// Under global saturation we wait at most [`GLOBAL_PUT_PERMIT_WAIT`] for a
/// permit, then fail CLOSED with 503 rather than block forever — the client
/// retries.
pub(crate) struct GlobalGetBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GlobalGetBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_get_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — we never
            // close it, so this is unreachable, but fail CLOSED if it happens.
            Ok(Err(_)) => Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global download budget unavailable",
            )
                .into_response()),
            // Timed out waiting: the container is globally saturated.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_GET_PERMITS,
                    "turbo GET global budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent downloads",
                )
                    .into_response())
            }
        }
    }
}

/// Per-request guard over the SEPARATE `/events` telemetry budget
/// ([`GLOBAL_TURBO_EVENTS_PERMITS`]) — decoupled from the PUT write budget so a
/// telemetry flood / slow-body events POST cannot starve real cache writes
/// (rt-nuclear cycle-2 #4/#8). As a `FromRequestParts` extractor it acquires
/// (and 503s on saturation) BEFORE the `Bytes` body is buffered — preserving the
/// C5 OOM bound on events (≤ permits × `EVENTS_BODY_LIMIT_BYTES`) on its own pool.
pub(crate) struct EventsBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for EventsBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_events_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — never closed
            // here, but fail CLOSED if it ever is.
            Ok(Err(_)) => {
                Err((StatusCode::SERVICE_UNAVAILABLE, "events budget unavailable").into_response())
            }
            // Timed out: the events pool is saturated. A telemetry flood now 503s
            // ITS OWN route without touching the PUT write plane.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_EVENTS_PERMITS,
                    "turbo /events budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent telemetry posts",
                )
                    .into_response())
            }
        }
    }
}

// ── Per-tenant /events concurrency guard (rt-nuclear cycle-2 #9) ───────────────

/// RAII release of one per-tenant in-flight `/events` slot.
///
/// Decrements the tenant's `events_inflight` count on `Drop`, so the slot is
/// freed on EVERY return path (success, handler error, panic, body-read
/// timeout). Carried out of the [`EventsConcurrencyGuard`] extractor into the
/// handler so the slot stays held for the whole request lifetime (including the
/// timed body read). Mirrors [`PutSlot`].
pub(crate) struct EventsSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for EventsSlot {
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

/// `FromRequestParts` extractor that reserves a per-tenant in-flight `/events`
/// slot (rt-nuclear cycle-2 #9 — events fairness).
///
/// The process-wide [`EventsBudgetGuard`] bounds the events pool in aggregate,
/// but with no PER-TENANT cap one tenant could take all
/// [`GLOBAL_TURBO_EVENTS_PERMITS`] permits and starve every other tenant's
/// telemetry. This mirrors the PUT plane's [`PutConcurrencyGuard`]: as a
/// `FromRequestParts` extractor it runs BEFORE the body is read, caps the tenant
/// at [`EVENTS_CONCURRENCY_LIMIT`] in-flight (429 over it), and yields an
/// [`EventsSlot`] that RAII-releases the slot when the handler returns. Like the
/// PUT guard it fails CLOSED (401) on a missing/sentinel tenant.
pub(crate) struct EventsConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: EventsSlot,
}

impl axum::extract::FromRequestParts<TurboRouteState> for EventsConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Per AUTHENTICATED tenant — same fail-CLOSED tenant resolution as the
        // PUT guard (`PutConcurrencyGuard`) and `AuthTenant`: a missing/empty or
        // sentinel tenant 401s and never reserves a slot.
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];
        if tenant.is_empty() || TENANT_SENTINELS.contains(&tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.events_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo /events concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= EVENTS_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = EVENTS_CONCURRENCY_LIMIT,
                    "turbo /events per-tenant concurrency limit reached; returning 429"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent telemetry posts for this tenant",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: EventsSlot {
                inflight: Arc::clone(&state.events_inflight),
                tenant_key,
            },
        })
    }
}

// ── Handler construction ──────────────────────────────────────────────────────

/// Build the [`TurboRouteState`] for the current build target and runtime.
///
/// # Runtime backing-store selection (v2)
///
/// When storage credentials are configured (`StorageEnv::from_env()`), the
/// handler is backed by the **durable** [`R2KvStore`](crate::storage::r2_kv::R2KvStore)
/// (closes the former `TODO(v2)`: artifacts now persist across container
/// restarts). Otherwise — dev / CI without secrets — it falls back to the
/// in-RAM [`InMemoryKvStore`]. Mirrors the `cas`/`ac` `build_handlers`
/// fallback convention; the route handlers and audit/validation logic are
/// identical for both backings (opaque-key semantics, no hash verify).
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn build_handlers() -> TurboRouteState {
    let audit = Arc::new(InMemoryTurboAuditSink::new());

    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_kv, StorageEnv};
        if StorageEnv::from_env().is_some() {
            // `block_in_place` rationale: build_handlers runs inside the
            // `#[tokio::main]` multi-thread runtime; a bare `block_on`
            // from a running future hangs. Mirrors `cas`/`ac` handlers.
            let handle = tokio::runtime::Handle::current();
            let built =
                tokio::task::block_in_place(|| handle.block_on(r2_kv::build_r2_kv_from_env()));
            match built {
                Some(Ok(store)) => {
                    let store = Arc::new(store);
                    let read: Arc<dyn corelink_turbo_bridge::adapter::CasReadStore> = store.clone();
                    let write: Arc<dyn corelink_turbo_bridge::adapter::CasWriteStore> = store;
                    tracing::info!("Turbo handler: R2KvStore (durable storage)");
                    let handler: Arc<dyn TurboArtifactHandler> =
                        Arc::new(CasAdapterTurboHandler::new(read, write, audit));
                    return TurboRouteState {
                        handler,
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                        get_inflight: Arc::new(Mutex::new(HashMap::new())),
                        events_inflight: Arc::new(Mutex::new(HashMap::new())),
                        write_locks: new_write_locks(),
                        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
                    };
                }
                Some(Err(e)) => {
                    // CAA-360 #7: storage creds ARE present but R2KvStore refused
                    // to build → do NOT silently fall back to the non-durable
                    // InMemory store (fail-OPEN → silent data loss). Mount the
                    // fail-CLOSED UnavailableTurboHandler so every verb 503s LOUDLY
                    // until storage is fixed (mirrors cas.rs UnavailableCasHandler).
                    tracing::error!(error = %e, "Turbo R2KvStore build failed; mounting fail-CLOSED 503 handler (audit #7)");
                    return TurboRouteState {
                        handler: Arc::new(UnavailableTurboHandler),
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                        get_inflight: Arc::new(Mutex::new(HashMap::new())),
                        events_inflight: Arc::new(Mutex::new(HashMap::new())),
                        write_locks: new_write_locks(),
                        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
                    };
                }
                None => {}
            }
        }
    }

    tracing::info!("Turbo handler: InMemoryKvStore (no storage credentials configured)");
    let store = Arc::new(InMemoryKvStore::new());
    let handler: Arc<dyn TurboArtifactHandler> =
        Arc::new(CasAdapterTurboHandler::new(store.clone(), store, audit));
    TurboRouteState {
        handler,
        quota: None,
        pat_gate: None,
        bytes: None,
        put_inflight: Arc::new(Mutex::new(HashMap::new())),
        get_inflight: Arc::new(Mutex::new(HashMap::new())),
        events_inflight: Arc::new(Mutex::new(HashMap::new())),
        write_locks: new_write_locks(),
        // Inert placeholder; `build_with_factory` sets the shared, D1-backed meter.
        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// Build the axum `Router` exposing all four `/v8/artifacts/*` routes.
///
/// Routes registered:
/// - `GET  /v8/artifacts/:hash`    → [`handle_get`]
/// - `PUT  /v8/artifacts/:hash`    → [`handle_put`]
/// - `POST /v8/artifacts/events`   → [`handle_events`]
/// - `POST /v8/artifacts/status`   → [`handle_status`]
///
/// The `events` and `status` routes must be registered BEFORE the `:hash`
/// wildcard route so matchit resolves the literals first.
pub fn router(state: TurboRouteState) -> Router {
    Router::new()
        // Fixed-path routes registered BEFORE the wildcard `:hash` routes so
        // matchit prefers the literal segments `events` / `status` over the
        // capture.
        //
        // C4: the events route is "accept-and-drop" telemetry with NO storage
        // and NO concurrency guard, so it must NOT inherit the 100 MiB artifact
        // body limit below. We layer its OWN tiny `EVENTS_BODY_LIMIT_BYTES`
        // (64 KiB) directly on the route handler — axum honours the INNERMOST
        // `DefaultBodyLimit`, so this per-route layer overrides the outer 100
        // MiB default for `/events` only, and axum rejects oversized telemetry
        // (413) BEFORE buffering it. The artifact GET/PUT and the static
        // `status` route keep the 100 MiB limit.
        .route(
            TURBO_EVENTS_ROUTE,
            post(handle_events).layer(axum::extract::DefaultBodyLimit::max(
                EVENTS_BODY_LIMIT_BYTES,
            )),
        )
        .route(TURBO_STATUS_ROUTE, post(handle_status))
        .route(TURBO_GET_ROUTE, get(handle_get).put(handle_put))
        // Per-route body cap: Turbo build artifacts are legitimately larger than
        // the 10 MiB global limit set in `main.rs`. This inner `DefaultBodyLimit`
        // layer overrides the outer global default for the `/v8/artifacts/*`
        // routes only (axum honours the innermost limit) while still bounding the
        // body at 100 MiB so a PAT cannot OOM the shared container. The `events`
        // route above carries its own (smaller, innermost) limit so this does
        // NOT widen its cap back to 100 MiB.
        .layer(axum::extract::DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES))
        .with_state(state)
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Returns 200 + raw artifact bytes on hit, 404 on miss, 400 on bad params,
/// 403 on cross-tenant, 503 on audit-closed.
#[allow(
    clippy::too_many_arguments,
    reason = "axum handler — every parameter is a request extractor (State / Path / Query / headers + the GET concurrency guards); not a refactorable argument list. Mirrors handle_put."
)]
async fn handle_get(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // H1: the per-tenant read concurrency reservation is a `FromRequestParts`
    // extractor (lock+count+increment, 429 on over-cap). axum runs every
    // `FromRequestParts` extractor during extraction — BEFORE the handler body
    // runs and buffers the artifact — so an over-cap GET is rejected 429 BEFORE
    // the (up to 100 MiB) artifact is read into the heap. Holding `_concurrency`
    // for the whole handler keeps the slot reserved until return; its `GetSlot`
    // RAII-releases on drop. Mirrors `handle_put`'s `PutConcurrencyGuard`.
    _concurrency: GetConcurrencyGuard,
    // H1: the process-wide GET budget — a SECOND `FromRequestParts` extractor. It
    // reserves one of `GLOBAL_TURBO_GET_PERMITS` global permits (503 on global
    // saturation) BEFORE the artifact is buffered, bounding aggregate
    // cross-tenant read heap on a pool SEPARATE from writes. The held permit
    // RAII-releases when the handler returns. Mirrors `GlobalPutBudgetGuard`.
    _global_budget: GlobalGetBudgetGuard,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo GET is a cache READ — require
    // `cas:rw` or `cas:r`. BEFORE any audit or storage. NO-OP for `cas:rw`.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the DO-injected, PAT-resolved authenticated tenant
    // (`auth.0`) — NOT the client `teamId`.  `teamId` is a Turborepo team label
    // demoted to a logical sub-namespace inside the authenticated tenant.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard: reject empty / overlong / `/`-bearing `teamId`
    // (it is interpolated into the storage key) BEFORE any audit or storage.
    // Mirrors the MAX_HASH_LEN guard; maps to 400 via `map_err`.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. 402 over-ceiling /
    // 503 fail-CLOSED. `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    // usage-metering-roi: keep the tenant for the fire-and-forget meter record
    // below — `caller_tenant` is moved into the request constructor.
    let meter_tenant = caller_tenant.clone();
    let req = TurboGetRequest::new(
        hash,
        params.team_id,
        params.slug,
        // Principal is filled in by the auth middleware in production;
        // Phase 0 uses a placeholder.
        format!("anon@{caller_tenant}"),
        caller_tenant,
        now_ms,
    );
    match state.handler.get(req) {
        Ok(resp) => {
            // usage-metering-roi: artifact GET HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&meter_tenant, crate::usage_meter::UsageEvent::ReadHit);
            (StatusCode::OK, resp.bytes).into_response()
        }
        Err(e) => {
            // usage-metering-roi: a genuine NotFound is a GET MISS; other errors
            // are faults, not classified ops.
            if matches!(e, corelink_turbo_bridge::TurboBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&meter_tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_err(e)
        }
    }
}

/// `PUT /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Accepts raw `application/octet-stream` body.  Returns 200 +
/// `{"urls": [...]}` on success.
#[allow(
    clippy::too_many_arguments,
    reason = "axum handler — every parameter is a request extractor (State / Path / Query / headers / body); they are not a refactorable argument list. Mirrors tier_select.rs / audit_export/stream.rs handler allows."
)]
async fn handle_put(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // finding #2: the concurrency reservation is a `FromRequestParts` extractor
    // declared AHEAD of `body: Bytes`, so axum runs it (lock+count+increment,
    // 429 on over-cap) BEFORE the body is buffered into the heap. Holding
    // `_concurrency` for the whole handler keeps the slot reserved until return;
    // its `PutSlot` RAII-releases on drop.
    _concurrency: PutConcurrencyGuard,
    // C5: the process-wide PUT budget — a SECOND `FromRequestParts` extractor
    // (so it too runs BEFORE `body: Bytes`). It reserves one of
    // `GLOBAL_TURBO_PUT_PERMITS` global permits (503 on global saturation)
    // BEFORE the body is buffered, bounding aggregate cross-tenant heap. The
    // held permit RAII-releases when the handler returns.
    _global_budget: GlobalPutBudgetGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo PUT is a cache WRITE — require
    // `cas:rw`. A read-only (`cas:r`) token is rejected here. NO-OP for
    // current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the authenticated `auth.0`, not the client `teamId`.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard on `teamId` (storage-key prefix) before any
    // audit or storage. Mirrors the MAX_HASH_LEN guard; maps to 400.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify_write(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_get`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }

    // C1 — per-object write serialization. Acquire the per-(tenant, team, hash)
    // async lock BEFORE `accrue` and hold it through the `handler.put`
    // (probe→put inside `R2KvStore::write`) and the `release`/reconcile below.
    // This makes the whole reserve→probe→commit→release sequence atomic per
    // STORED object: two concurrent same-key PUTs no longer both observe the
    // same `prior_len` and double-release it (which underflowed `bytes_used`
    // while real storage was unchanged ⇒ unbounded free storage). Different
    // objects hash to different shards and stay concurrent. The lock identity
    // matches the storage object exactly: `tenant = caller_tenant`, storage key
    // = `"<team_id>/<hash>"` (see `corelink_turbo_bridge::adapter` +
    // `R2KvStore::object_key`). Held to the end of the handler via `_write_lock`.
    let shard = write_lock_shard(&caller_tenant, &params.team_id, &hash);
    // `shard` is masked into `[0, TURBO_WRITE_LOCK_SHARDS)` and `write_locks`
    // has exactly that many entries, so `get` is always `Some`; `.get()` (vs
    // indexing) keeps the workspace `indexing_slicing = deny` lint satisfied.
    // The `None` arm is structurally unreachable — fail CLOSED 503 rather than
    // proceed unserialized (which would reopen the C1 double-release window).
    let Some(lock_cell) = state.write_locks.get(shard) else {
        tracing::error!(shard, "turbo write-lock shard out of range (unreachable)");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "write serialization unavailable",
        )
            .into_response();
    };
    let _write_lock = lock_cell.lock().await;

    let now_ms = SystemWallClock.now_ms();
    let byte_len = i64::try_from(body.len()).unwrap_or(i64::MAX);
    // Storage byte accounting (finding #1 / cluster-C): Turbo writes the artifact
    // to R2 directly via its OWN `R2KvStore` (NOT the shared `CasWriteHandler`, so
    // the `AccountingCasHandler` decorator does not cover it) — so the
    // reserve→commit→release discipline is applied HERE at the route. RESERVE
    // BEFORE `handler.put` so an over-cap PUT is rejected 402 BEFORE the R2 write
    // and no over-cap artifact is committed; a reservation fault ⇒ 503
    // fail-CLOSED. (Turbo's KV is opaque-keyed with no durable/idempotent bit, so
    // every stored PUT is charged; an inner failure releases the reservation.)
    if let Some(acc) = state.bytes.as_ref() {
        // The Worker-resolved per-tier cap (server-trusted header) seeds a fresh
        // `tenant_storage_state` row; `None` ⇒ fail-CLOSED on an unseeded tenant.
        let quota_seed = crate::byte_accounting::storage_quota_from_headers(&headers);
        match acc.accrue(&caller_tenant, byte_len, quota_seed).await {
            Ok(crate::byte_accounting::AccrueOutcome::Accrued) => {}
            Ok(crate::byte_accounting::AccrueOutcome::OverCap) => {
                return (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response();
            }
            Ok(crate::byte_accounting::AccrueOutcome::Indeterminate) => {
                tracing::error!(
                    tenant = %caller_tenant,
                    "turbo: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage accounting unavailable",
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, "turbo: byte reservation failed; failing closed");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage accounting unavailable",
                )
                    .into_response();
            }
        }
    }
    let req = TurboPutRequest::new(
        hash,
        params.team_id,
        params.slug,
        body.to_vec(),
        None, // duration_ms — Phase 0: not parsed from headers
        format!("anon@{caller_tenant}"),
        caller_tenant.clone(),
        now_ms,
    );
    match state.handler.put(req) {
        Ok(resp) => {
            // usage-metering-roi: artifact PUT is a WRITE op (fire-and-forget, no
            // await/I/O on the hot path).
            state
                .usage_meter
                .record(&caller_tenant, crate::usage_meter::UsageEvent::Write);
            // rt34 finding #3/#4/#5/#6: Turbo keys are OPAQUE/client-chosen (NOT
            // content-addressed), so an overwrite can change the stored SIZE. We
            // already accrued the full new body length (`byte_len`) above —
            // keeping that preserves the over-cap fail-closed check. Now reconcile
            // `bytes_used` to the TRUE on-disk delta: release the PRIOR size on an
            // overwrite (`prior_len = Some(n)`), netting `old + new - prior` (the
            // correct new total, since `old >= prior` held inductively). A fresh
            // insert (`prior_len == None`) releases nothing — the full new charge
            // stays. The OLD all-or-nothing rollback released the FULL new
            // reservation on ANY overwrite, letting a tenant store unbounded bytes
            // for free (PUT 1 byte, then PUT 100 MiB under the same key ⇒ released
            // the 100 MiB while it stayed on disk).
            if let Some(prior) = resp.prior_len {
                let prior_i64 = i64::try_from(prior).unwrap_or(i64::MAX);
                if prior_i64 > 0 {
                    if let Some(acc) = state.bytes.as_ref() {
                        if let Err(re) = acc.release(&caller_tenant, prior_i64).await {
                            tracing::warn!(error = %re, "turbo: prior-size release on overwrite failed (over-counts; conservative)");
                        }
                    }
                }
            }
            let body = PutArtifactResponse { urls: resp.urls };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => {
            // The R2 write failed AFTER we reserved — RELEASE the reservation so a
            // failed PUT does not permanently consume the tenant's headroom.
            if let Some(acc) = state.bytes.as_ref() {
                if let Err(re) = acc.release(&caller_tenant, byte_len).await {
                    tracing::warn!(error = %re, "turbo: reservation release after failed PUT failed (over-counts; conservative)");
                }
            }
            map_err(e)
        }
    }
}

/// `POST /v8/artifacts/events`
///
/// Accept-and-drop: any body accepted, always returns 200.  Turbo telemetry
/// is not CoreLink's to own — no state mutation, no audit emit.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. The Worker PAT-gates this path, but the
/// container must not trust that; an unauthenticated request never reaches the
/// handler. `AuthTenant` is `FromRequestParts`, so it precedes the body
/// extractor (axum 0.7 ordering rule).
///
/// # Fairness hardening (rt-nuclear cycle-2 #8 + #9)
///
/// `/events` holds a process-wide [`EventsBudgetGuard`] permit AND a per-tenant
/// [`EventsConcurrencyGuard`] slot for the WHOLE request. Two residual fairness
/// gaps are closed here:
///
/// - **#8 (slow-body):** the body is buffered manually under an
///   [`EVENTS_BODY_READ_TIMEOUT`] deadline rather than via the unbounded
///   `Bytes` extractor, so a slowloris dribbling its body can no longer pin a
///   permit + slot indefinitely — a stalled body aborts with 408 and the
///   permit/slot RAII-release when the handler returns. The body is still read
///   under the [`EVENTS_BODY_LIMIT_BYTES`] (64 KiB) cap (413 over it), so the
///   C4 OOM bound is preserved.
/// - **#9 (per-tenant cap):** the [`EventsConcurrencyGuard`] extractor (a
///   `FromRequestParts`, so it runs before the body is read) bounds one tenant
///   to [`EVENTS_CONCURRENCY_LIMIT`] in-flight events POSTs (429 over it),
///   mirroring the PUT plane's per-tenant guard — so one tenant cannot
///   monopolise the shared events pool.
async fn handle_events(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    // C5 + rt-nuclear cycle-2 #4/#8: events holds a permit from its OWN
    // dedicated budget (`GLOBAL_TURBO_EVENTS_PERMITS`), NOT the PUT write budget.
    // Sharing the PUT budget (the old C5 design) let a telemetry flood / slow-body
    // events POST hold PUT permits and starve real cache writes (cross-plane DoS).
    // As a `FromRequestParts` extractor it runs (and 503s on saturation) BEFORE
    // the body is read; combined with the 64 KiB `EVENTS_BODY_LIMIT_BYTES` cap
    // (C4) this keeps the same OOM bound (≤ permits × 64 KiB) on the events pool
    // while isolating it from writes.
    _events_budget: EventsBudgetGuard,
    // rt-nuclear cycle-2 #9: per-tenant events concurrency cap. A
    // `FromRequestParts` extractor (runs before the body) capping one tenant to
    // `EVENTS_CONCURRENCY_LIMIT` in-flight (429 over it) so no tenant hogs the
    // pool. Held for the whole handler; its `EventsSlot` RAII-releases on return.
    _events_concurrency: EventsConcurrencyGuard,
    // #8: take the RAW body (not the unbounded `Bytes` extractor) so the body
    // read happens INSIDE the handler under an `EVENTS_BODY_READ_TIMEOUT`
    // deadline — a slow/stalled body is aborted (408) rather than pinning the
    // permit + slot it holds.
    body: axum::body::Body,
) -> impl IntoResponse {
    // #8: bound the body read by wall-time AND by the events body cap (C4). On a
    // stalled body the timeout fires (408) and the handler returns, dropping the
    // held permit (`EventsBudgetGuard`) and per-tenant slot (`EventsSlot`); on an
    // over-cap body `to_bytes` errors and we map it to 413 — preserving the
    // existing 64 KiB cap behaviour without relying on the `Bytes` extractor.
    let bytes = match tokio::time::timeout(
        EVENTS_BODY_READ_TIMEOUT,
        axum::body::to_bytes(body, EVENTS_BODY_LIMIT_BYTES),
    )
    .await
    {
        Ok(Ok(b)) => b,
        // Body exceeded the events cap (or a transport error) — reject 413,
        // matching the prior `DefaultBodyLimit`-driven behaviour. Failing CLOSED
        // (413) rather than silently accepting keeps the OOM bound intact.
        Ok(Err(_)) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "events body too large or unreadable",
            )
                .into_response();
        }
        // #8: the body did not finish streaming within the deadline — a
        // slowloris. Abort 408; the permit + per-tenant slot release on return.
        Err(_) => {
            tracing::warn!(
                tenant_id = %auth.0,
                timeout_secs = EVENTS_BODY_READ_TIMEOUT.as_secs(),
                "turbo /events body read timed out (slow body); returning 408 and \
                 releasing the held events permit + slot"
            );
            return (StatusCode::REQUEST_TIMEOUT, "events body read timed out").into_response();
        }
    };
    let principal = format!("anon@{}", auth.0);
    let req = TurboEventsRequest::new(bytes.to_vec(), principal, 0u64);
    match state.handler.events(req) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /v8/artifacts/status`
///
/// Returns static `{"status":"enabled"}`.  No request body required.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. `AuthTenant` is `FromRequestParts`, so it
/// precedes the body extractor (axum 0.7 ordering rule).
async fn handle_status(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    _req: axum::extract::Request,
) -> impl IntoResponse {
    let principal = format!("anon@{}", auth.0);
    let _status_req = TurboStatusRequest::new(principal, 0u64);
    match state.handler.status() {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => map_err(e),
    }
}

// ── Error mapping ─────────────────────────────────────────────────────────────

/// Sentinel prefix on a [`TurboBridgeError::Internal`] message that [`map_err`]
/// maps to HTTP **503** (storage unavailable) rather than the generic 500.
const TURBO_STORAGE_UNAVAILABLE_SENTINEL: &str = "turbo-storage-unavailable: ";

/// Fail-CLOSED stand-in mounted when storage creds ARE present but the durable
/// `R2KvStore` refused to build (CAA-360 #7). A silent `InMemoryKvStore` fallback
/// there would serve a NON-durable cache with no alarm (fail-OPEN → silent data
/// loss); every verb here instead returns a sentinel `Internal` error that
/// [`map_err`] maps to 503. Mirrors `cas.rs::UnavailableCasHandler`.
#[derive(Debug)]
struct UnavailableTurboHandler;

impl UnavailableTurboHandler {
    fn unavailable() -> TurboBridgeError {
        TurboBridgeError::Internal(format!(
            "{TURBO_STORAGE_UNAVAILABLE_SENTINEL}R2KvStore refused to build"
        ))
    }
}

impl TurboArtifactHandler for UnavailableTurboHandler {
    fn put(&self, _req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn get(&self, _req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn events(&self, _req: TurboEventsRequest) -> Result<TurboEventsResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn status(&self) -> Result<TurboStatusResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
}

/// Map a [`TurboBridgeError`] to the canonical HTTP response.
///
/// All error details are logged before mapping so operators have a
/// diagnostic record without exposing internals to callers.
fn map_err(e: TurboBridgeError) -> axum::response::Response {
    tracing::warn!(error = ?e, "turbo bridge error");
    match e {
        TurboBridgeError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        TurboBridgeError::HashTooLong { .. } => {
            (StatusCode::BAD_REQUEST, "hash too long").into_response()
        }
        TurboBridgeError::TeamIdInvalid { .. } => {
            (StatusCode::BAD_REQUEST, "invalid teamId").into_response()
        }
        TurboBridgeError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        TurboBridgeError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve/commit
            // without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // CAA-360 #7: storage-unavailable (R2KvStore refused to build) → 503,
        // distinct from a generic 500, so clients retry rather than treat it as
        // a permanent server fault.
        TurboBridgeError::Internal(ref msg)
            if msg.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Fixed authenticated tenant injected via `x-corelink-tenant-id` so the
    /// `AuthTenant` extractor (fail-CLOSED) admits the request. PUT/GET in a
    /// round-trip MUST share this value or the GET reads a different tenant's
    /// namespace and 404s.
    const TEST_AUTH_TENANT: &str = "11111111-1111-1111-1111-111111111111";

    /// Read+write cache scope, mirroring every current prod PAT. Requests
    /// that exercise GET/PUT must carry this in `x-corelink-scope` or the
    /// fail-CLOSED scope gate rejects them 403.
    const TEST_SCOPE_RW: &str = "cas:rw";

    /// Build a test fixture using `InMemoryKvStore` + `InMemoryTurboAuditSink`.
    fn fixture() -> TurboRouteState {
        build_handlers()
    }

    /// Build the router from the fixture state.
    fn test_router() -> Router {
        router(fixture())
    }

    // ── Route constant sanity ─────────────────────────────────────────────────

    #[test]
    fn route_constants_use_brace_syntax_not_colon() {
        // Regression net for DEBT-029 (post axum-0.8) — matchit 0.8 treats the
        // legacy `:name` form as a literal path segment; `{name}` is the capture.
        assert!(
            !TURBO_GET_ROUTE.contains(':'),
            "TURBO_GET_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
        );
        assert!(TURBO_GET_ROUTE.contains("{hash}"));
        assert!(TURBO_PUT_ROUTE.contains("{hash}"));
        assert_eq!(TURBO_EVENTS_ROUTE, "/v8/artifacts/events");
        assert_eq!(TURBO_STATUS_ROUTE, "/v8/artifacts/status");
    }

    // ── build_handlers smoke test ─────────────────────────────────────────────

    #[test]
    fn build_handlers_returns_usable_state() {
        let state = build_handlers();
        // Status should always succeed.
        let resp = state.handler.status().expect("status");
        assert_eq!(resp.status, "enabled");
    }

    #[test]
    fn unavailable_turbo_handler_503s_loud_not_silent_inmemory() {
        // CAA-360 #7: when R2KvStore fails to build with creds present, the route
        // must 503 LOUDLY on every verb — never silently degrade to a non-durable
        // InMemory store. All verbs share one error path (Self::unavailable);
        // assert it carries the sentinel and that map_err turns it into 503.
        let err = UnavailableTurboHandler::unavailable();
        assert!(
            matches!(&err, TurboBridgeError::Internal(m) if m.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL)),
            "unavailable error must carry the storage-unavailable sentinel"
        );
        assert_eq!(map_err(err).status(), StatusCode::SERVICE_UNAVAILABLE);
        // status() routes through the same error → 503 (not 500, not a fake OK).
        let h = UnavailableTurboHandler;
        assert_eq!(
            map_err(h.status().expect_err("must be unavailable")).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    // ── GET happy path ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_returns_404_when_artifact_absent() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/deadbeef?teamId=team_a")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_then_get_round_trip() {
        let state = fixture();
        let app = router(state);

        // PUT
        let body_bytes = b"webpack-output-chunk".to_vec();
        let put_req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/abc123?teamId=team_x")
            .header("content-type", "application/octet-stream")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(body_bytes.clone()))
            .expect("put request");
        let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
        assert_eq!(put_resp.status(), StatusCode::OK);

        // GET — same hash + teamId should return the stored bytes.
        let get_req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/abc123?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("get request");
        let get_resp = app.oneshot(get_req).await.expect("get oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let response_body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(response_body.as_ref(), body_bytes.as_slice());
    }

    // ── opaque hash round-trip ────────────────────────────────────────────────

    #[tokio::test]
    async fn opaque_hash_round_trip() {
        // Turbo may send any string as hash (xxhash, sha512-prefix, etc.).
        // CoreLink stores it verbatim without hash verification.
        let state = fixture();
        let app = router(state);

        let opaque_hash = "turboxxhash-5e7c3b2a1f";
        let artifact = b"turbo-build-artifact-data".to_vec();

        let put_req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(artifact.clone()))
            .expect("put");
        let put_resp = app.clone().oneshot(put_req).await.expect("put");
        assert_eq!(put_resp.status(), StatusCode::OK);

        let get_req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("get");
        let get_resp = app.oneshot(get_req).await.expect("get");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), artifact.as_slice());
    }

    // ── cross-tenant guard ────────────────────────────────────────────────────

    #[tokio::test]
    async fn cross_tenant_isolated_via_caller_tenant_miss_not_denied() {
        // Repurposed from `put_cross_tenant_returns_403`. The isolation tenant
        // is now `AuthTenant` (auth.0), NOT the `teamId` query param, so a
        // `team_id == caller_tenant` denial is structurally impossible on this
        // path — at the HTTP layer a missing/invalid tenant is rejected 401 at
        // the extractor (fail-CLOSED) before the handler runs. We assert the
        // NEW invariant at the handler the route is wired to: an artifact
        // written under caller_tenant="tenantA" is unreachable from
        // caller_tenant="tenantB" with the SAME teamId+hash — a NotFound MISS,
        // not a denial.
        let state = fixture();
        state
            .handler
            .put(corelink_turbo_bridge::TurboPutRequest::new(
                "h1",
                "shared_team", // teamId — sub-namespace, NOT the tenant
                "s",
                b"tenantA bytes".to_vec(),
                None,
                "userA",
                "tenantA", // caller_tenant — the isolation dimension
                1,
            ))
            .expect("put under tenantA");
        let err = state
            .handler
            .get(corelink_turbo_bridge::TurboGetRequest::new(
                "h1",
                "shared_team",
                "s",
                "userB",
                "tenantB", // different caller_tenant ⇒ isolated
                2,
            ))
            .expect_err("tenantB must not reach tenantA's artifact");
        assert!(matches!(
            err,
            corelink_turbo_bridge::TurboBridgeError::NotFound { .. }
        ));
        // Same-tenant round-trip still serves the bytes.
        let ok = state
            .handler
            .get(corelink_turbo_bridge::TurboGetRequest::new(
                "h1",
                "shared_team",
                "s",
                "userA",
                "tenantA",
                3,
            ))
            .expect("tenantA round-trip");
        assert_eq!(ok.bytes, b"tenantA bytes".to_vec());
    }

    // ── events endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_events_always_200() {
        let app = test_router();
        // Turbo sends arbitrary JSON telemetry; we accept and drop.
        // F3: events now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("content-type", "application/json")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_events_empty_body_200() {
        let app = test_router();
        // F3: events now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── F3: events/status are fail-CLOSED without an authenticated tenant ──────

    #[tokio::test]
    async fn post_events_without_tenant_header_is_401() {
        // F3 (defense-in-depth): the container must not trust the Worker's
        // PAT gate. No `x-corelink-tenant-id` ⇒ `AuthTenant` rejects 401
        // BEFORE the accept-and-drop handler runs.
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_events_sentinel_tenant_is_401() {
        // A sentinel tenant value is not a real authenticated tenant.
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", "_unknown")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_status_without_tenant_header_is_401() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_status_sentinel_tenant_is_401() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .header("x-corelink-tenant-id", "_unknown")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── status endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_status_returns_enabled() {
        let app = test_router();
        // F3: status now requires the authenticated-tenant header.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["status"], "enabled");
    }

    // ── hash-too-long guard ───────────────────────────────────────────────────

    #[test]
    fn hash_too_long_maps_to_400() {
        let resp = map_err(TurboBridgeError::HashTooLong { len: 129, max: 128 });
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── GET missing teamId → 400 ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_missing_team_id_returns_400() {
        let app = test_router();
        // No teamId query param — axum Query extractor rejects this.
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/somehash")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── PUT missing teamId → 400 ──────────────────────────────────────────────

    #[tokio::test]
    async fn put_missing_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash")
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── teamId DoS / key-aliasing guard → 400 ─────────────────────────────────

    #[test]
    fn team_id_invalid_maps_to_400() {
        let resp = map_err(TurboBridgeError::TeamIdInvalid {
            len: 0,
            max: 256,
            reason: "team_id must not be empty",
        });
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_slash_in_team_id_returns_400() {
        let app = test_router();
        // `teamId=a/b` is `teamId=a%2Fb` here — a `/`-bearing value that would
        // escape the team sub-namespace key prefix. Rejected at the route guard.
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash?teamId=a%2Fb")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_empty_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/somehash?teamId=")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn put_overlong_team_id_returns_400() {
        let app = test_router();
        let long = "a".repeat(corelink_turbo_bridge::error::MAX_TEAM_ID_LEN + 1);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v8/artifacts/somehash?teamId={long}"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_slash_in_team_id_returns_400() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/somehash?teamId=a%2Fb")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    #[tokio::test]
    async fn put_with_read_only_scope_returns_403_insufficient_scope() {
        // A `cas:r` (read-only) token must NOT be able to PUT (write).
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    #[tokio::test]
    async fn put_with_rw_scope_succeeds() {
        // `cas:rw` is the current prod scope — PUT must still succeed.
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_with_missing_scope_returns_403() {
        // Fail-CLOSED: no `x-corelink-scope` header ⇒ no cache read.
        let app = test_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/h1?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_valid_team_id_accepted() {
        // Sanity: a conventional slug-style teamId still round-trips 200.
        let app = test_router();
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h1?teamId=team_AbC-123")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── F24: per-tenant PUT concurrency guard ─────────────────────────────────

    #[test]
    fn put_inflight_counter_is_shared_across_clones() {
        // `TurboRouteState::clone` shares the `Arc<Mutex<..>>` — so all
        // route handler invocations (axum clones state per request) see the
        // SAME counter. Verify the Arc is truly shared, not deep-copied.
        let state = build_handlers();
        let state2 = state.clone();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), 3);
        }
        let g = state2.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            Some(&3),
            "cloned state must share the same inflight counter"
        );
    }

    #[tokio::test]
    async fn put_at_limit_returns_429() {
        // Simulate reaching TURBO_PUT_CONCURRENCY_LIMIT by pre-seeding the
        // counter, then issue one more PUT — must get 429.
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_PUT_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_limit?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "at-limit PUT must return 429"
        );
    }

    #[tokio::test]
    async fn put_below_limit_succeeds_and_decrements_counter() {
        // A PUT that succeeds must release its concurrency slot (counter goes
        // back to 0 after the request completes, not leaked).
        let state = fixture();
        let app = router(state.clone());
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_decr?teamId=team_y")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(b"data".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        // After the handler returns, the guard must have decremented the counter.
        let g = state.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            None, // removed when count reaches 0
            "concurrency counter must be released after PUT completes"
        );
    }

    // ── H1: GET pre-buffer concurrency guard (read-path twin of PUT) ──────────

    #[tokio::test]
    async fn get_at_limit_returns_429() {
        // H1: mirror `put_at_limit_returns_429` for the read path. Simulate
        // reaching TURBO_GET_CONCURRENCY_LIMIT by pre-seeding the per-tenant GET
        // counter, then issue one more GET — the `GetConcurrencyGuard`
        // `FromRequestParts` extractor must reject it 429 BEFORE the handler
        // touches storage or buffers the (up to 100 MiB) artifact.
        let state = fixture();
        {
            let mut g = state.get_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_GET_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/h_get_limit?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "at-limit GET must return 429 (pre-buffer per-tenant guard)"
        );
    }

    #[tokio::test]
    async fn get_below_limit_succeeds_and_decrements_counter() {
        // A GET that completes must release its concurrency slot (counter goes
        // back to 0 / removed, not leaked). Mirrors the PUT decrement test.
        let state = fixture();
        let app = router(state.clone());
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/h_get_decr?teamId=team_y")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::empty())
            .expect("request");
        // 404 (artifact absent) is fine — the slot must still be released after
        // the handler returns on EVERY path.
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let g = state.get_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            None,
            "GET concurrency counter must be released after the request completes"
        );
    }

    // ── rt34 #3/#4/#5/#6: storage byte-delta accounting (NOT all-or-nothing) ──

    /// Region the test accountant is keyed on. Must match the value we read the
    /// `InMemoryByteStore` back with.
    const TEST_BYTES_REGION: &str = "iad";

    /// `0` cap = genuine unlimited — keeps every PUT under the cap so the test
    /// exercises the byte-DELTA math, not the over-cap gate. Seeded via the
    /// server-trusted `x-corelink-storage-quota-bytes` header.
    const UNLIMITED_QUOTA_HEADER: &str = "0";

    /// Build a route state wired with a real `ByteAccountant` over an in-memory
    /// `ByteStore`, so PUTs flow through the byte-delta reconciliation. Returns
    /// the state and the store handle for `bytes_used` assertions.
    fn fixture_with_byte_accounting() -> (
        TurboRouteState,
        Arc<crate::byte_accounting::testing::InMemoryByteStore>,
    ) {
        let store = Arc::new(crate::byte_accounting::testing::InMemoryByteStore::new());
        let dyn_store: Arc<dyn crate::byte_accounting::ByteStore> = store.clone();
        let acc = Arc::new(crate::byte_accounting::ByteAccountant::new(
            dyn_store,
            TEST_BYTES_REGION.to_owned(),
        ));
        let mut state = build_handlers();
        state.bytes = Some(acc);
        (state, store)
    }

    /// Issue a PUT of `body` to `hash` and assert 200. Carries the unlimited
    /// quota header so the fresh `tenant_storage_state` row seeds (else 503).
    async fn put_artifact(app: &Router, hash: &str, body: Vec<u8>) {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v8/artifacts/{hash}?teamId=team_x"))
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .header(
                crate::byte_accounting::STORAGE_QUOTA_HEADER,
                UNLIMITED_QUOTA_HEADER,
            )
            .body(Body::from(body))
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK, "PUT {hash} must 200");
    }

    /// The KILLING rt34 test: storage `bytes_used` tracks the TRUE on-disk delta
    /// across overwrites — the OLD all-or-nothing rollback let a tenant store
    /// unbounded bytes for free (PUT 1B then PUT 100MiB under the same key
    /// released the whole 100MiB while it stayed on disk).
    ///
    /// Covers the five delta cases: fresh insert charges full; same-size
    /// overwrite nets 0; GROW 1→big nets +(big-1); SHRINK big→1 decreases
    /// bytes_used. (The probe-error-treated-as-fresh case is covered at the
    /// store layer in `r2_kv::rt_write_probe_error_fails_closed_to_none`.)
    #[tokio::test]
    async fn put_overwrite_accounts_true_byte_delta_not_all_or_nothing() {
        let (state, store) = fixture_with_byte_accounting();
        let app = router(state);
        let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

        // 1) Fresh insert of 1 byte → charge full (used = 1).
        put_artifact(&app, "k", vec![b'a']).await;
        assert_eq!(used(), 1, "fresh insert charges the full new bytes");

        // 2) Same-size overwrite (1 → 1) → net 0 (accrue 1, release prior 1).
        put_artifact(&app, "k", vec![b'b']).await;
        assert_eq!(used(), 1, "same-size overwrite nets zero");

        // 3) GROW 1 → 100 (the exploit shape) → +99 (accrue 100, release prior 1).
        put_artifact(&app, "k", vec![0u8; 100]).await;
        assert_eq!(
            used(),
            100,
            "grow must add the delta — the OLD rollback would have released the \
             full 100 leaving used≈1 with 100 bytes on disk (the bypass)"
        );

        // 4) SHRINK 100 → 1 → -99 (accrue 1, release prior 100).
        put_artifact(&app, "k", vec![b'c']).await;
        assert_eq!(used(), 1, "shrink must decrease bytes_used to the new size");
    }

    /// A SECOND distinct key is charged independently — proves the release
    /// targets the OVERWRITTEN key's prior, not the whole tenant.
    #[tokio::test]
    async fn put_distinct_keys_accumulate_independently() {
        let (state, store) = fixture_with_byte_accounting();
        let app = router(state);
        let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

        put_artifact(&app, "k1", vec![0u8; 10]).await;
        assert_eq!(used(), 10);
        // Fresh second key: no prior to release → full add.
        put_artifact(&app, "k2", vec![0u8; 25]).await;
        assert_eq!(used(), 35, "two fresh keys sum (no spurious release)");
        // Overwrite k1 same size: net 0 → total unchanged.
        put_artifact(&app, "k1", vec![1u8; 10]).await;
        assert_eq!(
            used(),
            35,
            "same-size overwrite of one key leaves the total"
        );
    }

    // ── finding #2: the cap is enforced PRE-BUFFER (FromRequestParts) ─────────

    /// The KILLING finding-#2 test: a 5th concurrent PUT is rejected 429 BEFORE
    /// its body is buffered into the heap.
    ///
    /// The `PutConcurrencyGuard` is a `FromRequestParts` extractor declared
    /// AHEAD of `body: Bytes`; axum runs every `FromRequestParts` extractor
    /// before the single body extractor. We prove the ordering deterministically:
    /// with the tenant AT the cap, this PUT carries a body LARGER than the route's
    /// 100 MiB `DefaultBodyLimit`. If the body were buffered first (the OLD,
    /// in-handler guard), axum's body-limit layer would reject the oversized body
    /// (413/400). Because the concurrency extractor runs FIRST, we get **429**
    /// instead — and the oversized body is never read (the `Body` is streamed
    /// lazily; the extractor short-circuits before any of it is consumed, so the
    /// test stays cheap).
    #[tokio::test]
    async fn fifth_concurrent_put_rejected_429_before_body_buffering() {
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().unwrap();
            // Tenant already AT the limit (4 in-flight).
            g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_PUT_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        // A body strictly larger than the route's 100 MiB DefaultBodyLimit. If the
        // body extractor ran before the concurrency guard, the body-limit layer
        // would reject the oversized body; the pre-buffer guard makes it 429
        // without reading the body.
        let oversized = Body::from(vec![0u8; TURBO_BODY_LIMIT_BYTES + 1]);
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/h_oversized?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the 5th concurrent PUT must be rejected 429 by the FromRequestParts \
             guard BEFORE the body is buffered (else the oversized body would be \
             rejected by the body-limit layer instead)"
        );
    }

    // ── C1: per-object write serialization (no double-release / no underflow) ──

    /// The KILLING C1 test: N CONCURRENT shrink-PUTs to the SAME key must net
    /// the TRUE on-disk delta — never double-release the prior size.
    ///
    /// Without the per-object lock, two+ concurrent same-key PUTs each probe
    /// the SAME prior length `L` (`R2KvStore::write`'s GET-presence probe is not
    /// serialized) and each `release(L)`, underflowing `bytes_used` well below
    /// the true on-disk size while real storage is unchanged. We prime a large
    /// (1000-byte) object, then fire N=8 concurrent tiny (1-byte) overwrites.
    /// The TRUE final size is 1 byte, so `bytes_used` MUST equal 1 — never a
    /// double-released value (which would be 0 / underflowed, or some racy
    /// value < 1). The per-key `tokio::sync::Mutex` serializes probe→put→release
    /// so exactly ONE prior gets released per real overwrite.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_same_key_shrink_puts_net_true_delta_no_double_release() {
        let (state, store) = fixture_with_byte_accounting();
        let app = router(state);
        let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

        // Prime the key with a large object: bytes_used = 1000.
        put_artifact(&app, "shared", vec![0u8; 1000]).await;
        assert_eq!(used(), 1000, "primed large object");

        // Fire N concurrent tiny (1-byte) overwrites of the SAME key. N MUST be
        // <= TURBO_PUT_CONCURRENCY_LIMIT (4): the per-tenant PutConcurrencyGuard
        // 429s the (limit+1)th in-flight PUT, so N > 4 made this test flaky (a
        // 429 tripped the "each PUT 200s" assert depending on scheduling). 4
        // concurrent same-key shrinks still exercise the double-release race
        // (without the per-key lock they'd each probe prior=1000 and N-tuple
        // -release it), so the lock's correctness is proven deterministically.
        const N: usize = 4;
        let mut handles = Vec::with_capacity(N);
        for i in 0..N {
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                let req = Request::builder()
                    .method(Method::PUT)
                    .uri("/v8/artifacts/shared?teamId=team_x")
                    .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
                    .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
                    .header(
                        crate::byte_accounting::STORAGE_QUOTA_HEADER,
                        UNLIMITED_QUOTA_HEADER,
                    )
                    .body(Body::from(vec![b'a' + (i as u8 % 26)]))
                    .expect("request");
                app.oneshot(req).await.expect("oneshot").status()
            }));
        }
        for h in handles {
            assert_eq!(h.await.expect("join"), StatusCode::OK, "each PUT 200s");
        }

        // The TRUE on-disk size is 1 byte (last writer wins, all writes are
        // 1 byte). With the per-key lock, each overwrite releases exactly its
        // own prior, so bytes_used reconciles to the true size: 1. Without the
        // lock, concurrent probes would each see prior=1000 and double/N-tuple
        // -release it, underflowing bytes_used to 0 (saturating) — the bug.
        assert_eq!(
            used(),
            1,
            "concurrent same-key shrinks must net the TRUE on-disk size (1), \
             not a double-released / underflowed value"
        );
    }

    /// Distinct keys are NOT serialized against each other (the per-object lock
    /// only serializes the SAME object): concurrent PUTs to different keys both
    /// land and accumulate independently.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_distinct_key_puts_stay_concurrent_and_sum() {
        let (state, store) = fixture_with_byte_accounting();
        let app = router(state);
        let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

        let mut handles = Vec::new();
        for i in 0..6u32 {
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                let req = Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/v8/artifacts/key{i}?teamId=team_x"))
                    .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
                    .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
                    .header(
                        crate::byte_accounting::STORAGE_QUOTA_HEADER,
                        UNLIMITED_QUOTA_HEADER,
                    )
                    .body(Body::from(vec![0u8; 10]))
                    .expect("request");
                app.oneshot(req).await.expect("oneshot").status()
            }));
        }
        for h in handles {
            assert_eq!(h.await.expect("join"), StatusCode::OK);
        }
        assert_eq!(used(), 60, "six fresh distinct 10-byte keys sum to 60");
    }

    /// The shard index is a pure deterministic function of the lock identity
    /// tuple and always lands in `[0, TURBO_WRITE_LOCK_SHARDS)`; distinct
    /// identities generally map to distinct shards (sanity, not a guarantee).
    #[test]
    fn write_lock_shard_in_range_and_deterministic() {
        let a = write_lock_shard("tenantA", "team1", "hash1");
        let b = write_lock_shard("tenantA", "team1", "hash1");
        assert_eq!(a, b, "shard must be deterministic for a fixed identity");
        for (t, team, h) in [
            ("t", "x", "h"),
            ("tenant-uuid", "default", "deadbeef"),
            ("", "", ""),
        ] {
            assert!(
                write_lock_shard(t, team, h) < TURBO_WRITE_LOCK_SHARDS,
                "shard must be in range"
            );
        }
        // Domain separation: ("a","b",_) must not collapse to ("ab","",_).
        assert_ne!(
            write_lock_shard("a", "b", "h"),
            write_lock_shard("ab", "", "h"),
            "domain-separated identity components must not alias"
        );
    }

    // ── C4: events route has its OWN small body cap (413 over it) ──────────────

    /// An events POST OVER the small `EVENTS_BODY_LIMIT_BYTES` (64 KiB) cap is
    /// rejected (413) — the route must NOT inherit the 100 MiB artifact limit,
    /// so a telemetry body cannot OOM the container. A normal small telemetry
    /// POST still 200s.
    #[tokio::test]
    async fn events_over_small_cap_rejected_normal_still_200() {
        let app = test_router();

        // Over the 64 KiB events cap (but WELL under the 100 MiB artifact cap,
        // so a 200 here would prove the route wrongly inherited the big limit).
        let oversized = vec![0u8; EVENTS_BODY_LIMIT_BYTES + 1];
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header("content-type", "application/json")
            .body(Body::from(oversized))
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "events body over the 64 KiB cap must be rejected 413, NOT accepted \
             under the 100 MiB artifact limit"
        );

        // A normal small telemetry POST still succeeds.
        let small = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(small).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK, "small telemetry still 200s");
    }

    /// A 100 MiB artifact PUT is NOT rejected by the events cap (proves the
    /// small cap is route-LOCAL to `/events`, not applied to artifact PUTs).
    /// We assert the artifact route still accepts a body LARGER than the events
    /// cap (a 1 MiB body — far above 64 KiB, far below 100 MiB).
    #[tokio::test]
    async fn artifact_put_not_constrained_by_events_cap() {
        let app = test_router();
        let body = vec![0u8; EVENTS_BODY_LIMIT_BYTES * 4]; // 256 KiB > events cap
        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/bigart?teamId=team_x")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "an artifact PUT above the events cap (but under 100 MiB) must NOT be \
             413'd — the small cap is route-local to /events"
        );
    }

    // ── rt-nuclear cycle-2 #8: /events slow-body read timeout ──────────────────

    /// #8 (slowloris on the events pool): a `POST /v8/artifacts/events` whose
    /// body never finishes streaming must NOT pin the held events permit + slot
    /// forever — the server-side `EVENTS_BODY_READ_TIMEOUT` aborts it with 408,
    /// which returns the handler and RAII-releases the permit + the per-tenant
    /// slot. We assert (a) the request resolves to 408 well within a bound far
    /// shorter than "forever", and (b) the per-tenant slot is released after
    /// (so a stalled body did not leak a slot).
    #[tokio::test]
    async fn events_slow_body_times_out_408_and_releases_slot() {
        let state = fixture();
        let app = router(state.clone());

        // A body that yields ONE chunk then never completes (a slowloris that
        // dribbles a byte and stalls) — the canonical slow-body attack.
        let stalled = Body::from_stream(async_stream::stream! {
            yield Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"{"));
            // Never produce EOF: pend forever. The handler's read timeout, not
            // the stream, must end the request.
            futures::future::pending::<()>().await;
            // Unreachable, but satisfies the stream's item-type inference.
            yield Ok(axum::body::Bytes::new());
        });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header("content-type", "application/json")
            .body(stalled)
            .expect("request");

        // Bound the whole call by a deadline a bit longer than the handler's read
        // timeout: if the handler did NOT enforce its own timeout this outer
        // bound would fire (test would hang/err here), proving the regression.
        let resp = tokio::time::timeout(
            EVENTS_BODY_READ_TIMEOUT + Duration::from_secs(3),
            app.oneshot(req),
        )
        .await
        .expect("handler must self-abort the slow body — it must NOT hang forever")
        .expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::REQUEST_TIMEOUT,
            "a stalled events body must be aborted 408 by the server read timeout"
        );

        // The per-tenant slot must be released (back to 0 ⇒ entry removed) — a
        // timed-out request must not leak its concurrency slot.
        let g = state.events_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            None,
            "the events concurrency slot must be released after a 408 timeout"
        );
    }

    // ── rt-nuclear cycle-2 #9: per-tenant /events concurrency cap ───────────────

    #[test]
    fn events_inflight_counter_is_shared_across_clones() {
        // `TurboRouteState::clone` shares the `Arc<Mutex<..>>` — axum clones the
        // state per request, so every `/events` invocation must see the SAME
        // per-tenant counter. Mirrors `put_inflight_counter_is_shared_across_clones`.
        let state = build_handlers();
        let state2 = state.clone();
        {
            let mut g = state.events_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), 2);
        }
        let g = state2.events_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            Some(&2),
            "cloned state must share the same events inflight counter"
        );
    }

    /// #9 (per-tenant fairness): one tenant AT its `/events` concurrency cap is
    /// rejected 429 — while a DIFFERENT tenant (at 0 in-flight) is still served.
    /// Proves the cap is PER-TENANT, so one tenant cannot monopolise the events
    /// pool and starve others. Mirrors the PUT plane's `put_at_limit_returns_429`.
    #[tokio::test]
    async fn events_per_tenant_cap_429s_hog_but_other_tenant_still_served() {
        const OTHER_TENANT: &str = "22222222-2222-2222-2222-222222222222";
        let state = fixture();
        // Pre-seed the noisy tenant AT its cap (simulating EVENTS_CONCURRENCY_LIMIT
        // already-in-flight events POSTs for that tenant).
        {
            let mut g = state.events_inflight.lock().unwrap();
            g.insert(TEST_AUTH_TENANT.to_owned(), EVENTS_CONCURRENCY_LIMIT);
        }
        let app = router(state.clone());

        // The hog's next events POST is rejected 429 (BEFORE the body is read).
        let hog = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"x"}"#))
            .expect("request");
        let resp = app.clone().oneshot(hog).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "a tenant at its per-tenant events cap must be 429'd"
        );

        // A DIFFERENT tenant (0 in-flight) is still served — the cap did not
        // close the whole pool, only the hog's share.
        let other = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", OTHER_TENANT)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"y"}"#))
            .expect("request");
        let resp = app.oneshot(other).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "another tenant under its cap must still be served — fairness, not a \
             global lockout"
        );
    }

    /// A normal events POST releases its per-tenant slot on completion (the
    /// counter returns to 0 ⇒ entry removed, not leaked). Mirrors
    /// `put_below_limit_succeeds_and_decrements_counter`.
    #[tokio::test]
    async fn events_below_cap_succeeds_and_releases_slot() {
        let state = fixture();
        let app = router(state.clone());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"ok"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let g = state.events_inflight.lock().unwrap();
        assert_eq!(
            g.get(TEST_AUTH_TENANT),
            None,
            "the events concurrency slot must be released after a successful POST"
        );
    }

    // ── C5: process-wide PUT budget ────────────────────────────────────────────

    /// The process-wide budget is a TRUE singleton (every call to
    /// `global_turbo_put_budget()` returns the SAME `Arc<Semaphore>`), so all
    /// tenants/routers in the process share one budget. (We assert identity
    /// only — NOT live `available_permits`, which concurrent PUT tests mutate.)
    #[test]
    fn global_budget_is_process_wide_singleton() {
        let s1 = global_turbo_put_budget();
        let s2 = global_turbo_put_budget();
        assert!(
            Arc::ptr_eq(&s1, &s2),
            "global budget must be a process-wide singleton shared by all tenants"
        );
    }

    /// Unit test of the permit-accounting SEMANTICS on a fresh semaphore sized
    /// exactly like the global budget — isolated from the live singleton so it
    /// cannot flake against concurrent PUT tests. Proves: exactly
    /// `GLOBAL_TURBO_PUT_PERMITS` permits are acquirable; one more times out
    /// within `GLOBAL_PUT_PERMIT_WAIT` (the extractor's 503 path); dropping the
    /// held permits restores the budget.
    #[tokio::test]
    async fn global_budget_permit_accounting_saturates_then_restores() {
        let sem = Arc::new(Semaphore::new(GLOBAL_TURBO_PUT_PERMITS));
        let mut held = Vec::new();
        for _ in 0..GLOBAL_TURBO_PUT_PERMITS {
            held.push(
                Arc::clone(&sem)
                    .acquire_owned()
                    .await
                    .expect("permit available within the configured count"),
            );
        }
        assert_eq!(sem.available_permits(), 0, "budget fully drained");
        // One more acquire must time out (saturated) — exactly the path the
        // `GlobalPutBudgetGuard` extractor maps to 503.
        let timed_out =
            tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, Arc::clone(&sem).acquire_owned())
                .await
                .is_err();
        assert!(
            timed_out,
            "beyond the permit count, acquire must time out → extractor returns 503"
        );
        drop(held);
        assert_eq!(
            sem.available_permits(),
            GLOBAL_TURBO_PUT_PERMITS,
            "permits restored on drop (RAII release)"
        );
    }
}

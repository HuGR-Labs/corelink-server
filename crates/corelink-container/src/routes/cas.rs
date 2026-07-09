//! `GET /v1/cas/:tenant/:hash` — CAS read route wired against
//! [`corelink_handler_cas::CasReadHandler`].
//!
//! This is the **example end-to-end wire-up** demonstrating how the
//! R-prep handler-crate skeleton plugs into `apps/server`. The
//! production CF-Worker R2-bound handler would replace
//! `InMemoryCasHandler` under the `#[cfg(target_arch = "wasm32")]`
//! branch in the trait-object construction site (see crate-level
//! `corelink_handler_cas` doc).
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn CasReadHandler>` so the route
//! table stays binary-shape stable as we swap the in-memory fake for
//! the wasm32 CF-Worker impl. The same shape is used for the
//! Stripe webhook route's
//! [`corelink_stripe_real::webhook_dispatch::StateMaterializer`]
//! collaborator (wave 16 unification —
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`
//! §unification).
//!
//! # SLO emit
//!
//! Every entry into this route emits one `Sli::AvailCasGet` + one
//! `Sli::LatencyCasGetP99` observation through the
//! `SliObserver` collaborator on the handler. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-CAS-GET`
//! from "Sli-bound deferred" to "Sli emitted at handler layer".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasListHandler,
    CasListRequest, CasListResponse, CasReadHandler, CasReadRequest, CasReadResponse,
    CasWriteHandler, CasWriteRequest, CasWriteResponse, InMemoryAuditSink, InMemoryCasHandler,
    InMemorySliObserver,
};

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
/// ceiling, the extra 2 MiB headroom covers the manifest framing.
pub const BATCH_MAX_BYTES: usize = 8 * 1024 * 1024;

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
        if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
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

/// Maximum concurrent in-flight native CAS bulk READs for a single tenant. Excess
/// bulk reads are rejected 429 BEFORE the body is buffered. Bounds peak per-tenant
/// read-payload heap (`batch-read` accumulates up to [`BATCH_MAX_BYTES`] per
/// request) and read fan-out (`batch-exists`). SEPARATE axis from the write cap
/// [`CAS_WRITE_CONCURRENCY_LIMIT`] so reads and writes don't over-throttle each
/// other; same per-tenant ceiling (8).
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
        if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
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
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent reads").into_response(),
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

/// Build the canonical `Arc<dyn CasReadHandler>` for the current
/// build target and runtime environment.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets the function probes for storage credentials at
/// runtime:
///
/// - When `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`,
///   `R2_S3_ENDPOINT`, `CLOUDFLARE_ACCOUNT_ID`, `CF_API_TOKEN`, and
///   `D1_DATABASE_ID` are all present in the environment,
///   [`R2CasHandler`](crate::storage::r2_s3::R2CasHandler) is
///   constructed against `corelink-cas-prod` (override via
///   `R2_CAS_BUCKET`). This is the **production path**.
///
/// - When credentials are absent (unit tests, local dev, CI) the
///   function falls back to `InMemoryCasHandler`. No network I/O
///   occurs.
///
/// - When credentials ARE present but the R2 handler refuses to build
///   (today exactly the "`R2_TDK_HEX` required" case — F1, CAA-360) the
///   function does NOT silently degrade to `InMemoryCasHandler` (that
///   would serve a non-durable cache with no alarm). It instead mounts
///   the fail-CLOSED [`UnavailableCasHandler`], whose every read/write
///   maps to HTTP 503 until `R2_TDK_HEX` is set, and emits a loud,
///   structured `tracing::error!`.
///
/// On `wasm32-unknown-unknown` (Cloudflare Worker target) a
/// compile-error placeholder is emitted per the
/// `trait-abstraction-defer` rule; the wasm32 binding is out of
/// scope for WP-S1.
///
/// # Panics
///
/// Does not panic. If the R2 client cannot be constructed with creds
/// present (malformed endpoint URL, missing `R2_TDK_HEX`, etc.) an
/// error is logged and the route is served by the fail-CLOSED
/// [`UnavailableCasHandler`] (HTTP 503), never the silent `InMemory`
/// fallback.
#[must_use]
#[allow(
    clippy::type_complexity,
    reason = "builder returns a fixed read/write/delete/list (D-8) handler tuple; a named alias would orphan this function's doc block"
)]
pub fn build_handlers() -> (
    Arc<dyn CasReadHandler>,
    Arc<dyn CasWriteHandler>,
    Arc<dyn CasDeleteHandler>,
    Arc<dyn CasListHandler>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_s3, StorageEnv};

        // Probe for storage credentials.
        if StorageEnv::from_env().is_some() {
            // Credentials are present — try to build the real handler.
            // Empty-or-absent → default (see storage::env_or; mirrors the
            // AC fix for the 2026-06-05 prod incident — latent here).
            let bucket = crate::storage::env_or("R2_CAS_BUCKET", "corelink-cas-prod");
            // F7 (2026-06-13 CAA-360 audit) — CAS residency. The CAS storage
            // region is keyed per-env from `R2_CAS_REGION` (FROZEN CONTRACT),
            // set by `[env.prod-<region>].vars` in `wrangler.toml` and forwarded
            // by the DO `container.start({env})` list (F8). This is what makes a
            // regional env (sam/lhr/nrt/syd) key its CAS objects under its OWN
            // region instead of the US default — closing the Schrems-II / GDPR
            // Art. 44 gap for CAS content. The default `"iad"` applies only to
            // the IAD env; a regional env that lacks the binding silently
            // degrades to IAD, so the binding is asserted by the residency
            // invariant test (`residency_invariant_every_regional_env_sets_cas_region`)
            // that fails the build if any `[env.prod-<r>]` omits `R2_CAS_REGION`.
            let region = crate::storage::env_or("R2_CAS_REGION", "iad");

            // Construction is now sync-only (commit ead0f37a removed the
            // aws_config::defaults() IMDS probe). We keep block_in_place +
            // block_on around the async signature in case future R2 init
            // grows network work; the cost is zero when the inner future
            // resolves immediately.
            let handle = tokio::runtime::Handle::current();
            let built = tokio::task::block_in_place(|| {
                handle.block_on(r2_s3::build_r2_cas_handler_from_env(&bucket, &region))
            });
            match built {
                Some(Ok(handler)) => {
                    tracing::info!(
                        bucket = %bucket,
                        region = %region,
                        "CAS handler: R2S3 (real storage)"
                    );
                    // R2CasHandler implements both CasReadHandler and
                    // CasWriteHandler against the same R2 bucket; share
                    // one Arc behind both trait objects.
                    let shared: Arc<r2_s3::R2CasHandler> = Arc::new(handler);
                    let read: Arc<dyn CasReadHandler> = shared.clone();
                    let write: Arc<dyn CasWriteHandler> = shared.clone();
                    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
                    let list: Arc<dyn CasListHandler> = shared;
                    return (read, write, delete, list);
                }
                Some(Err(e)) => {
                    // F1 (CAA-360) fail-CLOSED + LOUD: storage creds ARE present
                    // (this is the production data plane), but the R2 handler
                    // refused to build — today exactly "R2_TDK_HEX required". We
                    // MUST NOT silently fall back to the non-durable `InMemory`
                    // cache (that is fail-OPEN-ish: a broken cache with no
                    // alarm). Mount the fail-CLOSED `UnavailableCasHandler`
                    // instead: every read/write returns 503 until `R2_TDK_HEX` is
                    // set. Distinct from the creds-ABSENT case below (`None` ⇒
                    // dev/CI `InMemory`, which is fine).
                    tracing::error!(
                        error = %e,
                        bucket = %bucket,
                        region = %region,
                        "CAS handler: R2S3 build REFUSED with storage creds present \
                         (R2_TDK_HEX unset/invalid?) — mounting fail-CLOSED 503 \
                         handler, NOT InMemory (F1 INV-TENANT-ISOLATION)"
                    );
                    let shared: Arc<UnavailableCasHandler> = Arc::new(UnavailableCasHandler);
                    let read: Arc<dyn CasReadHandler> = shared.clone();
                    let write: Arc<dyn CasWriteHandler> = shared.clone();
                    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
                    let list: Arc<dyn CasListHandler> = shared;
                    return (read, write, delete, list);
                }
                None => {
                    // Should not happen: we already checked is_some().
                }
            }
        }

        // Fallback: InMemory — creds ABSENT (unit tests, local dev, CI). This is
        // the dev/CI path; production always has storage creds and takes the R2
        // branch above (which now fails CLOSED on a missing TDK).
        tracing::info!("CAS handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryCasHandler> = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        (read, write, delete, list)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Placeholder for `corelink_handler_cas::cf_worker::CfWorkerCasHandler`
        // (deferred per trait-abstraction-defer; see crate-level docs).
        // Until that impl lands, the wasm32 build does not include
        // this route — the binary entry asserts the cfg at link time.
        compile_error!(
            "wasm32 CF-Worker CAS handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the CAS read + write routes.
///
/// Both routes share the `/v1/cas/:tenant/:hash` template; axum
/// disambiguates by HTTP method (GET vs PUT).
pub fn router(state: CasRouteState) -> Router {
    Router::new()
        .route(
            CAS_READ_ROUTE,
            get(handle_read).put(handle_write).delete(handle_delete),
        )
        .route(CAS_LIST_ROUTE, get(handle_list))
        // Bulk routes. These are SIBLINGS of `/v1/cas/:tenant/:hash`, not
        // captures of it: matchit-0.7 ranks the static `batch` / `batch-read` /
        // `batch-exists` literals ABOVE the `:hash` wildcard, so `POST
        // /v1/cas/t/batch` matches this route and `GET /v1/cas/t/<hash>` still
        // matches the read route (no method collision either — these are POST).
        .route(CAS_BATCH_ROUTE, post(handle_batch_write))
        .route(CAS_BATCH_READ_ROUTE, post(handle_batch_read))
        .route(CAS_BATCH_EXISTS_ROUTE, post(handle_batch_exists))
        .with_state(state)
}

/// `true` if `headers` declares a `Content-Type` whose media type (ignoring any
/// `; charset=…` suffix and ASCII case) is one of `accepted`. Used by the batch
/// routes for the FROZEN 415 gate: a wrong/absent type is rejected BEFORE the
/// body is parsed (the length-framed wire format must not be misread as a
/// generic octet-stream).
fn content_type_is(headers: &axum::http::HeaderMap, accepted: &[&str]) -> bool {
    let ct = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Strip parameters (`application/x-ndjson; charset=utf-8`) and trim.
    let media = ct.split(';').next().unwrap_or("").trim();
    accepted.iter().any(|a| media.eq_ignore_ascii_case(a))
}

/// 413 over-cap body shared by `/batch` and `/batch-read` (FROZEN contract).
fn batch_too_large() -> axum::response::Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(serde_json::json!({
            "error": "batch_too_large",
            "limit_objects": BATCH_MAX_OBJECTS,
            "limit_bytes": BATCH_MAX_BYTES,
        })),
    )
        .into_response()
}

/// One manifest line of a `/batch` upload: a claimed content hash + the exact
/// byte length that follows for that object in the concatenated payload.
#[derive(serde::Deserialize)]
struct BatchUploadManifestEntry {
    /// Claimed blake3 content hash (validated canonical per object).
    hash: String,
    /// Exact byte length of this object in the length-framed payload.
    len: u64,
}

/// One NDJSON request line of `/batch-read` and `/batch-exists`.
#[derive(serde::Deserialize)]
struct BatchHashRequest {
    /// Requested content hash.
    hash: String,
}

/// Split a length-framed batch body into its manifest text and the raw payload
/// bytes at the FIRST blank line (`\n\n`). The manifest is the bytes before the
/// terminating blank line; the payload is everything after it. `None` ⇒ no blank
/// line terminator present (a framing error ⇒ 400 for the whole request).
fn split_manifest(body: &[u8]) -> Option<(&[u8], &[u8])> {
    // The manifest is newline-delimited JSON terminated by a SINGLE blank line.
    // After the last manifest line's `\n` there is one more `\n` (the blank
    // line), so the separator is `\n\n`. An empty manifest (zero objects) is
    // still framed by a leading blank line, i.e. the body begins with `\n`.
    let sep = body.windows(2).position(|w| w == b"\n\n")?;
    // `sep` is the index of the manifest-terminating `\n`; `sep + 1` is the
    // blank line's `\n`. The manifest is `..=sep` (includes the terminating
    // newline) and the payload is everything after the blank line (`sep + 2..`).
    // `position` guarantees `sep + 1 < body.len()`, so `sep + 2 <= body.len()`
    // and both `split_at` indices are in bounds (no panic — clippy-safe).
    let (manifest, rest) = body.split_at(sep + 1);
    let payload = rest.get(1..).unwrap_or(&[]);
    Some((manifest, payload))
}

/// Run the native PAT possession gate (finding #4) when it is wired.
///
/// Reads the bearer PAT from the `Authorization` header and re-verifies it
/// (Argon2id, full Option-B pipeline) against the claimed `tenant`. `Some(resp)`
/// ⇒ REJECT (401 forged/wrong-tenant / 503 verifier fault); `None` ⇒ proceed (or
/// when the gate is absent in dev/CI). Called at the TOP of each billable
/// handler, AFTER the scope+tenant gate, BEFORE storage.
async fn pat_gate_reject(
    state: &CasRouteState,
    tenant: &str,
    headers: &axum::http::HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}

/// `GET /v1/cas/:tenant/:hash` handler.
///
/// The authenticated tenant (DO-injected `x-corelink-tenant-id`,
/// PAT-resolved by the Worker) is bound by the
/// [`crate::auth_tenant::AuthTenant`] extractor and is the SOLE
/// isolation key. The path `:tenant` is a client-controllable echo
/// that MUST match it (mismatch ⇒ 403 before any storage access).
async fn handle_read(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // The path `:tenant` is a client echo that MUST equal the
    // authenticated tenant; mismatch is a cross-tenant attempt and is
    // denied 403 BEFORE any storage access (no tenant quoted in the
    // body). Mirrors bazel_v2 / ac.rs.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key
    // (defense-in-depth alongside the AC gate; reuses the shared validator).
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-READ capability
    // (`cas:rw` or `cas:r`) in the Worker-trusted `x-corelink-scope` header.
    // BEFORE any storage access. Today every prod PAT is `cas:rw` so this is
    // a NO-OP for current traffic; it establishes the gate for tiered tokens.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4 — defense-in-depth): re-verify the
    // bearer PAT (Argon2id) resolves to the claimed tenant, AFTER the scope gate,
    // BEFORE storage. 401 forged/wrong-tenant; 503 verifier fault.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. Over-ceiling ⇒ 402;
    // store/clock fault ⇒ 503 (fail-CLOSED). `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    // 410-Gone tombstone gate (hugit-P2 seam B, WP-B). When a tombstone store is
    // wired, an erased `(tenant, hash)` short-circuits to HTTP 410 Gone — BEFORE
    // the R2 GET, so it is a single keyed D1 lookup off the hot path. An erased
    // artifact MUST return 410 (never 404 "never existed", never 200 resurrect).
    // A lookup-transport error fails CLOSED to 503 (PEN-2/REV-S1): an erased
    // artifact is GDPR/DSR-deleted, and confidentiality of legally-erased data
    // outweighs availability of a live blob during a transient D1 blip — never
    // resurrect (200) erased bytes just because the gate couldn't be consulted.
    // `None` ⇒ no gate wired ⇒ classic 200/404.
    if let Some(tombstones) = state.tombstones.as_ref() {
        match tombstones.is_tombstoned(&auth.0, &hash).await {
            Ok(true) => return (StatusCode::GONE, "erased").into_response(),
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "cas: tombstone gate lookup failed; failing CLOSED (503)");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "tombstone gate unavailable",
                )
                    .into_response();
            }
        }
    }
    // CAA-360 #6: real audit-event timestamp from the production SystemWallClock
    // (was hardcoded `0u64`, which stamped every audit event with epoch 0 and
    // made the audit log un-orderable / un-correlatable).
    let now_ms = SystemWallClock.now_ms();
    let req = CasReadRequest::new(
        auth.0.clone(),
        hash,
        // Principal is filled in by the auth middleware in
        // production; demo wire-up mirrors ac.rs.
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.read.read(req) {
        Ok(resp) => {
            // usage-metering-roi: read HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            (StatusCode::OK, resp.bytes).into_response()
        }
        Err(e) => {
            // usage-metering-roi: a genuine "not found" is a read MISS; every
            // other error is a fault, not a classified cache op — don't count it.
            if matches!(e, CasHandlerError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_err(e)
        }
    }
}

/// `PUT /v1/cas/:tenant/:hash` handler.
///
/// Accepts raw bytes in the body; the client claims the content hash
/// via the URL path. The handler enforces hash equality before
/// committing to durable storage. On success: 201 Created (fresh
/// insert) or 200 OK (idempotent re-write).
async fn handle_write(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (declared AHEAD of
    // `body: Bytes`, so axum runs it BEFORE the body is buffered). 429 on over-cap;
    // the RAII slot releases on return. Mirrors `bazel_v2::BazelPutGuard`.
    _concurrency: CasPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // See `handle_read`: the authenticated tenant is the sole
    // isolation key; the path `:tenant` is a client echo that must
    // match. Deny 403 BEFORE any storage access on mismatch.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key.
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // Scope gate (fail-CLOSED): the PAT must carry a cache-WRITE capability
    // (`cas:rw`) BEFORE any storage access. A read-only (`cas:r`) token is
    // rejected here. NO-OP for current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_read`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = CasWriteRequest::new(
        auth.0.clone(),
        hash,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    )
    // Thread the Worker-resolved per-tier storage cap (server-trusted header)
    // into the reservation so the decorator seeds a FRESH `tenant_storage_state`
    // row with the REAL cap (not the legacy uncapped `0`). Absent ⇒ `None` ⇒
    // fail-CLOSED on an unseeded tenant (never treated as unlimited).
    .with_storage_quota_bytes(crate::byte_accounting::storage_quota_from_headers(&headers));
    // Storage byte accounting (finding #1 / cluster B+C) is enforced INSIDE
    // `state.write` by the [`crate::byte_accounting::AccountingCasHandler`]
    // decorator (wired in `routes::build_with_factory` when D1 is present):
    // reserve→commit→release happens AT the write trait object — the SINGLE
    // chokepoint every CAS write surface (native / Bazel / OCI / adapters)
    // shares. So the cap is enforced atomically BEFORE the R2 PUT, and an
    // over-cap or accounting-fault write surfaces here as a sentinel-tagged
    // `Internal` error that `map_err` maps to 402 / 503. The route no longer
    // accrues (that would double-count through the decorated handler).
    match state.write.write(req) {
        Ok(resp) => {
            // usage-metering-roi: write (both fresh 201 and idempotent 200 are a
            // WRITE op) — fire-and-forget, no await/I/O on the hot path.
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.content_hash).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/cas/:tenant/batch` — bulk write (length-framed upload).
///
/// # Wire format (FROZEN)
///
/// The body is, in order: (a) a MANIFEST of newline-delimited JSON, one line per
/// object `{"hash":"<blake3-64hex>","len":<u64>}` in upload order; (b) a single
/// blank line `\n` terminating the manifest; (c) the concatenated raw object
/// bytes in manifest order, each exactly `len` bytes — there is NO per-object
/// delimiter, the manifest's `len` fields frame the payload.
///
/// # Gate sequence (mirrors `handle_write`)
///
/// Cross-tenant 403 → 415 wrong content-type → write-scope 403 → pat-gate →
/// then **ONE** [`QuotaGate::check_batch`] charge of `n` (the object count) for
/// the whole request. We charge ONCE per batch, NEVER per object: a per-object
/// charge would re-introduce the #318 quota-bypass-by-batching regression in
/// reverse — it would N×-bill a single auth'd request — and the cost model is
/// per-op, so `check_batch(tenant, n)` is the proportional, single-round-trip
/// charge (same shape as `bazel_v2::quota_reject_batch`). No tombstone gate
/// (writes are not tombstone-gated — mirrors `handle_write`).
///
/// # Per-object isolation
///
/// Each object is committed independently via `state.write.write(...)`, which is
/// the SAME content-verify the single PUT relies on (the write handler hashes
/// the bytes and returns `HashMismatch` on a forged claim) — the batch route
/// does not re-implement the hash check, it delegates to the one chokepoint. A
/// per-object failure (hash mismatch, storage fault) yields that object's
/// `status:"error"` with a message; the REST of the batch still commits. We do
/// NOT fail the whole batch on one bad object: bulk git ingest must make
/// forward progress and report the bad object, not lose the good ones.
///
/// # Caps (FROZEN)
///
/// ≤ [`BATCH_MAX_OBJECTS`] objects AND ≤ [`BATCH_MAX_BYTES`] of object bytes,
/// whichever is hit first ⇒ otherwise 413 `batch_too_large`. The byte cap is
/// checked against the manifest's declared `sum(len)` so an over-cap request is
/// rejected before any storage write.
///
/// # Response 200
///
/// JSON array `[{"hash":"…","status":"created|exists|error","error":<msg|null>}]`
/// in manifest order (`created` = fresh write, `exists` = idempotent
/// already-present, `error` = per-object failure).
async fn handle_batch_write(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // finding #2 (HIGH DoS): pre-body per-tenant concurrency reservation (declared
    // AHEAD of `body: Bytes`, so axum runs it BEFORE the up-to-BATCH_MAX_BYTES body
    // is buffered). 429 on over-cap; the RAII slot releases on return. SHARES the
    // same per-tenant CAS_WRITE_CONCURRENCY_LIMIT pool as `handle_write` (keyed on
    // the authenticated tenant) — a tenant's single+batch uploads count together.
    _concurrency: CasPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Cross-tenant: path echo must equal the authenticated tenant (mirrors
    // handle_write) — 403 BEFORE any parse or storage access.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // FROZEN: the length-framed upload is ONLY the batch content-type. A wrong
    // or absent type ⇒ 415 before the body is parsed.
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    // Write-scope gate (fail-CLOSED): a batch upload is a cache WRITE — require
    // `cas:rw` (mirrors handle_write).
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }

    // ── Parse + frame-validate the whole request (any framing error ⇒ 400) ──
    let Some((manifest_bytes, payload)) = split_manifest(&body) else {
        return (StatusCode::BAD_REQUEST, "missing manifest terminator").into_response();
    };
    let manifest_text = match std::str::from_utf8(manifest_bytes) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "manifest not utf-8").into_response(),
    };
    let mut entries: Vec<BatchUploadManifestEntry> = Vec::new();
    for line in manifest_text.lines() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<BatchUploadManifestEntry>(line) {
            Ok(e) => entries.push(e),
            Err(_) => return (StatusCode::BAD_REQUEST, "malformed manifest").into_response(),
        }
    }

    // ── Caps (FROZEN): ≤ N objects AND ≤ B bytes, whichever first ⇒ 413 ──
    let total_len: u64 = entries.iter().map(|e| e.len).sum();
    if entries.len() > BATCH_MAX_OBJECTS || total_len > BATCH_MAX_BYTES as u64 {
        return batch_too_large();
    }
    // Framing: the declared lengths must exactly account for the payload bytes
    // (no slack, no truncation) ⇒ otherwise 400 for the WHOLE request.
    if total_len != payload.len() as u64 {
        return (StatusCode::BAD_REQUEST, "payload length mismatch").into_response();
    }

    // ── ONE quota charge for the whole batch (n = object count) ──
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, entries.len()).await {
            return resp;
        }
    }

    // ── Commit each object independently; per-object errors do not abort ──
    let now_ms = SystemWallClock.now_ms();
    let quota_cap = crate::byte_accounting::storage_quota_from_headers(&headers);
    let mut results = Vec::with_capacity(entries.len());
    let mut offset: usize = 0;
    for entry in &entries {
        let end = offset + entry.len as usize;
        // `sum(entry.len) == payload.len()` was asserted above (framing gate),
        // so `offset..end` is always in bounds; `.get()` keeps it panic-free
        // (clippy `indexing_slicing`) — the `unwrap_or(&[])` is unreachable.
        let slice = payload.get(offset..end).unwrap_or(&[]);
        offset = end;
        // Canonical-hash gate (defense-in-depth, mirrors the single PUT). A
        // non-canonical claim is a per-object error, not a batch abort.
        if !super::ac::is_canonical_digest(&entry.hash) {
            results.push(serde_json::json!({
                "hash": entry.hash, "status": "error", "error": "malformed hash",
            }));
            continue;
        }
        let req = CasWriteRequest::new(
            auth.0.clone(),
            entry.hash.clone(),
            slice.to_vec(),
            format!("anon@{}", auth.0),
            auth.0.clone(),
            now_ms,
        )
        .with_storage_quota_bytes(quota_cap);
        // Content-verify + storage commit happen INSIDE state.write (the same
        // chokepoint the single PUT uses): the handler hashes the bytes and
        // returns HashMismatch on a forged claim; the accounting decorator
        // reserves/commits the bytes. durable=true ⇒ fresh, false ⇒ idempotent.
        match state.write.write(req) {
            Ok(resp) => {
                let status = if resp.durable { "created" } else { "exists" };
                results.push(serde_json::json!({
                    "hash": entry.hash, "status": status, "error": serde_json::Value::Null,
                }));
            }
            Err(e) => {
                let msg = batch_object_error_message(&e);
                results.push(serde_json::json!({
                    "hash": entry.hash, "status": "error", "error": msg,
                }));
            }
        }
    }
    (StatusCode::OK, Json(serde_json::Value::Array(results))).into_response()
}

/// Compact per-object error message for a `/batch` upload failure. Mirrors the
/// `map_err` taxonomy (NotFound/HashMismatch/CrossTenant/Audit/Internal) but
/// keeps a single object's failure OUT of the batch's HTTP status — the request
/// is 200 and the failure is reported in that object's `error` field. We never
/// leak internal storage detail (same discipline as `map_err`).
fn batch_object_error_message(e: &CasHandlerError) -> String {
    match e {
        CasHandlerError::HashMismatch { .. } => "content hash mismatch".to_owned(),
        CasHandlerError::NotFound { .. } => "not found".to_owned(),
        CasHandlerError::CrossTenantDenied { .. } => "cross-tenant".to_owned(),
        CasHandlerError::AuditFailed(_) => "audit closed".to_owned(),
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            "storage quota exceeded".to_owned()
        }
        // F-004 — a batch re-PUT of an erased hash is refused by the shared
        // tombstone gate; report it per-object (the rest of the batch proceeds).
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_GONE_SENTINEL) =>
        {
            "erased".to_owned()
        }
        _ => "internal error".to_owned(),
    }
}

/// `POST /v1/cas/:tenant/batch-read` — bulk read (length-framed download).
///
/// # Request
///
/// NDJSON `{"hash":"<blake3>"}` lines. Accepts [`BATCH_CONTENT_TYPE`] or
/// [`NDJSON_CONTENT_TYPE`] (the read-side body is a plain hash list).
///
/// # Gate sequence
///
/// Cross-tenant 403 → 415 → read-scope 403 → pat-gate → ONE
/// [`QuotaGate::check_batch`] charge of `n` (the hash count). Per hash the
/// tombstone gate is applied FIRST (mirrors `handle_read`: an erased
/// `(tenant, hash)` ⇒ `gone`), then the R2 read.
///
/// # Response 200
///
/// A MANIFEST of `{"hash":"…","len":<u64>,"status":"ok|absent|gone"}` NDJSON
/// lines + a single blank line `\n` + the concatenated raw bytes of the `ok`
/// objects in manifest order. `absent` (404-class) and `gone` (410-class
/// tombstoned) contribute zero bytes.
async fn handle_batch_read(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // brutal-fleet M2 (MED DoS): pre-body per-tenant bulk-read concurrency
    // reservation (declared AHEAD of `body: Bytes`, so axum runs it BEFORE the
    // body is buffered AND before the up-to-BATCH_MAX_BYTES payload accumulator is
    // built). 429 on over-cap; the RAII slot releases on return. SEPARATE pool
    // from the write guard (`CAS_READ_CONCURRENCY_LIMIT`, keyed on the
    // authenticated tenant) — bulk reads don't contend with the tenant's writes.
    _read_concurrency: CasReadConcurrencyGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE, NDJSON_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }

    let hashes = match parse_ndjson_hashes(&body) {
        Ok(h) => h,
        Err(rejection) => return rejection.into_response(),
    };
    // FROZEN cap: object count only on the request side (the response byte
    // volume is bounded by what is actually stored, but the request fan-out is
    // capped here so one request can't drive N unbounded reads behind one
    // charge).
    if hashes.len() > BATCH_MAX_OBJECTS {
        return batch_too_large();
    }

    // ONE quota charge for the whole batch.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, hashes.len()).await {
            return resp;
        }
    }

    let now_ms = SystemWallClock.now_ms();
    // Build the manifest and the payload in lock-step (manifest order). We cap
    // the accumulated payload at BATCH_MAX_BYTES: a tenant could request 2000
    // large blobs whose stored bytes exceed 8 MiB, which would blow the global
    // 10 MiB response budget — so an `ok` object that would push the payload
    // over the cap aborts the WHOLE request 413 (FROZEN: ≤ 8 MiB per batch).
    let mut manifest = String::new();
    let mut payload: Vec<u8> = Vec::new();

    // Per-hash outcome produced by a spawned read task. Carried back across the
    // spawn boundary (all variants are `Send`) and reassembled IN hash order so
    // the manifest/payload framing is byte-identical to the old serial loop.
    enum PerHash {
        /// Non-canonical hash, or a `NotFound` read ⇒ 404-class `absent`.
        Absent,
        /// Tombstoned `(tenant, hash)` ⇒ 410-class `gone`.
        Gone,
        /// Present blob bytes.
        Ok(Vec<u8>),
        /// Tombstone gate lookup faulted ⇒ fail CLOSED (503) for the whole batch.
        TombstoneFault,
        /// Read faulted (non-NotFound) ⇒ propagate via `map_err`.
        ReadErr(CasHandlerError),
    }

    // Fan out, in hash order, with bounded (BATCH_READ_FANOUT-way) concurrency.
    // Each task holds an owned semaphore permit for its whole lifetime, so at
    // most BATCH_READ_FANOUT reads are in flight at once. We MUST use
    // `tokio::spawn` (not `spawn_blocking`): the sync `read()` internally does
    // `tokio::task::block_in_place(..)`, which is only valid on a multi-thread
    // runtime WORKER thread — spawn_blocking threads are not workers and would
    // panic.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(BATCH_READ_FANOUT));
    let mut handles: Vec<tokio::task::JoinHandle<PerHash>> = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        // Acquire the permit BEFORE spawning so the in-flight count is bounded
        // to BATCH_READ_FANOUT (permit is moved into the task and held for its
        // lifetime). The semaphore is never closed here, so `Err` (closed) is
        // unreachable — but we fail CLOSED (500) rather than `.expect()` it
        // (clippy::expect-used is denied repo-wide).
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                return map_err(CasHandlerError::Internal(
                    "batch-read semaphore closed".into(),
                ))
            }
        };
        let read = state.read.clone();
        let tombstones = state.tombstones.clone();
        let tenant = auth.0.clone();
        let hash = hash.clone();
        handles.push(tokio::spawn(async move {
            // Hold the permit for the whole task lifetime.
            let _permit = permit;
            // Non-canonical hash ⇒ it cannot name a stored blob; report absent
            // (it is not a framing error and must not abort the batch).
            if !super::ac::is_canonical_digest(&hash) {
                return PerHash::Absent;
            }
            // Tombstone gate FIRST (mirrors handle_read): an erased blob is
            // `gone`, never resurrected, never reported absent. A lookup fault
            // fails CLOSED for the whole batch (the erase write-side is the
            // source of truth).
            if let Some(tombstones) = tombstones.as_ref() {
                match tombstones.is_tombstoned(&tenant, &hash).await {
                    Ok(true) => return PerHash::Gone,
                    Ok(false) => {}
                    Err(e) => {
                        // PEN-2/REV-S1: fail CLOSED, identical to the single-read
                        // gate (handle_read ~678). A tombstone-lookup transport
                        // error must NOT serve a possibly-erased (GDPR) object —
                        // fail the whole batch rather than risk resurrecting
                        // erased bytes during a D1 blip.
                        tracing::warn!(error = %e, "cas batch-read: tombstone gate lookup failed; failing CLOSED (503)");
                        return PerHash::TombstoneFault;
                    }
                }
            }
            let req = CasReadRequest::new(
                tenant.clone(),
                hash.clone(),
                format!("anon@{tenant}"),
                tenant.clone(),
                now_ms,
            );
            // Sync `read()` — `block_in_place` inside it is valid because this is
            // a multi-thread-runtime worker thread (`tokio::spawn`).
            match read.read(req) {
                Ok(resp) => PerHash::Ok(resp.bytes),
                Err(CasHandlerError::NotFound { .. }) => PerHash::Absent,
                Err(e) => PerHash::ReadErr(e),
            }
        }));
    }

    // Reassemble IN push order (== hash order). This ordering is LOAD-BEARING:
    // the client slices the concatenated payload by the manifest `len`s, so the
    // manifest lines and the payload segments must both follow request order.
    for (hash, handle) in hashes.iter().zip(handles) {
        let outcome = match handle.await {
            Ok(o) => o,
            // A spawned task panicked (or was cancelled) ⇒ internal read
            // failure. Map to the same 500 surface as a generic read error.
            Err(_join_err) => {
                return map_err(CasHandlerError::Internal("batch read task failed".into()));
            }
        };
        match outcome {
            PerHash::Absent => {
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": 0, "status": "absent"})
                ));
            }
            PerHash::Gone => {
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": 0, "status": "gone"})
                ));
            }
            PerHash::Ok(bytes) => {
                if payload.len() + bytes.len() > BATCH_MAX_BYTES {
                    return batch_too_large();
                }
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": bytes.len(), "status": "ok"})
                ));
                payload.extend_from_slice(&bytes);
            }
            PerHash::TombstoneFault => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "tombstone gate unavailable",
                )
                    .into_response();
            }
            PerHash::ReadErr(e) => return map_err(e),
        }
    }
    // Manifest + single blank-line terminator + concatenated payload.
    let mut out = manifest.into_bytes();
    out.push(b'\n');
    out.extend_from_slice(&payload);
    (StatusCode::OK, out).into_response()
}

/// `POST /v1/cas/:tenant/batch-exists` — bulk HEAD-class existence probe.
///
/// # Request
///
/// NDJSON `{"hash":"<blake3>"}` lines (≤ [`BATCH_MAX_OBJECTS`] hashes). Accepts
/// [`BATCH_CONTENT_TYPE`] or [`NDJSON_CONTENT_TYPE`].
///
/// # Probe method
///
/// HEAD-class: this MUST NOT read object bytes. The `CasReadHandler` trait has
/// no dedicated `exists`/HEAD method, so the cheapest correct probe available is
/// a `state.read.read(...)` whose `Ok`/`NotFound` outcome is mapped to
/// `present` — the bytes are discarded and never enter the response. (When a
/// true HEAD probe is added to the handler trait this should switch to it; until
/// then a read-and-discard is the only correct existence signal.) ONE
/// [`QuotaGate::check_batch`] charge of `n`.
///
/// # Response 200
///
/// JSON array `[{"hash":"…","present":<bool>}]` in request order.
async fn handle_batch_exists(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // brutal-fleet M2 (MED DoS): pre-body per-tenant bulk-read concurrency
    // reservation (declared AHEAD of `body: Bytes`). `batch-exists` doesn't
    // accumulate object bytes, but it fans out up to BATCH_MAX_OBJECTS storage
    // existence probes per request, so it shares the same bulk-read concurrency
    // axis as `batch-read`. 429 on over-cap; RAII slot releases on return.
    _read_concurrency: CasReadConcurrencyGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE, NDJSON_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }

    let hashes = match parse_ndjson_hashes(&body) {
        Ok(h) => h,
        Err(rejection) => return rejection.into_response(),
    };
    if hashes.len() > BATCH_MAX_OBJECTS {
        return batch_too_large();
    }

    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, hashes.len()).await {
            return resp;
        }
    }

    let now_ms = SystemWallClock.now_ms();
    let mut results = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        // A non-canonical hash cannot name a stored blob ⇒ present=false (it is
        // not a framing error).
        let present = if !super::ac::is_canonical_digest(hash) {
            false
        } else {
            let req = CasReadRequest::new(
                auth.0.clone(),
                hash.clone(),
                format!("anon@{}", auth.0),
                auth.0.clone(),
                now_ms,
            );
            // HEAD-class existence probe: `CasReadHandler::exists` is overridden
            // by the R2 handler as a single S3 `HeadObject` with NO body fetch
            // (r2_s3.rs) — using `read()` here would fetch every present object's
            // full bytes (tens of GiB of R2 GET egress over a closure-sized probe
            // — the exact anti-pattern the trait's `exists` default warns about),
            // defeating the whole point of a cheap dedup probe. `Ok(true)` ⇒
            // present; `Ok(false)`/`Err` ⇒ false (conservative — never claim a
            // blob is present on a storage fault; the caller re-uploads, which is
            // idempotent).
            matches!(state.read.exists(req), Ok(true))
        };
        results.push(serde_json::json!({ "hash": hash, "present": present }));
    }
    (StatusCode::OK, Json(serde_json::Value::Array(results))).into_response()
}

/// Parse an NDJSON `{"hash":"…"}` request body into an ordered list of hashes.
/// Blank lines are skipped; any malformed line ⇒ `Err((400, msg))` for the whole
/// request (a framing error — same discipline as the upload manifest). The error
/// is the small `(StatusCode, &str)` tuple (not a built `Response`) so the
/// `Result` stays cheap (clippy `result_large_err`); the caller turns it into a
/// response with `.into_response()`.
fn parse_ndjson_hashes(body: &[u8]) -> Result<Vec<String>, (StatusCode, &'static str)> {
    let text = std::str::from_utf8(body)
        .map_err(|_| (StatusCode::BAD_REQUEST, "request not utf-8"))?;
    let mut hashes = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<BatchHashRequest>(line) {
            Ok(r) => hashes.push(r.hash),
            Err(_) => return Err((StatusCode::BAD_REQUEST, "malformed request line")),
        }
    }
    Ok(hashes)
}

/// `DELETE /v1/cas/:tenant/:hash` handler — delete a blob (D-8).
///
/// Idempotent: returns 204 No Content whether the blob existed or not.
/// Requires a WRITE-capable PAT (mirrors `handle_write`).
async fn handle_delete(
    State(state): State<CasRouteState>,
    Path((tenant, hash)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access (mirrors read).
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed CAS hash BEFORE it derives an R2 key.
    if !super::ac::is_canonical_digest(&hash) {
        return (StatusCode::BAD_REQUEST, "malformed hash").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT may NEVER
    // delete — a stolen per-job credential must not be able to EVICT the
    // tenant's cache. Denied BEFORE the scope gate (the Worker forwards the
    // job's write scope, so the write bit alone would let it through).
    if runner_job.is_runner_job() {
        return (
            StatusCode::FORBIDDEN,
            "delete not permitted for a runner-job credential",
        )
            .into_response();
    }
    // Scope gate (fail-CLOSED): delete is a cache WRITE — require `cas:rw`.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = CasDeleteRequest::new(
        auth.0.clone(),
        hash,
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    );
    // Storage byte accounting (finding #1 / cluster-C): the reclaimed bytes are
    // RELEASED inside `state.delete` by the
    // [`crate::byte_accounting::AccountingCasHandler`] decorator (it reads the
    // size-bearing `CasDeleteResponse::reclaimed_bytes` the R2 delete handler now
    // populates via a pre-delete HEAD) so `bytes_used` drops. Idempotent: 204 No
    // Content for both deleted-existing and absent.
    match state.delete.delete(req) {
        Ok(resp) => {
            let _ = resp.existed;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/cas/:tenant` handler — paginated blob enumeration (D-8).
///
/// Requires a READ-capable PAT. Returns
/// `{ "blobs": [...], "next_cursor": <opaque|null> }`.
async fn handle_list(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // Scope gate (fail-CLOSED): list is a cache READ — require read cap.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Length-cap the opaque continuation cursor: a clean 400 reject rather than
    // letting an over-length value ride the edge query limit into the backend.
    if q.cursor.as_deref().is_some_and(|c| c.len() > MAX_CURSOR_LEN) {
        return (StatusCode::BAD_REQUEST, "cursor too long").into_response();
    }
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = CasListRequest::new(
        auth.0.clone(),
        format!("anon@{}", auth.0),
        auth.0,
        clamp_limit(q.limit),
        q.cursor,
        now_ms,
    );
    match state.list.list(req) {
        Ok(resp) => {
            let blobs: Vec<_> = resp
                .blobs
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "hash": b.hash,
                        "size": b.size,
                        "created_at": b.created_at,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "blobs": blobs,
                    "next_cursor": resp.next_cursor,
                })),
            )
                .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Map a [`CasHandlerError`] to the canonical HTTP response.
fn map_err(e: CasHandlerError) -> axum::response::Response {
    // Log the full error before mapping so operators retain a
    // diagnostic record (R2/S3 detail) without exposing internals to
    // the caller.
    tracing::warn!(error = ?e, "CAS handler error");
    match e {
        CasHandlerError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        CasHandlerError::HashMismatch { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "content hash mismatch").into_response()
        }
        CasHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CasHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit writes without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // F1 (CAA-360) fail-CLOSED: the route was mounted with storage creds
        // present but the R2 handler refused to build (R2_TDK_HEX unset/invalid),
        // so it is serving the `UnavailableCasHandler`. Surface that as 503
        // "storage unavailable" — the cache is unavailable until R2_TDK_HEX is
        // set, NOT a generic 500. See [`UnavailableCasHandler`].
        CasHandlerError::Internal(ref msg) if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL) => {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        // Storage byte-accounting decorator (finding #1 / cluster B+C): an
        // over-cap reservation ⇒ 402 (the tenant is over its storage cap); an
        // accounting-backend fault ⇒ 503 fail-CLOSED. Both ride a sentinel-tagged
        // `Internal` from `AccountingCasHandler` so they are distinct from a
        // generic 500.
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response()
        }
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "storage accounting unavailable").into_response()
        }
        // F-004 — the shared tombstone gate (`TombstoneGatedCasHandler`): a
        // write that re-PUTs an erased `(tenant, hash)` is refused 410 Gone (an
        // erased artifact must never be resurrected at the same content address),
        // and a gate-lookup transport fault fails CLOSED 503 (never serve/commit
        // when the erasure gate is unconsultable). These ride sentinel-tagged
        // `Internal` errors from the gate decorator so they are distinct from a
        // generic 500.
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_GONE_SENTINEL) =>
        {
            (StatusCode::GONE, "erased").into_response()
        }
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "tombstone gate unavailable").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
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
    use corelink_handler_cas::handler::fake_hash;

    /// `map_err` must map an `Internal` error to 503 ONLY when it carries the
    /// storage-unavailable sentinel; any other `Internal` is a generic 500.
    /// Kills the cargo-mutants "replace match guard with true" mutant on the
    /// `if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL)` guard (mirrors the
    /// ac.rs test) — with the guard forced to `true`, the non-sentinel case
    /// below would wrongly become 503.
    #[test]
    fn map_err_internal_is_503_only_for_storage_sentinel() {
        let storage = map_err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2_TDK_HEX unset"
        )));
        assert_eq!(
            storage.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "sentinel-tagged Internal must be 503"
        );

        let generic = map_err(CasHandlerError::Internal(
            "lock poisoned: unrelated failure".to_string(),
        ));
        assert_eq!(
            generic.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "non-sentinel Internal must be 500, not 503 (guard must not be `true`)"
        );
    }

    fn fixture() -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    /// Fixture with a tombstone store pre-seeded with `(tenant, hash)` so the
    /// read gate returns 410 Gone (WP-B).
    fn fixture_with_tombstone(tenant: &str, hash: &str) -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let store = Arc::new(crate::routes::cas_erase::InMemoryTombstoneStore::new());
        store.seed(tenant, hash);
        let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> = store;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: Some(tombstones),
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    /// Route state backed by the fail-CLOSED [`UnavailableCasHandler`] — the
    /// shape `build_handlers` returns when storage creds are present but the R2
    /// handler refuses to build (F1: `R2_TDK_HEX` unset/invalid).
    fn fixture_unavailable() -> CasRouteState {
        let shared = Arc::new(UnavailableCasHandler);
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(CAS_READ_ROUTE, "/v1/cas/{tenant}/{hash}");
        assert_eq!(CAS_WRITE_ROUTE, "/v1/cas/{tenant}/{hash}");
    }

    /// F1 (CAA-360) fail-CLOSED: the `UnavailableCasHandler`'s error maps to
    /// HTTP 503 "storage unavailable" (NOT the generic 500) so a forgotten
    /// `R2_TDK_HEX` is loud, not a silent non-durable cache.
    #[test]
    fn unavailable_handler_maps_to_503() {
        let err = UnavailableCasHandler
            .read(CasReadRequest::new("t1", "h", "anon@t1", "t1", 0))
            .expect_err("unavailable");
        assert!(matches!(err, CasHandlerError::Internal(_)));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn route_constant_uses_matchit_0_8_brace_syntax_not_colon() {
        // DEBT-029-cas regression net (post axum-0.8): matchit 0.8 parses
        // `{name}` as the capture and treats the legacy `:name` form as
        // LITERAL path bytes. Any reintroduction of `:name` would silently
        // route every real request to a router-level 404. Pin both the
        // absence of `:` and the presence of `{tenant}` + `{hash}`.
        assert!(
            !CAS_READ_ROUTE.contains(':'),
            "CAS_READ_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
        );
        assert!(CAS_READ_ROUTE.contains("{tenant}"));
        assert!(CAS_READ_ROUTE.contains("{hash}"));
    }

    /// F7 (2026-06-13 CAA-360 audit) — CAS data-residency invariant.
    ///
    /// Every non-IAD regional prod env (`[env.prod-sam|lhr|nrt|syd]`) MUST set
    /// `R2_CAS_REGION` in its `.vars` so the container keys that region's CAS
    /// objects under its OWN region instead of silently defaulting to the US
    /// `"iad"` (the Schrems-II / GDPR Art. 44 gap). This test fails the build if
    /// any regional env omits the binding — the fail-CLOSED, fail-LOUD guard
    /// that stops a future env-block edit from re-opening the residency hole.
    ///
    /// We assert the binding *exists* per region (the F7 contract). The bucket
    /// stays `corelink-cas-prod` until per-region CAS buckets are provisioned
    /// (infra follow-up), so we do not assert the bucket here.
    #[test]
    fn residency_invariant_every_regional_env_sets_cas_region() {
        // wrangler.toml lives at the repo root; this crate is at
        // crates/corelink-container, so climb two parents.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let wrangler_path = std::path::Path::new(manifest_dir)
            .join("..")
            .join("..")
            .join("wrangler.toml");
        let toml = std::fs::read_to_string(&wrangler_path).unwrap_or_else(|e| {
            panic!("cannot read {}: {e}", wrangler_path.display());
        });

        // Each non-IAD regional env must declare its CAS region. The IAD env is
        // exempt: its `R2_CAS_REGION` default ("iad") is the residency-correct
        // value, so an absent binding there is not a cross-border leak.
        for region in ["sam", "lhr", "nrt", "syd"] {
            let header = format!("[env.prod-{region}.vars]");
            let start = toml.find(&header).unwrap_or_else(|| {
                panic!("wrangler.toml missing `{header}` env block");
            });
            // Bound the search to this env block: from its header to the next
            // top-level `[` section after the header line.
            let after_header = start + header.len();
            let block_end = toml[after_header..]
                .find("\n[")
                .map_or(toml.len(), |rel| after_header + rel);
            let block = &toml[start..block_end];
            // Every residency-bearing key space must be pinned to this region.
            // CAS was the original F7 invariant; AC objects (which embed output
            // digests + command metadata) and chunk objects are equally
            // residency-bearing, and key under R2_AC_REGION / R2_CHUNK_REGION
            // (both default to "iad" in cas.rs/ac.rs `env_or`). A regional env
            // that drops ANY of the three silently keys that space under the US
            // default — the same cross-border leak the CAS check guards. Assert
            // all three so the build-time guard covers the full key surface.
            for var in ["R2_CAS_REGION", "R2_AC_REGION", "R2_CHUNK_REGION"] {
                let expected = format!("{var} = \"{region}\"");
                assert!(
                    block.contains(&expected),
                    "[env.prod-{region}] must set `{expected}` (residency \
                     invariant): a regional env without {var} keys that object \
                     space under the US default \"iad\" — a cross-border leak. \
                     Add the binding to wrangler.toml."
                );
            }
        }
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        // Smoke test the build_handlers shape on the native target.
        let (read, _write, _delete, _list) = build_handlers();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        // We don't have a seed entry point on the trait alone, so
        // we drive the read against a known-empty handler and assert
        // it returns NotFound. The full wire-up is exercised via
        // the handler-crate's own unit tests.
        let res = read.read(CasReadRequest::new("t1", hash, "anon", "t1", 0));
        assert!(matches!(res, Err(CasHandlerError::NotFound { .. })));
        // Build state to satisfy the router constructor.
        let _router = router(fixture());
    }

    /// PUT then GET round-trip via the route state drives both trait
    /// objects against the shared InMemory backing store — pins that
    /// the new write trait object writes to the SAME backing store the
    /// read trait object reads from (regression net for a future
    /// refactor that accidentally splits the backing store).
    #[test]
    fn put_then_get_round_trip_through_route_state() {
        let st = fixture();
        let bytes = b"corelink-cas-put-then-get".to_vec();
        let hash = fake_hash(&bytes);

        // PUT
        let upd = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        let upd_resp = st.write.write(upd).expect("write");
        assert!(upd_resp.durable, "fresh insert must be durable=true");

        // GET — must return the same bytes
        let rd = CasReadRequest::new("t1", hash.clone(), "anon@t1", "t1", 2);
        let rd_resp = st.read.read(rd).expect("read hit");
        assert_eq!(rd_resp.bytes, bytes);
        assert_eq!(rd_resp.content_hash, hash);
    }

    /// Idempotent retry on the write path: a second PUT with the same
    /// bytes returns durable=false (mirrors AC + handler-crate
    /// semantics).
    #[test]
    fn idempotent_write_retry_returns_durable_false() {
        let st = fixture();
        let bytes = b"r".to_vec();
        let hash = fake_hash(&bytes);
        let req1 = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
        assert!(st.write.write(req1).expect("first").durable);
        let req2 = CasWriteRequest::new("t1", hash, bytes, "anon@t1", "t1", 2);
        assert!(!st.write.write(req2).expect("retry").durable);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Authenticated tenant injected via `x-corelink-tenant-id` so the
    /// `AuthTenant` extractor admits the request; the scope header then
    /// gates read vs write.
    const TEST_TENANT: &str = "t1";

    /// A `cas:rw` PUT writes successfully (current prod scope — happy path).
    #[tokio::test]
    async fn put_with_rw_scope_succeeds() {
        let app = router(fixture());
        let bytes = b"cas-scope-rw".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // ── cf-multitenant WP5b: runner-job PAT deny-DELETE ──────────────────────

    /// A canonical 64-lowercase-hex CAS hash for the runner-job DELETE tests.
    const WP5B_HASH: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";

    /// runner-job marker present ⇒ CAS DELETE is denied 403 even with a
    /// write-capable scope (a per-job credential must not evict the cache).
    #[tokio::test]
    async fn runner_job_cas_delete_returns_403() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"delete not permitted for a runner-job credential");
    }

    /// runner-job + wildcard key (`*`) ⇒ CAS DELETE still denied (deny-DELETE is
    /// unconditional for a runner-job; the key pin only governs AC writes).
    #[tokio::test]
    async fn runner_job_cas_delete_wildcard_still_403() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// NO runner-job marker (normal PAT): CAS DELETE with a write scope behaves
    /// EXACTLY as before — 204 (idempotent), proving the WP5b check is a no-op.
    #[tokio::test]
    async fn no_runner_job_marker_cas_delete_is_204() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "normal delete unchanged");
    }

    /// A present-but-non-`"1"` marker is NOT a runner-job: CAS DELETE behaves as
    /// a normal write (204), proving the exact-`"1"` rule at the route.
    #[tokio::test]
    async fn runner_job_cas_marker_non_one_is_not_narrowed() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "true")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "marker \"true\" ⇒ not narrowed");
    }

    /// cluster F: with the tenant AT `CAS_WRITE_CONCURRENCY_LIMIT` in-flight
    /// writes, the next CAS write is rejected 429 BEFORE its body is buffered —
    /// the `CasPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
    /// Proven deterministically by sending a body LARGER than the 10 MiB global
    /// limit: if the body were buffered first, the body-limit layer would reject
    /// it; because the concurrency guard runs first, we get 429 and the oversized
    /// body is never read.
    #[tokio::test]
    async fn cas_write_at_concurrency_limit_returns_429_before_body() {
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), CAS_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]); // > 10 MiB
        let hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit native CAS write must be rejected 429 by the pre-body concurrency guard"
        );
    }

    /// A write BELOW the limit succeeds and releases its slot (counter back to 0).
    #[tokio::test]
    async fn cas_write_below_limit_releases_slot() {
        let state = fixture();
        let app = router(state.clone());
        let bytes = b"cas-slot-release".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let g = state.put_inflight.lock().expect("lock");
        assert_eq!(
            g.get(TEST_TENANT),
            None,
            "the concurrency slot must be released after the write completes"
        );
    }

    /// A `cas:r` (read-only) PUT is rejected 403 "insufficient scope"
    /// BEFORE storage.
    #[tokio::test]
    async fn put_with_read_only_scope_returns_403_insufficient_scope() {
        let app = router(fixture());
        let bytes = b"cas-scope-ro".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    /// F1 (CAA-360) end-to-end: a route mounted on the fail-CLOSED
    /// `UnavailableCasHandler` (creds present, `R2_TDK_HEX` missing) returns
    /// HTTP 503 for an in-scope GET — the cache refuses to serve, not a silent
    /// 200/404 from a non-durable InMemory fallback.
    #[tokio::test]
    async fn get_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// F1 (CAA-360) end-to-end: an in-scope PUT against the fail-CLOSED
    /// `UnavailableCasHandler` returns 503 — writes are refused (never a silent
    /// non-durable commit) until `R2_TDK_HEX` is set.
    #[tokio::test]
    async fn put_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let bytes = b"cas-unavailable".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A `cas:r` GET is allowed (read-only token reads). The artifact is
    /// absent so the route returns 404 — proving the scope gate PASSED
    /// (a denied scope would 403 before storage).
    #[tokio::test]
    async fn get_with_read_only_scope_passes_gate_then_404() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Fail-CLOSED: a GET with NO scope header is rejected 403.
    #[tokio::test]
    async fn get_with_missing_scope_returns_403() {
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_malformed_hash_returns_400() {
        // CAA-360 #9: a non-canonical :hash is rejected with 400 BEFORE storage,
        // even WITH valid auth + scope (the digest gate runs before the scope
        // gate). Covers the gate against a mutant that disables/inverts it.
        let app = router(fixture());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/not-a-canonical-hash"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── 410-Gone tombstone read gate (hugit-P2 seam B, WP-B) ─────────────────

    /// A GET for an ERASED `(tenant, hash)` returns HTTP 410 Gone — NOT 404,
    /// NOT 200. The tombstone gate runs after the scope gate and before the R2
    /// read.
    #[tokio::test]
    async fn get_erased_hash_returns_410_gone() {
        const ERASED: &str = "1111111111111111111111111111111111111111111111111111111111111111";
        let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/{ERASED}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::GONE);
    }

    /// With a tombstone store wired, a NON-erased hash still 404s (the gate is
    /// per-hash, not a blanket block).
    #[tokio::test]
    async fn get_non_erased_hash_with_store_still_404() {
        let app = router(fixture_with_tombstone(TEST_TENANT, "some-other-hash"));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Tombstone store whose `is_tombstoned` always errors — simulates a
    /// transient D1 transport fault on the read gate.
    #[derive(Debug, Default)]
    struct ErroringTombstoneStore;
    #[async_trait::async_trait]
    impl crate::routes::cas_erase::TombstoneStore for ErroringTombstoneStore {
        async fn is_tombstoned(&self, _tenant: &str, _digest: &str) -> Result<bool, String> {
            Err("d1 transport fault".to_owned())
        }
        async fn upsert(
            &self,
            _tenant: &str,
            _digest: &str,
            _reason: &str,
            _erased_at_ms: i64,
        ) -> Result<bool, String> {
            Ok(false)
        }
        async fn list_tenant_tombstones(&self, _tenant: &str) -> Result<Vec<String>, String> {
            Err("d1 transport fault".to_owned())
        }
    }

    /// Route state whose tombstone gate always errors (D1 fault).
    fn fixture_erroring_tombstone() -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> =
            Arc::new(ErroringTombstoneStore);
        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: Some(tombstones),
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    /// PEN-2 / REV-S1 regression: when the tombstone (GDPR-erasure) gate lookup
    /// ERRORS (transient D1 fault), the read fails CLOSED with 503 — it must
    /// NEVER fall through to the R2 read and risk resurrecting an erased blob.
    #[tokio::test]
    async fn get_with_tombstone_gate_error_fails_closed_503() {
        const HASH: &str = "2222222222222222222222222222222222222222222222222222222222222222";
        let app = router(fixture_erroring_tombstone());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/{HASH}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ── per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) ─────────

    /// Fixture whose state carries a `QuotaGate` backed by an in-memory quota
    /// store seeded so the NEXT billable op trips the ceiling. The flat per-op
    /// cost is `1` micro-USD; the seeded tenant is already AT its budget, so any
    /// charge projects over the cap.
    fn fixture_over_ceiling(tenant: &str) -> CasRouteState {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;

        let store = InMemoryQuotaStore::new();
        store.seed(
            tenant,
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 5_000_000, // already at the $5 cap
                cycle_anchor_ms: 1_700_000_000_000,
            },
        );
        let store: Arc<dyn QuotaStore> = Arc::new(store);
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        let gate = crate::routes::QuotaGate::new_for_test(guard, 1);

        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: Some(gate),
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    /// Once accrued spend has reached the monthly ceiling, a billable CAS read
    /// is rejected with HTTP 402 Payment Required — BEFORE storage. This is the
    /// load-bearing wiring assertion: the previously-dead `QuotaGuard` is now
    /// mounted and trips on a real HTTP request.
    #[tokio::test]
    async fn billable_request_over_ceiling_returns_402() {
        let app = router(fixture_over_ceiling(TEST_TENANT));
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: with room under the ceiling the same request passes the gate
    /// and proceeds to storage (404 for the absent blob — NOT 402). Proves the
    /// gate is not a blanket block.
    #[tokio::test]
    async fn billable_request_under_ceiling_proceeds() {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        // Empty store ⇒ fresh tenant at the $5 tripwire with 0 accrued.
        let store: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: Some(crate::routes::QuotaGate::new_for_test(guard, 1_000)),
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        };
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── finding #1: storage byte accounting (cap enforcement) ────────────────

    /// Fixture whose `write`/`delete` trait objects are wrapped in the
    /// [`AccountingCasHandler`] decorator over an in-memory byte store seeded AT
    /// a tiny cap, so the next write trips the cap (mirrors the production wiring
    /// in `routes::build_with_factory`).
    fn fixture_over_storage_cap(tenant: &str) -> CasRouteState {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, testing::Row, AccountingCasHandler, ByteAccountant,
            ByteStore,
        };

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let raw_write: Arc<dyn CasWriteHandler> = shared.clone();
        let raw_delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;

        let store = Arc::new(InMemoryByteStore::new());
        // Cap of 4 bytes, already at 4 ⇒ ANY further byte is over-cap.
        store.seed(tenant, "iad", Row { used: 4, quota: 4 });
        let store_dyn: Arc<dyn ByteStore> = store;
        let acc = Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()));
        let acct = Arc::new(AccountingCasHandler::new(raw_write, raw_delete, acc));
        let write: Arc<dyn CasWriteHandler> = acct.clone();
        let delete: Arc<dyn CasDeleteHandler> = acct;

        CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }

    /// The KILLING finding-#1 test: with the storage counter AT the cap, a PUT
    /// that would store new bytes is rejected 402 — the previously-inert storage
    /// cap now trips on a real HTTP write.
    // multi_thread: the `AccountingCasHandler` decorator bridges the async
    // accountant to the sync write trait with `block_in_place`, which requires a
    // multi-thread runtime (matches production `#[tokio::main]`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_over_storage_cap_returns_402() {
        let app = router(fixture_over_storage_cap(TEST_TENANT));
        let bytes = b"more-bytes-than-the-cap-allows".to_vec();
        let hash = fake_hash(&bytes);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(bytes))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: an uncapped tenant's PUT accrues its bytes and succeeds (201) —
    /// proving the counter MOVES (it never did before finding #1) and the gate
    /// is not a blanket block.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_under_storage_cap_accrues_and_succeeds() {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, AccountingCasHandler, ByteAccountant, ByteStore,
        };

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let raw_write: Arc<dyn CasWriteHandler> = shared.clone();
        let raw_delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let store = Arc::new(InMemoryByteStore::new()); // empty ⇒ uncapped fresh row
        let store_dyn: Arc<dyn ByteStore> = store.clone();
        let acct = Arc::new(AccountingCasHandler::new(
            raw_write,
            raw_delete,
            Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned())),
        ));
        let write: Arc<dyn CasWriteHandler> = acct.clone();
        let delete: Arc<dyn CasDeleteHandler> = acct;
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: None,
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        };
        let app = router(st);
        let body = b"hello-cas".to_vec();
        let body_len = body.len() as i64;
        let hash = fake_hash(&body);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            // The Worker injects the resolved per-tier storage cap; a fresh row
            // (empty store) is seeded from it. Without it the reservation fails
            // CLOSED (no cap ⇒ 503) — see byte_accounting::storage_quota_from_headers.
            .header(crate::byte_accounting::STORAGE_QUOTA_HEADER, "1000000")
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(
            store.used(TEST_TENANT, "iad"),
            body_len,
            "a durable write must accrue its bytes into the storage counter"
        );
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A request with the PAT gate wired but NO bearer Authorization header is
    /// rejected 401 — the native gate fails CLOSED on a missing token even when
    /// the Worker-set tenant header is present (defense-in-depth).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use crate::adapter_pat::PatRow;
        use corelink_pat::PatSigningKey;

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
        let verifier = verifier_with_row(
            "no-such-token".to_owned(),
            PatRow {
                tenant_id: TEST_TENANT.to_owned(),
                pat_hash: String::new(),
                scope: "cas:rw".to_owned(),
            },
            key,
        );
        let st = CasRouteState {
            read,
            write,
            delete,
            list,
            tombstones: None,
            quota: None,
            pat_gate: Some(Arc::new(NativePatGate::new_for_test(verifier))),
            put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        };
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            // No Authorization header — the gate must reject.
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── bulk CAS batch endpoints (hugit-P2: git-ingest fast path) ────────────

    /// Build a length-framed `/batch` upload body from `(hash, bytes)` pairs:
    /// the NDJSON manifest, a blank-line terminator, then the concatenated raw
    /// bytes — the FROZEN wire format.
    fn build_batch_upload(objs: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (hash, bytes) in objs {
            out.extend_from_slice(
                format!("{}\n", serde_json::json!({"hash": hash, "len": bytes.len()})).as_bytes(),
            );
        }
        out.push(b'\n'); // blank-line terminator
        for (_h, bytes) in objs {
            out.extend_from_slice(bytes);
        }
        out
    }

    /// (a) Happy path: a 3-object batch upload returns 200 with every object
    /// `created`.
    #[tokio::test]
    async fn batch_upload_all_created() {
        let app = router(fixture());
        let objs: Vec<(String, Vec<u8>)> = (0u8..3)
            .map(|i| {
                let b = vec![i; (i as usize) + 1];
                (fake_hash(&b), b)
            })
            .collect();
        let body = build_batch_upload(&objs);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(arr.len(), 3);
        for entry in &arr {
            assert_eq!(entry["status"], "created");
            assert!(entry["error"].is_null());
        }
    }

    /// finding #2 (HIGH DoS) on the BULK path: with the tenant AT
    /// `CAS_WRITE_CONCURRENCY_LIMIT` in-flight writes, the next `/batch` upload
    /// is rejected 429 BEFORE its body is buffered — the `_concurrency:
    /// CasPutGuard` `FromRequestParts` extractor on `handle_batch_write` runs
    /// ahead of `body: Bytes` and SHARES the same per-tenant pool as the single
    /// PUT (so single+batch uploads count together). Proven deterministically by
    /// sending a body LARGER than the limit: if the guard were missing, the body
    /// would be buffered and the request would 413 (batch byte-cap / body-limit)
    /// or otherwise parse — anything but 429. So a 429 here is a witness that the
    /// pre-body concurrency reservation ran first; DELETING the `CasPutGuard`
    /// extractor from `handle_batch_write` flips this to 413 and FAILS the test.
    #[tokio::test]
    async fn batch_write_at_concurrency_limit_returns_429_before_body() {
        let state = fixture();
        {
            let mut g = state.put_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), CAS_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        // > 10 MiB: if the guard is absent the body-limit/byte-cap layer rejects
        // it (413), never 429. Body content is irrelevant — the guard runs in
        // `FromRequestParts`, before the body is read.
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit /batch upload must be rejected 429 by the pre-body concurrency guard \
             (the CasPutGuard extractor on handle_batch_write)"
        );
    }

    /// Companion to the at-limit case: a `/batch` upload BELOW the limit succeeds
    /// and RELEASES its shared concurrency slot (counter back to 0). Together with
    /// the 429 test this pins that `handle_batch_write` both reserves AND releases
    /// the same per-tenant pool — a leaked slot would wedge the tenant's writes.
    #[tokio::test]
    async fn batch_write_below_limit_releases_slot() {
        let state = fixture();
        let app = router(state.clone());
        let b = b"batch-slot-release".to_vec();
        let body = build_batch_upload(&[(fake_hash(&b), b)]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let g = state.put_inflight.lock().expect("lock");
        assert_eq!(
            g.get(TEST_TENANT),
            None,
            "the batch upload's concurrency slot must be released after it completes"
        );
    }

    /// brutal-fleet M2 (MED DoS) on the BULK-READ path: with the tenant AT
    /// `CAS_READ_CONCURRENCY_LIMIT` in-flight bulk reads, the next `/batch-read`
    /// is rejected 429 BEFORE its body is buffered — the `_read_concurrency:
    /// CasReadConcurrencyGuard` `FromRequestParts` extractor on `handle_batch_read`
    /// runs ahead of `body: Bytes`. Proven deterministically by sending a body
    /// LARGER than the 10 MiB body limit: if the guard were missing, the body
    /// would be buffered and the request would 413 (body-limit) — anything but
    /// 429. So a 429 here is a witness that the pre-body read reservation ran
    /// first; DELETING the extractor flips this to 413 and FAILS the test. The
    /// reservation is on the SEPARATE `read_inflight` pool — pre-loading
    /// `put_inflight` would NOT trip it (asserted implicitly: the fixture's write
    /// pool stays empty).
    #[tokio::test]
    async fn batch_read_at_concurrency_limit_returns_429_before_body() {
        let state = fixture();
        {
            let mut g = state.read_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), CAS_READ_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        // > 10 MiB: absent the guard the body-limit layer rejects it (413), never
        // 429. Body content is irrelevant — the guard runs in `FromRequestParts`.
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit /batch-read must be rejected 429 by the pre-body read-concurrency guard \
             (the CasReadConcurrencyGuard extractor on handle_batch_read)"
        );
    }

    /// Same guard covers the sibling `/batch-exists` route (it shares the
    /// `CasReadConcurrencyGuard` read pool): at the cap ⇒ 429 before the body.
    #[tokio::test]
    async fn batch_exists_at_concurrency_limit_returns_429_before_body() {
        let state = fixture();
        {
            let mut g = state.read_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), CAS_READ_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit /batch-exists must be rejected 429 by the pre-body read-concurrency guard"
        );
    }

    /// Companion: a `/batch-read` BELOW the limit succeeds and RELEASES its read
    /// slot (read pool back to empty), pinning the RAII release on the read axis.
    #[tokio::test]
    async fn batch_read_below_limit_releases_slot() {
        let state = fixture();
        let app = router(state.clone());
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(format!(
                "{}\n",
                serde_json::json!({ "hash": fake_hash(b"absent-blob") })
            )))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let g = state.read_inflight.lock().expect("lock");
        assert_eq!(
            g.get(TEST_TENANT),
            None,
            "the batch-read's read-concurrency slot must be released after it completes"
        );
    }

    /// (b) Re-upload of already-present objects returns `exists` (idempotent).
    #[tokio::test]
    async fn batch_reupload_returns_exists() {
        let st = fixture();
        let b = b"reupload-me".to_vec();
        let objs = vec![(fake_hash(&b), b)];
        let body = build_batch_upload(&objs);
        // First upload.
        let req1 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body.clone()))
            .expect("request");
        let r1 = router(st.clone()).oneshot(req1).await.expect("oneshot");
        assert_eq!(r1.status(), StatusCode::OK);
        // Second upload — same bytes ⇒ exists.
        let req2 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let r2 = router(st).oneshot(req2).await.expect("oneshot");
        let bytes = axum::body::to_bytes(r2.into_body(), usize::MAX)
            .await
            .expect("body");
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(arr[0]["status"], "exists");
    }

    /// (c) One object with WRONG bytes (claimed hash != actual) is reported
    /// `error`; the OTHER objects in the same batch still `created`.
    #[tokio::test]
    async fn batch_upload_one_bad_object_others_commit() {
        let app = router(fixture());
        let good = b"good-object".to_vec();
        let bad_bytes = b"actual-bytes".to_vec();
        // Claim a hash that does not match bad_bytes ⇒ HashMismatch in the
        // write handler. fake_hash of DIFFERENT bytes gives a wrong claim of the
        // correct len, so the framing still holds but the content-verify fails.
        let wrong_claim = fake_hash(b"different");
        let objs = vec![
            (fake_hash(&good), good.clone()),
            (wrong_claim, bad_bytes.clone()),
        ];
        // Hand-build so the manifest len matches the ACTUAL payload bytes (the
        // framing is correct; only the per-object content-hash is wrong).
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "{}\n",
                serde_json::json!({"hash": fake_hash(&good), "len": good.len()})
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "{}\n",
                serde_json::json!({"hash": fake_hash(b"different"), "len": bad_bytes.len()})
            )
            .as_bytes(),
        );
        body.push(b'\n');
        body.extend_from_slice(&good);
        body.extend_from_slice(&bad_bytes);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK, "one bad object must NOT 4xx the whole batch");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(arr[0]["status"], "created");
        assert_eq!(arr[1]["status"], "error");
        assert_eq!(arr[1]["error"], "content hash mismatch");
        let _ = objs; // keep the descriptive binding alive for readers
    }

    /// (d1) Over-cap on object COUNT (2001 objects) ⇒ 413 batch_too_large.
    #[tokio::test]
    async fn batch_upload_over_object_cap_returns_413() {
        let app = router(fixture());
        // 2001 zero-length objects: framing trivially holds (empty payload), so
        // the ONLY thing that can trip is the object-count cap.
        let mut manifest = String::new();
        let zero_hash = "0".repeat(64);
        for _ in 0..(BATCH_MAX_OBJECTS + 1) {
            manifest.push_str(&format!(
                "{}\n",
                serde_json::json!({"hash": zero_hash, "len": 0})
            ));
        }
        let mut body = manifest.into_bytes();
        body.push(b'\n'); // terminator; empty payload follows
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["error"], "batch_too_large");
        assert_eq!(v["limit_objects"], BATCH_MAX_OBJECTS);
        assert_eq!(v["limit_bytes"], BATCH_MAX_BYTES);
    }

    /// (d2) Over-cap on declared BYTES (> 8 MiB sum(len)) ⇒ 413. We assert on
    /// the manifest's declared length so we never need to ship 8 MiB of body.
    #[tokio::test]
    async fn batch_upload_over_byte_cap_returns_413() {
        let app = router(fixture());
        // Two objects whose DECLARED lengths sum just over 8 MiB. The byte-cap
        // check runs on sum(len) BEFORE the payload-length framing check, so an
        // over-cap request is rejected without shipping the bytes.
        let zero_hash = "0".repeat(64);
        let half = (BATCH_MAX_BYTES / 2) as u64 + 1;
        let mut body = Vec::new();
        for _ in 0..2 {
            body.extend_from_slice(
                format!("{}\n", serde_json::json!({"hash": zero_hash, "len": half})).as_bytes(),
            );
        }
        body.push(b'\n');
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// (e) Wrong Content-Type ⇒ 415 BEFORE the body is parsed.
    #[tokio::test]
    async fn batch_upload_wrong_content_type_returns_415() {
        let app = router(fixture());
        let b = b"x".to_vec();
        let body = build_batch_upload(&[(fake_hash(&b), b)]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    /// (f) Framing mismatch: declared sum(len) != payload bytes ⇒ 400 for the
    /// whole request.
    #[tokio::test]
    async fn batch_upload_framing_mismatch_returns_400() {
        let app = router(fixture());
        let bytes = b"five!".to_vec(); // 5 bytes
        // Declare len=4 (one short) ⇒ sum(len) != payload.len().
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("{}\n", serde_json::json!({"hash": fake_hash(&bytes), "len": 4})).as_bytes(),
        );
        body.push(b'\n');
        body.extend_from_slice(&bytes);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// (i) Cross-tenant (path tenant != auth tenant) ⇒ 403 BEFORE any parse.
    #[tokio::test]
    async fn batch_upload_cross_tenant_returns_403() {
        let app = router(fixture());
        let b = b"x".to_vec();
        let body = build_batch_upload(&[(fake_hash(&b), b)]);
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/cas/other-tenant/batch")
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A read-only (`cas:r`) batch upload is rejected 403 (write scope required).
    #[tokio::test]
    async fn batch_upload_read_only_scope_returns_403() {
        let app = router(fixture());
        let b = b"x".to_vec();
        let body = build_batch_upload(&[(fake_hash(&b), b)]);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// (g) batch-read round-trip: seed two blobs, request three hashes (two
    /// present + one absent), assert the manifest statuses and that the
    /// concatenated payload is exactly the present blobs' bytes in order.
    #[tokio::test]
    async fn batch_read_round_trip_with_absent() {
        let st = fixture();
        let a = b"first-blob".to_vec();
        let bb = b"second-blob-longer".to_vec();
        let ha = fake_hash(&a);
        let hb = fake_hash(&bb);
        // Seed via the write trait object (the shared backing store).
        st.write
            .write(CasWriteRequest::new(
                TEST_TENANT,
                ha.clone(),
                a.clone(),
                "anon@t1",
                TEST_TENANT,
                1,
            ))
            .expect("seed a");
        st.write
            .write(CasWriteRequest::new(
                TEST_TENANT,
                hb.clone(),
                bb.clone(),
                "anon@t1",
                TEST_TENANT,
                2,
            ))
            .expect("seed b");
        let missing = "0".repeat(64);
        let ndjson = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"hash": ha}),
            serde_json::json!({"hash": missing}),
            serde_json::json!({"hash": hb}),
        );
        let app = router(st);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(ndjson))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        // Split manifest / payload at the blank-line terminator.
        let (manifest, payload) = split_manifest(&bytes).expect("framed response");
        let manifest = std::str::from_utf8(manifest).expect("utf8");
        let lines: Vec<&str> = manifest.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);
        let l0: serde_json::Value = serde_json::from_str(lines[0]).expect("l0");
        let l1: serde_json::Value = serde_json::from_str(lines[1]).expect("l1");
        let l2: serde_json::Value = serde_json::from_str(lines[2]).expect("l2");
        assert_eq!(l0["status"], "ok");
        assert_eq!(l1["status"], "absent");
        assert_eq!(l2["status"], "ok");
        // Payload = a || bb (present objects, in manifest order).
        let mut expected = a.clone();
        expected.extend_from_slice(&bb);
        assert_eq!(payload, expected.as_slice());
    }

    /// Parallel-fan-out invariant: a large mixed batch (present + absent,
    /// interleaved) must come back in EXACT request order despite the reads now
    /// running concurrently (BATCH_READ_FANOUT-way). Asserts, per hash: the
    /// manifest line order == request order, the reported `len`s, the per-hash
    /// status, and that the concatenated payload slices back to the right bytes
    /// for each present object. multi_thread so the sync `read()`'s
    /// `block_in_place` (in prod) is exercised on a real worker thread.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn batch_read_parallel_preserves_order_and_slicing() {
        let st = fixture();
        // 30 hashes: even index = present (seeded, unique bytes), odd = absent.
        // Present bytes deliberately vary in length so a mis-ordered slice would
        // corrupt the reassembly and fail the per-hash byte assertion.
        let n = 30usize;
        let mut req_hashes: Vec<String> = Vec::with_capacity(n);
        let mut expected_present: Vec<Option<Vec<u8>>> = Vec::with_capacity(n);
        for i in 0..n {
            if i % 2 == 0 {
                let bytes = format!("blob-{i}-{}", "x".repeat(i)).into_bytes();
                let h = fake_hash(&bytes);
                st.write
                    .write(CasWriteRequest::new(
                        TEST_TENANT,
                        h.clone(),
                        bytes.clone(),
                        "anon@t1",
                        TEST_TENANT,
                        i as u64 + 1,
                    ))
                    .expect("seed present blob");
                req_hashes.push(h);
                expected_present.push(Some(bytes));
            } else {
                // Distinct absent hash per slot (canonical but never stored).
                let h = fake_hash(format!("never-stored-{i}").as_bytes());
                req_hashes.push(h);
                expected_present.push(None);
            }
        }
        let ndjson: String = req_hashes
            .iter()
            .map(|h| format!("{}\n", serde_json::json!({ "hash": h })))
            .collect();

        let app = router(st);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(ndjson))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let (manifest, payload) = split_manifest(&body).expect("framed response");
        let manifest = std::str::from_utf8(manifest).expect("utf8");
        let lines: Vec<&str> = manifest.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), n, "one manifest line per requested hash");

        // Walk manifest + payload in lock-step, asserting request order, per-hash
        // status/len, and that each present segment slices back to its bytes.
        let mut cursor = 0usize;
        for (i, line) in lines.iter().enumerate() {
            let v: serde_json::Value = serde_json::from_str(line).expect("manifest json");
            assert_eq!(
                v["hash"],
                serde_json::Value::String(req_hashes[i].clone()),
                "manifest line {i} must be the i-th requested hash (order preserved)"
            );
            match &expected_present[i] {
                Some(bytes) => {
                    assert_eq!(v["status"], "ok", "line {i} should be present");
                    let len = v["len"].as_u64().expect("len") as usize;
                    assert_eq!(len, bytes.len(), "line {i} reported len mismatch");
                    assert_eq!(
                        &payload[cursor..cursor + len],
                        bytes.as_slice(),
                        "payload segment {i} must slice back to the seeded bytes"
                    );
                    cursor += len;
                }
                None => {
                    assert_eq!(v["status"], "absent", "line {i} should be absent");
                    assert_eq!(v["len"].as_u64(), Some(0), "absent line {i} has len 0");
                }
            }
        }
        assert_eq!(
            cursor,
            payload.len(),
            "the payload is exactly the concatenation of the present segments"
        );
    }

    /// batch-read reports a tombstoned hash as `gone` (mirrors handle_read's
    /// 410 gate), not `absent`.
    #[tokio::test]
    async fn batch_read_tombstoned_is_gone() {
        const ERASED: &str = "1111111111111111111111111111111111111111111111111111111111111111";
        let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
        let ndjson = format!("{}\n", serde_json::json!({"hash": ERASED}));
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(ndjson))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let (manifest, _payload) = split_manifest(&bytes).expect("framed");
        let line = std::str::from_utf8(manifest)
            .expect("utf8")
            .lines()
            .next()
            .expect("one line");
        let v: serde_json::Value = serde_json::from_str(line).expect("json");
        assert_eq!(v["status"], "gone");
    }

    /// (h) batch-exists reports present/absent correctly (HEAD-class).
    #[tokio::test]
    async fn batch_exists_present_and_absent() {
        let st = fixture();
        let a = b"exists-me".to_vec();
        let ha = fake_hash(&a);
        st.write
            .write(CasWriteRequest::new(
                TEST_TENANT,
                ha.clone(),
                a.clone(),
                "anon@t1",
                TEST_TENANT,
                1,
            ))
            .expect("seed");
        let missing = "0".repeat(64);
        let ndjson = format!(
            "{}\n{}\n",
            serde_json::json!({"hash": ha}),
            serde_json::json!({"hash": missing}),
        );
        let app = router(st);
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(ndjson))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["present"], true);
        assert_eq!(arr[1]["present"], false);
    }

    /// batch-exists over the 2000-hash cap ⇒ 413.
    #[tokio::test]
    async fn batch_exists_over_cap_returns_413() {
        let app = router(fixture());
        let zero_hash = "0".repeat(64);
        let mut body = String::new();
        for _ in 0..(BATCH_MAX_OBJECTS + 1) {
            body.push_str(&format!("{}\n", serde_json::json!({"hash": zero_hash})));
        }
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// The batch quota charge is ONE call of `n` (the object count), NOT one per
    /// object. We seed a quota store with EXACTLY enough budget for n charges at
    /// cost-per-op=1 and assert a 3-object batch passes; a per-object loop that
    /// also added an extra implicit charge, or a wrong `n`, would mis-bill. The
    /// companion assertion: with budget for only n-1, the SAME batch trips 402.
    #[tokio::test]
    async fn batch_upload_charges_quota_once_for_n() {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        fn state_with_budget(budget_micros: i64) -> CasRouteState {
            let audit = Arc::new(InMemoryAuditSink::new());
            let sli = Arc::new(InMemorySliObserver::new());
            let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
            let read: Arc<dyn CasReadHandler> = shared.clone();
            let write: Arc<dyn CasWriteHandler> = shared.clone();
            let delete: Arc<dyn CasDeleteHandler> = shared.clone();
            let list: Arc<dyn CasListHandler> = shared;
            let store = InMemoryQuotaStore::new();
            store.seed(
                TEST_TENANT,
                QuotaState {
                    monthly_budget_usd_micros: budget_micros,
                    accrued_usd_micros: 0,
                    cycle_anchor_ms: 1_700_000_000_000,
                },
            );
            let store: Arc<dyn QuotaStore> = Arc::new(store);
            let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
            let guard = Arc::new(QuotaGuard::new(store, clock));
            CasRouteState::new(
                read,
                write,
                delete,
                list,
                None,
                Some(crate::routes::QuotaGate::new_for_test(guard, 1)),
                None,
            )
        }

        let objs: Vec<(String, Vec<u8>)> = (0u8..3)
            .map(|i| {
                let b = vec![i + 1; (i as usize) + 1];
                (fake_hash(&b), b)
            })
            .collect();
        let body = build_batch_upload(&objs);

        // Budget for exactly 3 charges (cost=1 each) ⇒ the single check_batch(_,3)
        // fits ⇒ 200.
        let req_ok = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body.clone()))
            .expect("request");
        let r_ok = router(state_with_budget(3))
            .oneshot(req_ok)
            .await
            .expect("oneshot");
        assert_eq!(r_ok.status(), StatusCode::OK, "budget for n=3 must admit the batch");

        // Budget for only 2 ⇒ a single check_batch(_,3) over-projects ⇒ 402.
        let req_402 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
            .body(Body::from(body))
            .expect("request");
        let r_402 = router(state_with_budget(2))
            .oneshot(req_402)
            .await
            .expect("oneshot");
        assert_eq!(
            r_402.status(),
            StatusCode::PAYMENT_REQUIRED,
            "the batch charge must be n (=3); budget for 2 must trip 402"
        );
    }
}

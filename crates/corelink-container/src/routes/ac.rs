//! `GET /v1/ac/:tenant/:action_digest` + `PUT /v1/ac/:tenant/:action_digest`
//! — Action Cache (AC) read + update routes wired against
//! [`corelink_handler_ac::AcLookupHandler`] and
//! [`corelink_handler_ac::AcUpdateHandler`].
//!
//! This is the wave-11 end-to-end wire-up for the AC half of the
//! R-prep handler-crate skeleton (wave-8). The route shape mirrors
//! the cas-read wire-up exactly: the route stores `Arc<dyn ...Handler>`
//! trait objects so the in-memory fake can be swapped for the wasm32
//! CF-Worker R2-bound handler without touching the route layer.
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn AcLookupHandler>` +
//! `Arc<dyn AcUpdateHandler>` so the binary-shape stays stable as
//! the in-memory fakes are swapped for the wasm32 CF-Worker impl.
//! In production both trait objects point at the same underlying
//! handler instance (the `InMemoryAcHandler` implements both
//! halves), but they are kept distinct in the state shape so the
//! AC-read and AC-update SLO emit sites can be backed by
//! independent collaborators if/when needed.
//!
//! # SLO emit
//!
//! Each route entry emits the AC handler's
//! `Sli::AvailAcLookup` (+ `Sli::LatencyAcHitP99` on the lookup path)
//! through the `SliObserver` collaborator. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-AC` and
//! `SLO-LAT-AC-HIT-P99` from "Sli emitted at handler layer" to
//! "Sli emitted at handler layer **and** routed end-to-end via
//! apps/server".
//!
//! # Cross-tenant denial
//!
//! Cross-tenant attempts are rejected with HTTP 403 + an
//! `AuditEventKind::LookupDenied` / `UpdateDenied` row emitted
//! BEFORE the response. This pins `INV-TENANT-ISOLATION` at the
//! route boundary on top of the handler-layer enforcement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use crate::wall_clock::{SystemWallClock, WallClock};
use corelink_handler_ac::{
    AcDeleteHandler, AcDeleteRequest, AcDeleteResponse, AcHandlerError, AcListHandler,
    AcListRequest, AcListResponse, AcLookupHandler, AcLookupRequest, AcLookupResponse,
    AcUpdateHandler, AcUpdateRequest, AcUpdateResponse, InMemoryAcHandler, InMemoryAuditSink,
    InMemorySliObserver,
};

/// Canonical AC list route path — `GET /v1/ac/:tenant` (D-7).
pub const AC_LIST_ROUTE: &str = "/v1/ac/:tenant";

/// Default page size for the AC list route when `?limit` is absent.
const DEFAULT_LIST_LIMIT: u32 = 200;
/// Hard cap on the AC list page size (contract: `1..=1000`).
const MAX_LIST_LIMIT: u32 = 1000;

/// Query parameters for the paginated AC list route (`?limit=&cursor=`).
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

/// Canonical AC lookup route path (axum-0.7 / matchit-0.7 `:name` captures).
///
/// DEBT-029 (2026-05-16): previously declared with `{tenant}/{action_digest}`
/// which is matchit-0.8+ syntax and would have panicked at `Router::new()`
/// against the workspace-pinned axum 0.7 / matchit 0.7. Fixed by replacing
/// brace placeholders with the `:name` form used by every other live
/// route (see `admin_pilot.rs`, `signup.rs`).
pub const AC_LOOKUP_ROUTE: &str = "/v1/ac/:tenant/:action_digest";

/// Canonical AC update route path. The update path reuses the
/// same template; axum disambiguates by HTTP method.
pub const AC_UPDATE_ROUTE: &str = "/v1/ac/:tenant/:action_digest";

/// Shared route state — distinct trait objects for read and update.
#[derive(Clone)]
pub struct AcRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see
    /// [`build_handlers`].
    pub lookup: Arc<dyn AcLookupHandler>,
    /// Update handler (separate trait object — see crate-level docs).
    pub update: Arc<dyn AcUpdateHandler>,
    /// Delete handler (D-1) — write-capable. In production this points at
    /// the same shared handler instance as `lookup`/`update`.
    pub delete: Arc<dyn AcDeleteHandler>,
    /// List handler (D-7) — read-capable paginated ref enumeration.
    pub list: Arc<dyn AcListHandler>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed); checked at the TOP of each handler,
    /// AFTER the scope gate, BEFORE storage. `None` in dev/CI (not enforced).
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production; re-runs the full Argon2id Option-B verify
    /// at the TOP of each billable handler, AFTER scope+tenant, BEFORE storage.
    /// `None` in dev/CI (skipped). See [`crate::native_pat_gate`].
    pub pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    /// Per-tenant in-flight AC write concurrency counter (cluster F — the native
    /// AC update path buffers the full body before any gate and had NO per-tenant
    /// concurrency cap). Mirrors the Bazel/CAS `put_inflight` guard: a
    /// `FromRequestParts` extractor ([`AcPutGuard`]) declared AHEAD of `body: Bytes`
    /// increments this BEFORE the body is buffered and rejects the over-cap PUT
    /// 429; the RAII [`AcPutSlot`] releases on return.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    // Storage byte accounting (finding #1 / cluster B+C) is enforced INSIDE the
    // `update`/`delete` trait objects above by the
    // [`crate::byte_accounting::AccountingAcHandler`] decorator (wired in
    // `routes::build_with_factory`) — shared with the Bazel AC write surface.
}

impl AcRouteState {
    /// Construct an [`AcRouteState`] from its public collaborators, initializing
    /// the crate-private per-tenant in-flight write counter ([`Self::put_inflight`])
    /// to empty. Supported constructor for callers OUTSIDE the crate (integration
    /// smoke tests) that cannot name the `pub(crate)` field.
    #[must_use]
    pub fn new(
        lookup: Arc<dyn AcLookupHandler>,
        update: Arc<dyn AcUpdateHandler>,
        delete: Arc<dyn AcDeleteHandler>,
        list: Arc<dyn AcListHandler>,
        quota: Option<crate::routes::QuotaGate>,
        pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    ) -> Self {
        Self {
            lookup,
            update,
            delete,
            list,
            quota,
            pat_gate,
            put_inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl core::fmt::Debug for AcRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AcRouteState").finish_non_exhaustive()
    }
}

/// Maximum concurrent in-flight native AC writes for a single tenant. Excess
/// writes are rejected 429 BEFORE the body is buffered; mirrors
/// [`super::cas::CAS_WRITE_CONCURRENCY_LIMIT`].
pub const AC_WRITE_CONCURRENCY_LIMIT: usize = 8;

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `auth_tenant::AuthTenant`'s sentinel set so the pre-body write guard
/// fails CLOSED on the same non-authenticated values.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant in the
/// pre-body write guard. Does not leak which condition tripped.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// RAII release of one per-tenant in-flight AC write slot (decrements on every
/// return path — success, error, panic). See [`AcPutGuard`].
pub(crate) struct AcPutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for AcPutSlot {
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

/// `FromRequestParts` extractor reserving a per-tenant in-flight AC write slot
/// BEFORE the body is buffered (cluster F — pre-buffer OOM guard). Declared ahead
/// of `body: Bytes` in `handle_update` so axum 0.7 runs it first: a tenant already
/// at [`AC_WRITE_CONCURRENCY_LIMIT`] is rejected 429 with no body read. Mirrors
/// `bazel_v2::BazelPutGuard` exactly.
pub(crate) struct AcPutGuard {
    _slot: AcPutSlot,
}

#[axum::async_trait]
impl FromRequestParts<AcRouteState> for AcPutGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AcRouteState,
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
                    tracing::error!(tenant_id = %tenant_key, error = %e, "ac write concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= AC_WRITE_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = AC_WRITE_CONCURRENCY_LIMIT, "ac write concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent uploads").into_response(),
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: AcPutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

/// Sentinel carried in [`AcHandlerError::Internal`] by
/// [`UnavailableAcHandler`] so [`map_err`] can map the storage-unavailable
/// condition to **503** (rather than the generic 500 the `Internal` wildcard
/// arm yields). Mirrors `routes::cas`'s sentinel exactly: the handler-error
/// enum lives in a sibling crate and is `#[non_exhaustive]` without a dedicated
/// `Unavailable` variant, so we thread the distinction through `Internal`.
const STORAGE_UNAVAILABLE_SENTINEL: &str = "storage-unavailable: ";

/// Fail-CLOSED stand-in handler mounted in place of `InMemoryAcHandler` when
/// storage credentials ARE present but the R2 handler refused to build —
/// today exactly the "R2_TDK_HEX required" case (F1, CAA-360). Mirrors
/// `routes::cas::UnavailableCasHandler`.
///
/// A silent `InMemory` fallback there would serve a NON-durable cache with no
/// alarm (fail-OPEN-ish). Instead every lookup/update through this handler
/// fails CLOSED + LOUD: it returns [`AcHandlerError::Internal`] carrying the
/// [`STORAGE_UNAVAILABLE_SENTINEL`], which [`map_err`] maps to HTTP 503 — the
/// route is unavailable until `R2_TDK_HEX` is set. Distinct from the
/// creds-ABSENT path (`None` ⇒ dev/CI `InMemory`, which is fine).
#[derive(Debug)]
struct UnavailableAcHandler;

impl AcLookupHandler for UnavailableAcHandler {
    fn lookup(&self, _req: AcLookupRequest) -> Result<AcLookupResponse, AcHandlerError> {
        Err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 AC handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl AcUpdateHandler for UnavailableAcHandler {
    fn update(&self, _req: AcUpdateRequest) -> Result<AcUpdateResponse, AcHandlerError> {
        Err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 AC handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl AcDeleteHandler for UnavailableAcHandler {
    fn delete(&self, _req: AcDeleteRequest) -> Result<AcDeleteResponse, AcHandlerError> {
        Err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 AC handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl AcListHandler for UnavailableAcHandler {
    fn list(&self, _req: AcListRequest) -> Result<AcListResponse, AcHandlerError> {
        Err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 AC handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

/// Build the canonical `(Arc<dyn AcLookupHandler>, Arc<dyn AcUpdateHandler>)`
/// pair for the current build target.
///
/// # Runtime selection (WP-S1 Phase 1)
///
/// On native targets this returns one shared `InMemoryAcHandler`
/// instance behind both trait objects. A real R2/D1-backed AC handler
/// is scheduled for a follow-on WP; this function logs whether storage
/// credentials are configured so the operator can verify the env is
/// correct even before the real handler lands.
///
/// When storage credentials ARE present but the R2 handler refuses to
/// build (today exactly the "`R2_TDK_HEX` required" case — F1, CAA-360)
/// the function does NOT silently degrade to `InMemoryAcHandler` (that
/// would serve a non-durable cache with no alarm). It instead mounts the
/// fail-CLOSED [`UnavailableAcHandler`], whose every lookup/update maps to
/// HTTP 503 until `R2_TDK_HEX` is set, and emits a loud, structured
/// `tracing::error!`.
///
/// The wasm32 CF-Worker impl is deferred per the autonomous-execution
/// charter `trait-abstraction-defer` rule (tracked as
/// `WI-S04-CF-WIRING`).
#[must_use]
#[allow(
    clippy::type_complexity,
    reason = "builder returns a fixed lookup/update/delete/list (D-1/D-7) handler tuple; a named alias would orphan this function's doc block"
)]
pub fn build_handlers() -> (
    Arc<dyn AcLookupHandler>,
    Arc<dyn AcUpdateHandler>,
    Arc<dyn AcDeleteHandler>,
    Arc<dyn AcListHandler>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_s3, StorageEnv};

        // Probe for storage credentials. When present, construct the
        // real R2-backed handler; otherwise fall back to InMemory.
        // Mirrors `routes::cas::build_handler` exactly (see
        // crate-level docs + commit 832c7884 for the
        // `block_in_place` rationale).
        if StorageEnv::from_env().is_some() {
            // Empty-or-absent → default (the DO forwards `?? ""`;
            // see storage::env_or — prod AC-500 incident 2026-06-05).
            let bucket = crate::storage::env_or("R2_AC_BUCKET", "corelink-ac-iad");
            let region = crate::storage::env_or("R2_AC_REGION", "iad");

            // `routes::build_with_factory` is called from inside
            // `#[tokio::main]`, so a bare
            // `Handle::current().block_on(...)` panics with "Cannot
            // start a runtime from within a runtime". Wrap with
            // `tokio::task::block_in_place` (multi-thread runtime
            // only — `#[tokio::main]` guarantees that).
            let handle = tokio::runtime::Handle::current();
            let built = tokio::task::block_in_place(|| {
                handle.block_on(r2_s3::build_r2_ac_handler_from_env(&bucket, &region))
            });
            match built {
                Some(Ok(handler)) => {
                    tracing::info!(
                        bucket = %bucket,
                        region = %region,
                        "AC handler: R2S3 (real storage)"
                    );
                    let shared: Arc<r2_s3::R2AcHandler> = Arc::new(handler);
                    let lookup: Arc<dyn AcLookupHandler> = shared.clone();
                    let update: Arc<dyn AcUpdateHandler> = shared.clone();
                    let delete: Arc<dyn AcDeleteHandler> = shared.clone();
                    let list: Arc<dyn AcListHandler> = shared;
                    return (lookup, update, delete, list);
                }
                Some(Err(e)) => {
                    // F1 (CAA-360) fail-CLOSED + LOUD: storage creds ARE present
                    // (this is the production data plane), but the R2 handler
                    // refused to build — today exactly "R2_TDK_HEX required". We
                    // MUST NOT silently fall back to the non-durable `InMemory`
                    // cache (fail-OPEN-ish: a broken cache with no alarm). Mount
                    // the fail-CLOSED `UnavailableAcHandler` instead: every
                    // lookup/update returns 503 until `R2_TDK_HEX` is set.
                    // Distinct from the creds-ABSENT case below (`None` ⇒ dev/CI
                    // `InMemory`, which is fine). Mirrors `routes::cas`.
                    tracing::error!(
                        error = %e,
                        bucket = %bucket,
                        region = %region,
                        "AC handler: R2S3 build REFUSED with storage creds present \
                         (R2_TDK_HEX unset/invalid?) — mounting fail-CLOSED 503 \
                         handler, NOT InMemory (F1 INV-TENANT-ISOLATION)"
                    );
                    let shared: Arc<UnavailableAcHandler> = Arc::new(UnavailableAcHandler);
                    let lookup: Arc<dyn AcLookupHandler> = shared.clone();
                    let update: Arc<dyn AcUpdateHandler> = shared.clone();
                    let delete: Arc<dyn AcDeleteHandler> = shared.clone();
                    let list: Arc<dyn AcListHandler> = shared;
                    return (lookup, update, delete, list);
                }
                None => {
                    // Should not happen: we already checked is_some().
                }
            }
        }

        // Fallback: InMemory — creds ABSENT (unit tests, local dev, CI). This is
        // the dev/CI path; production always has storage creds and takes the R2
        // branch above (which now fails CLOSED on a missing TDK).
        tracing::info!("AC handler: InMemory (no storage credentials configured)");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryAcHandler> = Arc::new(InMemoryAcHandler::new(audit, sli));
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared.clone();
        let delete: Arc<dyn AcDeleteHandler> = shared.clone();
        let list: Arc<dyn AcListHandler> = shared;
        (lookup, update, delete, list)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Placeholder for `corelink_handler_ac::cf_worker::CfWorkerAcHandler`
        // (deferred per trait-abstraction-defer; see crate-level docs).
        compile_error!(
            "wasm32 CF-Worker AC handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Build the axum `Router` exposing the AC lookup + update routes.
pub fn router(state: AcRouteState) -> Router {
    Router::new()
        .route(
            AC_LOOKUP_ROUTE,
            get(handle_lookup).put(handle_update).delete(handle_delete),
        )
        .route(AC_LIST_ROUTE, get(handle_list_refs))
        .with_state(state)
}

/// CAA-360 #9: a canonical action/content digest is EXACTLY 64 lowercase hex
/// chars (BLAKE3-256 / SHA-256). The `:action_digest` / `:hash` path segment is
/// used to derive the R2 object key, so it MUST be validated BEFORE storage —
/// a malformed, oversized, or non-hex segment is rejected with 400 and never
/// reaches the storage layer (defense-in-depth alongside the axum single-segment
/// route, which already prevents `/`-based path traversal). Lowercase-only keeps
/// the content-addressing key canonical (no case-variant key collisions).
pub(crate) fn is_canonical_digest(d: &str) -> bool {
    d.len() == 64 && d.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

/// Run the native PAT possession gate (finding #4) when it is wired.
///
/// Reads the bearer PAT from the `Authorization` header and re-verifies it
/// (Argon2id, full Option-B pipeline) against the claimed `tenant`. `Some(resp)`
/// ⇒ REJECT (401 forged/wrong-tenant / 503 verifier fault); `None` ⇒ proceed (or
/// when the gate is absent in dev/CI).
async fn pat_gate_reject(
    state: &AcRouteState,
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

/// `GET /v1/ac/:tenant/:action_digest` handler — lookup.
async fn handle_lookup(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // The authenticated tenant (DO-injected `x-corelink-tenant-id`,
    // PAT-resolved by the Worker) is the SOLE isolation key. The path
    // `:tenant` is a client-controllable echo that MUST match it;
    // mismatch is a cross-tenant attempt and is denied 403 BEFORE any
    // storage access (no tenant quoted in the body). Mirrors bazel_v2.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // Scope gate (fail-CLOSED): AC lookup is a cache READ — require
    // `cas:rw` or `cas:r`. BEFORE any storage access. NO-OP for `cas:rw`.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. 402 over-ceiling /
    // 503 fail-CLOSED. `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    // CAA-360 #6: real audit-event timestamp from the production SystemWallClock
    // (was hardcoded `0u64`; see cas.rs for the same fix).
    let now_ms = SystemWallClock.now_ms();
    let req = AcLookupRequest::new(
        auth.0.clone(),
        action_digest,
        // Principal is filled by an auth middleware in production;
        // demo wire-up mirrors cas.rs.
        format!("anon@{}", auth.0),
        auth.0,
        now_ms,
    );
    match state.lookup.lookup(req) {
        Ok(resp) => (StatusCode::OK, resp.result_payload).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v1/ac/:tenant/:action_digest` handler — update.
#[allow(
    clippy::too_many_arguments,
    reason = "axum extractors are one-arg-each by design; a config struct would defeat FromRequestParts"
)]
async fn handle_update(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: axum::http::HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (declared AHEAD of
    // `body: Bytes`, so axum runs it BEFORE the body is buffered). 429 on over-cap;
    // the RAII slot releases on return. Mirrors `bazel_v2::BazelPutGuard`.
    _concurrency: AcPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // See `handle_lookup`: the authenticated tenant is the sole
    // isolation key; the path `:tenant` is a client echo that must
    // match. Deny 403 BEFORE any storage access on mismatch.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // Scope gate (fail-CLOSED): AC update is a cache WRITE — require
    // `cas:rw`. A read-only (`cas:r`) token is rejected here. NO-OP for
    // current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT with a pinned
    // AC key may write ONLY that exact key — a stolen per-job credential must not
    // write results outside the job it was minted for. A `"*"` pin (the launch
    // default) or no pin ⇒ no key restriction (create allowed at any key; overwrite
    // still 409 by INV-AC-RESULT-HASH-IMMUTABLE; delete still denied separately).
    if !runner_job.ac_key_allowed(&action_digest) {
        return (
            StatusCode::FORBIDDEN,
            "ac write outside the job's allowed key",
        )
            .into_response();
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_lookup`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = AcUpdateRequest::new(
        auth.0.clone(),
        action_digest,
        body.to_vec(),
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    )
    // Thread the Worker-resolved per-tier storage cap so the decorator seeds a
    // FRESH `tenant_storage_state` row with the REAL cap (not uncapped `0`).
    // Absent ⇒ `None` ⇒ fail-CLOSED on an unseeded tenant.
    .with_storage_quota_bytes(crate::byte_accounting::storage_quota_from_headers(&headers));
    // Storage byte accounting (finding #1 / cluster B+C) is enforced INSIDE
    // `state.update` by the [`crate::byte_accounting::AccountingAcHandler`]
    // decorator (reserve→commit→release at the AC update trait object, shared
    // with the Bazel AC write surface). An over-cap / accounting-fault write
    // surfaces here as a sentinel-tagged `Internal` error mapped to 402 / 503.
    match state.update.update(req) {
        Ok(resp) => {
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.action_digest).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `DELETE /v1/ac/:tenant/:action_digest` handler — delete a ref (D-1).
///
/// Idempotent: returns 204 No Content whether the ref existed or not.
/// Requires a WRITE-capable PAT (mirrors `handle_update`).
async fn handle_delete(
    State(state): State<AcRouteState>,
    Path((tenant, action_digest)): Path<(String, String)>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Cross-tenant: deny 403 BEFORE any storage access (mirrors lookup).
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // CAA-360 #9: reject a malformed action_digest BEFORE it derives an R2 key.
    if !is_canonical_digest(&action_digest) {
        return (StatusCode::BAD_REQUEST, "malformed action_digest").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): a narrowed runner-job PAT may NEVER
    // delete an AC ref — a stolen per-job credential must not be able to EVICT
    // the tenant's action cache. Denied BEFORE the scope gate (the Worker
    // forwards the job's write scope, so the write bit alone would let it through).
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
    let req = AcDeleteRequest::new(
        auth.0.clone(),
        action_digest,
        format!("anon@{}", auth.0),
        auth.0.clone(),
        now_ms,
    );
    // Storage byte accounting (finding #1 / cluster-C): the reclaimed bytes are
    // RELEASED inside `state.delete` by the
    // [`crate::byte_accounting::AccountingAcHandler`] decorator (reads the
    // size-bearing `AcDeleteResponse::reclaimed_bytes` the R2 delete handler now
    // populates via a pre-delete HEAD). Idempotent: 204 either way.
    match state.delete.delete(req) {
        Ok(resp) => {
            let _ = resp.existed;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/ac/:tenant` handler — paginated ref enumeration (D-7).
///
/// Requires a READ-capable PAT. Returns
/// `{ "refs": [...], "next_cursor": <opaque|null> }`.
async fn handle_list_refs(
    State(state): State<AcRouteState>,
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
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&auth.0).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    let req = AcListRequest::new(
        auth.0.clone(),
        format!("anon@{}", auth.0),
        auth.0,
        clamp_limit(q.limit),
        q.cursor,
        now_ms,
    );
    match state.list.list(req) {
        Ok(resp) => {
            let refs: Vec<_> = resp
                .refs
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "ref_key": r.ref_key,
                        "updated_at": r.updated_at,
                        "size": r.size,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "refs": refs,
                    "next_cursor": resp.next_cursor,
                })),
            )
                .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Map an [`AcHandlerError`] to the canonical HTTP response.
fn map_err(e: AcHandlerError) -> axum::response::Response {
    match e {
        AcHandlerError::Miss { .. } => (StatusCode::NOT_FOUND, "ac miss").into_response(),
        AcHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        AcHandlerError::DivergentBody { .. } => {
            // Same (tenant, action_digest), different bytes: a proven AC
            // result must never be silently overwritten. 409 Conflict.
            (StatusCode::CONFLICT, "divergent body").into_response()
        }
        AcHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve
            // bytes / commit updates without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // F1 (CAA-360) fail-CLOSED: the route was mounted with storage creds
        // present but the R2 handler refused to build (R2_TDK_HEX unset/invalid),
        // so it is serving the `UnavailableAcHandler`. Surface that as 503
        // "storage unavailable" — the cache is unavailable until R2_TDK_HEX is
        // set, NOT a generic 500. See [`UnavailableAcHandler`].
        AcHandlerError::Internal(ref msg) if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL) => {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        // Storage byte-accounting decorator (finding #1 / cluster B+C): over-cap
        // ⇒ 402, accounting-backend fault ⇒ 503 fail-CLOSED. Both ride a
        // sentinel-tagged `Internal` from `AccountingAcHandler`.
        AcHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response()
        }
        AcHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "storage accounting unavailable").into_response()
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
    use corelink_handler_ac::{AuditEventKind, Sli};

    /// A canonical 64-lowercase-hex action digest for router-level tests — the
    /// HTTP handlers now reject non-canonical digests with 400 (CAA-360 #9), so
    /// router tests must use a realistic digest. (Store-level tests call the
    /// handler trait directly, bypass the route gate, and keep their short stubs.)
    const VALID_DIGEST: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn is_canonical_digest_accepts_only_64_lowercase_hex() {
        // CAA-360 #9: the digest gate must accept exactly 64 lowercase hex and
        // reject everything else BEFORE it can become an R2 key.
        assert!(is_canonical_digest(&"0123456789abcdef".repeat(4))); // 64 lc hex
        assert!(is_canonical_digest(&"a".repeat(64)));
        assert!(!is_canonical_digest(&"a".repeat(63)), "too short");
        assert!(!is_canonical_digest(&"a".repeat(65)), "too long");
        assert!(!is_canonical_digest(&"A".repeat(64)), "uppercase not canonical");
        assert!(!is_canonical_digest(&"g".repeat(64)), "non-hex char");
        assert!(!is_canonical_digest("../../../etc/passwd"), "path traversal");
        assert!(!is_canonical_digest(""), "empty");
    }

    /// `map_err` must map an `Internal` error to 503 ONLY when it carries the
    /// storage-unavailable sentinel; any other `Internal` is a generic 500.
    /// Kills the cargo-mutants "replace match guard with true" mutant on the
    /// `if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL)` guard — with the guard
    /// forced to `true`, the non-sentinel case below would wrongly become 503.
    #[test]
    fn map_err_internal_is_503_only_for_storage_sentinel() {
        let storage = map_err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2_TDK_HEX unset"
        )));
        assert_eq!(
            storage.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "sentinel-tagged Internal must be 503"
        );

        let generic = map_err(AcHandlerError::Internal(
            "lock poisoned: unrelated failure".to_string(),
        ));
        assert_eq!(
            generic.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "non-sentinel Internal must be 500, not 503 (guard must not be `true`)"
        );
    }

    /// Test fixture: returns the route state plus the underlying
    /// audit + SLI sinks so each test can verify emit ordering.
    fn fixture() -> (
        Arc<InMemoryAuditSink>,
        Arc<InMemorySliObserver>,
        AcRouteState,
    ) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryAcHandler::new(audit.clone(), sli.clone()));
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared.clone();
        let delete: Arc<dyn AcDeleteHandler> = shared.clone();
        let list: Arc<dyn AcListHandler> = shared;
        (
            audit,
            sli,
            AcRouteState {
                lookup,
                update,
                delete,
                list,
                quota: None,
                pat_gate: None,
                put_inflight: Arc::new(Mutex::new(HashMap::new())),
            },
        )
    }

    /// Route state backed by the fail-CLOSED [`UnavailableAcHandler`] — the
    /// shape `build_handlers` returns when storage creds are present but the R2
    /// handler refuses to build (F1: `R2_TDK_HEX` unset/invalid).
    fn fixture_unavailable() -> AcRouteState {
        let shared = Arc::new(UnavailableAcHandler);
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared.clone();
        let delete: Arc<dyn AcDeleteHandler> = shared.clone();
        let list: Arc<dyn AcListHandler> = shared;
        AcRouteState {
            lookup,
            update,
            delete,
            list,
            quota: None,
            pat_gate: None,
            put_inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(AC_LOOKUP_ROUTE, "/v1/ac/:tenant/:action_digest");
        assert_eq!(AC_UPDATE_ROUTE, "/v1/ac/:tenant/:action_digest");
    }

    /// F1 (CAA-360) fail-CLOSED: the `UnavailableAcHandler`'s error maps to
    /// HTTP 503 "storage unavailable" (NOT the generic 500) so a forgotten
    /// `R2_TDK_HEX` is loud, not a silent non-durable cache.
    #[test]
    fn unavailable_handler_maps_to_503() {
        let err = UnavailableAcHandler
            .lookup(AcLookupRequest::new("t1", "d1", "anon@t1", "t1", 0))
            .expect_err("unavailable");
        assert!(matches!(err, AcHandlerError::Internal(_)));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        let (lookup, _update, _delete, _list) = build_handlers();
        let res = lookup.lookup(AcLookupRequest::new("t1", "d1", "anon", "t1", 0));
        assert!(matches!(res, Err(AcHandlerError::Miss { .. })));
        // Router constructor smoke.
        let (_a, _s, st) = fixture();
        let _router = router(st);
    }

    /// Happy path: PUT then GET round-trip emits audit + Sli on both
    /// legs and returns the stored bytes.
    #[test]
    fn put_then_get_round_trip_emits_audit_and_sli() {
        let (audit, sli, st) = fixture();
        // Update path.
        let upd_req = AcUpdateRequest::new("t1", "d1", b"result".to_vec(), "anon@t1", "t1", 1);
        let upd_resp = st.update.update(upd_req).expect("update");
        assert!(upd_resp.durable);
        // Lookup path.
        let lk_req = AcLookupRequest::new("t1", "d1", "anon@t1", "t1", 2);
        let lk_resp = st.lookup.lookup(lk_req).expect("lookup hit");
        assert_eq!(lk_resp.action_digest, "d1");
        assert_eq!(lk_resp.result_payload, b"result".to_vec());
        // Audit rows: UpdateAttempted + UpdateCommitted + LookupAttempted + LookupHit.
        let rows = audit.snapshot().expect("audit");
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::UpdateAttempted));
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::UpdateCommitted));
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::LookupAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupHit));
        // SLI: at least one AvailAcLookup + one LatencyAcHitP99 non-error.
        let obs = sli.snapshot().expect("sli");
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::AvailAcLookup && !o.is_error));
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::LatencyAcHitP99 && !o.is_error));
    }

    /// Cross-tenant lookup MUST emit `LookupDenied` BEFORE the error.
    /// Pins `INV-TENANT-ISOLATION` at the route boundary.
    #[test]
    fn cross_tenant_lookup_denied_audits_before_rejection() {
        let (audit, _sli, st) = fixture();
        let req = AcLookupRequest::new("victim", "d1", "attacker@attacker_t", "attacker_t", 1);
        let err = st.lookup.lookup(req).expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        // First row MUST be LookupDenied (fail-CLOSED ordering pin).
        assert_eq!(rows[0].kind, AuditEventKind::LookupDenied);
        assert_eq!(rows[0].tenant, "victim");
        assert_eq!(rows[0].principal, "attacker@attacker_t");
    }

    /// Audit-fail aborts the update mutation (fail-CLOSED). Verifies
    /// the auth/audit-fail handling path used by `map_err` returns
    /// 503 semantically through the handler error variant.
    #[test]
    fn audit_failure_aborts_update_and_maps_to_audit_closed() {
        let (audit, _sli, st) = fixture();
        audit.inject_failure("audit pipeline down").expect("inject");
        let upd = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1);
        let err = st.update.update(upd).expect_err("audit closed");
        assert!(matches!(err, AcHandlerError::AuditFailed(_)));
        // Verify the route-layer `map_err` translates this to 503.
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Idempotent retry: a second PUT with the same bytes does NOT
    /// re-create the entry (`durable=false`) and audit + Sli still
    /// fire on the retry — pins R-prep retry idempotency.
    #[test]
    fn idempotent_retry_keeps_state_stable() {
        let (audit, sli, st) = fixture();
        let req1 = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1);
        let r1 = st.update.update(req1).expect("first put");
        assert!(r1.durable, "fresh insert");
        let req2 = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 2);
        let r2 = st.update.update(req2).expect("retry put");
        assert!(!r2.durable, "retry MUST NOT be durable=true");
        // Audit rows: two UpdateAttempted + two UpdateCommitted.
        let rows = audit.snapshot().expect("audit");
        let attempts = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateAttempted)
            .count();
        let commits = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateCommitted)
            .count();
        assert_eq!(attempts, 2);
        assert_eq!(commits, 2);
        // SLI emit on every entry.
        let obs_count = sli.count(Sli::AvailAcLookup).expect("count");
        assert!(obs_count >= 2);
    }

    /// Auth-fail surrogate: a missing-entry lookup returns Miss; the
    /// route layer maps Miss to HTTP 404. Tenant binding is now done by
    /// the `crate::auth_tenant::AuthTenant` extractor on the handlers
    /// (path `:tenant` must equal the authenticated tenant else 403);
    /// the remaining auth-fail surrogate here is the cross-tenant
    /// denial above.
    #[test]
    fn miss_maps_to_404() {
        let (_audit, _sli, st) = fixture();
        let lk = AcLookupRequest::new("t1", "ghost", "anon@t1", "t1", 1);
        let err = st.lookup.lookup(lk).expect_err("miss");
        assert!(matches!(err, AcHandlerError::Miss { .. }));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Authenticated tenant injected via `x-corelink-tenant-id`; the scope
    /// header then gates lookup (read) vs update (write).
    const TEST_TENANT: &str = "t1";

    /// A `cas:rw` AC update succeeds (current prod scope — happy path).
    #[tokio::test]
    async fn update_with_rw_scope_succeeds() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// cluster F: with the tenant AT `AC_WRITE_CONCURRENCY_LIMIT` in-flight
    /// writes, the next AC update is rejected 429 BEFORE its body is buffered —
    /// the `AcPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
    /// Proven deterministically with a body LARGER than the 10 MiB global limit.
    #[tokio::test]
    async fn ac_write_at_concurrency_limit_returns_429_before_body() {
        let (_a, _s, st) = fixture();
        {
            let mut g = st.put_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), AC_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(st);
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]); // > 10 MiB
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit native AC write must be rejected 429 by the pre-body concurrency guard"
        );
    }

    /// An AC update BELOW the limit succeeds and releases its slot.
    #[tokio::test]
    async fn ac_write_below_limit_releases_slot() {
        let (_a, _s, st) = fixture();
        let inflight = st.put_inflight.clone();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let g = inflight.lock().expect("lock");
        assert_eq!(
            g.get(TEST_TENANT),
            None,
            "the concurrency slot must be released after the AC write completes"
        );
    }

    /// A `cas:r` (read-only) AC update is rejected 403 "insufficient scope".
    #[tokio::test]
    async fn update_with_read_only_scope_returns_403_insufficient_scope() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    /// F1 (CAA-360) end-to-end: an in-scope AC lookup against the fail-CLOSED
    /// `UnavailableAcHandler` (creds present, `R2_TDK_HEX` missing) returns
    /// HTTP 503 — the cache refuses to serve, not a silent miss from a
    /// non-durable InMemory fallback.
    #[tokio::test]
    async fn lookup_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// F1 (CAA-360) end-to-end: an in-scope AC update against the fail-CLOSED
    /// `UnavailableAcHandler` returns 503 — updates are refused (never a silent
    /// non-durable commit) until `R2_TDK_HEX` is set.
    #[tokio::test]
    async fn update_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A `cas:r` AC lookup passes the scope gate; the entry is absent so the
    /// route returns 404 (a denied scope would 403 before storage).
    #[tokio::test]
    async fn lookup_with_read_only_scope_passes_gate_then_404() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Fail-CLOSED: an AC lookup with NO scope header is rejected 403.
    #[tokio::test]
    async fn lookup_with_missing_scope_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── D-1 delete + D-7 list ────────────────────────────────────────────────

    #[test]
    fn list_route_constant_uses_colon_syntax() {
        assert_eq!(AC_LIST_ROUTE, "/v1/ac/:tenant");
        assert!(!AC_LIST_ROUTE.contains('{'));
    }

    #[test]
    fn clamp_limit_enforces_contract_window() {
        assert_eq!(clamp_limit(None), 200);
        assert_eq!(clamp_limit(Some(0)), 200);
        assert_eq!(clamp_limit(Some(50)), 50);
        assert_eq!(clamp_limit(Some(1000)), 1000);
        assert_eq!(clamp_limit(Some(5000)), 1000);
    }

    /// DELETE an existing ref → 204; a repeated DELETE → 204 (idempotent).
    #[tokio::test]
    async fn delete_existing_then_repeat_both_204() {
        let (_a, _s, st) = fixture();
        // Seed a ref via the write path.
        st.update
            .update(AcUpdateRequest::new(
                TEST_TENANT,
                "d1",
                b"r".to_vec(),
                "anon@t1",
                TEST_TENANT,
                1,
            ))
            .expect("seed");
        let app = router(st);
        let del = || {
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
                .header("x-corelink-tenant-id", TEST_TENANT)
                .header(crate::scope::SCOPE_HEADER, "cas:rw")
                .body(Body::empty())
                .expect("request")
        };
        let resp = app.clone().oneshot(del()).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        // Repeat delete (now absent) MUST also be 204.
        let resp2 = app.oneshot(del()).await.expect("oneshot");
        assert_eq!(resp2.status(), StatusCode::NO_CONTENT);
    }

    /// DELETE with a read-only PAT → 403 (write capability required).
    #[tokio::test]
    async fn delete_with_read_only_scope_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// Cross-tenant DELETE → 403 (path tenant != authenticated tenant).
    #[tokio::test]
    async fn cross_tenant_delete_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/ac/victim/{VALID_DIGEST}")
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── cf-multitenant WP5b: runner-job PAT narrowing ────────────────────────

    /// A second canonical digest, distinct from `VALID_DIGEST`, for the
    /// exact-key restriction tests.
    const OTHER_DIGEST: &str =
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    /// runner-job marker present ⇒ AC DELETE is denied 403 even with a
    /// write-capable scope (a per-job credential must not evict the cache).
    #[tokio::test]
    async fn runner_job_ac_delete_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
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

    /// runner-job + exact-key pin: an AC update to the pinned key passes the
    /// gate (201/200), to a DIFFERENT key is denied 403.
    #[tokio::test]
    async fn runner_job_ac_update_exact_key_enforced() {
        // Allowed key == the request path digest ⇒ passes.
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, VALID_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED, "pinned key must pass");

        // Allowed key != the request path digest ⇒ 403.
        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let req2 = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, OTHER_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp2 = app2.oneshot(req2).await.expect("oneshot");
        assert_eq!(resp2.status(), StatusCode::FORBIDDEN, "off-key write must 403");
        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"ac write outside the job's allowed key");
    }

    /// runner-job + wildcard key (`*`, the launch default): an AC update to ANY
    /// key passes the gate; DELETE is still denied.
    #[tokio::test]
    async fn runner_job_ac_wildcard_allows_write_but_denies_delete() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let put = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(put).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED, "wildcard write must pass");

        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::empty())
            .expect("request");
        let resp2 = app2.oneshot(del).await.expect("oneshot");
        assert_eq!(resp2.status(), StatusCode::FORBIDDEN, "wildcard delete still denied");
    }

    /// NO runner-job marker (normal PAT): AC update + delete behave EXACTLY as
    /// before — the WP5b checks are no-ops (even if a stray key-allow header
    /// with the WRONG key is present without the marker).
    #[tokio::test]
    async fn no_runner_job_marker_is_no_op() {
        // Update to VALID_DIGEST with an OTHER_DIGEST key-allow but NO marker →
        // still succeeds (the pin is ignored without the marker).
        let (_a, _s, st) = fixture();
        let app = router(st);
        let put = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, OTHER_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(put).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED, "no marker ⇒ pin ignored");

        // Delete with a write scope and no marker → 204 (unchanged).
        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .expect("request");
        let resp2 = app2.oneshot(del).await.expect("oneshot");
        assert_eq!(resp2.status(), StatusCode::NO_CONTENT, "normal delete unchanged");
    }

    /// A present-but-non-`"1"` marker is NOT a runner-job: delete behaves as a
    /// normal write (204), proving the exact-`"1"` rule at the route.
    #[tokio::test]
    async fn runner_job_marker_non_one_is_not_narrowed_at_route() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "0")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(del).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "marker \"0\" ⇒ not narrowed");
    }

    /// GET list returns the tenant's refs (read-only PAT is sufficient).
    #[tokio::test]
    async fn list_returns_tenant_refs() {
        let (_a, _s, st) = fixture();
        for d in ["d1", "d2", "d3"] {
            st.update
                .update(AcUpdateRequest::new(
                    TEST_TENANT,
                    d,
                    b"r".to_vec(),
                    "anon@t1",
                    TEST_TENANT,
                    1,
                ))
                .expect("seed");
        }
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let refs = v["refs"].as_array().expect("refs array");
        assert_eq!(refs.len(), 3);
        assert!(v.get("next_cursor").is_some());
    }

    /// List pagination: limit=1 returns one entry + a non-null cursor; the
    /// cursor fetches the next page.
    #[tokio::test]
    async fn list_pagination_cursor_walks_pages() {
        let (_a, _s, st) = fixture();
        for d in ["d1", "d2"] {
            st.update
                .update(AcUpdateRequest::new(
                    TEST_TENANT,
                    d,
                    b"r".to_vec(),
                    "anon@t1",
                    TEST_TENANT,
                    1,
                ))
                .expect("seed");
        }
        let app = router(st);
        let page1 = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}?limit=1"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.clone().oneshot(page1).await.expect("oneshot");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(v["refs"].as_array().expect("refs").len(), 1);
        let cursor = v["next_cursor"].as_str().expect("cursor present").to_owned();
        let page2 = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}?limit=1&cursor={cursor}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp2 = app.oneshot(page2).await.expect("oneshot");
        let body2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .expect("body");
        let v2: serde_json::Value = serde_json::from_slice(&body2).expect("json");
        assert_eq!(v2["refs"].as_array().expect("refs").len(), 1);
        // The two pages return distinct refs.
        assert_ne!(v["refs"][0]["ref_key"], v2["refs"][0]["ref_key"]);
    }

    /// Cross-tenant LIST → 403.
    #[tokio::test]
    async fn cross_tenant_list_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/ac/victim")
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── finding #1: storage byte accounting (cap enforcement) ────────────────

    /// The KILLING finding-#1 test: with the storage counter AT the cap, an AC
    /// update that would store new bytes is rejected 402 — the previously-inert
    /// storage cap now trips on a real HTTP write.
    // multi_thread: the `AccountingAcHandler` decorator bridges the async
    // accountant to the sync update trait with `block_in_place` (needs a
    // multi-thread runtime; matches production `#[tokio::main]`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_over_storage_cap_returns_402() {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, testing::Row, AccountingAcHandler, ByteAccountant, ByteStore,
        };

        let (_a, _s, mut st) = fixture();
        let store = Arc::new(InMemoryByteStore::new());
        store.seed(TEST_TENANT, "iad", Row { used: 4, quota: 4 }); // at the cap
        let store_dyn: Arc<dyn ByteStore> = store;
        let acc = Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()));
        // Wrap the AC update + delete trait objects in the decorator (mirrors the
        // production wiring) so the reservation runs BEFORE the inner update.
        let acct = Arc::new(AccountingAcHandler::new(
            st.update.clone(),
            st.delete.clone(),
            acc,
        ));
        st.update = acct.clone();
        st.delete = acct;
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result-bytes-over-cap".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: an uncapped tenant's AC update accrues its bytes and succeeds
    /// (201) — proving the storage counter MOVES (it never did before #1).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_under_storage_cap_accrues_and_succeeds() {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, AccountingAcHandler, ByteAccountant, ByteStore,
        };

        let (_a, _s, mut st) = fixture();
        let store = Arc::new(InMemoryByteStore::new()); // uncapped fresh row
        let store_dyn: Arc<dyn ByteStore> = store.clone();
        let acct = Arc::new(AccountingAcHandler::new(
            st.update.clone(),
            st.delete.clone(),
            Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned())),
        ));
        st.update = acct.clone();
        st.delete = acct;
        let app = router(st);
        let body = b"result".to_vec();
        let body_len = body.len() as i64;
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            // Worker-injected per-tier cap seeds the fresh row; without it the
            // reservation fails CLOSED (no cap ⇒ 503). See storage_quota_from_headers.
            .header(crate::byte_accounting::STORAGE_QUOTA_HEADER, "1000000")
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(
            store.used(TEST_TENANT, "iad"),
            body_len,
            "a durable AC update must accrue its bytes into the storage counter"
        );
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A request with the PAT gate wired but NO bearer Authorization header is
    /// rejected 401 — the native gate fails CLOSED on a missing token (defense-
    /// in-depth, even with the Worker-set tenant header present).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::adapter_pat::PatRow;
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use corelink_pat::PatSigningKey;

        let (_a, _s, mut st) = fixture();
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
        st.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            // No Authorization header — the gate rejects.
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

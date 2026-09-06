// `GET /v1/ac/:tenant/:action_digest` + `PUT /v1/ac/:tenant/:action_digest`
// — Action Cache (AC) read + update routes wired against
// [`corelink_handler_ac::AcLookupHandler`] and
// [`corelink_handler_ac::AcUpdateHandler`].
//
// This is the wave-11 end-to-end wire-up for the AC half of the
// R-prep handler-crate skeleton (wave-8). The route shape mirrors
// the cas-read wire-up exactly: the route stores `Arc<dyn ...Handler>`
// trait objects so the in-memory fake can be swapped for the wasm32
// CF-Worker R2-bound handler without touching the route layer.
//
// # Why a trait object
//
// The route handler accepts `Arc<dyn AcLookupHandler>` +
// `Arc<dyn AcUpdateHandler>` so the binary-shape stays stable as
// the in-memory fakes are swapped for the wasm32 CF-Worker impl.
// In production both trait objects point at the same underlying
// handler instance (the `InMemoryAcHandler` implements both
// halves), but they are kept distinct in the state shape so the
// AC-read and AC-update SLO emit sites can be backed by
// independent collaborators if/when needed.
//
// # SLO emit
//
// Each route entry emits the AC handler's
// `Sli::AvailAcLookup` (+ `Sli::LatencyAcHitP99` on the lookup path)
// through the `SliObserver` collaborator. Per the audit
// 2026-05-14 closure list, this transitions `SLO-AVAIL-AC` and
// `SLO-LAT-AC-HIT-P99` from "Sli emitted at handler layer" to
// "Sli emitted at handler layer **and** routed end-to-end via
// apps/server".
//
// # Cross-tenant denial
//
// Cross-tenant attempts are rejected with HTTP 403 + an
// `AuditEventKind::LookupDenied` / `UpdateDenied` row emitted
// BEFORE the response. This pins `INV-TENANT-ISOLATION` at the
// route boundary on top of the handler-layer enforcement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::wall_clock::{SystemWallClock, WallClock};
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use corelink_handler_ac::{
    AcDeleteHandler, AcDeleteRequest, AcDeleteResponse, AcHandlerError, AcListHandler,
    AcListRequest, AcListResponse, AcLookupHandler, AcLookupRequest, AcLookupResponse,
    AcUpdateHandler, AcUpdateRequest, AcUpdateResponse, InMemoryAcHandler, InMemoryAuditSink,
    InMemorySliObserver,
};

/// Canonical AC list route path — `GET /v1/ac/:tenant` (D-7).
pub const AC_LIST_ROUTE: &str = "/v1/ac/{tenant}";

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

/// Canonical AC lookup route path (axum-0.8 / matchit-0.8 `{name}` brace captures).
///
/// DEBT-029 (2026-05-16, historical): at the time this crate was pinned to
/// axum 0.7 / matchit 0.7, `:name` was the capture syntax and `{name}` was a
/// literal path segment (silent 404). The workspace has SINCE moved to
/// **axum 0.8 / matchit 0.8** (`Cargo.toml`: `axum = { version = "0.8", ... }`),
/// which inverted the syntax: `{name}` is now the capture form and a bare
/// `:name` is a literal that silently 404s. This constant and every other
/// live route in the workspace use the current `{name}` brace form — see the
/// pinning regression test `list_route_constant_uses_axum_0_8_brace_syntax`
/// below, which asserts brace-presence + colon-absence to prevent drift back
/// to the pre-0.8 syntax.
pub const AC_LOOKUP_ROUTE: &str = "/v1/ac/{tenant}/{action_digest}";

/// Canonical AC update route path. The update path reuses the
/// same template; axum disambiguates by HTTP method.
pub const AC_UPDATE_ROUTE: &str = "/v1/ac/{tenant}/{action_digest}";

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
    d.len() == 64
        && d.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
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

/// Write-path variant of [`pat_gate_reject`]: independently re-derives the PAT's
/// D1-stored `can_write` capability at the container (via
/// `NativePatGate::verify_write`), not just tenant possession — upholding the
/// Option-B invariant ("a compromised or misconfigured Worker cannot grant write
/// on its own") on the AC update/delete paths, matching the sibling surfaces
/// (Bazel/Turbo/cargo/OCI). A read-only PAT on a write path ⇒ `403` even if the
/// Worker-set scope header claimed write (deep-audit money/auth F-1).
async fn pat_gate_reject_write(
    state: &AcRouteState,
    tenant: &str,
    headers: &axum::http::HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify_write(tenant, bearer).await.err()
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
    // Native PAT write-capability gate (deep-audit money/auth F-1): re-derive the
    // PAT's D1 `can_write` at the container — an AC update is a mutation, so a
    // read-only PAT ⇒ 403 even if the Worker-set scope header claimed write.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
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
            // AC create-only (deny-overwrite) — anti AC-squat, the AC analog of
            // deny-DELETE. The store's `durable` flag is an ATOMIC put-if-absent
            // signal: `true` = the row was FRESHLY INSERTED (the entry was
            // absent); `false` = the `(tenant, action_digest)` entry ALREADY
            // EXISTED and this was a byte-identical idempotent no-op (NOT a
            // destructive overwrite — the store is content-immutable). A
            // create-only cred may CREATE but never touch an existing entry, so a
            // non-durable result is rejected 409. NO TOCTOU window: the decision
            // rides the store's OWN conditional insert (the same atomic point
            // that backs INV-AC-RESULT-HASH-IMMUTABLE), not a separate look-up.
            if runner_job.ac_create_only() && !resp.durable {
                return ac_create_only_conflict();
            }
            let code = if resp.durable {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (code, resp.action_digest).into_response()
        }
        // A create-only cred whose write hit an existing entry with DIVERGENT
        // bytes: the store already refused the overwrite (DivergentBody, the
        // INV-AC-RESULT-HASH-IMMUTABLE gate). Surface it with the consistent
        // AC_CREATE_ONLY body rather than the generic divergent-body 409 — both
        // are 409 rejections of the same overwrite attempt by this cred.
        Err(AcHandlerError::DivergentBody { .. }) if runner_job.ac_create_only() => {
            ac_create_only_conflict()
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
    // Native PAT write-capability gate (deep-audit money/auth F-1): an AC delete
    // is a mutation — re-derive the PAT's D1 `can_write` at the container, not
    // just possession. Read-only PAT on this write path ⇒ 403.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
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

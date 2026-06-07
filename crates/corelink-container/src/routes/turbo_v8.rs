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
//! # Backing store (Phase 0 / v1)
//!
//! The route state is backed by [`corelink_turbo_bridge::adapter::InMemoryKvStore`]
//! which stores artifacts in RAM and does NOT persist across container restarts.
//! This is the correct starting point — the backing store is a swappable port
//! trait ([`CasReadStore`] / [`CasWriteStore`]).
//!
//! **TODO(v2):** Replace `InMemoryKvStore` with a thin `R2KvStore` impl backed
//! by R2 directly (separate bucket / prefix — e.g. `corelink-turbo-cache-prod`).
//! See `specs/TODO-turbo-r2-backing-store.md` for the migration plan.
//! The route handlers and audit surface are unchanged by the swap.
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

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use corelink_turbo_bridge::{
    adapter::{CasAdapterTurboHandler, InMemoryKvStore},
    audit::InMemoryTurboAuditSink,
    error::validate_team_id,
    TurboArtifactHandler, TurboBridgeError, TurboEventsRequest, TurboGetRequest, TurboPutRequest,
    TurboStatusRequest,
};

// ── Route constants ───────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash` — retrieve a Turborepo build artifact.
///
/// MUST use matchit-0.7 `:name` syntax (not `{name}`).
pub const TURBO_GET_ROUTE: &str = "/v8/artifacts/:hash";

/// `PUT /v8/artifacts/:hash` — store a Turborepo build artifact.
///
/// Same template as GET; axum disambiguates by HTTP method.
pub const TURBO_PUT_ROUTE: &str = "/v8/artifacts/:hash";

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
}

impl core::fmt::Debug for TurboRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TurboRouteState").finish_non_exhaustive()
    }
}

// ── Handler construction ──────────────────────────────────────────────────────

/// Build the [`TurboRouteState`] for the current build target and runtime.
///
/// # Phase 0 / v1 — `InMemoryKvStore`
///
/// Artifacts are stored in RAM using [`InMemoryKvStore`] (does NOT persist
/// across container restarts).  The audit sink is also in-memory.
///
/// **TODO(v2):** Swap `InMemoryKvStore` for a thin `R2KvStore` impl backed
/// by R2 directly (separate bucket / prefix).  The route handlers and all
/// audit/validation logic are unchanged — only the `Arc<dyn CasReadStore>` /
/// `Arc<dyn CasWriteStore>` arguments to [`CasAdapterTurboHandler::new`] need
/// to be replaced.  See `specs/TODO-turbo-r2-backing-store.md`.
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn build_handlers() -> TurboRouteState {
    let audit = Arc::new(InMemoryTurboAuditSink::new());
    let store = Arc::new(InMemoryKvStore::new());
    let handler: Arc<dyn TurboArtifactHandler> =
        Arc::new(CasAdapterTurboHandler::new(store.clone(), store, audit));
    TurboRouteState { handler }
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
        .route(TURBO_EVENTS_ROUTE, post(handle_events))
        .route(TURBO_STATUS_ROUTE, post(handle_status))
        .route(TURBO_GET_ROUTE, get(handle_get).put(handle_put))
        // Per-route body cap: Turbo build artifacts are legitimately larger than
        // the 10 MiB global limit set in `main.rs`. This inner `DefaultBodyLimit`
        // layer overrides the outer global default for the `/v8/artifacts/*`
        // routes only (axum honours the innermost limit) while still bounding the
        // body at 100 MiB so a PAT cannot OOM the shared container.
        .layer(axum::extract::DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES))
        .with_state(state)
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Returns 200 + raw artifact bytes on hit, 404 on miss, 400 on bad params,
/// 403 on cross-tenant, 503 on audit-closed.
async fn handle_get(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
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
    let now_ms = 0u64;
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
        Ok(resp) => (StatusCode::OK, resp.bytes).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Accepts raw `application/octet-stream` body.  Returns 200 +
/// `{"urls": [...]}` on success.
async fn handle_put(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
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
    let now_ms = 0u64;
    let req = TurboPutRequest::new(
        hash,
        params.team_id,
        params.slug,
        body.to_vec(),
        None, // duration_ms — Phase 0: not parsed from headers
        format!("anon@{caller_tenant}"),
        caller_tenant,
        now_ms,
    );
    match state.handler.put(req) {
        Ok(resp) => {
            let body = PutArtifactResponse { urls: resp.urls };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
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
/// handler. `AuthTenant` is `FromRequestParts`, so it precedes the `Bytes`
/// body extractor (axum 0.7 ordering rule).
async fn handle_events(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let principal = format!("anon@{}", auth.0);
    let req = TurboEventsRequest::new(body.to_vec(), principal, 0u64);
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
    fn route_constants_use_colon_syntax_not_curly_braces() {
        // Regression net for DEBT-029 — matchit 0.7.3 treats `{name}` as a
        // literal path segment rather than a capture variable.
        assert!(
            !TURBO_GET_ROUTE.contains('{'),
            "TURBO_GET_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
        );
        assert!(TURBO_GET_ROUTE.contains(":hash"));
        assert!(TURBO_PUT_ROUTE.contains(":hash"));
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
}

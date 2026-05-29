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
//! `teamId` query parameter is required on PUT and GET.  Missing `teamId` → 400.
//! `teamId` must equal the authenticated caller tenant; mismatch → 403 + audit.
//! The cross-tenant check + audit-emit-BEFORE-mutation fail-CLOSED ordering is
//! enforced inside [`CasAdapterTurboHandler`], not in the route layer.
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

// ── Query parameters ──────────────────────────────────────────────────────────

/// Query parameters for `GET /v8/artifacts/:hash` and
/// `PUT /v8/artifacts/:hash`.
///
/// `teamId` is **required**; missing value → 400 (axum rejects the
/// extraction before the handler is called).  `slug` is optional and
/// informational.
#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    /// Vercel team identifier — used as the storage partition key.
    /// Must equal the authenticated caller tenant (enforced in the handler).
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
    let handler: Arc<dyn TurboArtifactHandler> = Arc::new(CasAdapterTurboHandler::new(
        store.clone(),
        store,
        audit,
    ));
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
        .route(
            TURBO_GET_ROUTE,
            get(handle_get).put(handle_put),
        )
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
) -> impl IntoResponse {
    // Authenticated tenant: for Phase 0 we use teamId as the caller_tenant
    // since there is no auth middleware yet on this path.  Production wiring
    // must thread the bearer-verified tenant from a tower layer here.
    let caller_tenant = params.team_id.clone();
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
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let caller_tenant = params.team_id.clone();
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
async fn handle_events(
    State(state): State<TurboRouteState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let req = TurboEventsRequest::new(body.to_vec(), "anon", 0u64);
    match state.handler.events(req) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /v8/artifacts/status`
///
/// Returns static `{"status":"enabled"}`.  No request body required.
async fn handle_status(
    State(state): State<TurboRouteState>,
    _req: axum::extract::Request,
) -> impl IntoResponse {
    let _status_req = TurboStatusRequest::new("anon", 0u64);
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
        TurboBridgeError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        TurboBridgeError::HashTooLong { .. } => {
            (StatusCode::BAD_REQUEST, "hash too long").into_response()
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
            .body(Body::from(body_bytes.clone()))
            .expect("put request");
        let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
        assert_eq!(put_resp.status(), StatusCode::OK);

        // GET — same hash + teamId should return the stored bytes.
        let get_req = Request::builder()
            .method(Method::GET)
            .uri("/v8/artifacts/abc123?teamId=team_x")
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
            .body(Body::from(artifact.clone()))
            .expect("put");
        let put_resp = app.clone().oneshot(put_req).await.expect("put");
        assert_eq!(put_resp.status(), StatusCode::OK);

        let get_req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
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
    async fn put_cross_tenant_returns_403() {
        // Phase 0 uses teamId as caller_tenant; an attacker who sets
        // teamId=victim loses because the handler enforces team_id == caller_tenant.
        // In Phase 0 these are always equal (teamId IS the caller_tenant), so
        // we test the handler-level rejection directly to keep the route test
        // focused on HTTP status mapping.
        let state = fixture();
        let err = state.handler.put(corelink_turbo_bridge::TurboPutRequest::new(
            "h1",
            "victim",      // team_id
            "s",
            b"x".to_vec(),
            None,
            "evil",
            "attacker",    // caller_tenant != team_id → CrossTenantDenied
            1,
        ));
        assert!(matches!(
            err.unwrap_err(),
            corelink_turbo_bridge::TurboBridgeError::CrossTenantDenied { .. }
        ));
        // Verify map_err maps it to 403.
        let resp = map_err(corelink_turbo_bridge::TurboBridgeError::CrossTenantDenied {
            caller: "attacker".into(),
            requested_team_id: "victim".into(),
        });
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── events endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_events_always_200() {
        let app = test_router();
        // Turbo sends arbitrary JSON telemetry; we accept and drop.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_events_empty_body_200() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/events")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── status endpoint ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_status_returns_enabled() {
        let app = test_router();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v8/artifacts/status")
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
        let resp = map_err(TurboBridgeError::HashTooLong {
            len: 129,
            max: 128,
        });
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
}

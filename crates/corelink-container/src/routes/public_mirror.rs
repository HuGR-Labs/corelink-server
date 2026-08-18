//! `POST /_internal/admin/public-mirror/*` — the increment-4b **server-only**
//! public-base MIRROR endpoint (F3.2 cross-tenant public OCI base-layer cache).
//!
//! # Security model
//!
//! - **Server-only, never tenant-reachable.** The path lives under
//!   `/_internal/admin/*`, which the edge Worker maps to the admin-consumer key;
//!   a tenant request is rejected at the edge (403) and never reaches this
//!   handler. This is the write side of Control 2 (only the operator, never a
//!   client, can promote bytes into `_public`).
//! - **Admin-gated (fail-CLOSED).** Every call must carry
//!   `x-corelink-internal-auth` matching a **consumer-specific**
//!   `CORELINK_PUBLIC_MIRROR_AUTH_KEY`, with the shared
//!   `CORELINK_INTERNAL_AUTH_KEY` as fallback (the per-consumer split in
//!   [`crate::routes::admin::resolve_internal_auth_key`]). This is DELIBERATELY
//!   distinct from the erase key: the mirror is an admin operation and HAS the
//!   shared fallback, whereas [`crate::routes::public_revoke`] uses the
//!   dedicated erase key with NO fallback (finding H4). If no key resolves the
//!   route is NOT mounted ([`build_state_from_env`] returns `None`).
//!
//! # S0 SCAFFOLD (this commit)
//!
//! This ships the module, the frozen [`build_state_from_env`] / [`router`]
//! signatures, and the auth gate — with a **stub** handler: an unauthenticated
//! caller gets `401` (proving the mount + gate are live), an authenticated
//! caller gets `501 Not Implemented`. There is NO `_public` write path here yet.
//! The SSRF-safe fetch → `verify_against_bytes` (fail-closed) → `MoatCache::put`
//! promote body lands in **inc4b / WP-A**. The endpoint is INERT until then.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};

use crate::routes::admin::resolve_internal_auth_key;
use crate::routes::internal_pat::internal_auth_ok;

/// Consumer-specific admin secret for the public-mirror surface. Resolved with
/// the shared-key fallback in [`resolve_internal_auth_key`].
const MIRROR_AUTH_KEY_ENV: &str = "CORELINK_PUBLIC_MIRROR_AUTH_KEY";

/// Route state: the operator admin secret gating every mirror call.
#[derive(Clone)]
pub struct PublicMirrorRouteState {
    /// Admin secret (consumer-specific, shared fallback) for the constant-time
    /// `x-corelink-internal-auth` gate.
    auth_key: Arc<str>,
}

impl std::fmt::Debug for PublicMirrorRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never leak the admin secret via Debug (mirrors PublicRevokeRouteState).
        f.debug_struct("PublicMirrorRouteState")
            .field("auth_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Build the route state from env, or `None` (route NOT mounted) when no admin
/// key resolves — the consumer-specific `CORELINK_PUBLIC_MIRROR_AUTH_KEY` or the
/// shared `CORELINK_INTERNAL_AUTH_KEY` fallback, each ≥ 32 chars. Fail-CLOSED:
/// absent config yields no route, exactly like the sibling admin surfaces.
#[must_use]
pub fn build_state_from_env() -> Option<PublicMirrorRouteState> {
    let auth_key = resolve_internal_auth_key(MIRROR_AUTH_KEY_ENV)?;
    Some(PublicMirrorRouteState { auth_key })
}

/// Mount the mirror router at its top-level `/_internal/admin/*` path.
pub fn router(state: PublicMirrorRouteState) -> Router {
    Router::new()
        .route(
            "/_internal/admin/public-mirror/promote",
            post(handle_promote),
        )
        .with_state(state)
}

/// `POST /_internal/admin/public-mirror/promote` — S0 STUB.
///
/// The auth gate runs FIRST (401 before any work), exactly as the sibling
/// admin/internal handlers. An authenticated caller currently gets
/// `501 Not Implemented`: the fetch/verify/promote body is inc4b (WP-A). No
/// `_public` (or any) write occurs here.
async fn handle_promote(
    State(state): State<PublicMirrorRouteState>,
    headers: HeaderMap,
    _body: Bytes,
) -> Response {
    if !internal_auth_ok(state.auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "not_implemented",
            "detail": "public-mirror promote lands in inc4b (WP-A)"
        })),
    )
        .into_response()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt;

    const TEST_KEY: &str = "test-admin-secret-key-at-least-32-chars!!";

    fn state() -> PublicMirrorRouteState {
        PublicMirrorRouteState {
            auth_key: Arc::from(TEST_KEY),
        }
    }

    fn req(auth: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/_internal/admin/public-mirror/promote");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn unauthenticated_is_401_route_is_reachable() {
        // The S0 DoD: the mount seam is live + gated. No auth ⇒ 401 (not 404).
        let resp = router(state()).oneshot(req(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_secret_is_401() {
        let resp = router(state())
            .oneshot(req(Some("wrong-but-also-32-chars-long-secret!!!")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn authenticated_is_501_stub_no_write() {
        // Authenticated reaches the stub — INERT, no write path yet (inc4b/WP-A).
        let resp = router(state()).oneshot(req(Some(TEST_KEY))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }
}

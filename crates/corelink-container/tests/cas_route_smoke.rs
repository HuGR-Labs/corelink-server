//! Integration smoke tests for `cas::router` path captures
//! (DEBT-029-cas regression net — follow-up to wave-30 stream-1
//! DEBT-029 closure on `ac.rs` + `admin.rs`).
//!
//! Pin: the route constant in `apps/server/src/routes/cas.rs` MUST
//! be wired with the matchit-0.7 (= axum-0.7) `:name` syntax — *not*
//! the matchit-0.8 `{name}` form. DEBT-029-cas was a latent bug in
//! which `CAS_READ_ROUTE` used the `{tenant}/{hash}` literals:
//! matchit-0.7 accepted them as LITERAL path segments at
//! `Router::new()` time (no panic), so every request with a real
//! tenant + hash value would have returned a router-level
//! `404 Not Found` (handler never fired; `SLO-AVAIL-CAS-GET` +
//! `SLO-LAT-CAS-GET-P99` would have flat-lined at 100% miss).
//!
//! These tests defend that behaviour by sending real path-param
//! requests through `tower::ServiceExt::oneshot` and asserting the
//! handler actually responded with a *meaningful* status + body (not
//! the generic axum router-miss 404). Concretely:
//!
//! * CAS read with a fresh `tenant` + `hash` → 404 with the
//!   handler-emitted `not found` body (NOT a router-miss empty body).
//! * Route constant has no curly braces and contains `:tenant` +
//!   `:hash` (negative-regression pin against future drift).
//! * Construction smoke: `cas::router(state)` composes without
//!   panic under the workspace-pinned matchit grammar.
//!
//! The body-string assertion is load-bearing: a router-miss would
//! return an empty body, so the `assert_eq!(body, "not found")`
//! check distinguishes "route reached the handler" from "matchit
//! treated `:tenant` as a literal segment".

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::indexing_slicing)]

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use corelink_handler_cas::{InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver};
use corelink_server::routes::cas::{self, CasRouteState, CAS_READ_ROUTE};
use tower::ServiceExt;

fn fresh_state() -> CasRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let handler = Arc::new(InMemoryCasHandler::new(audit, sli));
    CasRouteState { handler }
}

/// CAS read against a fresh handler MUST reach the handler and
/// surface the canonical `not found` 404 body. A router-miss (which
/// is what the DEBT-029-cas bug produced) would yield an empty body.
#[tokio::test]
async fn cas_read_route_reaches_handler_and_returns_handler_not_found_404() {
    let app = cas::router(fresh_state());

    let req = Request::builder()
        .uri("/v1/cas/tenant-a/abc123")
        .method("GET")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "handler must emit 404 (not found), not router-miss"
    );
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "not found",
        "body must be the handler-emitted not-found string — \
         router-miss would yield an empty body"
    );
}

/// Negative pin: a request against the literal pre-fix path
/// `/v1/cas/{tenant}/{hash}` (percent-encoded curly braces in the
/// URI) MUST still reach the handler with the literal segments as
/// the path params and emit the handler's 404 — i.e. the route
/// constant does NOT contain literal braces post-fix, so a curly
/// URI no longer collides with a "matching" literal route.
#[tokio::test]
async fn cas_read_route_does_not_match_literal_braces_uri() {
    let app = cas::router(fresh_state());

    let req = Request::builder()
        .uri("/v1/cas/%7Btenant%7D/%7Bhash%7D")
        .method("GET")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "not found",
        "even the literal-braces URI must reach the handler; \
         the handler resolves the unknown object to its canonical \
         not-found path"
    );

    // Pin the public route constant — guards against future regression
    // of the constant itself.
    assert!(
        !CAS_READ_ROUTE.contains('{'),
        "CAS_READ_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
    );
    assert!(CAS_READ_ROUTE.contains(":tenant"));
    assert!(CAS_READ_ROUTE.contains(":hash"));
}

/// Construction smoke: the CAS route constant must compose into a
/// functional axum `Router` on the native target — pins the
/// `Router::new().route(CAS_READ_ROUTE, …)` call path against the
/// live matchit 0.7 grammar.
#[tokio::test]
async fn cas_route_state_constructs_without_panic_on_native() {
    let _router = cas::router(fresh_state());
}

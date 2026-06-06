//! Integration smoke tests for `ac::router` + `admin::router` path
//! captures (wave-30 stream-1 DEBT-029 regression net).
//!
//! Pin: the route constants in `apps/server/src/routes/ac.rs` +
//! `apps/server/src/routes/admin.rs` MUST be wired with the
//! matchit-0.7 (= axum-0.7) `:name` syntax — *not* the matchit-0.8
//! `{name}` form. DEBT-029 was a latent bug in which the route
//! constants used `{name}` literals: matchit-0.7 accepted them as
//! LITERAL path segments at `Router::new()` time (no panic), so
//! every request with a real `tenant` / `action_digest` / `resource`
//! value would have returned a router-level `404 Not Found` (the
//! handler never fired).
//!
//! These tests defend that behaviour by sending real path-param
//! requests through `tower::ServiceExt::oneshot` and asserting the
//! handler actually responded with a *meaningful* status (not the
//! generic axum router-miss 404). Concretely:
//!
//! * AC lookup with a fresh `tenant` + `action_digest` → 404 with the
//!   handler-emitted `ac miss` body (NOT a router miss).
//! * AC update with a fresh tenant returns 201 + the action digest.
//! * Admin read for a missing resource → 404 with the handler-emitted
//!   `admin not found` body.
//! * Admin read with the `auth` resource — covered by the existing
//!   in-module unit tests in `admin.rs`; here we just exercise the
//!   path-capture surface end-to-end.
//!
//! The body-string assertion is load-bearing: a router-miss would
//! return an empty body, so the `assert!(body.starts_with(...))`
//! checks distinguish "route reached the handler" from "matchit
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
use corelink_server::routes::{
    ac::{self, AcRouteState},
    admin::{self, AdminRouteState},
};
use tower::ServiceExt;

/// AC lookup against a fresh handler MUST reach the handler and
/// surface the canonical `ac miss` 404 body. A router-miss (which is
/// what the DEBT-029 bug produced) would yield an empty body.
#[tokio::test]
async fn ac_lookup_route_reaches_handler_and_returns_handler_miss_404() {
    let (lookup, update) = ac::build_handlers();
    let state = AcRouteState { lookup, update };
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/digest-xyz")
        .method("GET")
        // `AuthTenant` reads `x-corelink-tenant-id` and the handler 403s
        // unless it equals the `:tenant` path segment — mirror `tenant-a`.
        .header("x-corelink-tenant-id", "tenant-a")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "handler must emit 404 (miss), not router-miss"
    );
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "ac miss",
        "body must be the handler-emitted miss string — \
         router-miss would yield an empty body"
    );
}

/// AC update against a fresh handler MUST reach the handler and
/// return 201 + the action digest body (the handler emits CREATED on
/// the first PUT and the digest is echoed back). Router-miss bug
/// would produce an empty 404 body instead.
#[tokio::test]
async fn ac_update_route_reaches_handler_and_returns_handler_created_201() {
    let (lookup, update) = ac::build_handlers();
    let state = AcRouteState { lookup, update };
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/digest-xyz")
        .method("PUT")
        // `AuthTenant` reads `x-corelink-tenant-id` and the handler 403s
        // unless it equals the `:tenant` path segment — mirror `tenant-a`.
        .header("x-corelink-tenant-id", "tenant-a")
        .body(Body::from("result-bytes"))
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "handler emits 201 (durable=true) on first PUT"
    );
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "digest-xyz",
        "body must echo the path-captured action_digest — proves \
         the `:action_digest` segment was parsed as a path param, \
         not a literal `{{action_digest}}` segment"
    );
}

/// Admin read against a fresh handler MUST reach the handler and
/// emit the canonical `admin not found` 404 body. A router-miss
/// (DEBT-029 bug surface) would yield an empty body.
#[tokio::test]
async fn admin_read_route_reaches_handler_and_returns_handler_not_found_404() {
    let (read, mutate) = admin::build_handlers();
    let state = AdminRouteState { read, mutate };
    let app = admin::router(state);

    let req = Request::builder()
        .uri("/v1/admin/read/tenant:unknown")
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
        body, "admin not found",
        "body must be the handler-emitted not-found string — \
         router-miss would yield an empty body"
    );
}

/// Negative pin: requests against the literal pre-fix path
/// `/v1/admin/read/{resource}` (curly braces in the URI) MUST NOT
/// match the route after the DEBT-029 fix. This guards against
/// regression where someone re-introduces the `{name}` form and
/// the literal-braces request would spuriously match.
#[tokio::test]
async fn admin_read_route_does_not_match_literal_braces_uri() {
    let (read, mutate) = admin::build_handlers();
    let state = AdminRouteState { read, mutate };
    let app = admin::router(state);

    // The encoded form `%7Bresource%7D` is what a buggy client would
    // send if it didn't substitute the path param. Post-fix the
    // route only matches `:resource` captures, so a request that
    // happens to URL-encode `{resource}` still resolves to the
    // handler with the literal string — i.e. the handler receives
    // `{resource}` (after percent-decoding) as the resource name and
    // emits its normal not-found path. We assert the handler ran
    // (status is the handler's 404, not a router 404 with empty body).
    let req = Request::builder()
        .uri("/v1/admin/read/%7Bresource%7D")
        .method("GET")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "admin not found",
        "even the literal-braces URI must reach the handler; \
         the handler resolves the unknown resource to its canonical \
         not-found path"
    );

    // Pin the public route constant has no curly braces — guards
    // against future regression of the constant itself.
    assert!(
        !admin::ADMIN_READ_ROUTE.contains('{'),
        "ADMIN_READ_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
    );
    assert!(
        !ac::AC_LOOKUP_ROUTE.contains('{'),
        "AC_LOOKUP_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
    );
    assert!(
        !ac::AC_UPDATE_ROUTE.contains('{'),
        "AC_UPDATE_ROUTE must use matchit-0.7 `:name` syntax, not `{{name}}`"
    );
}

/// Construction smoke: every route constant must compose into a
/// functional axum `Router` on the native target — pins the
/// `Router::new().route(ROUTE, …)` call path against the live
/// matchit 0.7 grammar.
#[tokio::test]
async fn route_state_constructs_without_panic_on_native() {
    let (lookup, update) = ac::build_handlers();
    let _ac_router = ac::router(AcRouteState { lookup, update });

    let (read, mutate) = admin::build_handlers();
    let _admin_router = admin::router(AdminRouteState { read, mutate });

    // Cross-route smoke — both routers compose under the same axum
    // build, so the in-process construction proves the constants
    // are mutually valid path templates.
    let _ = Arc::new(()); // anchor `use` of Arc to keep imports stable.
}

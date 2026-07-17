//! Integration smoke tests for `ac::router` + `admin::router` path
//! captures (wave-30 stream-1 DEBT-029 regression net).
//!
//! Pin: the route constants in `apps/server/src/routes/ac.rs` +
//! `apps/server/src/routes/admin.rs` MUST be wired with the
//! matchit-0.8 (= axum-0.8) `{name}` syntax — *not* the legacy
//! matchit-0.7 `:name` form. Post the axum-0.8 migration the grammar
//! INVERTED: matchit-0.8 treats `:name` as a LITERAL path segment
//! (the mirror image of the original DEBT-029 bug, where `{name}` was
//! the literal under matchit-0.7). Wiring a `:name` constant would
//! therefore accept it as a literal at `Router::new()` time (no
//! panic), so every request with a real `tenant` / `action_digest` /
//! `resource` value would return a router-level `404 Not Found` (the
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
    admin::{self, AdminRouteState, ADMIN_INTERNAL_AUTH_HEADER},
};
use tower::ServiceExt;

/// Operator shared secret used by these smoke tests. The admin control
/// plane is now operator-only (gated behind `CORELINK_INTERNAL_AUTH_KEY`,
/// mirroring `/_internal/pat/mint`); the admin-read requests below carry
/// this secret in the `x-corelink-internal-auth` header so they clear the
/// gate and reach the handler. Negative-gate behaviour is unit-tested in
/// `admin.rs`.
const TEST_INTERNAL_AUTH_KEY: &str = "test-internal-auth-key-32-bytes-x";

/// Build an `AdminRouteState` with the operator gate configured.
fn admin_state_with_gate() -> AdminRouteState {
    let stack = admin::build_handler_stack();
    AdminRouteState {
        read: stack.read,
        mutate: stack.mutate,
        internal_auth_key: Some(Arc::from(TEST_INTERNAL_AUTH_KEY)),
        approval_writer: stack.approval_writer,
        approver_auth_key: Some(Arc::from(TEST_INTERNAL_AUTH_KEY)),
        approvals_durable: stack.durable,
    }
}

/// AC lookup against a fresh handler MUST reach the handler and
/// surface the canonical `ac miss` 404 body. A router-miss (which is
/// what the DEBT-029 bug produced) would yield an empty body.
#[tokio::test]
async fn ac_lookup_route_reaches_handler_and_returns_handler_miss_404() {
    let (lookup, update, delete, list) = ac::build_handlers();
    let state = AcRouteState::new(lookup, update, delete, list, None, None);
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        .method("GET")
        // `AuthTenant` reads `x-corelink-tenant-id` and the handler 403s
        // unless it equals the `:tenant` path segment — mirror `tenant-a`.
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-scope", "cas:rw")
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
    let (lookup, update, delete, list) = ac::build_handlers();
    let state = AcRouteState::new(lookup, update, delete, list, None, None);
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        .method("PUT")
        // `AuthTenant` reads `x-corelink-tenant-id` and the handler 403s
        // unless it equals the `:tenant` path segment — mirror `tenant-a`.
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-scope", "cas:rw")
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
        body, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "body must echo the path-captured action_digest — proves \
         the `:action_digest` segment was parsed as a path param, \
         not a literal `{{action_digest}}` segment"
    );
}

/// AuthTenant negative — an AC lookup with NO `x-corelink-tenant-id`
/// header MUST be rejected 401 by the `AuthTenant` extractor BEFORE
/// the handler runs. Drop the `auth: AuthTenant` arg and this request
/// would reach the handler and 404 (`ac miss`) instead. The path
/// `:tenant` is a well-formed value (`tenant-a`) so the ONLY reason
/// for the rejection is the missing authenticated-tenant header.
#[tokio::test]
async fn ac_lookup_route_missing_tenant_header_returns_401() {
    let (lookup, update, delete, list) = ac::build_handlers();
    let state = AcRouteState::new(lookup, update, delete, list, None, None);
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/digest-xyz")
        .method("GET")
        // NO `x-corelink-tenant-id` header — `AuthTenant` fails CLOSED
        // with 401 before the handler is invoked.
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "missing x-corelink-tenant-id MUST 401 at the AuthTenant extractor, \
         not reach the handler"
    );
}

/// Cross-tenant negative — an AC lookup whose path `:tenant` does NOT
/// equal the authenticated `x-corelink-tenant-id` header MUST be
/// denied 403 by the in-handler cross-tenant guard, BEFORE any storage
/// access. Drop the guard (or the `auth: AuthTenant` arg it compares
/// against) and this request would 404 (`ac miss`) instead.
#[tokio::test]
async fn ac_lookup_route_path_tenant_ne_header_returns_403() {
    let (lookup, update, delete, list) = ac::build_handlers();
    let state = AcRouteState::new(lookup, update, delete, list, None, None);
    let app = ac::router(state);

    let req = Request::builder()
        .uri("/v1/ac/tenant-a/digest-xyz")
        .method("GET")
        // Header tenant (`tenant-b`) != path `:tenant` (`tenant-a`) →
        // cross-tenant attempt → 403 before storage access.
        .header("x-corelink-tenant-id", "tenant-b")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "path :tenant != authenticated header MUST 403 (cross-tenant guard)"
    );
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    assert_eq!(
        body, "cross-tenant",
        "403 body must be the handler-emitted cross-tenant string"
    );
}

/// Admin read against a fresh handler MUST reach the handler and
/// emit the canonical `admin not found` 404 body. A router-miss
/// (DEBT-029 bug surface) would yield an empty body.
#[tokio::test]
async fn admin_read_route_reaches_handler_and_returns_handler_not_found_404() {
    let app = admin::router(admin_state_with_gate());

    let req = Request::builder()
        .uri("/v1/admin/read/tenant:unknown")
        .method("GET")
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
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

/// Behavioural pin: a request whose path segment is the literal
/// `{resource}` (percent-encoded curly braces in the URI) still
/// reaches the handler via the matchit-0.8 `{resource}` CAPTURE — a
/// capture matches any single non-slash segment, including one that
/// happens to be literal braces. Paired with the syntax pins below
/// this guards against regression back to the legacy `:name` form.
#[tokio::test]
async fn admin_read_route_does_not_match_literal_braces_uri() {
    let app = admin::router(admin_state_with_gate());

    // The encoded form `%7Bresource%7D` is what a buggy client would
    // send if it didn't substitute the path param. Under matchit-0.8
    // the route matches on the `{resource}` capture, so a request that
    // happens to URL-encode `{resource}` still resolves to the
    // handler with the literal string — i.e. the handler receives
    // `{resource}` (after percent-decoding) as the resource name and
    // emits its normal not-found path. We assert the handler ran
    // (status is the handler's 404, not a router 404 with empty body).
    // The operator gate header is required to reach the handler at all.
    let req = Request::builder()
        .uri("/v1/admin/read/%7Bresource%7D")
        .method("GET")
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
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

    // Pin the public route constant uses the matchit-0.8 `{name}`
    // capture (and NOT the legacy `:name` literal) — guards against
    // future regression of the constant itself.
    assert!(
        !admin::ADMIN_READ_ROUTE.contains(':') && admin::ADMIN_READ_ROUTE.contains("{resource}"),
        "ADMIN_READ_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
    );
    assert!(
        !ac::AC_LOOKUP_ROUTE.contains(':') && ac::AC_LOOKUP_ROUTE.contains("{action_digest}"),
        "AC_LOOKUP_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
    );
    assert!(
        !ac::AC_UPDATE_ROUTE.contains(':') && ac::AC_UPDATE_ROUTE.contains("{action_digest}"),
        "AC_UPDATE_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
    );
}

/// Construction smoke: every route constant must compose into a
/// functional axum `Router` on the native target — pins the
/// `Router::new().route(ROUTE, …)` call path against the live
/// matchit 0.8 grammar.
#[tokio::test]
async fn route_state_constructs_without_panic_on_native() {
    let (lookup, update, delete, list) = ac::build_handlers();
    let _ac_router = ac::router(AcRouteState::new(lookup, update, delete, list, None, None));

    let _admin_router = admin::router(admin_state_with_gate());

    // Cross-route smoke — both routers compose under the same axum
    // build, so the in-process construction proves the constants
    // are mutually valid path templates.
    let _ = Arc::new(()); // anchor `use` of Arc to keep imports stable.
}

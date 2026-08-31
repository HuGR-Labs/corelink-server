//! The property: **once the scope check passes, EVERY credential the
//! verifier cannot vouch for terminates as a flat 401 — and it terminates
//! after actually reaching the adapter.**
//!
//! Two things are being fixed at once, and the second is the easy one to
//! lose. The first is the uniform verdict: absent token, wrong-prefix token,
//! and well-formed-but-unknown token must be indistinguishable to the
//! caller, so the surface cannot be used to enumerate which token shapes
//! exist (the container's sibling symmetry, `INV-AUTH-PAT-OVERLOAD-SHED-
//! UNIFORM`, makes the same argument for the 503 shed).
//!
//! The second is that the 401 must come from the RESOLVER, not from a
//! routing accident. `unknown_pat_is_401_resolver_runs` sends a
//! `corelink_`-prefixed, HMAC-invalid token precisely so the request has to
//! traverse `nest_service` and land on the adapter's auth shim before it
//! fails. A tenant-strip regression that 404'd everything would still leave
//! the other cases "passing" for the wrong reason; that one would not.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use axum::http::StatusCode;
use tower::ServiceExt; // for `.oneshot`

use super::tests_support::{get, router_rejecting, SCOPE_RW};

#[tokio::test]
async fn missing_pat_is_401_reaches_adapter() {
    // scope present → gate passes → adapter authenticate fails → 401.
    let app = router_rejecting();
    let resp = app
        .oneshot(get("/pip/t/simple/requests/", None, Some(SCOPE_RW)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_prefix_pat_is_401() {
    // After the prefix-fix the adapter rejects a non-`corelink_` token at
    // the auth shim. (Before the fix, pip deferred the prefix check to the
    // resolver, which the EmptyLookup still rejects → 401 either way.)
    let app = router_rejecting();
    let resp = app
        .oneshot(get(
            "/pip/t/simple/requests/",
            Some("ghp_github"),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_pat_is_401_resolver_runs() {
    // corelink_-prefixed but HMAC-invalid → reaches the resolver (proves
    // nest_service routed to the adapter) → InvalidPat → 401.
    let app = router_rejecting();
    let resp = app
        .oneshot(get(
            "/pip/t/simple/requests/",
            Some("corelink_not-a-real-token"),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

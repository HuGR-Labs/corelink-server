//! The property: **`pip_gate` decides admission from the server-trusted
//! scope header and the HTTP method, and an unmapped method is denied
//! rather than routed.**
//!
//! Both halves are security properties, not routing trivia. The scope check
//! is the only thing standing between a read-scoped PAT and the cache
//! surface. The method check is fail-CLOSED by construction: the pip adapter
//! only routes GET today, so a DELETE would 405 downstream anyway — which is
//! exactly why the gate must refuse it ITSELF. If the gate fell through on
//! unmapped methods, a future adapter route could ship without anyone making
//! an explicit scope decision for it, and nothing would notice.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use axum::body::Body;
use axum::http::{Method, Request as HttpRequest, StatusCode};
use tower::ServiceExt; // for `.oneshot`

use super::tests_support::{get, router_rejecting, SCOPE_RW};
use super::SCOPE_HEADER;

#[tokio::test]
async fn missing_scope_is_403_at_the_gate() {
    let app = router_rejecting();
    let resp = app
        .oneshot(get(
            "/pip/t/simple/requests/",
            Some("corelink_whatever"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unmapped_methods_are_403_even_with_rw_scope() {
    // Fail-CLOSED method gate: DELETE/PATCH carry a FULL cas:rw scope but
    // are not a mapped cache operation, so the gate must deny them (403)
    // rather than fall through to downstream routing.
    for method in [Method::DELETE, Method::PATCH] {
        let app = router_rejecting();
        let req = HttpRequest::builder()
            .method(method.clone())
            .uri("/pip/t/simple/requests/")
            .header("authorization", "Bearer corelink_whatever")
            .header(SCOPE_HEADER, SCOPE_RW)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "{method} with cas:rw must fail closed at the gate"
        );
    }
}

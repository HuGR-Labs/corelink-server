//! The dedicated `BILLING_INGEST_AUTH_KEY` is the ONLY key to this route:
//! absent, the route is not mounted at all; present but not matched by the
//! request, the request is 401 and nothing is staged.
//!
//! Both ends are the same fail-CLOSED posture, and it is a money property, not
//! a hygiene one: this endpoint writes the rows a tenant is measured on, so an
//! unauthenticated caller that reached it could mint usage.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use tower::ServiceExt;

use super::tests_support::{hex64, ingest_request, record_json, state_with, tenant_a, FakeStore};
use super::*;

#[tokio::test]
async fn missing_service_secret_returns_401() {
    let app = router(state_with(Arc::new(FakeStore::new())));
    let req = ingest_request(
        None,
        serde_json::json!([record_json(&tenant_a(), &hex64(0xAB))]),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_service_secret_returns_401() {
    let app = router(state_with(Arc::new(FakeStore::new())));
    let req = ingest_request(
        Some("wrong-secret-which-is-also-32-chars!!"),
        serde_json::json!([record_json(&tenant_a(), &hex64(0xAB))]),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn build_state_returns_none_when_secret_absent() {
    if std::env::var("BILLING_INGEST_AUTH_KEY").is_err() {
        assert!(build_state_from_env().is_none());
    }
}

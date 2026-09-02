//! A fault that makes the BATCH ITSELF un-interpretable rejects the whole
//! request with 400 and stages nothing.
//!
//! The counterpart of `tests_record_skip`: there, one bad record among many is
//! skipped so the batch still drains. Here there is no batch left to drain —
//! an empty array, a body that is not JSON, or an `event_kind` outside the
//! canonical enum (which fails at deserialization, before any per-record
//! validation runs). Retrying such a request can never succeed, so 400 is what
//! tells the runner to drop it rather than retain it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use axum::body::Body;
use axum::http::{self, Request};
use tower::ServiceExt;

use super::tests_support::{
    hex64, ingest_request, record_json, state_with, tenant_a, FakeStore, TEST_AUTH_KEY,
};
use super::*;

#[tokio::test]
async fn empty_batch_is_400() {
    let app = router(state_with(Arc::new(FakeStore::new())));
    let resp = app
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), serde_json::json!([])))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_json_is_400() {
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/internal/v1/billing/usage")
        .header("content-type", "application/json")
        .header("x-corelink-internal-auth", TEST_AUTH_KEY)
        .body(Body::from("{not-json"))
        .unwrap();
    let app = router(state_with(Arc::new(FakeStore::new())));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_event_kind_is_400() {
    let mut rec = record_json(&tenant_a(), &hex64(0x11));
    rec["event_kind"] = serde_json::json!("not_a_real_kind");
    let app = router(state_with(Arc::new(FakeStore::new())));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([rec]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

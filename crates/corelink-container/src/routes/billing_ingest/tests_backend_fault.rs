//! A staging-backend failure surfaces as 503 — never as a 202 that tells the
//! runner a batch landed when nothing was written.
//!
//! One test, one property, and it is the one that decides whether unbilled
//! work is silently discarded: 503 is what makes the runner keep the buffer
//! and push it again once D1 is reachable. A swallowed store error would look
//! exactly like a successful ingest from the caller's side.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use tower::ServiceExt;

use super::tests_support::{
    hex64, ingest_request, record_json, state_with, tenant_a, FakeStore, TEST_AUTH_KEY,
};
use super::*;

#[tokio::test]
async fn backend_fault_returns_503() {
    let app = router(state_with(Arc::new(FakeStore::failing("d1 unreachable"))));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json(&tenant_a(), &hex64(0x17))]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

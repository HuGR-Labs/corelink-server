//! Blob-upload OOM-DoS regression tests (enterprise-DD HIGH).
//!
//! The blob `PATCH` / `PUT` body is ATTACKER-CONTROLLED: without a cap the
//! handler buffered it via `to_bytes(body, usize::MAX)`, so any authenticated
//! tenant could drive an unbounded single allocation (multi-GB heap → OOM of
//! the shared container process). The fix caps the per-request body at the
//! configured `blob_size_limit_bytes` and returns `413 Payload Too Large`
//! BEFORE the allocation completes.
//!
//! These tests pin a tiny `blob_size_limit_bytes` so the over-cap path can be
//! exercised without allocating a multi-GiB body.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

#[path = "oci_common.rs"]
mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt as _;

use common::TestRig;
use corelink_adapter_host::oci::auth::OciScope;
use corelink_adapter_host::oci::config::defaults;

/// Tiny blob ceiling used by the DoS tests (16 bytes).
const TINY_BLOB_LIMIT: u64 = 16;

fn token(rig: &TestRig) -> String {
    rig.mint_token(&OciScope::new(
        "foo/bar",
        vec![String::from("push"), String::from("pull")],
    ))
}

/// Open a blob-upload session and return its UUID.
async fn open_session(rig: &TestRig, bearer: &str) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/bar/blobs/uploads/")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("post-open");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    resp.headers()
        .get("Location")
        .expect("Location")
        .to_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string()
}

/// A `PATCH` body over the configured cap is rejected with `413` (NOT OOM /
/// NOT a 500) before the unbounded buffer is ever allocated.
#[tokio::test]
async fn patch_body_over_cap_returns_413_not_oom() {
    let rig = TestRig::with_blob_limit(TINY_BLOB_LIMIT);
    let bearer = token(&rig);
    let uuid = open_session(&rig, &bearer).await;

    // Body is well over the 16-byte cap (but trivially small for the test
    // process — the point is the cap fires on the size hint, not the bytes).
    let oversized = vec![0u8; (TINY_BLOB_LIMIT as usize) + 4096];
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/v2/foo/bar/blobs/uploads/{uuid}"))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(oversized))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("patch");
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "over-cap PATCH body must be 413, never OOM/500"
    );
}

/// A `PUT` finalize whose trailing chunk is over the cap is likewise `413`.
#[tokio::test]
async fn put_trailing_chunk_over_cap_returns_413() {
    let rig = TestRig::with_blob_limit(TINY_BLOB_LIMIT);
    let bearer = token(&rig);
    let uuid = open_session(&rig, &bearer).await;

    let oversized = vec![0u8; (TINY_BLOB_LIMIT as usize) + 4096];
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/v2/foo/bar/blobs/uploads/{uuid}?digest=sha256:{}",
            "0".repeat(64)
        ))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(oversized))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "over-cap PUT trailing chunk must be 413"
    );
}

/// A `PATCH` body within the cap succeeds (202) — the cap does not reject
/// legitimate uploads.
#[tokio::test]
async fn patch_body_within_cap_succeeds() {
    let rig = TestRig::with_blob_limit(TINY_BLOB_LIMIT);
    let bearer = token(&rig);
    let uuid = open_session(&rig, &bearer).await;

    let within = vec![0u8; (TINY_BLOB_LIMIT as usize) - 1];
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/v2/foo/bar/blobs/uploads/{uuid}"))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(within))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("patch");
    assert_eq!(
        resp.status(),
        StatusCode::ACCEPTED,
        "within-cap PATCH body must be accepted (202)"
    );
}

/// The cap bound (the configured/default max-blob ceiling that bounds the
/// single `to_bytes` allocation) is finite and sane: non-zero, and within a
/// realistic range for an OCI image layer. Pins the value so a corruption of
/// the `5 * 1024 * 1024 * 1024` default is caught.
#[test]
fn blob_cap_is_finite_and_sane() {
    let cap = defaults::BLOB_SIZE_LIMIT_BYTES;
    assert_eq!(cap, 5 * 1024 * 1024 * 1024, "default cap is 5 GiB");
    assert!(cap > 1024 * 1024, "cap must allow a realistic layer (>1 MiB)");
    assert!(
        cap <= 64 * 1024 * 1024 * 1024,
        "cap must stay finite + bounded (<=64 GiB)"
    );
    // usize-convertible on the 64-bit container target (the actual `to_bytes`
    // bound is `usize`); never falls back to the unbounded sentinel there.
    assert!(usize::try_from(cap).is_ok(), "cap fits usize on 64-bit");
}

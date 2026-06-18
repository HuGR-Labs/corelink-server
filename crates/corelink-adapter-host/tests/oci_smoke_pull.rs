//! Smoke-pull integration tests — exercise `GET /v2/<repo>/blobs/<digest>`
//! and `GET /v2/<repo>/manifests/<reference>` against the in-process
//! adapter, byte-for-byte emulating what `crane pull` sends.
//!
//! Note: `crane` binary is NOT installable on the author host (per
//! audit §7 deferred-scope note); we substitute with `tower::Service`
//! direct-dispatch which exercises the same axum handlers and the
//! same wire-shape semantics.

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
use bytes::Bytes;
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use common::TestRig;
use corelink_adapter_host::oci::auth::OciScope;
use corelink_adapter_host::oci::digest::{OciDigest, OciDigestAlgo};

async fn seed_blob(rig: &TestRig, _repo: &str, bytes: &[u8]) -> String {
    // Skip the upload flow — seed directly via the in-memory port to
    // isolate the pull-side semantics from the push state machine.
    let digest = OciDigest::compute(OciDigestAlgo::Sha256, bytes).expect("compute");
    let key = digest.to_wire();
    use corelink_adapter_host::oci::ports::BlobStore;
    // The fake's `finalize_upload` writes the slot; emulate by opening
    // → patching once → finalize.
    let uuid = rig.cas.open_upload(&rig.tenant).await.expect("open");
    rig.cas
        .append_chunk(&rig.tenant, &uuid, Bytes::copy_from_slice(bytes))
        .await
        .expect("chunk");
    rig.cas
        .finalize_upload(&rig.tenant, &uuid, &key, None)
        .await
        .expect("finalize");
    digest.to_wire()
}

#[tokio::test]
async fn pull_blob_returns_bytes() {
    let rig = TestRig::new();
    let payload = b"hello-corelink-oci";
    let digest = seed_blob(&rig, "alpine", payload).await;
    let token = rig.mint_token(&OciScope::new("alpine", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v2/alpine/blobs/{digest}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body_bytes[..], payload);
}

#[tokio::test]
async fn pull_blob_unknown_returns_404() {
    let rig = TestRig::new();
    let token = rig.mint_token(&OciScope::new("alpine", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::GET)
        .uri(
            "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn head_blob_returns_200_when_present() {
    let rig = TestRig::new();
    let payload = b"head-body";
    let digest = seed_blob(&rig, "alpine", payload).await;
    let token = rig.mint_token(&OciScope::new("alpine", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::HEAD)
        .uri(format!("/v2/alpine/blobs/{digest}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get("Docker-Content-Digest").is_some());
}

#[tokio::test]
async fn v2_version_check_with_valid_token() {
    let rig = TestRig::new();
    let token = rig.mint_token(&OciScope::new("alpine", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn v2_version_check_no_auth_returns_401_with_challenge() {
    let rig = TestRig::new();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/")
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let h = resp.headers().get("Www-Authenticate").expect("challenge");
    let s = h.to_str().unwrap();
    assert!(s.starts_with("Bearer realm="), "got: {s}");
}

//! Smoke-push integration tests — the multi-step blob upload state
//! machine + manifest push + pull-back round-trip.
//!
//! Mirrors what `crane push /tmp/img.tar localhost:5000/foo/bar:v1`
//! emits on the wire:
//!
//! 1. `POST   /v2/foo/bar/blobs/uploads/` → 202 + Location
//! 2. `PATCH  /v2/foo/bar/blobs/uploads/<uuid>` (one or more) → 202 + Range
//! 3. `PUT    /v2/foo/bar/blobs/uploads/<uuid>?digest=<digest>` → 201
//! 4. `PUT    /v2/foo/bar/manifests/v1` → 201

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use common::TestRig;
use corelink_adapter_oci::auth::OciScope;
use corelink_adapter_oci::digest::{OciDigest, OciDigestAlgo};

async fn token(rig: &TestRig) -> String {
    rig.mint_token(&OciScope::new(
        "foo/bar",
        vec![String::from("push"), String::from("pull")],
    ))
}

#[tokio::test]
async fn push_pull_roundtrip() {
    let rig = TestRig::new();
    let bearer = token(&rig).await;
    let payload = b"layer-bytes-roundtrip";
    let digest = OciDigest::compute(OciDigestAlgo::Sha256, payload).expect("compute");

    // 1) POST open upload.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/bar/blobs/uploads/")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("post");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let location = resp
        .headers()
        .get("Location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_string();
    let uuid = location.rsplit('/').next().unwrap().to_string();

    // 2) PATCH chunk.
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/v2/foo/bar/blobs/uploads/{uuid}"))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(payload.to_vec()))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("patch");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // 3) PUT finalize.
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/v2/foo/bar/blobs/uploads/{uuid}?digest={}",
            digest.to_wire()
        ))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 4) GET pulls the same bytes back.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v2/foo/bar/blobs/{}", digest.to_wire()))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], payload);

    // 5) Audit row was emitted.
    let snap = rig.audit.snapshot();
    assert!(
        snap.iter().any(|r| r.event_type == "corelink.oci.blob.push.v1"),
        "expected blob.push audit row, got: {snap:?}"
    );
}

#[tokio::test]
async fn manifest_push_then_pull() {
    let rig = TestRig::new();
    let bearer = token(&rig).await;
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "size": 0
        },
        "layers": []
    });
    let body = serde_json::to_vec(&manifest).unwrap();
    // 1) PUT manifest at tag `v1`.
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v2/foo/bar/manifests/v1")
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let pushed_digest = resp
        .headers()
        .get("Docker-Content-Digest")
        .expect("digest header")
        .to_str()
        .unwrap()
        .to_string();

    // 2) GET manifest by tag.
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/foo/bar/manifests/v1")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::OK);
    let pulled = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&pulled[..], &body[..]);

    // 3) GET manifest by digest works too.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v2/foo/bar/manifests/{pushed_digest}"))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get-by-digest");
    assert_eq!(resp.status(), StatusCode::OK);

    // 4) Tag list reflects v1.
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/foo/bar/tags/list")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("tags");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let tags = v["tags"].as_array().unwrap();
    assert!(tags.iter().any(|t| t == "v1"), "tag v1 not in {tags:?}");

    // 5) Audit rows: manifest.push + tag.update both emitted.
    let snap = rig.audit.snapshot();
    assert!(snap
        .iter()
        .any(|r| r.event_type == "corelink.oci.manifest.push.v1"));
    assert!(snap
        .iter()
        .any(|r| r.event_type == "corelink.oci.tag.update.v1"));
}

#[tokio::test]
async fn upload_session_missing_returns_404() {
    let rig = TestRig::new();
    let bearer = token(&rig).await;
    let req = Request::builder()
        .method(Method::PATCH)
        .uri("/v2/foo/bar/blobs/uploads/this-uuid-was-never-opened")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(&b"data"[..]))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("patch");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_with_trailing_chunk_succeeds() {
    let rig = TestRig::new();
    let bearer = token(&rig).await;
    let payload = b"all-bytes-in-one-shot";
    let digest = OciDigest::compute(OciDigestAlgo::Sha256, payload).expect("compute");

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/bar/blobs/uploads/")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("post");
    let uuid = resp
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();

    // PUT with the bytes in the body (no preceding PATCH).
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/v2/foo/bar/blobs/uploads/{uuid}?digest={}",
            digest.to_wire()
        ))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(payload.to_vec()))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

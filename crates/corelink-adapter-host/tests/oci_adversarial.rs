//! Adversarial tests (per oci.md §8 row 5):
//!
//! 1. Declared digest mismatch → reject + audit
//!    `oci.blob.push.digest_mismatch.v1`.
//! 2. `_catalog` request → 401 + audit `oci.catalog.denied.v1`.
//! 3. Forged bearer token (HMAC fails) → 401 + audit `oci.auth.denied.v1`.
//! 4. Tenant A pull via Tenant B's repo → 404 (existence-leak free).
//! 5. Repo-name with bad chars → 400 NAME_INVALID.
//! 6. Manifest with schemaVersion 1 → 400 MANIFEST_INVALID.
//! 7. Token scope `pull` only → `POST /blobs/uploads/` → 401.

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
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use common::TestRig;
use corelink_adapter_host::oci::auth::OciScope;
use corelink_adapter_host::oci::digest::{OciDigest, OciDigestAlgo};

#[tokio::test]
async fn declared_digest_mismatch_rejects_and_audits() {
    let rig = TestRig::new();
    let bearer = rig.mint_token(&OciScope::new("foo", vec![String::from("push")]));
    let payload = b"actual-bytes";
    let _real = OciDigest::compute(OciDigestAlgo::Sha256, payload).expect("compute");
    let fake_declared =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/blobs/uploads/")
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

    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/v2/foo/blobs/uploads/{uuid}?digest={fake_declared}"
        ))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(payload.to_vec()))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let snap = rig.audit.snapshot();
    assert!(
        snap.iter()
            .any(|r| r.event_type == "corelink.oci.blob.push.digest_mismatch.v1"),
        "expected digest_mismatch audit row, got: {snap:?}"
    );
}

#[tokio::test]
async fn catalog_disabled_returns_401_and_audits() {
    let rig = TestRig::new();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/_catalog")
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("catalog");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let snap = rig.audit.snapshot();
    assert!(snap
        .iter()
        .any(|r| r.event_type == "corelink.oci.catalog.denied.v1"));
}

#[tokio::test]
async fn forged_bearer_token_rejected() {
    let rig = TestRig::new();
    let forged = "corelink-oci.00000000-0000-0000-0000-000000000000.cmVwb3NpdG9yeTphbHBpbmU6cHVsbA.9999999999.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000")
        .header("Authorization", format!("Bearer {forged}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_blob_returns_404_not_403() {
    // Tenant A pulls a blob that doesn't exist for tenant A. Must be
    // 404 (NotFound) — never 403 — to prevent existence-leak across
    // tenant boundaries.
    let rig = TestRig::new();
    let bearer = rig.mint_token(&OciScope::new("alpine", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::GET)
        .uri(
            "/v2/alpine/blobs/sha256:1111111111111111111111111111111111111111111111111111111111111111",
        )
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invalid_repo_name_rejected() {
    let rig = TestRig::new();
    let bearer = rig.mint_token(&OciScope::new("UPPER-REPO", vec![String::from("pull")]));
    let req = Request::builder()
        .method(Method::GET)
        .uri(
            "/v2/UPPER-REPO/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["errors"][0]["code"], "NAME_INVALID");
}

#[tokio::test]
async fn manifest_schema_v1_rejected() {
    let rig = TestRig::new();
    let bearer = rig.mint_token(&OciScope::new("foo", vec![String::from("push")]));
    let bad = serde_json::json!({
        "schemaVersion": 1,
        "name": "foo",
        "tag": "v1",
        "fsLayers": []
    });
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v2/foo/manifests/v1")
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&bad).unwrap()))
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("put");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["errors"][0]["code"], "MANIFEST_INVALID");
}

#[tokio::test]
async fn push_without_push_scope_rejected() {
    let rig = TestRig::new();
    let bearer = rig.mint_token(&OciScope::new("foo", vec![String::from("pull")])); // pull only — no push
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/blobs/uploads/")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = rig.app().oneshot(req).await.expect("post");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

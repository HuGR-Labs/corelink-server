//! `POST /token` — the OAuth2 (`grant_type=password`) token endpoint that
//! real `docker push` uses, end-to-end.
//!
//! The classic `GET /token` (Basic header) endpoint is exercised by the
//! pull/login smoke tests; THIS file pins the wire shape `docker push`
//! actually emits: an `application/x-www-form-urlencoded` body with
//! `grant_type=password`, `service`, `scope`, `username`, `password` (the
//! PAT). Before the fix the route registered only `GET /token`, so this
//! POST returned `405 Method Not Allowed` and `docker push` could never
//! obtain a bearer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use corelink_adapter_host::oci::config::defaults;
use corelink_adapter_host::oci::digest::{OciDigest, OciDigestAlgo};
use corelink_adapter_host::oci::ports::testing::{InMemoryBlobStore, InMemoryKv};
use corelink_adapter_host::oci::ports::{PortResult, ResolvedPat, TenantResolver};
use corelink_adapter_host::oci::{router, AppState, OciAdapterConfig};
use corelink_audit::ports::InMemoryAuditEmitter;
use corelink_core::{SecretWrap, TenantId};

const FIXED_NOW_MS: u64 = 1_700_000_000_000;

fn fixed_clock_ms() -> u64 {
    FIXED_NOW_MS
}

/// A resolver that maps one fixed PAT string to a tenant with a chosen
/// write capability — lets the test drive a real WRITE-capable push flow
/// (the shared `StaticTenantResolver`'s default `resolve_pat_capability`
/// reports `can_write = false`).
#[derive(Debug)]
struct WriteCapableResolver {
    pat: String,
    tenant: TenantId,
    can_write: bool,
}

#[async_trait::async_trait]
impl TenantResolver for WriteCapableResolver {
    async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId> {
        if pat.expose() == self.pat {
            Ok(self.tenant)
        } else {
            Err(String::from("unknown pat"))
        }
    }

    async fn resolve_pat_capability(&self, pat: &SecretWrap) -> PortResult<ResolvedPat> {
        let tenant = self.resolve_pat(pat).await?;
        Ok(ResolvedPat {
            tenant,
            can_write: self.can_write,
            // Genuine-unlimited sentinel so the in-memory blob store (which
            // ignores the cap) accepts the push.
            storage_cap_bytes: Some(0),
        })
    }
}

fn rig(pat: &str, can_write: bool) -> (AppState, Arc<InMemoryAuditEmitter>) {
    let cas: Arc<InMemoryBlobStore> = Arc::new(InMemoryBlobStore::default());
    let kv: Arc<InMemoryKv> = Arc::new(InMemoryKv::default());
    let audit: Arc<InMemoryAuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
    let resolver = Arc::new(WriteCapableResolver {
        pat: pat.to_string(),
        tenant: TenantId::from_uuid(uuid::Uuid::nil()),
        can_write,
    });
    let cfg = OciAdapterConfig::new(
        ([127u8, 0, 0, 1], 0).into(),
        String::from("http://localhost:5000/token"),
        defaults::BLOB_SIZE_LIMIT_BYTES,
        defaults::MULTIPART_CHUNK_SIZE_BYTES,
        false,
        defaults::TOKEN_TTL_SECS,
        SecretWrap::new("x".repeat(32)),
        cas,
        kv,
        resolver,
        None,
        audit.clone(),
    );
    (AppState::new(Arc::new(cfg), fixed_clock_ms), audit)
}

fn basic(pat: &str) -> String {
    use base64::Engine as _;
    let enc = base64::engine::general_purpose::STANDARD.encode(format!("hugr:{pat}"));
    format!("Basic {enc}")
}

/// Drive `POST /token` with a urlencoded form body and return the parsed
/// JSON response + the status.
async fn post_token(state: &AppState, form: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(Body::from(form.to_string()))
        .unwrap();
    let resp = router(state.clone())
        .oneshot(req)
        .await
        .expect("post token");
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
async fn post_token_form_password_mints_bearer() {
    let pat = "clp_write_pat";
    let (state, _audit) = rig(pat, true);
    // Exactly the body docker push sends (scope separators percent-encoded).
    let form = "grant_type=password&service=corelink-oci\
&scope=repository%3Afoo%2Fbar%3Apush%2Cpull\
&username=hugr&password=clp_write_pat";
    let (status, json) = post_token(&state, form).await;
    assert_eq!(status, StatusCode::OK, "POST /token must not 405/401");
    let token = json["token"].as_str().expect("token field");
    assert_eq!(json["access_token"].as_str(), Some(token));
    assert!(json["expires_in"].is_number());

    // The minted bearer passes verify with the WRITE-capable push scope.
    let verified = corelink_adapter_host::oci::auth::verify(
        &SecretWrap::new("x".repeat(32)),
        token,
        FIXED_NOW_MS / 1000,
    )
    .expect("verify minted bearer");
    assert!(verified.scope.allows("foo/bar", "push"));
    assert!(verified.scope.allows("foo/bar", "pull"));
}

#[tokio::test]
async fn post_token_basic_header_fallback_mints_bearer() {
    // No form `password` — credentials only in the Basic header (refresh /
    // header-only clients). Must still mint.
    let pat = "clp_write_pat";
    let (state, _audit) = rig(pat, true);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Authorization", basic(pat))
        .body(Body::from(
            "grant_type=password&service=corelink-oci&scope=repository%3Afoo%2Fbar%3Apull",
        ))
        .unwrap();
    let resp = router(state.clone()).oneshot(req).await.expect("post");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn post_token_readonly_pat_is_downscoped_to_pull() {
    // A read-only PAT requesting push must get a PULL-only bearer (no scope
    // escalation) — same downscope the GET path applies.
    let pat = "clp_readonly_pat";
    let (state, _audit) = rig(pat, false);
    let form = "grant_type=password&service=corelink-oci\
&scope=repository%3Afoo%2Fbar%3Apush%2Cpull&username=hugr&password=clp_readonly_pat";
    let (status, json) = post_token(&state, form).await;
    assert_eq!(status, StatusCode::OK);
    let token = json["token"].as_str().expect("token");
    let verified = corelink_adapter_host::oci::auth::verify(
        &SecretWrap::new("x".repeat(32)),
        token,
        FIXED_NOW_MS / 1000,
    )
    .expect("verify");
    assert!(verified.scope.allows("foo/bar", "pull"));
    assert!(
        !verified.scope.allows("foo/bar", "push"),
        "read-only PAT must NOT obtain a push bearer via POST /token"
    );
}

#[tokio::test]
async fn post_token_no_credentials_is_401_not_405() {
    let (state, _audit) = rig("clp_pat", true);
    // No `password` form field, no Basic header → 401 (NOT 405: the route
    // exists, the request is just unauthenticated).
    let (status, _json) = post_token(&state, "grant_type=password&service=corelink-oci").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// The FULL real-docker push flow driven through `POST /token`: exchange
/// the PAT for a bearer, then push a blob + manifest with it. This is the
/// end-to-end proof that `docker push` works (it previously dead-ended at
/// the `405` on `POST /token`).
#[tokio::test]
async fn full_docker_push_flow_via_post_token() {
    let pat = "clp_write_pat";
    let (state, audit) = rig(pat, true);
    let form = "grant_type=password&service=corelink-oci\
&scope=repository%3Afoo%2Fbar%3Apush%2Cpull&username=hugr&password=clp_write_pat";
    let (status, json) = post_token(&state, form).await;
    assert_eq!(status, StatusCode::OK);
    let bearer = json["token"].as_str().expect("token").to_string();

    let payload = b"docker-push-via-post-token";
    let digest = OciDigest::compute(OciDigestAlgo::Sha256, payload).expect("compute");

    // POST open upload.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v2/foo/bar/blobs/uploads/")
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let resp = router(state.clone()).oneshot(req).await.expect("post open");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
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

    // PUT finalize with the bytes in the body.
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "/v2/foo/bar/blobs/uploads/{uuid}?digest={}",
            digest.to_wire()
        ))
        .header("Authorization", format!("Bearer {bearer}"))
        .body(Body::from(payload.to_vec()))
        .unwrap();
    let resp = router(state.clone()).oneshot(req).await.expect("put blob");
    assert_eq!(resp.status(), StatusCode::CREATED);

    // PUT manifest.
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": digest.to_wire(),
            "size": payload.len()
        },
        "layers": []
    });
    let mbody = serde_json::to_vec(&manifest).unwrap();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v2/foo/bar/manifests/v1")
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .body(Body::from(mbody))
        .unwrap();
    let resp = router(state.clone())
        .oneshot(req)
        .await
        .expect("put manifest");
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Audit row for the blob push was emitted.
    let snap = audit.snapshot();
    assert!(snap
        .iter()
        .any(|r| r.event_type == "corelink.oci.blob.push.v1"));
}

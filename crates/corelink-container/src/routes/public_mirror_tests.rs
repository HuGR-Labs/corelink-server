#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use axum::body::Body;
use corelink_adapter_host::oci::digest::OciDigestAlgo;
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler,
    CasWriteRequest, CasWriteResponse,
};
use http::Request;
use tower::ServiceExt;

use crate::adapter_cache::UrlMapStore;

use super::*;

#[test]
fn bearer_param_extracts_dockerhub_service_not_blob_host() {
    // The real Docker Hub blob-401 challenge: the blob HOST is
    // `registry-1.docker.io` but the token `service` is `registry.docker.io`
    // (no `-1`). The fetcher MUST echo this `service`, not re-derive it from
    // the upstream host, or auth.docker.io mints a token the registry 401s.
    let header = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#;
    assert_eq!(
        parse_bearer_realm(header).as_deref(),
        Some("https://auth.docker.io/token"),
    );
    assert_eq!(
        parse_bearer_param(header, "service").as_deref(),
        Some("registry.docker.io"),
    );
    assert_eq!(
        parse_bearer_param(header, "scope").as_deref(),
        Some("repository:library/alpine:pull"),
    );
    assert_eq!(parse_bearer_param(header, "absent"), None);
    assert_eq!(parse_bearer_param("Basic realm=\"x\"", "service"), None);
}

const TEST_KEY: &str = "test-admin-secret-key-at-least-32-chars!!";
/// A real alpine amd64 rootfs layer digest (the commented example pin in the
/// shipped manifest). Used ONLY as an allowlist key in-test — never fetched
/// (the fake fetcher returns canned bytes).
const ALPINE_DIGEST: &str =
    "sha256:25f1d6b1951ac8eb3740558fe94cb83d377bdadf95fd9f98b50d2e1b96130471";

/// Canonical `sha256:<hex>` wire digest of `bytes` (via the same
/// `OciDigest` compute the verify path uses).
fn sha256_wire(bytes: &[u8]) -> String {
    OciDigest::compute(OciDigestAlgo::Sha256, bytes)
        .expect("sha256 compute")
        .to_wire()
}

/// Fake fetcher: hands back canned bytes for any (repo, digest), or a fixed
/// error. Records the last (repo, digest) it was asked for.
#[derive(Debug)]
struct FakeFetcher {
    result: Result<Vec<u8>, String>,
    calls: Mutex<Vec<(String, String)>>,
}
impl FakeFetcher {
    fn returning(bytes: Vec<u8>) -> Self {
        Self {
            result: Ok(bytes),
            calls: Mutex::new(Vec::new()),
        }
    }
    fn failing(msg: &str) -> Self {
        Self {
            result: Err(msg.to_owned()),
            calls: Mutex::new(Vec::new()),
        }
    }
}
#[async_trait]
impl UpstreamBlobFetcher for FakeFetcher {
    async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
        self.calls
            .lock()
            .unwrap()
            .push((repository.to_owned(), digest.to_owned()));
        self.result.clone()
    }
}

/// In-memory url→content-hash map recording (namespace, url_hash).
#[derive(Default, Debug)]
struct FakeMap(Mutex<HashMap<(String, String), String>>);
#[async_trait]
impl UrlMapStore for FakeMap {
    async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(ns.to_owned(), url_hash.to_owned()))
            .cloned())
    }
    async fn put(
        &self,
        ns: &str,
        url_hash: &str,
        content_hash: &str,
        _len: u64,
    ) -> Result<(), String> {
        self.0.lock().unwrap().insert(
            (ns.to_owned(), url_hash.to_owned()),
            content_hash.to_owned(),
        );
        Ok(())
    }
}

/// Non-verifying in-memory CAS keyed by (namespace, claimed_hash). Records
/// every stored blob so a test can assert the `_public` write happened.
#[derive(Debug, Default)]
struct RecordingCas(Mutex<HashMap<(String, String), Vec<u8>>>);
impl CasReadHandler for RecordingCas {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        match self
            .0
            .lock()
            .unwrap()
            .get(&(req.tenant.clone(), req.hash.clone()))
        {
            Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
            None => Err(CasHandlerError::Internal("absent".into())),
        }
    }
}
impl CasWriteHandler for RecordingCas {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        self.0
            .lock()
            .unwrap()
            .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
        Ok(CasWriteResponse::new(req.claimed_hash, true))
    }
}

/// Deterministic non-crypto hasher for the moat's content-address (the moat
/// hasher is orthogonal to the OCI digest verify — that uses real sha256).
fn fake_hash(bytes: &[u8]) -> String {
    format!("h{:08x}", bytes.iter().map(|b| u32::from(*b)).sum::<u32>())
}

/// Build a moat over the recording CAS + fake map.
fn moat(cas: Arc<RecordingCas>, map: Arc<FakeMap>) -> Arc<MoatCache> {
    Arc::new(MoatCache::new(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        map,
        fake_hash,
        MIRROR_SERVICE_PRINCIPAL,
    ))
}

/// Allowlist containing exactly `digests` (hermetic — does NOT depend on the
/// shipped deny-all manifest, mirroring WP-B's `with_allowlist` pattern).
fn allowlist_with(digests: &[&str]) -> Arc<PublicBaseAllowlist> {
    let manifest = digests.join("\n");
    Arc::new(PublicBaseAllowlist::parse(&manifest).expect("test allowlist parses"))
}

fn state(
    moat: Arc<MoatCache>,
    allowlist: Arc<PublicBaseAllowlist>,
    fetcher: Arc<dyn UpstreamBlobFetcher>,
) -> PublicMirrorRouteState {
    PublicMirrorRouteState {
        auth_key: Arc::from(TEST_KEY),
        moat,
        allowlist,
        fetcher,
    }
}

fn promote_req(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(http::Method::POST)
        .uri("/_internal/admin/public-mirror/promote")
        .header("content-type", "application/json");
    if let Some(a) = auth {
        b = b.header("x-corelink-internal-auth", a);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

// ── S0 auth-gate tests (kept green) ──────────────────────────────────────

#[tokio::test]
async fn unauthenticated_is_401() {
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[]),
        Arc::new(FakeFetcher::returning(vec![])),
    );
    let resp = router(st)
        .oneshot(promote_req(
            None,
            serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    // No write occurred.
    assert!(cas.0.lock().unwrap().is_empty());
    assert!(map.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn wrong_secret_is_401() {
    let st = state(
        moat(
            Arc::new(RecordingCas::default()),
            Arc::new(FakeMap::default()),
        ),
        allowlist_with(&[]),
        Arc::new(FakeFetcher::returning(vec![])),
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some("wrong-but-also-32-chars-long-secret!!!"),
            serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── inc4b promote tests ──────────────────────────────────────────────────

#[tokio::test]
async fn allowlisted_digest_promotes_into_public_namespace() {
    // An admin-authed hit with an allowlisted digest whose bytes match →
    // a `_public` map row + CAS blob is written (namespace == PUBLIC_NAMESPACE).
    let bytes = b"alpine-rootfs-layer-bytes".to_vec();
    let digest = sha256_wire(&bytes);
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[&digest]),
        Arc::new(FakeFetcher::returning(bytes.clone())),
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": digest, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // The map row is under PUBLIC_NAMESPACE keyed by the canonical digest.
    let map_guard = map.0.lock().unwrap();
    let content_hash = map_guard
        .get(&(PUBLIC_NAMESPACE.to_owned(), digest.clone()))
        .expect("_public map row written");
    // The CAS blob is stored under PUBLIC_NAMESPACE + that content hash.
    let cas_guard = cas.0.lock().unwrap();
    let stored = cas_guard
        .get(&(PUBLIC_NAMESPACE.to_owned(), content_hash.clone()))
        .expect("_public CAS blob written");
    assert_eq!(stored, &bytes);
}

#[tokio::test]
async fn non_allowlisted_digest_is_rejected_no_write() {
    // Arbitrary (non-allowlisted) digest → 422, NO fetch, NO write.
    let bytes = b"whatever".to_vec();
    let digest = sha256_wire(&bytes);
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let fetcher = Arc::new(FakeFetcher::returning(bytes.clone()));
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[]), // deny-all
        Arc::clone(&fetcher) as Arc<dyn UpstreamBlobFetcher>,
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": digest, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(cas.0.lock().unwrap().is_empty(), "no CAS write");
    assert!(map.0.lock().unwrap().is_empty(), "no map write");
    assert!(
        fetcher.calls.lock().unwrap().is_empty(),
        "allowlist gate rejects BEFORE any upstream fetch"
    );
}

#[tokio::test]
async fn verify_mismatch_fails_closed_no_write() {
    // Bytes that do NOT hash to the (allowlisted) declared digest → 422,
    // fail-closed, NO write. Uses the real alpine digest but the fake fetcher
    // returns different bytes.
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[ALPINE_DIGEST]),
        Arc::new(FakeFetcher::returning(b"not-the-alpine-bytes".to_vec())),
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(cas.0.lock().unwrap().is_empty(), "digest-lie never written");
    assert!(map.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn idempotent_repromote_no_duplicate() {
    // Re-promoting the same bytes succeeds and leaves exactly one `_public`
    // row + one CAS blob (content-addressed, map-upserted).
    let bytes = b"debian-rootfs-layer".to_vec();
    let digest = sha256_wire(&bytes);
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[&digest]),
        Arc::new(FakeFetcher::returning(bytes.clone())),
    );
    let router = router(st);
    for _ in 0..2 {
        let resp = router
            .clone()
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": digest, "repository": "library/debian"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    assert_eq!(
        map.0.lock().unwrap().len(),
        1,
        "exactly one _public map row"
    );
    assert_eq!(cas.0.lock().unwrap().len(), 1, "exactly one CAS blob");
}

#[tokio::test]
async fn upstream_fetch_error_maps_502_no_write() {
    let bytes = b"x".to_vec();
    let digest = sha256_wire(&bytes);
    let cas = Arc::new(RecordingCas::default());
    let map = Arc::new(FakeMap::default());
    let st = state(
        moat(Arc::clone(&cas), Arc::clone(&map)),
        allowlist_with(&[&digest]),
        Arc::new(FakeFetcher::failing("connect refused")),
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": digest, "repository": "library/alpine"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    assert!(cas.0.lock().unwrap().is_empty());
    assert!(map.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_repository_is_rejected_no_fetch() {
    let bytes = b"y".to_vec();
    let digest = sha256_wire(&bytes);
    let fetcher = Arc::new(FakeFetcher::returning(bytes));
    let st = state(
        moat(
            Arc::new(RecordingCas::default()),
            Arc::new(FakeMap::default()),
        ),
        allowlist_with(&[&digest]),
        Arc::clone(&fetcher) as Arc<dyn UpstreamBlobFetcher>,
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": digest, "repository": "../evil"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(
        fetcher.calls.lock().unwrap().is_empty(),
        "repo validation rejects before any fetch"
    );
}

#[tokio::test]
async fn malformed_body_is_400() {
    let st = state(
        moat(
            Arc::new(RecordingCas::default()),
            Arc::new(FakeMap::default()),
        ),
        allowlist_with(&[]),
        Arc::new(FakeFetcher::returning(vec![])),
    );
    let resp = router(st)
        .oneshot(promote_req(
            Some(TEST_KEY),
            serde_json::json!({"digest": ALPINE_DIGEST}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn validate_repository_accepts_and_rejects() {
    for ok in [
        "library/alpine",
        "library/debian",
        "a",
        "a/b/c",
        "foo-bar.baz_qux",
    ] {
        assert!(validate_repository(ok).is_ok(), "{ok} should be valid");
    }
    for bad in [
        "",
        "/leading",
        "trailing/",
        "a//b",
        "../evil",
        "UP",
        "a/../b",
        "has space",
    ] {
        assert!(
            validate_repository(bad).is_err(),
            "{bad} should be rejected"
        );
    }
}

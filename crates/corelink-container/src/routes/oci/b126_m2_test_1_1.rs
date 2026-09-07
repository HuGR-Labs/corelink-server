use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{Method, Request as HttpRequest, StatusCode};
use base64::Engine as _;
use corelink_handler_cas::{
    CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
};
use corelink_pat::{
    mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId, SCOPE_CACHE_R,
    SCOPE_CACHE_RW,
};
use tower::ServiceExt; // for `.oneshot`
use uuid::Uuid;

use crate::adapter_cache::canonical_hash_hex;
use crate::adapter_pat::{PatRow, PatRowLookup};
use crate::scope::SCOPE_HEADER;

use super::*;

const SCOPE_RW: &str = "cas:rw";
/// 32-byte raw HMAC key for the adapter's session tokens (config
/// `sanity_check` requires ≥32 bytes).
const OCI_KEY: &str = "oci-session-key-0123456789abcdef"; // 32 bytes

/// `PatRowLookup` that knows ONE token_id → row; everything else
/// unknown.
struct OneTokenLookup {
    token_id: String,
    row: PatRow,
}
#[async_trait]
impl PatRowLookup for OneTokenLookup {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        Ok((token_id == self.token_id).then(|| self.row.clone()))
    }
}

/// `PatRowLookup` that knows nothing (rejects every token).
struct EmptyLookup;
#[async_trait]
impl PatRowLookup for EmptyLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        Ok(None)
    }
}

/// In-memory url→content-hash map (mirrors brew's `FakeMap`).
#[derive(Default)]
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

/// In-memory `ManifestKvStore` fake for the route tests (prod wires the
/// durable `crate::adapter_oci_kv::OciKvStore`).
#[derive(Default, Debug)]
struct OciKvFake(Mutex<HashMap<(String, String), Bytes>>);
#[async_trait]
impl ManifestKvStore for OciKvFake {
    async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(tenant.to_canonical_text(), key.to_owned()))
            .cloned())
    }
    async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert((tenant.to_canonical_text(), key.to_owned()), value);
        Ok(())
    }
    async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
        let t = tenant.to_canonical_text();
        Ok(self
            .0
            .lock()
            .unwrap()
            .keys()
            .filter(|(kt, ks)| *kt == t && ks.starts_with(prefix))
            .map(|(_, ks)| ks.clone())
            .collect())
    }
}

/// Non-verifying CAS stub (accepts any claimed_hash) — `oci::router`
/// wires production `canonical_hash_hex`, which a verifying in-memory
/// handler would reject. Keyed by `(tenant, hash)`.
#[derive(Debug, Default)]
struct StubCas(Mutex<HashMap<(String, String), Vec<u8>>>);
impl CasReadHandler for StubCas {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        match self
            .0
            .lock()
            .unwrap()
            .get(&(req.tenant.clone(), req.hash.clone()))
        {
            Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
            None => Err(CasHandlerError::Internal("stub: absent".into())),
        }
    }
}
impl CasWriteHandler for StubCas {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        self.0
            .lock()
            .unwrap()
            .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
        Ok(CasWriteResponse::new(req.claimed_hash, true))
    }
}
impl corelink_handler_cas::CasDeleteHandler for StubCas {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, CasHandlerError> {
        let removed = self
            .0
            .lock()
            .unwrap()
            .remove(&(req.tenant.clone(), req.hash.clone()));
        let reclaimed = removed.as_ref().map(|b| b.len() as u64).unwrap_or(0);
        Ok(corelink_handler_cas::CasDeleteResponse::with_reclaimed(
            removed.is_some(),
            reclaimed,
        ))
    }
}

fn test_key() -> Arc<PatSigningKey> {
    Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
}

/// Router whose verifier rejects ALL PATs (empty lookup); cas/map
/// unused. Session HMAC key is valid so the route MOUNTS.
fn router_rejecting() -> Router {
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
    router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,
        None,
        None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        None, // suspend resolver: off (dedicated G4b suspend tests below)
    )
}

fn req(
    method: Method,
    uri: &str,
    authorization: Option<&str>,
    scope: Option<&str>,
) -> HttpRequest<Body> {
    let mut b = HttpRequest::builder().method(method).uri(uri);
    if let Some(a) = authorization {
        b = b.header("authorization", a);
    }
    if let Some(s) = scope {
        b = b.header(SCOPE_HEADER, s);
    }
    b.body(Body::empty()).unwrap()
}

/// `Authorization: Basic base64("oci:<pat>")` — the OCI Basic leg.
fn basic(pat: &str) -> String {
    let enc = base64::engine::general_purpose::STANDARD.encode(format!("oci:{pat}"));
    format!("Basic {enc}")
}

#[tokio::test]
async fn read_only_pat_token_is_downscoped_to_pull_only() {
    // SECURITY (scope-escalation fix): a read-only (`cas:r`) PAT
    // exchanging at `/token` for `push,pull` is granted PULL-ONLY. The
    // minted bearer still PULLS (GET an absent blob → 404, i.e. the pull
    // scope passed) but CANNOT PUSH (manifest PUT → denied).
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xCAFE);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0x1234)),
        PatScopes::from_u64(SCOPE_CACHE_R), // READ-ONLY
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: "cas:r".to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
    let cas = Arc::new(StubCas::default());
    let app = router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,
        None,
        None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        None, // suspend resolver: off (dedicated G4b suspend tests below)
    );

    // Exchange the read-only PAT (requesting push,pull) for a bearer.
    let resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:push,pull",
            Some(&basic(&pt)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bearer = json["token"].as_str().expect("token field").to_owned();

    // PULL is granted: GET an absent blob → 404 (pull scope passed; a
    // denied scope would be 401/403).
    let get = req(
        Method::GET,
        "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        Some(&format!("Bearer {bearer}")),
        None,
    );
    let resp = app.clone().oneshot(get).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "pull-only bearer must still pull (absent blob ⇒ 404)"
    );

    // PUSH is denied: a manifest PUT with the pull-only bearer is rejected.
    let put = HttpRequest::builder()
        .method(Method::PUT)
        .uri("/v2/alpine/manifests/latest")
        .header("authorization", format!("Bearer {bearer}"))
        .header("content-type", "application/vnd.oci.image.manifest.v1+json")
        .body(Body::from(r#"{"schemaVersion":2}"#))
        .unwrap();
    let resp = app.oneshot(put).await.unwrap();
    assert!(
        !resp.status().is_success(),
        "read-only PAT must not push (pull-only bearer); got {}",
        resp.status()
    );
}

#[tokio::test]
async fn missing_bearer_on_data_plane_is_401_reaches_adapter() {
    // scope present → gate passes → adapter `/v2/*` dispatch finds no
    // bearer → 401 + Www-Authenticate. Proves `.merge` routed to the
    // adapter.
    let app = router_rejecting();
    let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                None,
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(resp.headers().get("Www-Authenticate").is_some());
}

#[tokio::test]
async fn forged_bearer_on_data_plane_is_401() {
    // A `corelink-oci.`-shaped but HMAC-invalid bearer → adapter
    // verify fails → 401. (The data plane never touches the PAT
    // resolver — it verifies the session HMAC.)
    let forged = "corelink-oci.00000000-0000-0000-0000-000000000000.cmVwb3NpdG9yeTphbHBpbmU6cHVsbA.9999999999.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let app = router_rejecting();
    let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                Some(&format!("Bearer {forged}")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_pat_at_token_is_401_resolver_runs() {
    // `/token` with a `corelink_`-prefixed but HMAC-invalid PAT in
    // Basic auth → gate passes (GET ⇒ read) → adapter `/token` →
    // OciPatResolver → shared verifier → InvalidPat → 401. Proves
    // `.merge` routed `/token` to the adapter AND the Option-B
    // resolver ran.
    let app = router_rejecting();
    let resp = app
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:pull",
            Some(&basic("corelink_not-a-real-token")),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_exchange_then_blob_pull_round_trip() {
    // End-to-end: mint a real PAT; exchange it at `/token` for an
    // HMAC bearer; seed the moat with a blob under the PAT's tenant
    // namespace; pull it back → 200 + bytes. Proves: `.merge` mount,
    // Option-B resolve at `/token`, HMAC mint/verify, scope gate,
    // and the moat blob-key (OCI digest → blake3) round-trip.
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xBEEF);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0xF00D)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: SCOPE_RW.to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

    // Seed the moat: a blob whose OCI digest is the moat url_hash,
    // bytes stored content-addressed by blake3, under the PAT
    // tenant's namespace (canonical UUID text).
    let bytes = b"oci-layer-bytes".to_vec();
    let oci_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let content_hash = canonical_hash_hex(&bytes);
    let tenant_ns = tenant_uuid.to_string();
    let cas = Arc::new(StubCas::default());
    cas.0
        .lock()
        .unwrap()
        .insert((tenant_ns.clone(), content_hash.clone()), bytes.clone());
    let map = Arc::new(FakeMap::default());
    map.0
        .lock()
        .unwrap()
        .insert((tenant_ns, oci_digest.to_owned()), content_hash);

    let app = router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        map,
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,
        None,
        None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        None, // suspend resolver: off (dedicated G4b suspend tests below)
    );

    // Leg 1: exchange the PAT (Basic) for an HMAC bearer at /token.
    let resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:pull",
            Some(&basic(&pt)),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bearer = json["token"].as_str().expect("token field").to_owned();

    // Leg 2: pull the seeded blob with the minted bearer.
    let resp = app
        .oneshot(req(
            Method::GET,
            &format!("/v2/alpine/blobs/{oci_digest}"),
            Some(&format!("Bearer {bearer}")),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let pulled = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(pulled.as_ref(), bytes.as_slice());
}

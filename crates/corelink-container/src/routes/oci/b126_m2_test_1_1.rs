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

#[tokio::test]
async fn finalize_rejects_digest_lie_and_persists_nothing() {
    // rt-nuclear cycle-2 #2: a finalize that declares a digest NOT matching
    // the uploaded bytes MUST be rejected BEFORE the bytes are persisted, so
    // no digest-lie ever lands in the (tenant, blob_key) slot — content-
    // addressing is enforced by the store itself, not just the caller.
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test",
    ));
    let store = OciMoatStore::new(moat, false);
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xC));
    let uuid = store.open_upload(&tenant).await.unwrap();
    store
        .append_chunk(&tenant, &uuid, Bytes::from_static(b"real-content"))
        .await
        .unwrap();
    // A lying digest (64 hex zeros) — NOT sha256("real-content").
    let lie = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    assert!(
        store
            .finalize_upload(&tenant, &uuid, lie, None)
            .await
            .is_err(),
        "a digest-lie finalize must be rejected"
    );
    // And NOTHING was persisted under the lying key (no poisoned slot).
    assert!(
        store.get_blob(&tenant, lie).await.unwrap().is_none(),
        "a rejected digest-lie must not leave a persisted slot"
    );
}

#[tokio::test]
async fn upload_session_is_tenant_scoped() {
    // Confused-deputy guard: the shared `_oci` DO buffers ALL tenants'
    // uploads in one process map, so a tenant may only append/cancel/
    // finalize a session it opened. A cross-tenant uuid must look like an
    // absent session (no existence oracle) and must NOT mutate it.
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test",
    ));
    let store = OciMoatStore::new(moat, false);
    let tenant_a = TenantId::from_uuid(Uuid::from_u128(0xA));
    let tenant_b = TenantId::from_uuid(Uuid::from_u128(0xB));

    let uuid = store.open_upload(&tenant_a).await.unwrap();

    // Tenant B cannot touch tenant A's session (each → not-found error).
    assert!(store
        .append_chunk(&tenant_b, &uuid, Bytes::from_static(b"x"))
        .await
        .is_err());
    assert!(store.cancel_upload(&tenant_b, &uuid).await.is_err());
    assert!(store
        .finalize_upload(&tenant_b, &uuid, "sha256:00", None)
        .await
        .is_err());

    // A's session is intact (B's attempts were rejected before any mutation),
    // so tenant A can still append.
    assert!(store
        .append_chunk(&tenant_a, &uuid, Bytes::from_static(b"x"))
        .await
        .is_ok());
}

/// Build an `OciMoatStore` whose moat write handler is the REAL
/// `AccountingCasHandler` (byte-accounting) over an in-memory `ByteStore`,
/// so a `finalize_upload` reserves against the threaded cap exactly as
/// production does. Returns the store + the byte store (to assert the
/// counter) + the byte region.
fn accounting_oci_store() -> (
    OciMoatStore,
    Arc<crate::byte_accounting::testing::InMemoryByteStore>,
    String,
) {
    use crate::byte_accounting::{
        testing::InMemoryByteStore, AccountingCasHandler, ByteAccountant,
    };
    // `StubCas` is non-verifying (accepts any claimed_hash) — the moat's
    // `production` ctor wires the real `canonical_hash_hex`, which a verifying
    // in-memory handler would reject. The byte-accounting decorator wraps it
    // exactly as production wraps the R2 handler.
    let inner = Arc::new(StubCas::default());
    let byte_store = Arc::new(InMemoryByteStore::new());
    let region = "iad".to_owned();
    let accountant = Arc::new(ByteAccountant::new(byte_store.clone(), region.clone()));
    let acct = Arc::new(AccountingCasHandler::new(
        Arc::clone(&inner) as Arc<dyn CasWriteHandler>,
        Arc::clone(&inner) as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        accountant,
    ));
    let moat = Arc::new(MoatCache::production(
        inner as Arc<dyn CasReadHandler>,
        acct as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-cap-test",
    ));
    (OciMoatStore::new(moat, false), byte_store, region)
}

/// Push a blob of `len` bytes through open→append→finalize under `cap`.
async fn push_blob(
    store: &OciMoatStore,
    tenant: &TenantId,
    len: usize,
    cap: Option<i64>,
) -> Result<(), String> {
    let bytes = vec![0xABu8; len];
    let digest = corelink_adapter_host::oci::digest::OciDigest::compute(
        corelink_adapter_host::oci::digest::OciDigestAlgo::Sha256,
        &bytes,
    )
    .map_err(|e| format!("{e:?}"))?;
    let uuid = store.open_upload(tenant).await?;
    store
        .append_chunk(tenant, &uuid, Bytes::from(bytes))
        .await?;
    store
        .finalize_upload(tenant, &uuid, &digest.to_wire(), cap)
        .await
        .map(|_| ())
}

// ── F3.2 inc6 (WP-B): flag-gated `_public` routing of allowlisted OCI
//    digests ────────────────────────────────────────────────────────────
//
// These prove the routing contract WITHOUT `env::set_var` (the flag is a
// constructor param) and with a hermetic allowlist via `with_allowlist`
// (production loads the six-pin baked manifest once at the router).

/// The `sha256:<hex>` wire digest of `bytes` — the exact string the allowlist
/// must contain for `routes_to_public` to fire.
fn digest_wire(bytes: &[u8]) -> String {
    corelink_adapter_host::oci::digest::OciDigest::compute(
        corelink_adapter_host::oci::digest::OciDigestAlgo::Sha256,
        bytes,
    )
    .expect("sha256 digest computes")
    .to_wire()
}

/// Push explicit `bytes` (so the digest is predictable/allowlistable) through
/// open→append→finalize under `cap`. Returns the digest wire string.
async fn push_bytes(
    store: &OciMoatStore,
    tenant: &TenantId,
    bytes: &[u8],
    cap: Option<i64>,
) -> Result<String, String> {
    let wire = digest_wire(bytes);
    let uuid = store.open_upload(tenant).await?;
    store
        .append_chunk(tenant, &uuid, Bytes::copy_from_slice(bytes))
        .await?;
    store
        .finalize_upload(tenant, &uuid, &wire, cap)
        .await
        .map(|_| wire)
}

/// A dedup-configurable `OciMoatStore` over a `StubCas` + an inspectable
/// `FakeMap`, so a test can COUNT rows per namespace (`_public` vs
/// per-tenant) after a push. `StubCas` is non-verifying, but the moat still
/// content-addresses with the real blake3 hasher, so identical bytes dedup.
fn dedup_store(dedup: bool, allowlist: PublicBaseAllowlist) -> (OciMoatStore, Arc<FakeMap>) {
    let cas = Arc::new(StubCas::default());
    let map = Arc::new(FakeMap::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::clone(&map) as Arc<dyn UrlMapStore>,
        "oci-dedup-test",
    ));
    (OciMoatStore::with_allowlist(moat, dedup, allowlist), map)
}

/// Count url→hash map rows whose namespace == `ns`.
fn rows_in_ns(map: &FakeMap, ns: &str) -> usize {
    map.0
        .lock()
        .unwrap()
        .keys()
        .filter(|(n, _)| n == ns)
        .count()
}

/// An accounting-backed dedup store (real byte reservation), so the quota
/// test can assert the per-tenant counter moves (private) or not (public).
fn accounting_dedup_store(
    allowlist: PublicBaseAllowlist,
) -> (
    OciMoatStore,
    Arc<crate::byte_accounting::testing::InMemoryByteStore>,
    String,
) {
    use crate::byte_accounting::{
        testing::InMemoryByteStore, AccountingCasHandler, ByteAccountant,
    };
    let inner = Arc::new(StubCas::default());
    let byte_store = Arc::new(InMemoryByteStore::new());
    let region = "iad".to_owned();
    let accountant = Arc::new(ByteAccountant::new(byte_store.clone(), region.clone()));
    let acct = Arc::new(AccountingCasHandler::new(
        Arc::clone(&inner) as Arc<dyn CasWriteHandler>,
        Arc::clone(&inner) as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
        accountant,
    ));
    let moat = Arc::new(MoatCache::production(
        inner as Arc<dyn CasReadHandler>,
        acct as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-dedup-cap-test",
    ));
    (
        OciMoatStore::with_allowlist(moat, true, allowlist),
        byte_store,
        region,
    )
}

#[tokio::test]
async fn inc6_two_tenants_same_allowlisted_base_share_one_public_row() {
    // DoD: two DISTINCT tenants push the SAME allowlisted digest → exactly
    // ONE shared `_public` row (cross-tenant content dedup — the moat).
    let base = b"alpine-3.20-base-layer-bytes".as_slice();
    let wire = digest_wire(base);
    let allowlist = PublicBaseAllowlist::parse(&wire).expect("valid digest allowlist");
    let (store, map) = dedup_store(true, allowlist);

    let ta = TenantId::from_uuid(Uuid::from_u128(0xA1));
    let tb = TenantId::from_uuid(Uuid::from_u128(0xB2));
    let wa = push_bytes(&store, &ta, base, Some(0)).await.unwrap();
    let wb = push_bytes(&store, &tb, base, Some(0)).await.unwrap();
    assert_eq!(wa, wb, "same bytes → same digest");

    assert_eq!(
        rows_in_ns(&map, PUBLIC_NAMESPACE),
        1,
        "two tenants pushing the same allowlisted base must share ONE `_public` row"
    );
    assert_eq!(
        rows_in_ns(&map, &ta.to_canonical_text()),
        0,
        "an allowlisted base must NOT also occupy a per-tenant row"
    );
    assert_eq!(rows_in_ns(&map, &tb.to_canonical_text()), 0);
    // Both tenants read the shared bytes back.
    assert_eq!(
        store.get_blob(&ta, &wire).await.unwrap().as_deref(),
        Some(base)
    );
    assert_eq!(
        store.get_blob(&tb, &wire).await.unwrap().as_deref(),
        Some(base)
    );
}

#[tokio::test]
async fn inc6_private_layer_stays_per_tenant() {
    // DoD: a private (non-allowlisted) layer is NEVER shared — one row per
    // tenant, nothing in `_public`, even with the flag ON.
    let private = b"proprietary-secret-layer".as_slice();
    // Allowlist a DIFFERENT digest so the flag is on but this blob misses it.
    let other = digest_wire(b"some-unrelated-allowlisted-base");
    let allowlist = PublicBaseAllowlist::parse(&other).unwrap();
    let (store, map) = dedup_store(true, allowlist);

    let ta = TenantId::from_uuid(Uuid::from_u128(0xA1));
    let tb = TenantId::from_uuid(Uuid::from_u128(0xB2));
    push_bytes(&store, &ta, private, Some(1_000)).await.unwrap();
    push_bytes(&store, &tb, private, Some(1_000)).await.unwrap();

    assert_eq!(
        rows_in_ns(&map, PUBLIC_NAMESPACE),
        0,
        "a non-allowlisted private layer must never reach `_public`"
    );
    assert_eq!(rows_in_ns(&map, &ta.to_canonical_text()), 1);
    assert_eq!(rows_in_ns(&map, &tb.to_canonical_text()), 1);
}

#[tokio::test]
async fn inc6_write_namespace_equals_read_namespace_no_split_brain() {
    // DoD (parity / split-brain): the write predicate is the strict subset;
    // the read path may also use existence-based `_public` resolution.
    // (1) An allowlisted push lands ONLY in `_public` (no per-tenant row), yet
    //     get_blob still returns it → the read resolved `_public`, matching the
    //     write. (2) The predicate is stable across paths and gated by BOTH the
    //     flag and the allowlist. (3) `_public` miss falls back to per-tenant.
    let base = b"parity-base-layer".as_slice();
    let wire = digest_wire(base);
    let other = digest_wire(b"not-this-one");
    let allowlist = PublicBaseAllowlist::parse(&wire).unwrap();

    // Shared moat so an off-store per-tenant write is visible to an on-store
    // read (fallback proof).
    let cas = Arc::new(StubCas::default());
    let map = Arc::new(FakeMap::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::clone(&map) as Arc<dyn UrlMapStore>,
        "oci-parity-test",
    ));
    let on = OciMoatStore::with_allowlist(Arc::clone(&moat), true, allowlist.clone());
    let off = OciMoatStore::with_allowlist(Arc::clone(&moat), false, allowlist);

    // Predicate parity: identical decision on both paths; gated by flag AND
    // allowlist.
    assert!(on.routes_to_public(&wire), "flag on + allowlisted ⇒ public");
    assert!(
        !on.routes_to_public(&other),
        "flag on + not-allowlisted ⇒ tenant"
    );
    assert!(
        !off.routes_to_public(&wire),
        "flag off ⇒ tenant even if allowlisted"
    );

    let t = TenantId::from_uuid(Uuid::from_u128(0xC3));
    push_bytes(&on, &t, base, Some(0)).await.unwrap();
    assert_eq!(rows_in_ns(&map, PUBLIC_NAMESPACE), 1);
    assert_eq!(
        rows_in_ns(&map, &t.to_canonical_text()),
        0,
        "write went to `_public` only; the read must find it there, not per-tenant"
    );
    assert_eq!(on.get_blob(&t, &wire).await.unwrap().as_deref(), Some(base));

    // Fallback: an allowlisted digest present ONLY per-tenant (written when the
    // flag was off) is still served by an on-store read via the per-tenant
    // fallback after the `_public` miss — WITHOUT fetching upstream.
    let legacy = b"allowlisted-but-written-per-tenant".as_slice();
    let legacy_wire = digest_wire(legacy);
    // Extend the allowlist to cover the legacy digest too.
    let al2 = PublicBaseAllowlist::parse(&format!("{wire}\n{legacy_wire}")).unwrap();
    let on2 = OciMoatStore::with_allowlist(Arc::clone(&moat), true, al2);
    let tl = TenantId::from_uuid(Uuid::from_u128(0xC4));
    push_bytes(&off, &tl, legacy, Some(1_000)).await.unwrap();
    assert_eq!(rows_in_ns(&map, &tl.to_canonical_text()), 1);
    assert_eq!(
        on2.get_blob(&tl, &legacy_wire).await.unwrap().as_deref(),
        Some(legacy),
        "`_public` miss must fall back to the per-tenant namespace"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();

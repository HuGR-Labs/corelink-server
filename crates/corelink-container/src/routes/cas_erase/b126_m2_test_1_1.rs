use super::*;
use axum::body::Body;
use axum::http::{Method, Request};
use tower::ServiceExt;

use corelink_privacy_erasure_worker::legitimacy::{
    FailingDsrLegitimacyStore, InMemoryDsrLegitimacyStore,
};

const TEST_KEY: &str = "test-internal-auth-key-32-bytes-x";
// The legitimacy gate keys on a canonical UUID tenant (the `dsr_requested`
// table stores UUID tenant ids), so the route fixtures use a UUID tenant.
const TENANT: &str = "550e8400-e29b-41d4-a716-446655440000";
// A different legitimate tenant (cross-tenant forgery test).
const TENANT_B: &str = "11111111-2222-3333-4444-555555555555";
const DIGEST: &str = "deadbeef0123";
// The DSR id the in-memory legitimacy store is seeded with for TENANT.
const DSR_ID: &str = "99999999-8888-7777-6666-555544443333";

/// Build a route state with an in-memory legitimacy store pre-seeded so
/// `(DSR_ID, TENANT)` is legitimate. Returns the eraser + tombstone fakes
/// for assertions.
fn state() -> (
    CasEraseRouteState,
    Arc<InMemoryBlobEraser>,
    Arc<InMemoryTombstoneStore>,
) {
    let legit = InMemoryDsrLegitimacyStore::new();
    legit.insert_requested(
        Uuid::try_parse(DSR_ID).expect("dsr uuid"),
        Uuid::try_parse(TENANT).expect("tenant uuid"),
    );
    build_state(Arc::new(legit))
}

/// Build a route state with an explicit legitimacy store.
fn build_state(
    legitimacy: Arc<dyn DsrLegitimacyStore>,
) -> (
    CasEraseRouteState,
    Arc<InMemoryBlobEraser>,
    Arc<InMemoryTombstoneStore>,
) {
    let tombstones = Arc::new(InMemoryTombstoneStore::new());
    let eraser = Arc::new(InMemoryBlobEraser::new());
    let st = CasEraseRouteState {
        tombstones: tombstones.clone(),
        eraser: eraser.clone(),
        internal_auth_key: Arc::from(TEST_KEY),
        legitimacy: Some(legitimacy),
    };
    (st, eraser, tombstones)
}

fn erase_req(auth: &str, tenant: &str, digest: &str, body_tenant: &str) -> Request<Body> {
    erase_req_dsr(auth, tenant, digest, body_tenant, DSR_ID)
}

fn erase_req_dsr(
    auth: &str,
    tenant: &str,
    digest: &str,
    body_tenant: &str,
    dsr_id: &str,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(format!("/_internal/cas/{tenant}/{digest}/erase"));
    if !auth.is_empty() {
        b = b.header(INTERNAL_AUTH_HEADER, auth);
    }
    b.body(Body::from(
        serde_json::json!({ "tenant": body_tenant, "dsr_id": dsr_id, "reason": "dsr" }).to_string(),
    ))
    .expect("request")
}

#[tokio::test]
async fn erase_without_auth_is_401() {
    let (st, _e, _t) = state();
    let resp = router(st)
        .oneshot(erase_req("", TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn erase_happy_path_deletes_bytes_and_writes_tombstone() {
    let (st, eraser, tombstones) = state();
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        eraser.was_erased(TENANT, DIGEST),
        "R2 bytes must be deleted"
    );
    assert!(
        tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "tombstone must be written"
    );
}

#[tokio::test]
async fn erase_cross_tenant_is_403_before_storage() {
    let (st, eraser, _t) = state();
    // path tenant TENANT, body tenant TENANT_B → cross-tenant (pure handler).
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT_B))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(!eraser.was_erased(TENANT_B, DIGEST));
    assert!(!eraser.was_erased(TENANT, DIGEST));
}

#[tokio::test]
async fn erase_invalid_digest_is_400() {
    let (st, _e, _t) = state();
    // matchit will not match a '/' inside :hash, so use a non-slash invalid
    // char that still reaches the handler: an overlong digest.
    let long = "a".repeat(200);
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, &long, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn re_erase_is_idempotent_ok() {
    let (st, _e, tombstones) = state();
    let app = router(st);
    let r1 = app
        .clone()
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(r1.status(), StatusCode::OK);
    // second erase = idempotent no-op, still 200.
    let r2 = app
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(r2.status(), StatusCode::OK);
    assert!(tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
}

#[tokio::test]
async fn in_memory_tombstone_upsert_reports_prior_existence() {
    let t = InMemoryTombstoneStore::new();
    assert!(!t.upsert(TENANT, DIGEST, "r", 1).await.expect("u1"));
    assert!(t.upsert(TENANT, DIGEST, "r", 2).await.expect("u2"));
}

// ──────────────────────────────────────────────────────────────────────
// DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9) — leaked-key defense
// ──────────────────────────────────────────────────────────────────────

/// Valid auth + a present `dsr_requested` row (seeded) ⇒ the erase
/// proceeds (200) and the bytes are deleted.
#[tokio::test]
async fn erase_with_legit_dsr_row_proceeds() {
    let (st, eraser, tombstones) = state();
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(eraser.was_erased(TENANT, DIGEST));
    assert!(tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
}

/// Valid auth + a `dsr_id` with NO matching `dsr_requested` row ⇒ 403 and
/// NOTHING is erased (the leaked-internal-key blast radius is closed).
#[tokio::test]
async fn erase_no_dsr_row_is_403_and_no_delete() {
    // Empty legitimacy store: every (dsr_id, tenant) is NOT requested.
    let (st, eraser, tombstones) = build_state(Arc::new(InMemoryDsrLegitimacyStore::new()));
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(
        !eraser.was_erased(TENANT, DIGEST),
        "no delete without a legit row"
    );
    assert!(!tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
}

/// Legitimacy store FAULT (D1 error) ⇒ fail-CLOSED 503, NOTHING erased.
/// Erasure is irreversible — ambiguous legitimacy must DENY.
#[tokio::test]
async fn erase_legitimacy_error_is_503_fail_closed() {
    let (st, eraser, tombstones) = build_state(Arc::new(FailingDsrLegitimacyStore::new()));
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !eraser.was_erased(TENANT, DIGEST),
        "no delete on legitimacy fault"
    );
    assert!(!tombstones.is_tombstoned(TENANT, DIGEST).await.expect("q"));
}

/// No legitimacy store wired (state.legitimacy == None) ⇒ fail-CLOSED 503.
/// (Unreachable in prod — build_state_from_env fail-CLOSES the route — but
/// the handler must still DENY if it ever occurs.)
#[tokio::test]
async fn erase_legitimacy_none_is_503_fail_closed() {
    let tombstones = Arc::new(InMemoryTombstoneStore::new());
    let eraser = Arc::new(InMemoryBlobEraser::new());
    let st = CasEraseRouteState {
        tombstones,
        eraser: eraser.clone(),
        internal_auth_key: Arc::from(TEST_KEY),
        legitimacy: None,
    };
    let resp = router(st)
        .oneshot(erase_req(TEST_KEY, TENANT, DIGEST, TENANT))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!eraser.was_erased(TENANT, DIGEST));
}

/// Cross-tenant via the legitimacy gate: a `dsr_id` legitimate for TENANT
/// CANNOT authorise erasing TENANT_B (the gate binds on BOTH dsr_id AND
/// tenant). Here path==body==TENANT_B (so the pure cross-tenant check
/// passes) but only `(DSR_ID, TENANT)` is seeded ⇒ 403 from the gate.
#[tokio::test]
async fn erase_dsr_id_for_other_tenant_is_403() {
    // Seed legitimacy ONLY for TENANT, then attempt to erase TENANT_B's
    // blob with TENANT's dsr_id.
    let legit = InMemoryDsrLegitimacyStore::new();
    legit.insert_requested(
        Uuid::try_parse(DSR_ID).expect("dsr uuid"),
        Uuid::try_parse(TENANT).expect("tenant uuid"),
    );
    let (st, eraser, tombstones) = build_state(Arc::new(legit));
    let resp = router(st)
        .oneshot(erase_req_dsr(TEST_KEY, TENANT_B, DIGEST, TENANT_B, DSR_ID))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(
        !eraser.was_erased(TENANT_B, DIGEST),
        "no cross-tenant erase via foreign dsr_id"
    );
    assert!(!tombstones.is_tombstoned(TENANT_B, DIGEST).await.expect("q"));
}

/// A missing `dsr_id` field in the body ⇒ 400 (deserialization rejects it
/// before any storage touch). Required-field enforcement.
#[tokio::test]
async fn erase_missing_dsr_id_is_400() {
    let body = serde_json::json!({ "tenant": TENANT, "reason": "dsr" }).to_string();
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/_internal/cas/{TENANT}/{DIGEST}/erase"))
        .header(INTERNAL_AUTH_HEADER, TEST_KEY)
        .body(Body::from(body))
        .expect("request");
    let (st, eraser, _t) = state();
    let resp = router(st).oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(!eraser.was_erased(TENANT, DIGEST));
}

// ──────────────────────────────────────────────────────────────────────
// Production R2 eraser — by-construction key-layout proof + fail-CLOSED
// ──────────────────────────────────────────────────────────────────────

/// A fixed, non-zero TDK so the derived prefix is deterministic.
fn test_tdk() -> Arc<TenantDerivationKey> {
    let bytes = Zeroizing::new([7u8; 32]);
    Arc::new(TenantDerivationKey::from_bytes(bytes))
}

/// The eraser's LIST/DELETE key for a UUID tenant is EXACTLY the whole-blob
/// key the CAS writer keys under: `<region>/<derive_prefix(tdk,uuid)>/<digest>`.
/// This is the by-construction guarantee that the erase cannot silently
/// no-op on a key-derivation mismatch (the earlier R2Ac bug class).
#[test]
fn eraser_prefix_matches_writer_derivation_for_uuid_tenant() {
    let tdk = test_tdk();
    let eraser = R2CasBlobEraser::new(tdk.clone(), "corelink-cas-prod".to_owned());
    let tenant_uuid = "550e8400-e29b-41d4-a716-446655440000";
    let uid = Uuid::try_parse(tenant_uuid).expect("uuid");
    // What the WRITER derives (R2CasHandler::r2_key UUID branch).
    let writer_prefix = derive_prefix(&tdk, uid).to_string();
    // What the ERASER derives.
    let eraser_prefix = eraser.tenant_prefix(tenant_uuid).unwrap();
    assert_eq!(eraser_prefix, writer_prefix, "prefix must match the writer");
    assert_eq!(writer_prefix.len(), TENANT_PREFIX_LEN);
    // And the assembled LIST key is the leading path of the blob key.
    let blob_key = crate::storage::r2_s3::R2S3Client::blob_key(
        "iad",
        &writer_prefix,
        DIGEST,
        corelink_handler_cas::DigestAlgo::Blake3,
    );
    let list_key = crate::storage::r2_s3::R2S3Client::blob_key(
        "iad",
        &eraser_prefix,
        DIGEST,
        corelink_handler_cas::DigestAlgo::Blake3,
    );
    assert_eq!(list_key, blob_key);
    assert!(blob_key.starts_with("iad/"), "key={blob_key}");
    assert!(blob_key.ends_with(&format!("/{DIGEST}")), "key={blob_key}");
}

/// F3.2 B1b: erasing the `_public` shared-dedup namespace derives the SAME
/// reserved sentinel prefix the `_public` CAS writer uses
/// (`r2_s3::public_namespace_prefix`) — so the public-revocation R2 delete
/// addresses the exact key `MoatCache::put` created. This is the
/// by-construction guarantee that a revoked public blob's bytes are
/// physically erasable (the BLOCKER-1 fix), NOT a per-principal miss.
#[test]
fn eraser_prefix_for_public_namespace_matches_writer_sentinel() {
    let tdk = test_tdk();
    let eraser = R2CasBlobEraser::new(tdk.clone(), "corelink-cas-prod".to_owned());
    let writer_public_prefix = crate::storage::r2_s3::public_namespace_prefix(&tdk);
    let eraser_public_prefix = eraser
        .tenant_prefix(crate::adapter_cache::PUBLIC_NAMESPACE)
        .expect("_public prefix derivable");
    assert_eq!(
        eraser_public_prefix, writer_public_prefix,
        "_public erase prefix must equal the writer's sentinel prefix"
    );
    assert_eq!(writer_public_prefix.len(), TENANT_PREFIX_LEN);
}

/// Non-UUID tenant uses the writer's raw-padded 16-char fallback (dev/test
/// fixtures), so a digest erase still keys identically to the writer.
#[test]
fn eraser_prefix_pads_non_uuid_tenant_to_16() {
    let eraser = R2CasBlobEraser::new(test_tdk(), "corelink-cas-prod".to_owned());
    let prefix = eraser.tenant_prefix("t1").unwrap();
    assert_eq!(prefix.len(), TENANT_PREFIX_LEN);
    assert!(prefix.starts_with("t1"), "prefix={prefix}");
}

/// The five canonical CAS regions match the DSR Wave 1 tenant-wide adapter
/// (`adapter_r2_cas::CAS_REGIONS`) — same sweep, same order.
#[test]
fn cas_regions_match_dsr_adapter() {
    assert_eq!(CAS_REGIONS, &["sam", "iad", "lhr", "nrt", "syd"]);
}

/// Fail-CLOSED: with no internal-auth key the route state is never built
/// (route unmounted) even if other env were present.
#[test]
fn build_state_without_auth_key_is_none() {
    assert!(build_state_from_env(None).is_none());
}

/// Fail-CLOSED: an auth key is present but `R2_TDK_HEX` is absent ⇒ the
/// eraser cannot derive the tenant prefix, so the route is NOT mounted.
/// (We cannot mutate process env safely in a shared test binary, so this
/// asserts the no-TDK branch directly via the loader.)
#[test]
fn build_state_fail_closed_without_tdk() {
    std::env::remove_var("R2_TDK_HEX");
    assert!(load_tdk_from_env().is_none(), "no TDK ⇒ loader None");
    // With the loader None, build_state_from_env short-circuits to None
    // regardless of D1/bucket env.
    assert!(build_state_from_env(Some(Arc::from(TEST_KEY))).is_none());
}

// ──────────────────────────────────────────────────────────────────────
// WP-2b — BloomTombstoneStore (the read-hot-path D1 round-trip cut)
// ──────────────────────────────────────────────────────────────────────

use std::sync::atomic::AtomicUsize;

/// Inner [`TombstoneStore`] that counts `is_tombstoned` calls (proves the
/// fast path never touches the inner store) and wraps an
/// [`InMemoryTombstoneStore`] for the authoritative answer. `seed` writes
/// DIRECTLY to the inner set (NOT through the bloom) — simulating a
/// tombstone written by ANOTHER container instance.
#[derive(Debug)]
struct CountingInner {
    inner: InMemoryTombstoneStore,
    reads: AtomicUsize,
}
impl CountingInner {
    fn new() -> Self {
        Self {
            inner: InMemoryTombstoneStore::new(),
            reads: AtomicUsize::new(0),
        }
    }
    fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Write a tombstone DIRECTLY to the inner store (another instance).
    fn seed_inner(&self, tenant: &str, digest: &str) {
        self.inner.seed(tenant, digest);
    }
}
#[async_trait]
impl TombstoneStore for CountingInner {
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.is_tombstoned(tenant, digest).await
    }
    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String> {
        self.inner
            .upsert(tenant, digest, reason, erased_at_ms)
            .await
    }
    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
        // NOT counted in `reads` (which counts `is_tombstoned` probes only):
        // the bloom reload seeds from here (F-010), but the assertion target
        // is the per-lookup D1 round-trip, not the once-per-window reload.
        self.inner.list_tenant_tombstones(tenant).await
    }
}

/// Inner store whose `is_tombstoned` always errors (D1 fault) — to prove
/// the wrapper propagates the inner `Err` unchanged on the maybe path.
#[derive(Debug, Default)]
struct ErroringInner;
#[async_trait]
impl TombstoneStore for ErroringInner {
    async fn is_tombstoned(&self, _t: &str, _d: &str) -> Result<bool, String> {
        Err("d1 fault".to_owned())
    }
    async fn upsert(&self, _t: &str, _d: &str, _r: &str, _e: i64) -> Result<bool, String> {
        // Upsert succeeds so the bloom bit can be set before the read probe.
        Ok(false)
    }
    async fn list_tenant_tombstones(&self, _t: &str) -> Result<Vec<String>, String> {
        // Reload-seed errors (degraded path): the bloom keeps only its
        // carry-forward bits + the just_reloaded fall-through stays authoritative.
        Err("d1 fault".to_owned())
    }
}

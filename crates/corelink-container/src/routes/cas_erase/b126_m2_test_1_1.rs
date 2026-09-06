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

/// A long staleness window so a freshly-loaded tenant bloom stays "fresh"
/// for the whole test (the just-reloaded fall-through fires only on the
/// FIRST touch of a tenant).
const LONG_WINDOW: Duration = Duration::from_secs(3600);

/// Warm a tenant's bloom past the one-shot just-reloaded epoch so the fast
/// path is armed: touch any digest once (that first touch falls through),
/// after which fresh `definitely-absent` lookups skip the inner store.
async fn warm(store: &BloomTombstoneStore, tenant: &str) {
    let _ = store.is_tombstoned(tenant, "warm-up-digest").await;
}

/// (a) A non-tombstoned digest → `Ok(false)` with ZERO inner calls on the
/// fast path (after the tenant bloom is warmed past its first touch).
#[tokio::test]
async fn bloom_fast_path_skips_inner_for_absent_digest() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await; // first touch falls through (1 inner read)
    let before = inner.reads();
    let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
    assert!(!r, "absent digest ⇒ Ok(false)");
    assert_eq!(
        inner.reads(),
        before,
        "fast path must NOT touch the inner store"
    );
}

/// (b) A digest tombstoned THROUGH the wrapper → `Ok(true)` and stays true.
#[tokio::test]
async fn bloom_through_write_is_true_and_stays_true() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await;
    assert!(store.upsert(TENANT, DIGEST, "dsr", 1).await.is_ok());
    // Bloom hit (we wrote it) ⇒ falls through to inner, which is authoritative.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "tombstoned ⇒ true"
    );
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "stays true"
    );
}

/// (c) NO FALSE NEGATIVE: a digest tombstoned DIRECTLY in the inner store
/// (another instance) is correctly reported `true` AFTER a refresh — and
/// the bounded-window behaviour is asserted: with a ZERO window EVERY
/// lookup is stale ⇒ always falls through ⇒ always authoritative.
#[tokio::test]
async fn bloom_no_false_negative_cross_instance_after_refresh() {
    let inner = Arc::new(CountingInner::new());
    // Zero window ⇒ every tenant_bloom call re-stamps ⇒ just_reloaded=true
    // ⇒ every lookup falls through to the authoritative inner store. This
    // is the worst case (window→0 = the wrapper is a pass-through, never a
    // false negative) and the boundary the bound is measured against.
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        Duration::ZERO,
    );
    // Another instance writes the tombstone directly to D1 (not via bloom).
    inner.seed_inner(TENANT, DIGEST);
    // The wrapper must NEVER report this absent.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "cross-instance tombstone must read true after refresh"
    );

    // And the WITHIN-window staleness bound: with a long window, the FIRST
    // touch of a tenant falls through (catches the cross-instance write);
    // subsequent fast-path lookups of OTHER absent digests skip D1, but a
    // cross-instance write that lands AFTER the bloom was loaded is only
    // guaranteed visible after the window elapses (next reload). Assert the
    // first-touch catch:
    let inner2 = Arc::new(CountingInner::new());
    let store2 = BloomTombstoneStore::with_params(
        inner2.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    inner2.seed_inner(TENANT, DIGEST); // present before first touch
    assert!(
        store2.is_tombstoned(TENANT, DIGEST).await.expect("q"),
        "first-touch reload catches the cross-instance tombstone"
    );
}

/// (c2) F-010 REGRESSION — WITHIN-WINDOW false-negative is closed by the D1
/// reload-seed. A digest tombstoned by a SEPARATE writer (the erase route's
/// own `D1TombstoneStore`, which does NOT share this bloom) is present in D1
/// before the bloom's first (re)load. With a LONG window the bloom is
/// (re)loaded exactly once on first touch; the OLD code seeded the fresh bloom
/// from in-process writes ONLY, so the FIRST read fell through (410) but
/// STAMPED an EMPTY bloom — every subsequent read within the ~30s window then
/// hit the empty-bloom fast path and returned `Ok(false)` WITHOUT consulting
/// D1 (the 410→404/200 GDPR downgrade). The fix seeds the reload from the
/// authoritative D1 set, so the SECOND (and every) within-window read still
/// sees the digest in the bloom → falls through → stays `true`. This asserts
/// the in-code invariant #1 ("NO FALSE NEGATIVE … within at most one window").
#[tokio::test]
async fn bloom_no_within_window_false_negative_for_cross_writer_tombstone() {
    let inner = Arc::new(CountingInner::new());
    // LONG window: the bloom is (re)loaded ONCE, so the only thing standing
    // between a second read and a false negative is the reload SEED (F-010).
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    // A SEPARATE writer (not via this bloom) tombstones the digest in D1.
    inner.seed_inner(TENANT, DIGEST);
    // First read: triggers the one-shot (re)load → seeds the bloom from D1 →
    // falls through → true.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q1"),
        "first within-window read of a cross-writer tombstone must be true"
    );
    // SECOND read, still WITHIN the (long) window — the regression point. The
    // bloom is NOT re-stamped (window not elapsed), so the fast path runs; it
    // MUST still see the seeded bit and fall through to D1 → true. Pre-fix
    // this returned Ok(false) (false negative / GDPR 410 bypass).
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q2"),
        "F-010: second within-window read MUST NOT be a false negative"
    );
    // And a THIRD, to be thorough.
    assert!(
        store.is_tombstoned(TENANT, DIGEST).await.expect("q3"),
        "F-010: stays true for every within-window read"
    );
    // A genuinely-absent OTHER digest still fast-paths to false (the seed did
    // not over-set membership for unrelated digests).
    assert!(
        !store.is_tombstoned(TENANT, "00absent00").await.expect("q4"),
        "an un-tombstoned digest still reads absent"
    );
}

/// (d) False-positive path: a bloom HIT on a digest that is NOT actually
/// tombstoned falls through to the inner store and returns its authoritative
/// `Ok(false)` — never a wrongful 410. We force a "hit" by writing the
/// digest through the wrapper (sets the bits) but NOT into the inner set
/// (simulating a bloom bit set with no real tombstone, e.g. an upsert whose
/// inner write later rolled back). Here we use the erroring-free inner and
/// assert the authoritative answer wins.
#[tokio::test]
async fn bloom_false_positive_falls_through_to_authoritative_inner() {
    let inner = Arc::new(CountingInner::new());
    let store = BloomTombstoneStore::with_params(
        inner.clone(),
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await;
    // Set the bloom bit directly (a "false positive": bit set, no inner row).
    let tb = store
        .fresh_tenant_bloom(TENANT)
        .expect("warmed bloom is fresh");
    tb.bloom
        .insert(&BloomTombstoneStore::bloom_key(TENANT, DIGEST));
    let before = inner.reads();
    let r = store.is_tombstoned(TENANT, DIGEST).await.expect("q");
    assert!(
        !r,
        "bloom false-positive must defer to the authoritative inner Ok(false)"
    );
    assert_eq!(
        inner.reads(),
        before + 1,
        "maybe-present must consult the inner store exactly once"
    );
}

/// (e) Inner error propagates UNCHANGED on the maybe-present path
/// (preserves today's fail-OPEN semantics — invariant 3).
#[tokio::test]
async fn bloom_inner_error_propagates_on_maybe_path() {
    let inner: Arc<dyn TombstoneStore> = Arc::new(ErroringInner);
    let store = BloomTombstoneStore::with_params(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    );
    warm(&store, TENANT).await; // first touch also errors, fine
                                // Write through to set a bloom bit → guarantees a maybe-present probe.
    let _ = store.upsert(TENANT, DIGEST, "r", 1).await;
    let err = store.is_tombstoned(TENANT, DIGEST).await;
    assert_eq!(
        err,
        Err("d1 fault".to_owned()),
        "inner Err must propagate unchanged"
    );
}

/// (f) Concurrency: many tasks read + write concurrently; no panic, no torn
/// state, and every through-the-wrapper write is observed true afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bloom_concurrent_reads_and_writes() {
    let inner = Arc::new(CountingInner::new());
    let store = Arc::new(BloomTombstoneStore::with_params(
        inner,
        DEFAULT_BLOOM_BITS,
        DEFAULT_BLOOM_HASHES,
        LONG_WINDOW,
    ));
    let mut handles = Vec::new();
    for i in 0..64u32 {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            let d = format!("digest-{i:04}");
            // half write, all read.
            if i % 2 == 0 {
                let _ = s.upsert(TENANT, &d, "r", i64::from(i)).await;
            }
            let _ = s.is_tombstoned(TENANT, &d).await;
        }));
    }
    for h in handles {
        h.await.expect("task");
    }
    // Every even digest was written THROUGH the wrapper ⇒ must read true.
    for i in (0..64u32).step_by(2) {
        let d = format!("digest-{i:04}");
        assert!(
            store.is_tombstoned(TENANT, &d).await.expect("q"),
            "written digest {d} must be tombstoned"
        );
    }
}

/// Bloom unit: one-sided error guarantee — once inserted, `contains` is true.
#[test]
fn bloom_unit_no_false_negative_and_bounded() {
    let b = Bloom::new(DEFAULT_BLOOM_BITS, DEFAULT_BLOOM_HASHES);
    for i in 0..1000 {
        b.insert(&format!("k{i}"));
    }
    for i in 0..1000 {
        assert!(
            b.contains(&format!("k{i}")),
            "inserted key must always be present"
        );
    }
    // Bounded: the bit-array size is fixed regardless of element count.
    assert_eq!(b.words.len(), DEFAULT_BLOOM_BITS / 64);
}

// ──────────────────────────────────────────────────────────────────────
// F-004 — TombstoneGatedCasHandler (the centralized shared-seam erase gate)
// ──────────────────────────────────────────────────────────────────────

use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasReadHandler,
    CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest, CasWriteResponse,
};

/// Minimal CAS handler fake: read/exists always succeed, write always
/// commits, delete always succeeds — so any 404/410/503 in the tests below
/// can ONLY come from the tombstone gate, not from the underlying handler.
#[derive(Debug, Default)]
struct AlwaysOkCas {
    wrote: Mutex<Vec<(String, String)>>,
    deleted: Mutex<Vec<(String, String)>>,
}
impl CasReadHandler for AlwaysOkCas {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        Ok(CasReadResponse::new(b"live-bytes".to_vec(), req.hash))
    }
}
impl CasWriteHandler for AlwaysOkCas {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        lock_or_recover(&self.wrote).push((req.tenant.clone(), req.claimed_hash.clone()));
        Ok(CasWriteResponse::new(req.claimed_hash, true))
    }
}
impl CasDeleteHandler for AlwaysOkCas {
    fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        lock_or_recover(&self.deleted).push((req.tenant.clone(), req.hash.clone()));
        Ok(CasDeleteResponse::new(true))
    }
}

fn gated_with(tombstones: Arc<dyn TombstoneStore>) -> (TombstoneGatedCasHandler, Arc<AlwaysOkCas>) {
    let backing = Arc::new(AlwaysOkCas::default());
    let gated = TombstoneGatedCasHandler::new(
        backing.clone() as Arc<dyn CasReadHandler>,
        backing.clone() as Arc<dyn CasWriteHandler>,
        backing.clone() as Arc<dyn CasDeleteHandler>,
        tombstones,
    );
    (gated, backing)
}

fn read_req() -> CasReadRequest {
    CasReadRequest::new(
        TENANT.to_owned(),
        DIGEST.to_owned(),
        format!("anon@{TENANT}"),
        TENANT.to_owned(),
        0,
    )
}
fn write_req() -> CasWriteRequest {
    CasWriteRequest::new(
        TENANT.to_owned(),
        DIGEST.to_owned(),
        b"resurrect".to_vec(),
        format!("anon@{TENANT}"),
        TENANT.to_owned(),
        0,
    )
}

/// A tombstoned read is refused at the SHARED seam (NotFound) — so NO
/// surface (cargo/brew/npm/pip/bazel/turbo/oci) can serve erased bytes,
/// even though only the native route has the inline 410 gate. (F-004 read.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_read_of_tombstoned_is_notfound() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
    let (gated, _backing) = gated_with(ts);
    let r = gated.read(read_req());
    assert!(
        matches!(r, Err(CasHandlerError::NotFound { .. })),
        "tombstoned read must be refused at the shared seam, got {r:?}"
    );
    // exists likewise reports absent.
    assert!(!gated.exists(read_req()).expect("exists"));
}

/// A non-tombstoned read delegates to the backing handler (live bytes).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_read_of_live_blob_delegates() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    let (gated, _backing) = gated_with(ts);
    let resp = gated.read(read_req()).expect("live read");
    assert_eq!(resp.bytes, b"live-bytes".to_vec());
    assert!(gated.exists(read_req()).expect("exists"));
}

/// A re-PUT of a tombstoned (tenant, hash) is REFUSED at the shared seam with
/// the GONE sentinel — so erased bytes cannot be resurrected at the same
/// content address via ANY write surface (the prior write path was ungated).
/// (F-004 write — the resurrection vector.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_of_tombstoned_is_refused_gone() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    ts.upsert(TENANT, DIGEST, "dsr", 1).await.expect("seed");
    let (gated, backing) = gated_with(ts);
    let r = gated.write(write_req());
    match r {
        Err(CasHandlerError::Internal(msg)) => {
            assert!(
                msg.starts_with(TOMBSTONE_GONE_SENTINEL),
                "re-PUT must carry the GONE sentinel, got {msg}"
            );
        }
        other => panic!("re-PUT of erased blob must be refused, got {other:?}"),
    }
    // The backing handler was NEVER reached — no resurrection.
    assert!(
        lock_or_recover(&backing.wrote).is_empty(),
        "no bytes were written"
    );
}

/// A write to a LIVE (non-tombstoned) hash delegates and commits normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gated_write_of_live_hash_delegates() {
    let ts = Arc::new(InMemoryTombstoneStore::new());
    let (gated, backing) = gated_with(ts);
    let resp = gated.write(write_req()).expect("live write");
    assert!(resp.durable);
    assert_eq!(lock_or_recover(&backing.wrote).len(), 1);
}

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();

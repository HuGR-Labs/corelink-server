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

#[tokio::test]
async fn inc6_non_allowlisted_write_is_per_tenant_and_quota_charged() {
    // DoD: a non-allowlisted push charges the tenant's quota and is capped;
    // an allowlisted push is the ONLY route to the uncapped (`Some(0)`) seed —
    // it does NOT charge the tenant even far past its cap.
    let base = b"uncapped-allowlisted-base".as_slice();
    let base_wire = digest_wire(base);
    let allowlist = PublicBaseAllowlist::parse(&base_wire).unwrap();
    let (store, byte_store, region) = accounting_dedup_store(allowlist);

    let tenant = TenantId::from_uuid(Uuid::from_u128(0xD00D));
    let t_text = tenant.to_canonical_text();
    let cap = Some(10i64);

    // Private 5-byte push (≤10) → per-tenant, quota charged.
    push_bytes(&store, &tenant, b"12345", cap)
        .await
        .expect("under-cap private push succeeds");
    assert_eq!(
        byte_store.used(&t_text, &region),
        5,
        "a private layer must charge the tenant's quota"
    );
    // Private 20-byte push (>10) → REJECTED by the resolved cap.
    let err = push_bytes(&store, &tenant, &[0u8; 20], cap)
        .await
        .expect_err("over-cap private push is rejected");
    assert!(
        err.contains(crate::byte_accounting::OVER_CAP_SENTINEL),
        "over-cap private push must carry the 402 sentinel; got: {err}"
    );
    assert_eq!(
        byte_store.used(&t_text, &region),
        5,
        "rejected push moved nothing"
    );

    // Allowlisted 1000-byte base at the SAME tiny cap → SUCCEEDS (routes to
    // `_public`, uncapped `Some(0)`); charges `_public`, not the tenant.
    push_bytes(&store, &tenant, base, cap)
        .await
        .expect("allowlisted base bypasses the per-tenant cap (uncapped `_public`)");
    assert_eq!(
        byte_store.used(&t_text, &region),
        5,
        "the allowlisted base must NOT charge the tenant (uncapped path) — the ONLY \
             route to the uncapped `Some(0)` seed is an `is_allowlisted` hit"
    );
}

#[tokio::test]
async fn inc6_unallowlisted_config_object_stays_per_tenant() {
    // The routing predicate is digest-only. This config-shaped payload is
    // intentionally NOT in the owner allowlist, so it stays per-tenant.
    let layer = b"a-real-allowlisted-layer".as_slice();
    let layer_wire = digest_wire(layer);
    let config = br#"{"architecture":"amd64","os":"linux"}"#.as_slice();
    let config_wire = digest_wire(config);
    let allowlist = PublicBaseAllowlist::parse(&layer_wire).unwrap();
    let (store, map) = dedup_store(true, allowlist);

    // The listed digest routes public; the unlisted config digest does not.
    assert!(store.routes_to_public(&layer_wire));
    assert!(
        !store.routes_to_public(&config_wire),
        "an unlisted digest must stay private regardless of payload shape"
    );

    let t = TenantId::from_uuid(Uuid::from_u128(0xE5));
    push_bytes(&store, &t, config, Some(1_000)).await.unwrap();
    assert_eq!(
        rows_in_ns(&map, PUBLIC_NAMESPACE),
        0,
        "an unlisted config object must not land in `_public`"
    );
    assert_eq!(
        rows_in_ns(&map, &t.to_canonical_text()),
        1,
        "the unlisted config object stays per-tenant"
    );
}

#[tokio::test]
async fn inc6_owner_pinned_index_shaped_blob_routes_via_finalize_only() {
    // Adversarial truth control: `routes_to_public` has only a digest, not
    // an OCI media type. An owner-listed index-shaped BLOB payload therefore
    // routes public through upload/finalize, but its exact bytes remain
    // content-address-verified before persistence. This does not imply that
    // a manifest PUT can write `_public`; that path uses ManifestKvStore.
    let index = br#"{"schemaVersion":2,"manifests":[]}"#.as_slice();
    let index_wire = digest_wire(index);
    let allowlist = PublicBaseAllowlist::parse(&index_wire).unwrap();
    let (store, map) = dedup_store(true, allowlist);
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xE6));

    assert!(store.routes_to_public(&index_wire));
    push_bytes(&store, &tenant, index, Some(1_000))
        .await
        .unwrap();
    assert_eq!(rows_in_ns(&map, PUBLIC_NAMESPACE), 1);
    assert_eq!(rows_in_ns(&map, &tenant.to_canonical_text()), 0);
}

#[tokio::test]
async fn inc6_manifest_put_index_stays_tenant_scoped() {
    // The same index-shaped JSON is a manifest when sent to the manifest
    // endpoint. Its PUT path writes only the tenant-keyed ManifestKvStore,
    // never the blob moat or `_public` namespace.
    let index = Bytes::from_static(
            br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","size":1}]}"#,
        );
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xE7));
    let other = TenantId::from_uuid(Uuid::from_u128(0xE8));
    let kv = OciKvFake::default();
    let auditor = InMemoryAuditEmitter::new();
    let scope = corelink_adapter_host::oci::auth::OciScope::new("alpine", vec!["push".to_owned()]);

    corelink_adapter_host::oci::push::manifest::put(
        &kv,
        &auditor,
        &tenant,
        &scope,
        "alpine",
        "latest",
        "application/vnd.oci.image.index.v1+json",
        index.clone(),
        1_000,
    )
    .await
    .expect("index manifest PUT should be accepted");

    {
        let rows = kv.0.lock().unwrap();
        assert!(!rows.is_empty(), "manifest PUT must persist tenant KV rows");
        assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));
    }
    assert!(kv
        .get(&other, "oci_manifest:alpine:latest")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn inc6_flag_off_is_inert_no_op() {
    // Binding invariant: flag OFF ⇒ the whole WP is INERT — even an allowlisted
    // base stays per-tenant (byte-identical to pre-inc6). Proves the no-op.
    let base = b"would-be-shared-base".as_slice();
    let wire = digest_wire(base);
    let allowlist = PublicBaseAllowlist::parse(&wire).unwrap();
    let (store, map) = dedup_store(false, allowlist);

    let ta = TenantId::from_uuid(Uuid::from_u128(0xA1));
    let tb = TenantId::from_uuid(Uuid::from_u128(0xB2));
    push_bytes(&store, &ta, base, Some(1_000)).await.unwrap();
    push_bytes(&store, &tb, base, Some(1_000)).await.unwrap();
    assert_eq!(
        rows_in_ns(&map, PUBLIC_NAMESPACE),
        0,
        "flag OFF ⇒ nothing may reach `_public`, even an allowlisted digest"
    );
    assert_eq!(rows_in_ns(&map, &ta.to_canonical_text()), 1);
    assert_eq!(rows_in_ns(&map, &tb.to_canonical_text()), 1);
}

// ── M2 (WP-G): `_public` READ = EXISTENCE (not allowlist) ────────────────

/// A blob present ONLY in `_public` and NOT individually allowlisted (a
/// transitively-promoted child layer) is served for a tenant that never
/// pushed it when dedup is ON (existence read); with dedup OFF, `_public` is
/// not consulted → per-tenant miss → `None`.
#[tokio::test]
async fn m2_existence_read_serves_public_blob_for_unpushed_tenant() {
    let base = b"m2-transitively-promoted-child-layer".as_slice();
    let wire = digest_wire(base);
    let cas = Arc::new(StubCas::default());
    let map = Arc::new(FakeMap::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::clone(&map) as Arc<dyn UrlMapStore>,
        "oci-m2-existence-test",
    ));
    // Seed the blob ONLY in `_public` (as the resolver's closure promote does)
    // — the digest is NOT on any allowlist.
    moat.put(PUBLIC_NAMESPACE, &wire, base.to_vec(), Some(0))
        .await
        .unwrap();

    // Empty (deny-all) allowlist → the read cannot be an allowlist hit; only
    // the M2 existence read can serve it.
    let store_on =
        OciMoatStore::with_allowlist(Arc::clone(&moat), true, PublicBaseAllowlist::default());
    let store_off =
        OciMoatStore::with_allowlist(Arc::clone(&moat), false, PublicBaseAllowlist::default());
    let unpushed = TenantId::from_uuid(Uuid::from_u128(0xD40D)); // never pushed

    assert!(
        !store_on.routes_to_public(&wire),
        "the digest is NOT allowlisted — the WRITE-path predicate stays false"
    );
    assert_eq!(
        store_on
            .get_blob(&unpushed, &wire)
            .await
            .unwrap()
            .as_deref(),
        Some(base),
        "dedup ON ⇒ the existence read serves the `_public` copy cross-tenant"
    );
    assert_eq!(
        store_off.get_blob(&unpushed, &wire).await.unwrap(),
        None,
        "dedup OFF ⇒ `_public` is not consulted → per-tenant miss → None"
    );
}

/// A `_public` MISS falls back to the per-tenant namespace for ANY blob key
/// (dedup ON); a different tenant that never pushed it still gets `None`.
#[tokio::test]
async fn m2_existence_read_falls_back_to_per_tenant_on_public_miss() {
    let private = b"m2-private-only-layer".as_slice();
    let wire = digest_wire(private);
    let cas = Arc::new(StubCas::default());
    let map = Arc::new(FakeMap::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::clone(&map) as Arc<dyn UrlMapStore>,
        "oci-m2-fallback-test",
    ));
    let owner = TenantId::from_uuid(Uuid::from_u128(0xE60E));
    // Seed ONLY in the owner's per-tenant namespace (nothing in `_public`).
    moat.put(&owner.to_canonical_text(), &wire, private.to_vec(), Some(0))
        .await
        .unwrap();

    let store =
        OciMoatStore::with_allowlist(Arc::clone(&moat), true, PublicBaseAllowlist::default());
    // Owner: `_public` miss → per-tenant hit.
    assert_eq!(
        store.get_blob(&owner, &wire).await.unwrap().as_deref(),
        Some(private),
        "a `_public` miss must fall back to the per-tenant namespace"
    );
    // A different tenant: `_public` miss AND their own per-tenant miss → None
    // (no cross-tenant private leak).
    let other = TenantId::from_uuid(Uuid::from_u128(0xF70F));
    assert_eq!(
        store.get_blob(&other, &wire).await.unwrap(),
        None,
        "a private per-tenant blob is never served to another tenant"
    );
}

#[tokio::test]
async fn downgraded_tenant_oci_write_over_resolved_cap_is_rejected() {
    // WP #10 regression. A DOWNGRADED tenant (resolved cap = 1000 bytes)
    // pushing exclusively over OCI:
    //   * an UNDER-cap push succeeds and accrues bytes_used;
    //   * an OVER-cap push is REJECTED (the resolved cap threaded from the
    //     bearer reserves against `tenant_storage_state`), so OCI can no
    //     longer over-store past the (possibly stale) cap.
    let (store, byte_store, region) = accounting_oci_store();
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xD0_0D));
    let t_text = tenant.to_canonical_text();
    let cap = Some(1_000i64);

    // 600 < 1000 → accrues, seeds the row with the REAL cap.
    push_blob(&store, &tenant, 600, cap)
        .await
        .expect("under-cap OCI push must succeed");
    assert_eq!(byte_store.used(&t_text, &region), 600);

    // 500 more would total 1100 > 1000 → REJECTED (over the resolved cap).
    let err = push_blob(&store, &tenant, 500, cap)
        .await
        .expect_err("over-cap OCI push must be rejected");
    assert!(
        err.contains(crate::byte_accounting::OVER_CAP_SENTINEL),
        "over-cap rejection must carry the 402 sentinel; got: {err}"
    );
    // The rejected push did NOT move the counter.
    assert_eq!(byte_store.used(&t_text, &region), 600);
}

#[tokio::test]
async fn oci_write_with_indeterminate_cap_on_fresh_tenant_fails_closed() {
    // WP #10 fail-closed mirror of native: an unresolvable cap (`None`) on a
    // tenant with NO `tenant_storage_state` row must be REFUSED (never seed
    // an uncapped row from absence) — absence is never treated as unlimited.
    let (store, byte_store, region) = accounting_oci_store();
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xFEED));
    let err = push_blob(&store, &tenant, 100, None)
        .await
        .expect_err("indeterminate cap on a fresh tenant must fail closed");
    assert!(
        err.contains(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL),
        "fresh-tenant indeterminate cap must fail closed (503 sentinel); got: {err}"
    );
    assert_eq!(
        byte_store.used(&tenant.to_canonical_text(), &region),
        0,
        "a fail-closed push must not seed or move the counter"
    );
}

#[tokio::test]
async fn open_upload_enforces_per_tenant_session_cap() {
    // F25 — per-tenant open-session cap.
    //
    // Open OCI_MAX_OPEN_SESSIONS_PER_TENANT sessions for tenant A; the next
    // open must fail. Tenant B is unaffected (independent counter).
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test-cap",
    ));
    let store = OciMoatStore::new(moat, false);
    let tenant_a = TenantId::from_uuid(Uuid::from_u128(0xAA));
    let tenant_b = TenantId::from_uuid(Uuid::from_u128(0xBB));

    // Fill tenant A's quota.
    let mut sessions = Vec::new();
    for _ in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
        let uuid = store.open_upload(&tenant_a).await.unwrap();
        sessions.push(uuid);
    }
    assert_eq!(sessions.len(), OCI_MAX_OPEN_SESSIONS_PER_TENANT);

    // One more for tenant A must be rejected (cap reached).
    let err = store.open_upload(&tenant_a).await.unwrap_err();
    assert!(
        err.contains("too many open upload sessions"),
        "expected cap error, got: {err}"
    );

    // Tenant B is not affected by tenant A's sessions.
    let b_session = store.open_upload(&tenant_b).await;
    assert!(
        b_session.is_ok(),
        "tenant B must not be blocked by tenant A's sessions"
    );

    // After cancelling one of A's sessions the cap is relaxed.
    store.cancel_upload(&tenant_a, &sessions[0]).await.unwrap();
    let new_session = store.open_upload(&tenant_a).await;
    assert!(
        new_session.is_ok(),
        "tenant A must be able to open a new session after cancelling one"
    );
}

/// F25 route-level test: `POST /v2/<repo>/blobs/uploads/` returns
/// `429 Too Many Requests` + `Retry-After` when the per-tenant session
/// cap is exhausted.
///
/// This exercises the full HTTP path (adapter router → `open()` handler
/// → `OciMoatStore::open_upload` → error-mapping → `err_response`) so
/// we verify both the status code and the presence of the `Retry-After`
/// header.
#[tokio::test]
async fn upload_session_cap_returns_429_with_retry_after() {
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xCAFF00);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0xDEAD)),
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

    // Obtain a push+pull bearer for the test tenant.
    let token_resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:myrepo:push,pull",
            Some(&basic(&pt)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(token_resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(token_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bearer = format!("Bearer {}", json["token"].as_str().expect("token field"));

    // Open OCI_MAX_OPEN_SESSIONS_PER_TENANT sessions — each must return 202.
    for _ in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
        let resp = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/v2/myrepo/blobs/uploads/")
                    .header("authorization", &bearer)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::ACCEPTED,
            "expected 202 while filling quota"
        );
    }

    // The next POST must hit the cap → 429 + Retry-After.
    let resp = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method(Method::POST)
                .uri("/v2/myrepo/blobs/uploads/")
                .header("authorization", &bearer)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "expected 429 when session cap is reached"
    );
    assert!(
        resp.headers().contains_key("retry-after"),
        "429 response must carry a Retry-After header"
    );
}

/// Audit #6 / WP-OCI-DOS — global in-flight byte ceiling.
///
/// Sending a chunk that would push the global inflight byte counter over
/// [`OCI_MAX_INFLIGHT_BYTES`] must be rejected with the same
/// "too many open upload sessions" port error (the adapter maps this to
/// 429). This verifies that a single tenant cannot exhaust the shared
/// heap by sending one very large chunk even if it stays below the blob
/// size limit.
///
/// We bypass the ceiling constant by directly manipulating the atomic
/// counter on the `OciMoatStore` — we don't need to allocate GiBs of
/// RAM to test the guard.
#[tokio::test]
async fn append_chunk_respects_global_inflight_ceiling() {
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test-ceiling",
    ));
    let store = OciMoatStore::new(moat, false);
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xCE117));

    let uuid = store.open_upload(&tenant).await.unwrap();

    // Pre-load the global counter to just below the ceiling so a
    // 1-byte chunk tip it over.
    store
        .inflight_bytes
        .store(OCI_MAX_INFLIGHT_BYTES, Ordering::Relaxed);

    let err = store
        .append_chunk(&tenant, &uuid, Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    assert!(
        err.contains("too many open upload sessions"),
        "expected ceiling error, got: {err}"
    );

    // After cancel the counter is released and a fresh session can accept
    // chunks (reset counter so the test is self-contained).
    store.cancel_upload(&tenant, &uuid).await.unwrap();
    store.inflight_bytes.store(0, Ordering::Relaxed);

    let uuid2 = store.open_upload(&tenant).await.unwrap();
    let ok = store
        .append_chunk(&tenant, &uuid2, Bytes::from_static(b"hello"))
        .await;
    assert!(ok.is_ok(), "chunk must succeed when counter is reset");
}

/// rt-nuclear #3/#12 — per-tenant in-flight byte budget (noisy-neighbour
/// starvation). With ONE tenant at its per-tenant slice (well below the
/// GLOBAL ceiling), that tenant's next chunk is rejected 429-mapped, while a
/// DIFFERENT tenant can still push — proving the per-tenant cap isolates
/// tenants. The counter is released on cancel so the tenant recovers.
#[tokio::test]
async fn append_chunk_respects_per_tenant_inflight_budget() {
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test-per-tenant",
    ));
    let store = OciMoatStore::new(moat, false);
    let hog = TenantId::from_uuid(Uuid::from_u128(0x803));
    let victim = TenantId::from_uuid(Uuid::from_u128(0x71C7100));
    assert!(OCI_MAX_INFLIGHT_BYTES_PER_TENANT < OCI_MAX_INFLIGHT_BYTES);

    // The global counter is far below the global ceiling — only the
    // per-tenant slice should trip here.
    let hog_uuid = store.open_upload(&hog).await.unwrap();
    store
        .tenant_inflight
        .lock()
        .unwrap()
        .insert(hog.to_canonical_text(), OCI_MAX_INFLIGHT_BYTES_PER_TENANT);

    let err = store
        .append_chunk(&hog, &hog_uuid, Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    assert!(
        err.contains("too many open upload sessions"),
        "expected per-tenant budget 429-mapped error, got: {err}"
    );
    // The global counter must NOT have been left credited by the rejected
    // append (the global reservation was rolled back).
    assert_eq!(
        store.inflight_bytes.load(Ordering::Relaxed),
        0,
        "a per-tenant-budget rejection must roll back the global reservation"
    );

    // A DIFFERENT tenant is unaffected — no cross-tenant starvation.
    let victim_uuid = store.open_upload(&victim).await.unwrap();
    let ok = store
        .append_chunk(&victim, &victim_uuid, Bytes::from_static(b"hello"))
        .await;
    assert!(
        ok.is_ok(),
        "a second tenant must still push while the first is at its per-tenant budget"
    );

    // After the hog cancels, its per-tenant counter is released and it can
    // push again.
    store.cancel_upload(&hog, &hog_uuid).await.unwrap();
    store
        .tenant_inflight
        .lock()
        .unwrap()
        .remove(&hog.to_canonical_text());
    let hog_uuid2 = store.open_upload(&hog).await.unwrap();
    let recovered = store
        .append_chunk(&hog, &hog_uuid2, Bytes::from_static(b"again"))
        .await;
    assert!(
        recovered.is_ok(),
        "tenant must recover after its budget is released"
    );
}

/// B-225 — the session ceiling rejects before extending the buffer and
/// rolls back both reservations made for the rejected chunk.
#[tokio::test]
async fn append_chunk_rejects_before_session_buffer_crosses_ceiling() {
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test-session-ceiling",
    ));
    let store =
        OciMoatStore::with_allowlist_and_blob_limit(moat, false, PublicBaseAllowlist::default(), 4);
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xB225));
    let uuid = store.open_upload(&tenant).await.unwrap();
    {
        let mut uploads = store.uploads.lock().unwrap();
        uploads
            .get_mut(&uuid)
            .unwrap()
            .buf
            .resize(store.max_blob_size_bytes as usize, 0);
    }

    let err = store
        .append_chunk(&tenant, &uuid, Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    assert!(err.contains("per-session configured blob byte ceiling"));
    assert_eq!(
        store.uploads.lock().unwrap().get(&uuid).unwrap().buf.len(),
        store.max_blob_size_bytes as usize,
        "rejected PATCH must not append to the session buffer"
    );
    assert_eq!(store.inflight_bytes.load(Ordering::Relaxed), 0);
    assert!(store.tenant_inflight.lock().unwrap().is_empty());
}

/// Audit #6 / WP-OCI-DOS — lazy abandoned-session reaper.
///
/// Sessions that have been idle for longer than [`OCI_SESSION_IDLE_TIMEOUT_MS`]
/// are reaped on the next `open_upload`. This prevents a crashed client from
/// permanently locking its tenant out of the per-tenant session cap (F25).
///
/// We inject a stale session directly into the store's upload map to avoid
/// needing to sleep for the full 15-minute timeout.
#[tokio::test]
async fn open_upload_reaps_abandoned_sessions() {
    let cas = Arc::new(StubCas::default());
    let moat = Arc::new(MoatCache::production(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        "oci-test-reaper",
    ));
    let store = OciMoatStore::new(moat, false);
    let tenant = TenantId::from_uuid(Uuid::from_u128(0xABAD1DEA));
    let tenant_text = tenant.to_canonical_text();

    // Fill the per-tenant cap with synthetic stale sessions (last_active
    // set to epoch 0, guaranteed older than any timeout).
    {
        let mut g = store.uploads.lock().unwrap();
        for i in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
            let stale_uuid = format!("{tenant_text}:stale{i:04}");
            g.insert(
                stale_uuid,
                UploadSession {
                    buf: vec![0u8; 1024],
                    last_active_ms: 0, // epoch → always stale
                },
            );
        }
        // Reflect the fake bytes in the global counter.
        store.inflight_bytes.store(
            (OCI_MAX_OPEN_SESSIONS_PER_TENANT as u64) * 1024,
            Ordering::Relaxed,
        );
    }

    // The cap is now full (OCI_MAX_OPEN_SESSIONS_PER_TENANT stale
    // sessions). Without the reaper, open_upload would return an error.
    // With the reaper, all stale sessions are evicted BEFORE the cap check
    // and a new session is opened successfully.
    let result = store.open_upload(&tenant).await;
    assert!(
        result.is_ok(),
        "open_upload must reap stale sessions and succeed; got: {:?}",
        result.err()
    );

    // Stale sessions freed their bytes from the global counter.
    // The new session added 0 bytes (empty buffer), so inflight_bytes
    // should be 0 after reap.
    assert_eq!(
        store.inflight_bytes.load(Ordering::Relaxed),
        0,
        "inflight_bytes must be 0 after stale sessions are reaped"
    );
}

/// Audit #5 / WP-OCI-DOS — manifest PUT body cap.
///
/// A `PUT /v2/<repo>/manifests/<ref>` body larger than
/// `MAX_MANIFEST_BYTES` (4 MiB) must be rejected with `413 Payload Too
/// Large` before any heap allocation for schema parsing. This exercises
/// the full HTTP path through the router so we see the correct status code.
#[tokio::test]
async fn manifest_put_oversized_body_returns_413() {
    use corelink_adapter_host::oci::server::handlers::MAX_MANIFEST_BYTES;

    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0x0DEBAD);
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

    // Get a push bearer.
    let token_resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:myimg:push,pull",
            Some(&basic(&pt)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(token_resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(token_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let bearer = format!("Bearer {}", json["token"].as_str().expect("token field"));

    // Send a body that is 1 byte over the cap — must be 413.
    let oversized = vec![b'x'; MAX_MANIFEST_BYTES + 1];
    let resp = app
        .oneshot(
            HttpRequest::builder()
                .method(Method::PUT)
                .uri("/v2/myimg/manifests/latest")
                .header("authorization", bearer)
                .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "oversized manifest body must return 413"
    );
}

// ── Cluster E / A24: UNAUTH /token backend fault is OPAQUE ─────────────────

/// `PatRowLookup` that always fails with a RAW backend string mimicking the
/// CF D1 HTTP API error (status + body, possibly SQL). The verifier maps
/// this to `VerifyError::Backend`, which the OCI resolver surfaces as the
/// adapter `Auth` error — the A24 leak path.
struct BackendErrLookup;
#[async_trait]
impl PatRowLookup for BackendErrLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        Err(
                "D1 HTTP 500 Internal Server Error: {\"errors\":[{\"code\":7500,\
                 \"message\":\"no such table: pat in SELECT tenant_id, pat_hash, scope FROM pat WHERE token_id = ?1\"}]}"
                    .to_owned(),
            )
    }
}

#[allow(dead_code)]
const B126_M2_TEST_1_2_REANCHOR: () = ();

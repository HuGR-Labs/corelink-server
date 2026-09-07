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

    // The global counter is far below the global ceiling — only the
    // per-tenant slice should trip here.
    let hog_uuid = store.open_upload(&hog).await.unwrap();
    let hog_key = hog.to_canonical_text();
    let seeded_hog_budget = {
        let mut tenant_inflight = store.tenant_inflight.lock().unwrap();
        tenant_inflight.insert(hog_key.clone(), OCI_MAX_INFLIGHT_BYTES_PER_TENANT);
        tenant_inflight.get(&hog_key).copied()
    };
    assert_eq!(
        seeded_hog_budget,
        Some(OCI_MAX_INFLIGHT_BYTES_PER_TENANT),
        "hog setup must seed the per-tenant budget"
    );
    assert!(
        seeded_hog_budget.is_some_and(|bytes| bytes < OCI_MAX_INFLIGHT_BYTES),
        "per-tenant budget must remain below the global in-flight byte budget"
    );

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

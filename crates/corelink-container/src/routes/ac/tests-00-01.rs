    /// A NON-create-only runner-job cred (marker set, NO create-only header) may
    /// still overwrite (idempotent re-write of an existing entry → 200 OK) — the
    /// deny-DELETE / ac-key-allow behavior is UNCHANGED by this feature.
    #[tokio::test]
    async fn non_create_only_runner_job_overwrite_still_allowed() {
        let (_a, _s, st) = fixture();
        seed_ac_entry(&st, b"result");
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            // NO create-only header ⇒ overwrite (idempotent) still allowed.
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "a runner-job WITHOUT create-only re-writing an identical entry is an idempotent 200"
        );
    }

    /// GET list returns the tenant's refs (read-only PAT is sufficient).
    #[tokio::test]
    async fn list_returns_tenant_refs() {
        let (_a, _s, st) = fixture();
        for d in ["d1", "d2", "d3"] {
            st.update
                .update(AcUpdateRequest::new(
                    TEST_TENANT,
                    d,
                    b"r".to_vec(),
                    "anon@t1",
                    TEST_TENANT,
                    1,
                ))
                .expect("seed");
        }
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let refs = v["refs"].as_array().expect("refs array");
        assert_eq!(refs.len(), 3);
        assert!(v.get("next_cursor").is_some());
    }

    /// List pagination: limit=1 returns one entry + a non-null cursor; the
    /// cursor fetches the next page.
    #[tokio::test]
    async fn list_pagination_cursor_walks_pages() {
        let (_a, _s, st) = fixture();
        for d in ["d1", "d2"] {
            st.update
                .update(AcUpdateRequest::new(
                    TEST_TENANT,
                    d,
                    b"r".to_vec(),
                    "anon@t1",
                    TEST_TENANT,
                    1,
                ))
                .expect("seed");
        }
        let app = router(st);
        let page1 = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}?limit=1"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.clone().oneshot(page1).await.expect("oneshot");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(v["refs"].as_array().expect("refs").len(), 1);
        let cursor = v["next_cursor"]
            .as_str()
            .expect("cursor present")
            .to_owned();
        let page2 = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}?limit=1&cursor={cursor}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp2 = app.oneshot(page2).await.expect("oneshot");
        let body2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .expect("body");
        let v2: serde_json::Value = serde_json::from_slice(&body2).expect("json");
        assert_eq!(v2["refs"].as_array().expect("refs").len(), 1);
        // The two pages return distinct refs.
        assert_ne!(v["refs"][0]["ref_key"], v2["refs"][0]["ref_key"]);
    }

    /// Cross-tenant LIST → 403.
    #[tokio::test]
    async fn cross_tenant_list_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/ac/victim")
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── finding #1: storage byte accounting (cap enforcement) ────────────────

    /// The KILLING finding-#1 test: with the storage counter AT the cap, an AC
    /// update that would store new bytes is rejected 402 — the previously-inert
    /// storage cap now trips on a real HTTP write.
    // multi_thread: the `AccountingAcHandler` decorator bridges the async
    // accountant to the sync update trait with `block_in_place` (needs a
    // multi-thread runtime; matches production `#[tokio::main]`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_over_storage_cap_returns_402() {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, testing::Row, AccountingAcHandler, ByteAccountant,
            ByteStore,
        };

        let (_a, _s, mut st) = fixture();
        let store = Arc::new(InMemoryByteStore::new());
        store.seed(TEST_TENANT, "iad", Row { used: 4, quota: 4 }); // at the cap
        let store_dyn: Arc<dyn ByteStore> = store;
        let acc = Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()));
        // Wrap the AC update + delete trait objects in the decorator (mirrors the
        // production wiring) so the reservation runs BEFORE the inner update.
        let acct = Arc::new(AccountingAcHandler::new(
            st.update.clone(),
            st.delete.clone(),
            acc,
        ));
        st.update = acct.clone();
        st.delete = acct;
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result-bytes-over-cap".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// Control: an uncapped tenant's AC update accrues its bytes and succeeds
    /// (201) — proving the storage counter MOVES (it never did before #1).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_under_storage_cap_accrues_and_succeeds() {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, AccountingAcHandler, ByteAccountant, ByteStore,
        };

        let (_a, _s, mut st) = fixture();
        let store = Arc::new(InMemoryByteStore::new()); // uncapped fresh row
        let store_dyn: Arc<dyn ByteStore> = store.clone();
        let acct = Arc::new(AccountingAcHandler::new(
            st.update.clone(),
            st.delete.clone(),
            Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned())),
        ));
        st.update = acct.clone();
        st.delete = acct;
        let app = router(st);
        let body = b"result".to_vec();
        let body_len = body.len() as i64;
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            // Worker-injected per-tier cap seeds the fresh row; without it the
            // reservation fails CLOSED (no cap ⇒ 503). See storage_quota_from_headers.
            .header(crate::byte_accounting::STORAGE_QUOTA_HEADER, "1000000")
            .body(Body::from(body))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(
            store.used(TEST_TENANT, "iad"),
            body_len,
            "a durable AC update must accrue its bytes into the storage counter"
        );
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A request with the PAT gate wired but NO bearer Authorization header is
    /// rejected 401 — the native gate fails CLOSED on a missing token (defense-
    /// in-depth, even with the Worker-set tenant header present).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::adapter_pat::PatRow;
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use corelink_pat::PatSigningKey;

        let (_a, _s, mut st) = fixture();
        let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
        let verifier = verifier_with_row(
            "no-such-token".to_owned(),
            PatRow {
                tenant_id: TEST_TENANT.to_owned(),
                pat_hash: String::new(),
                scope: "cas:rw".to_owned(),
                find_only: false,
                runner_job: false,
            },
            key,
        );
        st.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            // No Authorization header — the gate rejects.
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

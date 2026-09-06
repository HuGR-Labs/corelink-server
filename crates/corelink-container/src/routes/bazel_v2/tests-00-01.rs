    #[tokio::test]
    async fn cas_read_sentinel_tenant_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/_unknown/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            // Sentinel tenant header must be rejected even though instance
            // == header (so the cross-tenant check would otherwise pass).
            .header("x-corelink-tenant-id", "_unknown")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cas_write_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let payload = b"no-tenant".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ac_read_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn find_missing_missing_tenant_header_returns_401() {
        let app = router(make_state());
        // A valid JSON body — the 401 must fire BEFORE the body is parsed.
        let body = serde_json::json!({
            "blobDigests": [{"hash": HASH_A, "sizeBytes": 5}]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── finding #4: native PAT possession gate ───────────────────────────────

    /// A Bazel CAS read with the PAT gate wired but NO bearer Authorization
    /// header is rejected 401 — the native gate fails CLOSED on a missing token
    /// even though the Worker-set tenant header is present (defense-in-depth on
    /// the previously HMAC-only Bazel REAPI surface).
    #[tokio::test]
    async fn pat_gate_missing_bearer_returns_401() {
        use crate::adapter_pat::PatRow;
        use crate::native_pat_gate::testing::verifier_with_row;
        use crate::native_pat_gate::NativePatGate;
        use corelink_pat::PatSigningKey;

        let mut state = make_state();
        let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
        let verifier = verifier_with_row(
            "no-such-token".to_owned(),
            PatRow {
                tenant_id: TENANT.to_owned(),
                pat_hash: String::new(),
                scope: "cas:rw".to_owned(),
                find_only: false,
                runner_job: false,
            },
            key,
        );
        state.pat_gate = Some(Arc::new(NativePatGate::new_for_test(verifier)));
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/5"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            // No Authorization header — the gate rejects.
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── cluster F: per-tenant pre-body write concurrency cap ──────────────────

    /// With the tenant AT `BAZEL_WRITE_CONCURRENCY_LIMIT` in-flight writes, the
    /// next CAS write is rejected 429 BEFORE its body is buffered — the
    /// `BazelPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
    /// Proven deterministically by sending a body LARGER than the 64 MiB cache
    /// limit: if the body were buffered first, the body-limit layer would reject
    /// it; because the concurrency guard runs first, we get 429 and the oversized
    /// body is never read.
    #[tokio::test]
    async fn cas_write_at_concurrency_limit_returns_429_before_body() {
        let state = make_state();
        {
            let mut g = state.put_inflight.lock().unwrap();
            g.insert(TENANT.to_owned(), BAZEL_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(state);
        let oversized = Body::from(vec![0u8; CACHE_ENTRY_MAX_BYTES + 1]); // > 64 MiB
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{HASH_A}/5");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(oversized)
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit Bazel write must be rejected 429 by the pre-body concurrency guard"
        );
    }

    /// A write BELOW the limit succeeds and releases its slot (counter back to 0).
    #[tokio::test]
    async fn cas_write_below_limit_releases_slot() {
        let state = make_state();
        let app = router(state.clone());
        let payload = b"slot-release".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/uploads/u1/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let g = state.put_inflight.lock().unwrap();
        assert_eq!(
            g.get(TENANT),
            None,
            "the concurrency slot must be released after the write completes"
        );
    }

    // ── Stock-Bazel HTTP cache alias (/bazel/cache/{cas,ac}/:hash) ─────────────
    //
    // These mirror the REST-scheme tests above but against the stock alias shape
    // (no :instance, no :size). Isolation here is by the per-tenant namespace
    // (uniform 404 cross-tenant), not the adapter's :instance!=tenant 403.

    /// Seed a blob via the stock CAS-write alias, returning nothing (the caller
    /// already knows the hash). 204 on success.
    async fn seed_http_cas(app: &axum::Router, tenant: &str, hash: &str, payload: &[u8]) {
        let uri = format!("/bazel/cache/cas/{hash}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.to_vec()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "seed PUT should 204");
    }

    /// Happy path: stock CAS write then read round-trips the exact bytes — proves
    /// `bazel --remote_cache=https://host/bazel/cache` works end to end.
    #[tokio::test]
    async fn http_cas_write_then_read_round_trip() {
        let app = router(make_state());
        let payload = b"stock bazel http cache".to_vec();
        let hash = sha256_hex(&payload);
        seed_http_cas(&app, TENANT, &hash, &payload).await;

        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{hash}"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    /// The stock alias shares the SAME store as the REST scheme: a blob written
    /// via the REST CAS-write route is readable via the stock CAS-read alias
    /// (one store, two front doors).
    #[tokio::test]
    async fn http_cas_read_sees_rest_written_blob() {
        let app = router(make_state());
        let payload = b"written via REST, read via stock".to_vec();
        let hash = sha256_hex(&payload);
        // Seed through the REST scheme.
        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{hash}"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    /// Happy path: stock AC write then read round-trips the payload.
    #[tokio::test]
    async fn http_ac_write_then_read_round_trip() {
        let app = router(make_state());
        let hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let payload = b"stock_action_result".to_vec();
        let uri = format!("/bazel/cache/ac/{hash}");

        let put = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.clone()))
            .unwrap();
        let put_resp = app.clone().oneshot(put).await.expect("PUT oneshot");
        assert_eq!(put_resp.status(), StatusCode::NO_CONTENT);

        let get = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let get_resp = app.oneshot(get).await.expect("GET oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let got = to_bytes(get_resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    /// Read miss → 404.
    #[tokio::test]
    async fn http_cas_read_miss_returns_404() {
        let app = router(make_state());
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{HASH_A}"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Tenant isolation (uniform 404): tenant A writes a blob; a DIFFERENT
    /// tenant B reading the SAME hash gets a 404 — B never sees A's bytes and
    /// cannot even distinguish presence. This is the alias's isolation invariant
    /// (the REST scheme's :instance!=tenant 403 has no analog here because there
    /// is no :instance to mismatch — the tenant is the header alone).
    #[tokio::test]
    async fn http_cross_tenant_read_returns_404_not_bytes() {
        let app = router(make_state());
        let payload = b"tenant A private blob".to_vec();
        let hash = sha256_hex(&payload);
        seed_http_cas(&app, "tenant-a", &hash, &payload).await;

        // tenant-b asks for the same hash → miss (namespace-isolated).
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{hash}"))
            .method("GET")
            .header("x-corelink-tenant-id", "tenant-b")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "a cross-tenant hash must be a uniform 404, never another tenant's bytes"
        );
    }

    /// Fail-CLOSED: a stock CAS read with NO `x-corelink-tenant-id` header → 401
    /// (even with a valid scope) BEFORE any storage access.
    #[tokio::test]
    async fn http_cas_read_missing_tenant_returns_401() {
        let app = router(make_state());
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{HASH_A}"))
            .method("GET")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// A sentinel tenant (`_public`, the shared dedup namespace) is rejected 401
    /// on the alias too — a masquerade can never reach the shared namespace.
    #[tokio::test]
    async fn http_cas_read_public_sentinel_returns_401() {
        let app = router(make_state());
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{HASH_A}"))
            .method("GET")
            .header(
                "x-corelink-tenant-id",
                crate::adapter_cache::PUBLIC_NAMESPACE,
            )
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Fail-CLOSED scope: a read-only (`cas:r`) token on a stock CAS write is
    /// rejected 403 BEFORE the adapter — the privilege-escalation the gate stops.
    #[tokio::test]
    async fn http_cas_write_read_only_scope_returns_403() {
        let app = router(make_state());
        let payload = b"read-only-cannot-write".to_vec();
        let hash = sha256_hex(&payload);
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{hash}"))
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// Content-addressing boundary: a stock CAS write whose body does NOT hash to
    /// the URL `:hash` is rejected 422 — poisoned bytes never enter the keyspace.
    #[tokio::test]
    async fn http_cas_write_sha256_mismatch_returns_422() {
        let app = router(make_state());
        let payload = b"honest bytes".to_vec();
        let wrong_hash = "0".repeat(64);
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{wrong_hash}"))
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// WP5b parity on the alias: a narrowed runner-job PAT pinned to a DIFFERENT
    /// AC key may not write another key through the stock AC alias (403 before
    /// storage); `"*"` (launch default) is a NO-OP that still writes.
    #[tokio::test]
    async fn http_ac_write_runner_job_pin_denies_other_key() {
        let write_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let pinned_key = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let payload = b"result".to_vec();
        let uri = format!("/bazel/cache/ac/{write_hash}");

        let app = router(make_state());
        let put = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, pinned_key)
            .body(Body::from(payload.clone()))
            .unwrap();
        let resp = app.oneshot(put).await.expect("PUT oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "pinned-key mismatch must 403"
        );

        let app2 = router(make_state());
        let put2 = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::from(payload))
            .unwrap();
        let resp2 = app2.oneshot(put2).await.expect("PUT oneshot");
        assert_eq!(
            resp2.status(),
            StatusCode::NO_CONTENT,
            "wildcard write must pass"
        );
    }

    /// A read-only (`cas:r`) token PASSES the read gate on a stock CAS read: the
    /// blob is absent so the route returns 404 — proving the gate let the read
    /// through (a denied scope would 403 before storage).
    #[tokio::test]
    async fn http_cas_read_read_only_scope_passes_gate_then_404() {
        let app = router(make_state());
        let req = Request::builder()
            .uri(format!("/bazel/cache/cas/{HASH_A}"))
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

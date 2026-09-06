
    use super::*;

    #[test]
    fn bazel_http_ceiling_is_the_shared_cache_entry_boundary() {
        assert_eq!(
            corelink_bazel_bridge::MAX_BLOB_SIZE_BYTES,
            CACHE_ENTRY_MAX_BYTES as u64
        );
    }
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use corelink_bazel_bridge::digest::sha256_hex;
    use tower::ServiceExt;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TENANT: &str = "acme";

    /// Build a `HeaderMap` carrying the given `x-corelink-tenant-id` value.
    fn headers_with_tenant(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "x-corelink-tenant-id",
            axum::http::HeaderValue::from_str(value).expect("valid header value"),
        );
        h
    }

    // ── Reserved-sentinel tenant rejection (fix-#4 parity) ────────────────────
    //
    // The REAPI surface has NO `AuthTenant` extractor; `caller_tenant` is the
    // backstop and MUST reject the SAME reserved set as `auth_tenant`, incl.
    // `_oci` and `_public` (the shared cross-tenant dedup namespace). A `_public`
    // claim reaching storage poisons the shared namespace.

    #[tokio::test]
    async fn caller_tenant_rejects_oci_sentinel() {
        let h = headers_with_tenant("_oci");
        assert!(
            caller_tenant(&h).is_err(),
            "_oci must never be accepted as a tenant on the REAPI surface"
        );
    }

    #[tokio::test]
    async fn caller_tenant_rejects_public_namespace_sentinel() {
        let h = headers_with_tenant(crate::adapter_cache::PUBLIC_NAMESPACE);
        assert!(
            caller_tenant(&h).is_err(),
            "_public (shared dedup namespace) must never be accepted as a tenant"
        );
    }

    #[tokio::test]
    async fn caller_tenant_accepts_concrete_tenant() {
        let h = headers_with_tenant("acme");
        assert_eq!(
            caller_tenant(&h).expect("concrete tenant must be accepted"),
            "acme"
        );
    }

    fn make_state() -> BazelRouteState {
        build_handlers()
    }

    /// Shared fixture: PUT a blob via the route, return the hash used.
    async fn seed_cas_via_route(app: &axum::Router, tenant: &str, hash: &str, payload: &[u8]) {
        let size = payload.len();
        let uuid = "test-uuid-0000";
        let uri = format!("/bazel/v2/{tenant}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", tenant)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.to_vec()))
            .unwrap();
        // Must clone the router for oneshot; caller holds the arc in state.
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "seed PUT should return 204"
        );
    }

    // ── CAS read happy path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_hit_returns_200_with_bytes() {
        let state = make_state();
        let payload = b"hello bazel".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state.clone());

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let got = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    // ── CAS read miss ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_miss_returns_404() {
        let state = make_state();
        let app = router(state);
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── CAS write + read round trip ───────────────────────────────────────────

    #[tokio::test]
    async fn cas_write_then_read_round_trip() {
        let state = make_state();
        let payload = b"round trip data".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let size = payload.len();
        let read_uri = format!("/bazel/v2/{TENANT}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&read_uri)
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

    // ── AC write + read round trip ────────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_then_read_round_trip() {
        let state = make_state();
        let hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let payload = b"action_result_payload".to_vec();
        let size = payload.len();
        let app = router(state);

        // AC write (PUT)
        let write_uri = format!("/bazel/v2/{TENANT}/blobs/ac/{hash}/{size}");
        let put_req = Request::builder()
            .uri(&write_uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload.clone()))
            .unwrap();
        let put_resp = app.clone().oneshot(put_req).await.expect("PUT oneshot");
        assert_eq!(put_resp.status(), StatusCode::NO_CONTENT);

        // AC read (GET)
        let get_req = Request::builder()
            .uri(&write_uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let get_resp = app.oneshot(get_req).await.expect("GET oneshot");
        assert_eq!(get_resp.status(), StatusCode::OK);
        let got = to_bytes(get_resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    /// WP5b parity with the native `ac.rs` gate: a narrowed runner-job PAT with a
    /// PINNED AC key may NOT write a DIFFERENT key through the Bazel AC surface
    /// (403 before any storage), and the `"*"` wildcard (the launch default) is a
    /// NO-OP that still writes. Closes the pin-bypass where a per-job credential
    /// could route an arbitrary AC write via `/bazel/v2/.../blobs/ac/:hash`.
    #[tokio::test]
    async fn runner_job_ac_pin_denies_bazel_write_to_other_key() {
        let write_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let pinned_key = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let payload = b"result".to_vec();
        let size = payload.len();
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{write_hash}/{size}");

        // Pinned to a DIFFERENT key → 403 before storage.
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
            "pinned key mismatch must 403"
        );
        let body = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert_eq!(body.as_ref(), b"ac write outside the job's allowed key");

        // Wildcard `"*"` (launch default) → NO-OP, the write proceeds.
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

    // ── findMissingBlobs — all absent ────────────────────────────────────────

    #[tokio::test]
    async fn find_missing_all_absent_returns_both_digests() {
        let state = make_state();
        let app = router(state);
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": HASH_A, "sizeBytes": 5},
                {"hash": hash_b, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v["missingBlobDigests"]
                .as_array()
                .expect("missingBlobDigests")
                .len(),
            2
        );
    }

    // ── findMissingBlobs — one present, one absent ────────────────────────────

    #[tokio::test]
    async fn find_missing_partial_returns_only_absent() {
        let state = make_state();
        let payload = b"present blob".to_vec();
        let hash = sha256_hex(&payload);
        let app = router(state);

        seed_cas_via_route(&app, TENANT, &hash, &payload).await;

        let hash_absent = HASH_A;
        let body = serde_json::json!({
            "blobDigests": [
                {"hash": &hash, "sizeBytes": payload.len()},
                {"hash": hash_absent, "sizeBytes": 5}
            ]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let missing = v["missingBlobDigests"].as_array().expect("array");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0]["hash"], hash_absent);
    }

    // ── findMissingBlobs — quota charged for EVERY digest (CAA-360 #14/#18) ────

    /// Build a `BazelRouteState` whose quota gate is seeded so the tenant has
    /// `budget_micros` of headroom at a flat `1` micro-USD per op. Mirrors
    /// `cas::fixture_over_ceiling`'s wiring (in-memory store + fake clock).
    fn make_state_with_quota(tenant: &str, budget_micros: i64) -> BazelRouteState {
        use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
        use crate::wall_clock::InMemoryFakeWallClock;

        let mut state = build_handlers();
        let store = InMemoryQuotaStore::new();
        store.seed(
            tenant,
            QuotaState {
                monthly_budget_usd_micros: budget_micros,
                accrued_usd_micros: 0,
                cycle_anchor_ms: 1_700_000_000_000,
            },
        );
        let store: Arc<dyn QuotaStore> = Arc::new(store);
        // Clock pinned AT the anchor so the cycle never rolls mid-test.
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        state.quota = Some(crate::routes::QuotaGate::new_for_test(guard, 1));
        state
    }

    /// A `findMissingBlobs` batch of N > 64 digests must be charged for ALL N
    /// digests, not the legacy 64-iteration cap. With a budget of 100 micro-USD
    /// at 1 µ$/op, a 150-digest batch costs 150 µ$ — over the ceiling → 402.
    /// Under the old `BATCH_QUOTA_ITERS_CAP = 64`, only 64 µ$ would have been
    /// charged (64 < 100), so the batch would have WRONGLY returned 200. This
    /// test fails on that regression and passes on the proportional charge.
    #[tokio::test]
    async fn find_missing_charges_every_digest_beyond_cap_trips_402() {
        let state = make_state_with_quota(TENANT, 100);
        let app = router(state);

        // 150 distinct, canonical 64-hex digests (> the old 64 cap).
        let blob_digests: Vec<serde_json::Value> = (0..150u32)
            .map(|i| serde_json::json!({ "hash": format!("{i:064x}"), "sizeBytes": 5 }))
            .collect();
        let body = serde_json::json!({ "blobDigests": blob_digests }).to_string();

        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::PAYMENT_REQUIRED,
            "150-digest batch (150 µ$) must exceed the 100 µ$ ceiling — proves \
             every digest is charged, not a 64-capped subset"
        );
    }

    /// Control: a batch that fits under the ceiling proceeds (200), proving the
    /// batch gate is proportional, not a blanket block. 80 digests = 80 µ$ < 100.
    #[tokio::test]
    async fn find_missing_under_ceiling_proceeds() {
        let state = make_state_with_quota(TENANT, 100);
        let app = router(state);

        let blob_digests: Vec<serde_json::Value> = (0..80u32)
            .map(|i| serde_json::json!({ "hash": format!("{i:064x}"), "sizeBytes": 5 }))
            .collect();
        let body = serde_json::json!({ "blobDigests": blob_digests }).to_string();

        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "80-digest batch (80 µ$) is under the 100 µ$ ceiling — must proceed"
        );
    }

    // ── Cross-tenant denial — CAS read ────────────────────────────────────────

    #[tokio::test]
    async fn cas_read_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        // instance = "victim" in URL, but caller header = "attacker"
        let uri = format!("/bazel/v2/victim/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            // Carry a valid cache scope so the request CLEARS the scope gate
            // and reaches the adapter's cross-tenant check (the SUT here).
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── Cross-tenant denial — AC write ────────────────────────────────────────

    #[tokio::test]
    async fn ac_write_cross_tenant_returns_403() {
        let state = make_state();
        let app = router(state);
        let hash = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
        let uri = format!("/bazel/v2/victim/blobs/ac/{hash}/8");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", "attacker")
            .header("x-corelink-token-prefix", "tok_attacker")
            // Valid write scope so the request clears the scope gate and the
            // adapter's cross-tenant check is what produces the 403.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"payload".to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── CAS write size mismatch returns 422 ───────────────────────────────────

    #[tokio::test]
    async fn cas_write_size_mismatch_returns_422() {
        let state = make_state();
        let app = router(state);
        let payload = b"five!".to_vec(); // 5 bytes
        let hash = sha256_hex(&payload);
        // Claim size 99 but send 5 bytes.
        let uuid = "test-uuid-mismatch";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/99");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── CAS write with a MISMATCHED sha256 digest returns 422 ─────────────────

    /// REAPI v2 content-addresses with SHA-256: a write whose body does NOT
    /// hash to the client-supplied digest is rejected 422 at the REAPI
    /// boundary (`verify_sha256`) BEFORE delegating to the shared handler —
    /// poisoned bytes never enter the `bazel/sha256/` keyspace.
    #[tokio::test]
    async fn cas_write_sha256_mismatch_returns_422() {
        let state = make_state();
        let app = router(state);
        let payload = b"honest bazel bytes".to_vec();
        // A well-formed but WRONG sha256 digest (correct length, wrong value).
        let wrong_hash = "0".repeat(64);
        let size = payload.len();
        let uuid = "test-uuid-poison";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{wrong_hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── Invalid digest in URL returns 400 ─────────────────────────────────────

    #[tokio::test]
    async fn cas_read_bad_digest_returns_400() {
        let state = make_state();
        let app = router(state);
        // hash is too short → invalid digest
        let uri = format!("/bazel/v2/{TENANT}/blobs/badhash/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── build_handlers smoke test ─────────────────────────────────────────────

    #[test]
    fn build_handlers_constructs_without_panic() {
        let state = build_handlers();
        let _r = router(state);
    }

    // ── Route constants use matchit-0.8 brace syntax (NOT the legacy 0.7 colon form) ──

    #[test]
    fn route_paths_use_brace_syntax_not_colon() {
        // Regression net per DEBT-029 (post axum-0.8): `:name` is a LITERAL in
        // matchit 0.8; `{name}` is the capture.
        let paths = [
            "/bazel/v2/{instance}/blobs/{hash}/{size}",
            "/bazel/v2/{instance}/blobs/ac/{hash}/{size}",
            "/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}",
            "/bazel/v2/{instance}/findMissingBlobs",
        ];
        for p in paths {
            assert!(
                !p.contains(':'),
                "path {p:?} must use {{name}} syntax, not :name"
            );
        }
    }

    // ── Cache-scope enforcement (x-corelink-scope) ────────────────────────────
    //
    // The Bazel REAPI surface was the one PAT-reachable cache surface left
    // UNGATED after PR #159 wired the scope gate into CAS/AC/Turbo. These
    // tests pin the gate fail-CLOSED on the Bazel handlers so a read-only
    // (`cas:r`) token can no longer write blobs via Bazel. They mirror the
    // CAS/AC/Turbo scope tests: missing scope → 403, read-only on a write →
    // 403, `cas:rw`/`admin` → passes the gate.

    /// Read the 403 response body as bytes (helper for the assertions below).
    async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
        to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body")
            .to_vec()
    }

    /// Fail-CLOSED: a CAS read with NO `x-corelink-scope` header is rejected
    /// 403 "insufficient scope" BEFORE any storage access.
    #[tokio::test]
    async fn cas_read_missing_scope_returns_403_insufficient_scope() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// Fail-CLOSED: a CAS write with NO scope header is rejected 403 BEFORE
    /// the adapter `cas_put` is reached (the gap the cold review flagged).
    #[tokio::test]
    async fn cas_write_missing_scope_returns_403_insufficient_scope() {
        let app = router(make_state());
        let payload = b"no-scope-write".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-noscope";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token on a CAS write is rejected 403 BEFORE the
    /// adapter — the core privilege-escalation the gate prevents.
    #[tokio::test]
    async fn cas_write_read_only_scope_returns_403() {
        let app = router(make_state());
        let payload = b"read-only-cannot-write".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-readonly";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token on an AC write is rejected 403 BEFORE the
    /// adapter (AC update is a cache write).
    #[tokio::test]
    async fn ac_write_read_only_scope_returns_403() {
        let app = router(make_state());
        let hash = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let uri = format!("/bazel/v2/{TENANT}/blobs/ac/{hash}/7");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"payload".to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// Fail-CLOSED: findMissingBlobs (a read/existence probe) with NO scope
    /// header is rejected 403 BEFORE the find-missing handler runs.
    #[tokio::test]
    async fn find_missing_missing_scope_returns_403() {
        let app = router(make_state());
        let body = serde_json::json!({
            "blobDigests": [{"hash": HASH_A, "sizeBytes": 5}]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header("content-type", "application/json")
            // no x-corelink-scope header → fail-CLOSED
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// ADR-0071: a find-ONLY (`find-missing`) token PASSES the find-missing gate
    /// (200, not 403) — the true least-privilege existence probe works.
    #[tokio::test]
    async fn find_missing_find_only_scope_passes_gate() {
        let app = router(make_state());
        let body = serde_json::json!({
            "blobDigests": [{"hash": HASH_A, "sizeBytes": 5}]
        })
        .to_string();
        let req = Request::builder()
            .uri(format!("/bazel/v2/{TENANT}/findMissingBlobs"))
            .method("POST")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header("content-type", "application/json")
            .header(crate::scope::SCOPE_HEADER, "find-missing")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "find-only must pass find-missing"
        );
    }

    /// ADR-0071: a find-ONLY (`find-missing`) token is DENIED on a CAS read
    /// (403) — it grants existence probes ONLY, never download. Proves the
    /// least-privilege boundary (find ⊄ read).
    #[tokio::test]
    async fn find_only_scope_denied_on_cas_read() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "find-missing")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_bytes(resp).await, b"insufficient scope");
    }

    /// A read-only (`cas:r`) token PASSES the read gate on a CAS read: the
    /// blob is absent so the route returns 404 — proving the gate let the
    /// read through (a denied scope would 403 before storage).
    #[tokio::test]
    async fn cas_read_read_only_scope_passes_gate_then_404() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// An `admin` token (the prod superset, pre back-fill) passes the write
    /// gate: a CAS write succeeds (204) — proving `admin` grants cache rw and
    /// the gate is a NO-OP for live admin-scoped traffic.
    #[tokio::test]
    async fn cas_write_admin_scope_succeeds() {
        let app = router(make_state());
        let payload = b"admin-writes-ok".to_vec();
        let hash = sha256_hex(&payload);
        let size = payload.len();
        let uuid = "test-uuid-admin";
        let uri = format!("/bazel/v2/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
        let req = Request::builder()
            .uri(&uri)
            .method("PUT")
            .header("x-corelink-tenant-id", TENANT)
            .header("x-corelink-token-prefix", "tok_test")
            .header(crate::scope::SCOPE_HEADER, "admin")
            .body(Body::from(payload))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // ── F-defense: fail-CLOSED on missing / sentinel tenant header ─────────────
    //
    // The previous `caller_tenant` fell back to the `"_unknown"` sentinel, so
    // an unauthenticated request flowed into the bridge. These prove every
    // Bazel surface now rejects 401 (and BEFORE any storage / body parse) when
    // the authenticated-tenant header is absent or a sentinel — even with a
    // valid cache scope.

    #[tokio::test]
    async fn cas_read_missing_tenant_header_returns_401() {
        let app = router(make_state());
        let uri = format!("/bazel/v2/{TENANT}/blobs/{HASH_A}/10");
        let req = Request::builder()
            .uri(&uri)
            .method("GET")
            // No x-corelink-tenant-id; valid scope so the 401 is the tenant
            // gate, not the scope gate.
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

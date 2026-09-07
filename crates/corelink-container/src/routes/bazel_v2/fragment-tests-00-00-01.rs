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

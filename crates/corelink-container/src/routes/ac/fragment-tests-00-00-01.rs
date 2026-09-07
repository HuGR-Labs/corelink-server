    /// Cross-tenant DELETE → 403 (path tenant != authenticated tenant).
    #[tokio::test]
    async fn cross_tenant_delete_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/ac/victim/{VALID_DIGEST}")
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── cf-multitenant WP5b: runner-job PAT narrowing ────────────────────────

    /// A second canonical digest, distinct from `VALID_DIGEST`, for the
    /// exact-key restriction tests.
    const OTHER_DIGEST: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    /// runner-job marker present ⇒ AC DELETE is denied 403 even with a
    /// write-capable scope (a per-job credential must not evict the cache).
    #[tokio::test]
    async fn runner_job_ac_delete_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(
            body.as_ref(),
            b"delete not permitted for a runner-job credential"
        );
    }

    /// runner-job + exact-key pin: an AC update to the pinned key passes the
    /// gate (201/200), to a DIFFERENT key is denied 403.
    #[tokio::test]
    async fn runner_job_ac_update_exact_key_enforced() {
        // Allowed key == the request path digest ⇒ passes.
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, VALID_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED, "pinned key must pass");

        // Allowed key != the request path digest ⇒ 403.
        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let req2 = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, OTHER_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp2 = app2.oneshot(req2).await.expect("oneshot");
        assert_eq!(
            resp2.status(),
            StatusCode::FORBIDDEN,
            "off-key write must 403"
        );
        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"ac write outside the job's allowed key");
    }

    /// runner-job + wildcard key (`*`, the launch default): an AC update to ANY
    /// key passes the gate; DELETE is still denied.
    #[tokio::test]
    async fn runner_job_ac_wildcard_allows_write_but_denies_delete() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let put = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(put).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "wildcard write must pass"
        );

        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
            .body(Body::empty())
            .expect("request");
        let resp2 = app2.oneshot(del).await.expect("oneshot");
        assert_eq!(
            resp2.status(),
            StatusCode::FORBIDDEN,
            "wildcard delete still denied"
        );
    }

    /// NO runner-job marker (normal PAT): AC update + delete behave EXACTLY as
    /// before — the WP5b checks are no-ops (even if a stray key-allow header
    /// with the WRONG key is present without the marker).
    #[tokio::test]
    async fn no_runner_job_marker_is_no_op() {
        // Update to VALID_DIGEST with an OTHER_DIGEST key-allow but NO marker →
        // still succeeds (the pin is ignored without the marker).
        let (_a, _s, st) = fixture();
        let app = router(st);
        let put = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, OTHER_DIGEST)
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(put).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "no marker ⇒ pin ignored"
        );

        // Delete with a write scope and no marker → 204 (unchanged).
        let (_a2, _s2, st2) = fixture();
        let app2 = router(st2);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::empty())
            .expect("request");
        let resp2 = app2.oneshot(del).await.expect("oneshot");
        assert_eq!(
            resp2.status(),
            StatusCode::NO_CONTENT,
            "normal delete unchanged"
        );
    }

    /// A present-but-non-`"1"` marker is NOT a runner-job: delete behaves as a
    /// normal write (204), proving the exact-`"1"` rule at the route.
    #[tokio::test]
    async fn runner_job_marker_non_one_is_not_narrowed_at_route() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let del = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "0")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(del).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "marker \"0\" ⇒ not narrowed"
        );
    }

    // ── AC create-only (deny-overwrite / anti AC-squat) ──────────────────────

    /// Seed an AC entry at `VALID_DIGEST` (tenant `TEST_TENANT`) via the store's
    /// update trait directly (bypasses the route gates) with the given bytes.
    fn seed_ac_entry(st: &AcRouteState, bytes: &[u8]) {
        st.update
            .update(AcUpdateRequest::new(
                TEST_TENANT,
                VALID_DIGEST,
                bytes.to_vec(),
                "anon@t1",
                TEST_TENANT,
                1,
            ))
            .expect("seed");
    }

    /// Build a create-only runner-job AC PUT to `VALID_DIGEST` with the given body.
    fn create_only_put(body: &'static [u8]) -> Request<Body> {
        Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .header(crate::scope::RUNNER_JOB_HEADER, "1")
            .header(crate::scope::RUNNER_JOB_AC_CREATE_ONLY_HEADER, "1")
            .body(Body::from(body.to_vec()))
            .expect("request")
    }

    /// create-only cred + an ALREADY-EXISTING entry (byte-identical) → 409
    /// AC_CREATE_ONLY. This is the distinguishing behavior: even an idempotent
    /// re-write (which a normal cred returns 200 for) is refused for create-only.
    #[tokio::test]
    async fn create_only_overwrite_existing_returns_409() {
        let (_a, _s, st) = fixture();
        seed_ac_entry(&st, b"result"); // first writer establishes the entry
        let app = router(st);
        let resp = app
            .oneshot(create_only_put(b"result"))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(v["error"], "AC_CREATE_ONLY");
    }

    /// create-only cred + a DIVERGENT overwrite of an existing entry → 409 with
    /// the consistent AC_CREATE_ONLY body (not the generic "divergent body").
    #[tokio::test]
    async fn create_only_divergent_overwrite_returns_409_create_only() {
        let (_a, _s, st) = fixture();
        seed_ac_entry(&st, b"original");
        let app = router(st);
        let resp = app
            .oneshot(create_only_put(b"poisoned"))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            v["error"], "AC_CREATE_ONLY",
            "a create-only cred's overwrite must be AC_CREATE_ONLY, not divergent-body"
        );
    }

    /// create-only cred + an ABSENT entry → the FIRST write proceeds normally
    /// (201 CREATED). Create-only denies OVERWRITE, never the initial CREATE.
    #[tokio::test]
    async fn create_only_fresh_entry_succeeds() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let resp = app
            .oneshot(create_only_put(b"result"))
            .await
            .expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "the first write must succeed"
        );
    }

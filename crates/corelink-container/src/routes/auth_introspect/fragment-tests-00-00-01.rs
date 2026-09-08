
    #[test]
    fn introspect_response_roundtrips_max_vcpu_h() {
        // Deserialize the full wire shape (the runners repo mirrors this) and
        // confirm max_vcpu_h round-trips as a top-level u32.
        let json = serde_json::json!({
            "valid": true,
            "tenant_id": "11111111-1111-4111-8111-111111111111",
            "plan": "pro",
            "max_concurrency": 40,
            "max_vcpu_h": 240
        });
        let parsed: IntrospectResponse = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.max_concurrency, Some(40));
        assert_eq!(parsed.max_vcpu_h, Some(240));
    }

    // ── Invalid PAT → {valid:false} ──────────────────────────────────────────

    #[tokio::test]
    async fn invalid_pat_returns_valid_false_no_tenant() {
        // Forged token (bad HMAC) → InvalidPat → 200 { valid:false }, no oracle.
        let verifier = Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::backend("unused")),
            test_key(),
        ));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!({ "token": "corelink_pat_not-a-real-token" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["valid"], serde_json::json!(false));
        let obj = v.as_object().unwrap();
        assert!(
            !obj.contains_key("tenant_id"),
            "no tenant_id on valid:false"
        );
        assert!(!obj.contains_key("plan"), "no plan on valid:false");
    }

    #[test]
    fn invalid_response_shape_serialises() {
        let v = serde_json::to_value(IntrospectResponse::invalid()).unwrap();
        assert_eq!(v, serde_json::json!({ "valid": false }));
    }

    // ── Backend error → 503 ──────────────────────────────────────────────────

    #[tokio::test]
    async fn verifier_backend_error_returns_503() {
        // A valid-HMAC token that reaches D1, where the lookup faults → Backend
        // → 503 (the fabric maps this to Err(Unreachable)).
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 7);
        let verifier = Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::backend("d1 unreachable")),
            key,
        ));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(Some(TEST_AUTH_KEY), serde_json::json!({ "token": pt }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ── Route NOT mounted when the dedicated secret is absent ────────────────

    #[test]
    fn build_state_returns_none_when_secret_absent() {
        // FABRIC_INTROSPECT_AUTH_KEY is not set in the test process → None.
        // (Guard against a polluted env so the assertion is meaningful.)
        if std::env::var("FABRIC_INTROSPECT_AUTH_KEY").is_err() {
            assert!(build_state_from_env().is_none());
        }
    }

    // ── tier_for_tenant: D1 fault surfaces as Err (→ route 503) ──────────────

    #[tokio::test]
    async fn tier_for_tenant_errors_on_d1_fault() {
        let d1 = unreachable_d1();
        let err = tier_for_tenant(&d1, "11111111-1111-1111-1111-111111111111").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn is_valid_tier_matches_canonical_set() {
        for t in [
            "free",
            "solo",
            "starter",
            "team",
            "pro",
            "org",
            "max",
            "enterprise",
        ] {
            assert!(is_valid_tier(t), "{t} should be valid");
        }
        assert!(!is_valid_tier("platinum"));
        assert!(!is_valid_tier(""));
    }

    // ── resolve-tenant: tenant-per-org primitive (migration 0083) ────────────

    /// Build a `POST /internal/v1/auth/resolve-tenant` request, with an optional
    /// `X-Corelink-Internal-Auth` header.
    fn resolve_request(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/internal/v1/auth/resolve-tenant")
            .header("content-type", "application/json");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    /// A `verifier` that is never reached by the resolve route (it has its own
    /// D1-only path); the resolve tests only exercise the auth gate + D1 read.
    fn unused_verifier() -> Arc<PatVerifier> {
        Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::backend("unused")),
            test_key(),
        ))
    }

    fn resolve_app() -> Router {
        router(state_with(unused_verifier(), unreachable_d1()))
    }

    #[test]
    fn decode_resolved_tenant_seeded_org_yields_some() {
        // A `tenant_org_map` row carrying the org's tenant → Some(tenant_id):
        // the resolution that backs githugr's per-org token exchange.
        let mut row = serde_json::Map::new();
        row.insert(
            "tenant_id".to_owned(),
            serde_json::json!("11111111-1111-4111-8111-111111111111"),
        );
        assert_eq!(
            decode_resolved_tenant(&[row]),
            Some("11111111-1111-4111-8111-111111111111".to_owned()),
            "a seeded org row must resolve to its mapped tenant"
        );
    }

    #[test]
    fn decode_resolved_tenant_unmapped_org_yields_none() {
        // Empty result set (GATED-INERT: no rows until provisioning) → None →
        // the handler returns 404 org_not_mapped (the showcase-tenant fallback
        // path is the caller's; this endpoint never auto-provisions).
        assert_eq!(
            decode_resolved_tenant(&[]),
            None,
            "an unmapped org must yield None (→ 404 org_not_mapped)"
        );
    }

    #[test]
    fn decode_resolved_tenant_malformed_row_yields_none() {
        // A row whose tenant_id is missing / non-string / empty must NOT resolve
        // to a wrong/blank tenant — it falls to None (clean 404), never a bad
        // isolation boundary.
        let mut missing = serde_json::Map::new();
        missing.insert("other".to_owned(), serde_json::json!("x"));
        assert_eq!(decode_resolved_tenant(&[missing]), None);

        let mut non_str = serde_json::Map::new();
        non_str.insert("tenant_id".to_owned(), serde_json::json!(42));
        assert_eq!(decode_resolved_tenant(&[non_str]), None);

        let mut empty = serde_json::Map::new();
        empty.insert("tenant_id".to_owned(), serde_json::json!(""));
        assert_eq!(decode_resolved_tenant(&[empty]), None);
    }

    #[tokio::test]
    async fn resolve_missing_service_secret_returns_401() {
        let resp = resolve_app()
            .oneshot(resolve_request(
                None,
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn resolve_wrong_service_secret_returns_401() {
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some("wrong-secret-which-is-also-32-chars!!"),
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn resolve_unknown_field_returns_400() {
        // deny_unknown_fields: an extra field is rejected on shape (after the
        // auth gate passes, before any D1 read).
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!({ "clerk_org_id": "org_123", "evil": true }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert_eq!(v["error"], serde_json::json!("invalid_body"));
    }

    #[tokio::test]
    async fn resolve_d1_fault_returns_503_failclosed() {
        // A well-formed, authenticated request whose D1 lookup faults must 503 —
        // never a guessed tenant (which would break tenant isolation).
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a D1 fault on resolve must fail CLOSED (503), never serve a guessed tenant"
        );
    }

    #[tokio::test]
    async fn resolve_tenant_for_org_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err (→ route 503), mirroring
        // tier_for_tenant's fail-CLOSED contract.
        let d1 = unreachable_d1();
        let err = resolve_tenant_for_org(&d1, "org_123").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn resolve_tenant_response_serialises() {
        let v = serde_json::to_value(ResolveTenantResponse {
            tenant_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "tenant_id": "11111111-1111-4111-8111-111111111111" })
        );
    }

    // ── cf-multitenant WP3: tenant-per-installation resolve + repo allowlist ──

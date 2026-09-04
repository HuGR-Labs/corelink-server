use super::*;

// ── Keys routes ───────────────────────────────────────────────────────────

#[tokio::test]
async fn keys_create_returns_201_with_token() {
    let (state, _) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "name": "ci-key",
        "scopes": ["cache:read", "cache:write"]
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "t4")
        .header("x-corelink-token-prefix", "clpat_t4")
        // Minting a write credential (`cache:write`) requires a write-capable
        // caller (cluster-A scope gate): a read-only principal cannot
        // self-escalate. A real write-scoped PAT caller carries this header.
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["pat"]["name"], "ci-key");
    assert!(v["token"].as_str().expect("token").starts_with("clpat_"));
}

#[tokio::test]
async fn pats_create_uses_customer_tenant_and_returns_shown_once_token() {
    let (state, _) = fixture();
    let app = router(state);
    let body = serde_json::to_string(&serde_json::json!({
        "label": "ci-key",
        "scopes": ["cache:read", "cache:write"]
    }))
    .unwrap();
    let req = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-write")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["pat"]["name"], "ci-key");
    assert!(v["token"]
        .as_str()
        .expect("shown-once token")
        .starts_with("clpat_"));
}

#[tokio::test]
async fn pats_create_without_trusted_tenant_is_unauthorized() {
    let (state, _) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"label":"ci-key"}"#))
        .unwrap();

    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_pat_issue_rate_limit_is_tenant_scoped_and_audited() {
    let (mut state, _) = fixture();
    let rate_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rate_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    state.pat_issue_rate_limiter = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rate_audit.clone(),
        rate_metrics,
        pat_issue_rate_limit_config(),
    ));
    let body = r#"{"label":"rate-test","scopes":["cache:read"]}"#;

    for _ in 0..10 {
        let request = Request::builder()
            .uri("/v1/pats")
            .method("POST")
            .header("x-corelink-tenant-id", "tenant-rate-a")
            .header("x-corelink-token-prefix", "clerk")
            .header(crate::scope::SCOPE_HEADER, "read-only")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let blocked = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", "tenant-rate-a")
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-only")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let blocked_response = router(state.clone()).oneshot(blocked).await.unwrap();
    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = blocked_response
        .headers()
        .get(axum::http::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap();
    assert_eq!(retry_after, 360, "retry-after must name the next token");

    // Tenant B has a separate endpoint bucket and is not starved by A.
    let other = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", "tenant-rate-b")
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-only")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let other_response = router(state).oneshot(other).await.unwrap();
    assert_eq!(other_response.status(), StatusCode::CREATED);

    let records = rate_audit.snapshot();
    assert!(records.len() >= 12);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == corelink_ratelimit::RateLimitEventType::Allowed)
            .count(),
        11
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == corelink_ratelimit::RateLimitEventType::Denied429)
            .count(),
        1
    );
    assert!(records.iter().any(|record| {
        record.tenant_id == uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"tenant-rate-a")
            && record.event_type == corelink_ratelimit::RateLimitEventType::Denied429
    }));
}

#[tokio::test]
async fn public_pat_issue_rate_limit_fails_closed_on_pipeline_error() {
    let (mut state, _) = fixture();
    state.pat_issue_rate_limiter = Arc::new(InMemoryTokenBucketRateLimiter::new(
        Arc::new(FailingRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        pat_issue_rate_limit_config(),
    ));
    let request = Request::builder()
        .uri("/v1/pats")
        .method("POST")
        .header("x-corelink-tenant-id", "tenant-rate-failure")
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-only")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"label":"rate-test","scopes":["cache:read"]}"#,
        ))
        .unwrap();

    let response = router(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// Both public aliases must spend from the same `pat-issue` class whether
/// the Worker authenticated the caller as a Clerk session or a native PAT.
#[tokio::test]
async fn pat_issue_limit_covers_every_public_alias_and_auth_mode() {
    let arms = [
        (
            "/v1/pats",
            r#"{"label":"rate-test","scopes":["cache:read"]}"#,
        ),
        (
            "/v1/customer/keys",
            r#"{"name":"rate-test","scopes":["cache:read"]}"#,
        ),
    ];
    for (path, body) in arms {
        for token_prefix in ["clerk", "clpat_native"] {
            let (mut state, _) = fixture();
            state.pat_issue_rate_limiter = Arc::new(InMemoryTokenBucketRateLimiter::new(
                Arc::new(InMemoryRateLimitAuditSink::new()),
                Arc::new(InMemoryRateLimitMetrics::new()),
                pat_issue_rate_limit_config(),
            ));
            for _ in 0..10 {
                let request = Request::builder()
                    .uri(path)
                    .method("POST")
                    .header("x-corelink-tenant-id", "tenant-all-aliases")
                    .header("x-corelink-token-prefix", token_prefix)
                    .header(crate::scope::SCOPE_HEADER, "read-only")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap();
                assert_eq!(
                    router(state.clone())
                        .oneshot(request)
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::CREATED
                );
            }
            let request = Request::builder()
                .uri(path)
                .method("POST")
                .header("x-corelink-tenant-id", "tenant-all-aliases")
                .header("x-corelink-token-prefix", token_prefix)
                .header(crate::scope::SCOPE_HEADER, "read-only")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            let response = router(state).oneshot(request).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "{path} / {token_prefix}"
            );
            assert_eq!(
                response
                    .headers()
                    .get(axum::http::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
                Some("360"),
                "{path} / {token_prefix} must expose the next-token wait"
            );
        }
    }
}

/// The aliases are not independent budgets. Interleave Clerk and native
/// PAT calls deliberately: partitioning the limiter by route spelling or
/// authentication mode would make this mutant admit the eleventh mint.
#[tokio::test]
async fn pat_issue_aliases_and_auth_modes_share_one_tenant_bucket() {
    let (mut state, _) = fixture();
    state.pat_issue_rate_limiter = Arc::new(InMemoryTokenBucketRateLimiter::new(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        pat_issue_rate_limit_config(),
    ));
    let mixed_requests = [
        (
            "/v1/pats",
            r#"{"label":"mixed","scopes":["cache:read"]}"#,
            "clerk",
        ),
        (
            "/v1/customer/keys",
            r#"{"name":"mixed","scopes":["cache:read"]}"#,
            "clpat_native",
        ),
    ];

    for n in 0..10 {
        let (path, body, token_prefix) = mixed_requests[n % mixed_requests.len()];
        let request = Request::builder()
            .uri(path)
            .method("POST")
            .header("x-corelink-tenant-id", "tenant-shared-mixed-bucket")
            .header("x-corelink-token-prefix", token_prefix)
            .header(crate::scope::SCOPE_HEADER, "read-only")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(
            router(state.clone())
                .oneshot(request)
                .await
                .unwrap()
                .status(),
            StatusCode::CREATED,
            "mixed request {n} ({path}, {token_prefix}) must consume the shared burst"
        );
    }

    let request = Request::builder()
        .uri("/v1/customer/keys")
        .method("POST")
        .header("x-corelink-tenant-id", "tenant-shared-mixed-bucket")
        .header("x-corelink-token-prefix", "clerk")
        .header(crate::scope::SCOPE_HEADER, "read-only")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"eleventh","scopes":["cache:read"]}"#))
        .unwrap();
    let response = router(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        Some("360"),
        "the shared bucket must name the next token, not a fake high wait"
    );
}

/// A limiter pipeline error happens before the handler audit or storage
/// mutation. This is deliberately checked for both wire aliases.
#[tokio::test]
async fn pat_issue_pipeline_failure_has_no_handler_audit_or_mint_side_effects() {
    for (path, body) in [
        (
            "/v1/pats",
            r#"{"label":"rate-test","scopes":["cache:read"]}"#,
        ),
        (
            "/v1/customer/keys",
            r#"{"name":"rate-test","scopes":["cache:read"]}"#,
        ),
    ] {
        let (mut state, shared, customer_audit) = fixture_with_observable_audit();
        state.pat_issue_rate_limiter = Arc::new(InMemoryTokenBucketRateLimiter::new(
            Arc::new(FailingRateLimitAuditSink::new()),
            Arc::new(InMemoryRateLimitMetrics::new()),
            pat_issue_rate_limit_config(),
        ));
        let request = Request::builder()
            .uri(path)
            .method("POST")
            .header("x-corelink-tenant-id", "tenant-rate-failure")
            .header("x-corelink-token-prefix", "clerk")
            .header(crate::scope::SCOPE_HEADER, "read-only")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(
            router(state).oneshot(request).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(
            CustomerKeysHandler::list(
                shared.as_ref(),
                KeysListRequest::new("tenant-rate-failure", "test", now_ms())
            )
            .unwrap()
            .pats
            .is_empty(),
            "{path} must not mint a PAT after a limiter failure"
        );
        assert!(
            customer_audit.snapshot().unwrap().is_empty(),
            "{path} must not emit a handler audit event"
        );
    }
}

#[test]
fn pat_issue_refill_is_exact_at_360_second_boundary() {
    let limiter = InMemoryTokenBucketRateLimiter::new(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        pat_issue_rate_limit_config(),
    );
    let tenant = uuid::Uuid::new_v4();
    let key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(tenant, PAT_ISSUE_ENDPOINT_ID);

    for _ in 0..10 {
        assert!(limiter
            .try_acquire(tenant, key.clone(), 1, 0)
            .unwrap()
            .decision
            .is_allow());
    }
    let denied = limiter.try_acquire(tenant, key.clone(), 1, 0).unwrap();
    assert!(denied.decision.is_deny());
    assert!(matches!(
        denied.decision,
        RateLimitDecision::Deny429 {
            retry_after_secs: 360,
            ..
        }
    ));
    assert!(limiter
        .try_acquire(tenant, key.clone(), 1, 359_999)
        .unwrap()
        .decision
        .is_deny());
    assert!(limiter
        .try_acquire(tenant, key.clone(), 1, 360_000)
        .unwrap()
        .decision
        .is_allow());

    // Once drained again, exactly ten tokens arrive over the next hour.
    for n in 1..=10 {
        assert!(limiter
            .try_acquire(tenant, key.clone(), 1, 360_000 + n * 360_000)
            .unwrap()
            .decision
            .is_allow());
    }
}

#[tokio::test]
async fn keys_list_returns_seeded_pats() {
    let (state, shared) = fixture();
    let pat = PatRow::new(
        "pat_001",
        "my-key",
        vec!["cache:read".into()],
        "2026-05-01T00:00:00Z",
        None,
        None,
    );
    shared.seed_pat("t5", pat).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/keys")
        .method("GET")
        .header("x-corelink-tenant-id", "t5")
        // #336 (rt-nuclear #7): listing credentials requires a cache-write
        // (dashboard `read-write`) scope — a read-only PAT must not enumerate keys.
        .header("x-corelink-scope", "read-write")
        .header("x-corelink-role", "owner")
        .header("x-corelink-token-prefix", "clpat_t5")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["pats"].as_array().expect("pats").len(), 1);
    assert_eq!(v["pats"][0]["name"], "my-key");
}

#[tokio::test]
async fn keys_revoke_returns_revoked_pat() {
    let (state, shared) = fixture();
    let pat = PatRow::new(
        "pat_rev",
        "revoke-me",
        vec![],
        "2026-05-01T00:00:00Z",
        None,
        None,
    );
    shared.seed_pat("t6", pat).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/keys/pat_rev/revoke")
        .method("POST")
        .header("x-corelink-tenant-id", "t6")
        // #336 (rt-nuclear #7): revoking a credential requires a cache-write
        // (dashboard `read-write`) scope — a read-only PAT must not revoke keys.
        .header("x-corelink-scope", "read-write")
        .header("x-corelink-role", "owner")
        .header("x-corelink-token-prefix", "clpat_t6")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    // revoked_at is now set.
    assert!(v["pat"]["revoked_at"].is_string());
}

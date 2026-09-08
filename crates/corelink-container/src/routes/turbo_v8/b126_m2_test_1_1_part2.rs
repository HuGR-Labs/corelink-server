#[tokio::test]
async fn post_status_without_tenant_header_is_401() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/status")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_status_sentinel_tenant_is_401() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/status")
        .header("x-corelink-tenant-id", "_unknown")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── status endpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn post_status_returns_enabled() {
    let app = test_router();
    // F3: status now requires the authenticated-tenant header.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/status")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["status"], "enabled");
}

// ── hash-too-long guard ───────────────────────────────────────────────────

#[test]
fn hash_too_long_maps_to_400() {
    let resp = map_err(TurboBridgeError::HashTooLong { len: 129, max: 128 });
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── GET missing teamId → 400 ──────────────────────────────────────────────

#[tokio::test]
async fn get_missing_team_id_returns_400() {
    let app = test_router();
    // No teamId query param — axum Query extractor rejects this.
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/somehash")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── PUT missing teamId → 400 ──────────────────────────────────────────────

#[tokio::test]
async fn put_missing_team_id_returns_400() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/somehash")
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── teamId DoS / key-aliasing guard → 400 ─────────────────────────────────

#[test]
fn team_id_invalid_maps_to_400() {
    let resp = map_err(TurboBridgeError::TeamIdInvalid {
        len: 0,
        max: 256,
        reason: "team_id must not be empty",
    });
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_slash_in_team_id_returns_400() {
    let app = test_router();
    // `teamId=a/b` is `teamId=a%2Fb` here — a `/`-bearing value that would
    // escape the team sub-namespace key prefix. Rejected at the route guard.
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/somehash?teamId=a%2Fb")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_empty_team_id_returns_400() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/somehash?teamId=")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_overlong_team_id_returns_400() {
    let app = test_router();
    let long = "a".repeat(corelink_turbo_bridge::error::MAX_TEAM_ID_LEN + 1);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v8/artifacts/somehash?teamId={long}"))
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_slash_in_team_id_returns_400() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/somehash?teamId=a%2Fb")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── scope enforcement (x-corelink-scope) ─────────────────────────────────

#[tokio::test]
async fn put_with_read_only_scope_returns_403_insufficient_scope() {
    // A `cas:r` (read-only) token must NOT be able to PUT (write).
    let app = test_router();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h1?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), b"insufficient scope");
}

#[tokio::test]
async fn put_with_rw_scope_succeeds() {
    // `cas:rw` is the current prod scope — PUT must still succeed.
    let app = test_router();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h1?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_with_missing_scope_returns_403() {
    // Fail-CLOSED: no `x-corelink-scope` header ⇒ no cache read.
    let app = test_router();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/h1?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_valid_team_id_accepted() {
    // Sanity: a conventional slug-style teamId still round-trips 200.
    let app = test_router();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h1?teamId=team_AbC-123")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── F24: per-tenant PUT concurrency guard ─────────────────────────────────

#[test]
fn put_inflight_counter_is_shared_across_clones() {
    // `TurboRouteState::clone` shares the `Arc<Mutex<..>>` — so all
    // route handler invocations (axum clones state per request) see the
    // SAME counter. Verify the Arc is truly shared, not deep-copied.
    let state = build_handlers();
    let state2 = state.clone();
    {
        let mut g = state.put_inflight.lock().unwrap();
        g.insert(TEST_AUTH_TENANT.to_owned(), 3);
    }
    let g = state2.put_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        Some(&3),
        "cloned state must share the same inflight counter"
    );
}

#[tokio::test]
async fn put_at_limit_returns_429() {
    // Simulate reaching TURBO_PUT_CONCURRENCY_LIMIT by pre-seeding the
    // counter, then issue one more PUT — must get 429.
    let state = fixture();
    {
        let mut g = state.put_inflight.lock().unwrap();
        g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_PUT_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h_limit?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "at-limit PUT must return 429"
    );
}

#[tokio::test]
async fn put_below_limit_succeeds_and_decrements_counter() {
    // A PUT that succeeds must release its concurrency slot (counter goes
    // back to 0 after the request completes, not leaked).
    let state = fixture();
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h_decr?teamId=team_y")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(b"data".to_vec()))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    // After the handler returns, the guard must have decremented the counter.
    let g = state.put_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        None, // removed when count reaches 0
        "concurrency counter must be released after PUT completes"
    );
}

// ── H1: GET pre-buffer concurrency guard (read-path twin of PUT) ──────────

#[tokio::test]
async fn get_at_limit_returns_429() {
    // H1: mirror `put_at_limit_returns_429` for the read path. Simulate
    // reaching TURBO_GET_CONCURRENCY_LIMIT by pre-seeding the per-tenant GET
    // counter, then issue one more GET — the `GetConcurrencyGuard`
    // `FromRequestParts` extractor must reject it 429 BEFORE the handler
    // touches storage or buffers the (up to 100 MiB) artifact.
    let state = fixture();
    {
        let mut g = state.get_inflight.lock().unwrap();
        g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_GET_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/h_get_limit?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "at-limit GET must return 429 (pre-buffer per-tenant guard)"
    );
}

#[tokio::test]
async fn get_below_limit_succeeds_and_decrements_counter() {
    // A GET that completes must release its concurrency slot (counter goes
    // back to 0 / removed, not leaked). Mirrors the PUT decrement test.
    let state = fixture();
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/h_get_decr?teamId=team_y")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("request");
    // 404 (artifact absent) is fine — the slot must still be released after
    // the handler returns on EVERY path.
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let g = state.get_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        None,
        "GET concurrency counter must be released after the request completes"
    );
}

// ── rt34 #3/#4/#5/#6: storage byte-delta accounting (NOT all-or-nothing) ──

/// Region the test accountant is keyed on. Must match the value we read the
/// `InMemoryByteStore` back with.
const TEST_BYTES_REGION: &str = "iad";

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();

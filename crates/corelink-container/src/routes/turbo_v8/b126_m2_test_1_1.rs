use super::*;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt; // for `.oneshot()`

/// Fixed authenticated tenant injected via `x-corelink-tenant-id` so the
/// `AuthTenant` extractor (fail-CLOSED) admits the request. PUT/GET in a
/// round-trip MUST share this value or the GET reads a different tenant's
/// namespace and 404s.
const TEST_AUTH_TENANT: &str = "11111111-1111-1111-1111-111111111111";

/// Read+write cache scope, mirroring every current prod PAT. Requests
/// that exercise GET/PUT must carry this in `x-corelink-scope` or the
/// fail-CLOSED scope gate rejects them 403.
const TEST_SCOPE_RW: &str = "cas:rw";

/// Build a test fixture using `InMemoryKvStore` + `InMemoryTurboAuditSink`.
fn fixture() -> TurboRouteState {
    build_handlers()
}

/// Build the router from the fixture state.
fn test_router() -> Router {
    router(fixture())
}

// ── Route constant sanity ─────────────────────────────────────────────────

#[test]
fn route_constants_use_brace_syntax_not_colon() {
    // Regression net for DEBT-029 (post axum-0.8) — matchit 0.8 treats the
    // legacy `:name` form as a literal path segment; `{name}` is the capture.
    assert!(
        !TURBO_GET_ROUTE.contains(':'),
        "TURBO_GET_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
    );
    assert!(TURBO_GET_ROUTE.contains("{hash}"));
    assert!(TURBO_PUT_ROUTE.contains("{hash}"));
    assert_eq!(TURBO_EVENTS_ROUTE, "/v8/artifacts/events");
    assert_eq!(TURBO_STATUS_ROUTE, "/v8/artifacts/status");
}

// ── build_handlers smoke test ─────────────────────────────────────────────

#[test]
fn build_handlers_returns_usable_state() {
    let state = build_handlers();
    // Status should always succeed.
    let resp = state.handler.status().expect("status");
    assert_eq!(resp.status, "enabled");
}

#[test]
fn unavailable_turbo_handler_503s_loud_not_silent_inmemory() {
    // CAA-360 #7: when R2KvStore fails to build with creds present, the route
    // must 503 LOUDLY on every verb — never silently degrade to a non-durable
    // InMemory store. All verbs share one error path (Self::unavailable);
    // assert it carries the sentinel and that map_err turns it into 503.
    let err = UnavailableTurboHandler::unavailable();
    assert!(
        matches!(&err, TurboBridgeError::Internal(m) if m.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL)),
        "unavailable error must carry the storage-unavailable sentinel"
    );
    assert_eq!(map_err(err).status(), StatusCode::SERVICE_UNAVAILABLE);
    // status() routes through the same error → 503 (not 500, not a fake OK).
    let h = UnavailableTurboHandler;
    assert_eq!(
        map_err(h.status().expect_err("must be unavailable")).status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

// ── GET happy path ────────────────────────────────────────────────────────

#[tokio::test]
async fn get_returns_404_when_artifact_absent() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/deadbeef?teamId=team_a")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_then_get_round_trip() {
    let state = fixture();
    let app = router(state);

    // PUT
    let body_bytes = b"webpack-output-chunk".to_vec();
    let put_req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/abc123?teamId=team_x")
        .header("content-type", "application/octet-stream")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(body_bytes.clone()))
        .expect("put request");
    let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
    assert_eq!(put_resp.status(), StatusCode::OK);

    // GET — same hash + teamId should return the stored bytes.
    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/abc123?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get_req).await.expect("get oneshot");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let response_body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(response_body.as_ref(), body_bytes.as_slice());
}

// ── x-artifact-tag (Turborepo signature) ──────────────────────────────────

/// F-009 / WP-9a — protocol conformance for the Turborepo artifact
/// signature. A client with `TURBO_REMOTE_CACHE_SIGNATURE_KEY` set sends
/// `x-artifact-tag` (an opaque base64 HMAC over the hash + body, computed
/// with a key the SERVER never holds) on PUT, and verifies the SAME value
/// echoed back on GET. Before this test CoreLink dropped the header
/// entirely on both verbs, so a customer who enabled signature
/// verification against CoreLink got SILENCE, not verification.
///
/// This is the DoD-1 test: a PUT carrying the tag stores it and the
/// matching GET returns it. It is written to compile against the
/// PRE-fix code (headers are plain strings), so it fails on the
/// ASSERTION — not on a compile error — if the feature is reverted.
#[tokio::test]
async fn artifact_tag_round_trips_put_to_get() {
    let state = fixture();
    let app = router(state);

    // A real turbo tag: base64 of an HMAC-SHA256 (44 chars).
    let tag = "dGhpcy1pcy1hLXR1cmJvLXNpZ25hdHVyZS10YWctdmFsdWU=";
    let artifact = b"signed-build-output".to_vec();

    let put_req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/signedhash01?teamId=team_sig")
        .header("content-type", "application/octet-stream")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .header(ARTIFACT_TAG_HEADER, tag)
        .body(Body::from(artifact.clone()))
        .expect("put request");
    let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
    assert_eq!(put_resp.status(), StatusCode::OK);

    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/signedhash01?teamId=team_sig")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get_req).await.expect("get oneshot");
    assert_eq!(get_resp.status(), StatusCode::OK);

    let echoed = get_resp
        .headers()
        .get(ARTIFACT_TAG_HEADER)
        .map(|v| v.to_str().unwrap_or("<non-ascii>").to_owned());
    assert_eq!(
        echoed.as_deref(),
        Some(tag),
        "a GET for an artifact PUT with x-artifact-tag MUST echo that exact tag; \
             dropping it silently disables the client's signature verification"
    );

    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        body.as_ref(),
        artifact.as_slice(),
        "the artifact bytes must be unchanged by tag handling"
    );
}

/// DoD-2 + DoD-3 — ABSENCE of a tag stays valid. Most clients never set
/// `TURBO_REMOTE_CACHE_SIGNATURE_KEY`, so an untagged PUT is the NORMAL
/// case: it must behave exactly as before, and the matching GET must
/// return the bytes with NO `x-artifact-tag` header and NO error.
/// Fail-closed applies to a tag that is present and malformed, never to
/// one that is absent.
#[tokio::test]
async fn absent_artifact_tag_is_valid_on_both_verbs() {
    let state = fixture();
    let app = router(state);

    let artifact = b"unsigned-build-output".to_vec();
    let put_req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/unsignedhash01?teamId=team_nosig")
        .header("content-type", "application/octet-stream")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(artifact.clone()))
        .expect("put request");
    let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
    assert_eq!(
        put_resp.status(),
        StatusCode::OK,
        "an untagged PUT must succeed exactly as before"
    );

    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/unsignedhash01?teamId=team_nosig")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get_req).await.expect("get oneshot");
    assert_eq!(
        get_resp.status(),
        StatusCode::OK,
        "a GET for an artifact stored WITHOUT a tag must not error"
    );
    assert!(
        get_resp.headers().get(ARTIFACT_TAG_HEADER).is_none(),
        "no tag was stored, so none may be invented on the way out"
    );
    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), artifact.as_slice());
}

/// A PRESENT but malformed tag fails CLOSED with 400 — it is never
/// silently dropped, which would hand the client a cache entry it
/// believes is signed. (Absence is still fine; see the test above.)
#[tokio::test]
async fn oversized_artifact_tag_is_rejected_400_not_dropped() {
    let state = fixture();
    let app = router(state);

    let too_long = "A".repeat(corelink_turbo_bridge::MAX_ARTIFACT_TAG_LEN + 1);
    let put_req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/oversizedtag01?teamId=team_sig")
        .header("content-type", "application/octet-stream")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .header(ARTIFACT_TAG_HEADER, &too_long)
        .body(Body::from(b"bytes".to_vec()))
        .expect("put request");
    let put_resp = app.clone().oneshot(put_req).await.expect("put oneshot");
    assert_eq!(
        put_resp.status(),
        StatusCode::BAD_REQUEST,
        "an oversized tag must be refused, never stored or silently dropped"
    );

    // Nothing was stored: the artifact is still absent.
    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/oversizedtag01?teamId=team_sig")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get_req).await.expect("get oneshot");
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

/// The tag sidecar must not weaken the B-024 create-only refusal: a
/// second PUT to the same key is still 409, and the FIRST tag survives
/// (the refused write never replaces the stored tag either).
#[tokio::test]
async fn create_only_409_still_holds_with_a_tag_and_first_tag_survives() {
    let state = fixture();
    let app = router(state);

    let first_tag = "Zmlyc3QtdGFn";
    let second_tag = "c2Vjb25kLXRhZw==";

    let mk_put = |tag: &str, body: &'static [u8]| {
        Request::builder()
            .method(Method::PUT)
            .uri("/v8/artifacts/dupetaghash?teamId=team_sig")
            .header("content-type", "application/octet-stream")
            .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
            .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
            .header(ARTIFACT_TAG_HEADER, tag)
            .body(Body::from(body))
            .expect("put request")
    };

    let first = app
        .clone()
        .oneshot(mk_put(first_tag, b"first"))
        .await
        .expect("first put");
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .clone()
        .oneshot(mk_put(second_tag, b"second"))
        .await
        .expect("second put");
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "B-024 create-only must survive the tag feature"
    );

    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/dupetaghash?teamId=team_sig")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get_req).await.expect("get oneshot");
    assert_eq!(get_resp.status(), StatusCode::OK);
    assert_eq!(
        get_resp
            .headers()
            .get(ARTIFACT_TAG_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(first_tag),
        "the refused second PUT must not replace the first tag"
    );
    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), b"first".as_slice());
}

// ── opaque hash round-trip ────────────────────────────────────────────────

#[tokio::test]
async fn opaque_hash_round_trip() {
    // Turbo may send any string as hash (xxhash, sha512-prefix, etc.).
    // CoreLink stores it verbatim without hash verification.
    let state = fixture();
    let app = router(state);

    let opaque_hash = "turboxxhash-5e7c3b2a1f";
    let artifact = b"turbo-build-artifact-data".to_vec();

    let put_req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(artifact.clone()))
        .expect("put");
    let put_resp = app.clone().oneshot(put_req).await.expect("put");
    assert_eq!(put_resp.status(), StatusCode::OK);

    let get_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v8/artifacts/{opaque_hash}?teamId=t1"))
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get");
    let get_resp = app.oneshot(get_req).await.expect("get");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), artifact.as_slice());
}

// ── cross-tenant guard ────────────────────────────────────────────────────

#[tokio::test]
async fn cross_tenant_isolated_via_caller_tenant_miss_not_denied() {
    // Repurposed from `put_cross_tenant_returns_403`. The isolation tenant
    // is now `AuthTenant` (auth.0), NOT the `teamId` query param, so a
    // `team_id == caller_tenant` denial is structurally impossible on this
    // path — at the HTTP layer a missing/invalid tenant is rejected 401 at
    // the extractor (fail-CLOSED) before the handler runs. We assert the
    // NEW invariant at the handler the route is wired to: an artifact
    // written under caller_tenant="tenantA" is unreachable from
    // caller_tenant="tenantB" with the SAME teamId+hash — a NotFound MISS,
    // not a denial.
    let state = fixture();
    state
        .handler
        .put(corelink_turbo_bridge::TurboPutRequest::new(
            "h1",
            "shared_team", // teamId — sub-namespace, NOT the tenant
            "s",
            b"tenantA bytes".to_vec(),
            None,
            "userA",
            "tenantA", // caller_tenant — the isolation dimension
            1,
        ))
        .expect("put under tenantA");
    let err = state
        .handler
        .get(corelink_turbo_bridge::TurboGetRequest::new(
            "h1",
            "shared_team",
            "s",
            "userB",
            "tenantB", // different caller_tenant ⇒ isolated
            2,
        ))
        .expect_err("tenantB must not reach tenantA's artifact");
    assert!(matches!(
        err,
        corelink_turbo_bridge::TurboBridgeError::NotFound { .. }
    ));
    // Same-tenant round-trip still serves the bytes.
    let ok = state
        .handler
        .get(corelink_turbo_bridge::TurboGetRequest::new(
            "h1",
            "shared_team",
            "s",
            "userA",
            "tenantA",
            3,
        ))
        .expect("tenantA round-trip");
    assert_eq!(ok.bytes, b"tenantA bytes".to_vec());
}

// ── events endpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn post_events_always_200() {
    let app = test_router();
    // Turbo sends arbitrary JSON telemetry; we accept and drop.
    // F3: events now requires the authenticated-tenant header.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("content-type", "application/json")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn post_events_empty_body_200() {
    let app = test_router();
    // F3: events now requires the authenticated-tenant header.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── F3: events/status are fail-CLOSED without an authenticated tenant ──────

#[tokio::test]
async fn post_events_without_tenant_header_is_401() {
    // F3 (defense-in-depth): the container must not trust the Worker's
    // PAT gate. No `x-corelink-tenant-id` ⇒ `AuthTenant` rejects 401
    // BEFORE the accept-and-drop handler runs.
    let app = test_router();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_events_sentinel_tenant_is_401() {
    // A sentinel tenant value is not a real authenticated tenant.
    let app = test_router();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", "_unknown")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

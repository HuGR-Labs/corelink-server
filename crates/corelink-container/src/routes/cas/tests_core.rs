/// `map_err` must map an `Internal` error to 503 ONLY when it carries the
/// storage-unavailable sentinel; any other `Internal` is a generic 500.
/// Kills the cargo-mutants "replace match guard with true" mutant on the
/// `if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL)` guard (mirrors the
/// ac.rs test) — with the guard forced to `true`, the non-sentinel case
/// below would wrongly become 503.
#[test]
fn map_err_internal_is_503_only_for_storage_sentinel() {
    let storage = map_err(CasHandlerError::Internal(format!(
        "{STORAGE_UNAVAILABLE_SENTINEL}R2_TDK_HEX unset"
    )));
    assert_eq!(
        storage.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "sentinel-tagged Internal must be 503"
    );

    let generic = map_err(CasHandlerError::Internal(
        "lock poisoned: unrelated failure".to_string(),
    ));
    assert_eq!(
        generic.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "non-sentinel Internal must be 500, not 503 (guard must not be `true`)"
    );
}

#[tokio::test]
async fn pre_body_guards_reject_oversized_tenant_before_cloning() {
    let state = fixture();
    let request = axum::http::Request::builder()
        .header(
            "x-corelink-tenant-id",
            "t".repeat(crate::auth_tenant::MAX_TENANT_ID_BYTES + 1),
        )
        .body(())
        .expect("request");
    let (mut parts, _) = request.into_parts();
    let write = match CasPutGuard::from_request_parts(&mut parts, &state).await {
        Ok(_) => panic!("write guard must reject oversized tenant"),
        Err(rejection) => rejection,
    };
    assert_eq!(write.status(), StatusCode::UNAUTHORIZED);

    let request = axum::http::Request::builder()
        .header(
            "x-corelink-tenant-id",
            "t".repeat(crate::auth_tenant::MAX_TENANT_ID_BYTES + 1),
        )
        .body(())
        .expect("request");
    let (mut parts, _) = request.into_parts();
    let read = match CasReadConcurrencyGuard::from_request_parts(&mut parts, &state).await {
        Ok(_) => panic!("read guard must reject oversized tenant"),
        Err(rejection) => rejection,
    };
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);
}

fn fixture() -> CasRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: None,
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// Fixture with a tombstone store pre-seeded with `(tenant, hash)` so the
/// read gate returns 410 Gone (WP-B).
fn fixture_with_tombstone(tenant: &str, hash: &str) -> CasRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    let store = Arc::new(crate::routes::cas_erase::InMemoryTombstoneStore::new());
    store.seed(tenant, hash);
    let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> = store;
    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: Some(tombstones),
        quota: None,
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// Route state backed by the fail-CLOSED [`UnavailableCasHandler`] — the
/// shape `build_handlers` returns when storage creds are present but the R2
/// handler refuses to build (F1: `R2_TDK_HEX` unset/invalid).
fn fixture_unavailable() -> CasRouteState {
    let shared = Arc::new(UnavailableCasHandler);
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: None,
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

#[test]
fn route_constants_match_canonical_path() {
    assert_eq!(CAS_READ_ROUTE, "/v1/cas/{tenant}/{hash}");
    assert_eq!(CAS_WRITE_ROUTE, "/v1/cas/{tenant}/{hash}");
}

/// F1 (CAA-360) fail-CLOSED: the `UnavailableCasHandler`'s error maps to
/// HTTP 503 "storage unavailable" (NOT the generic 500) so a forgotten
/// `R2_TDK_HEX` is loud, not a silent non-durable cache.
#[test]
fn unavailable_handler_maps_to_503() {
    let err = UnavailableCasHandler
        .read(CasReadRequest::new("t1", "h", "anon@t1", "t1", 0))
        .expect_err("unavailable");
    assert!(matches!(err, CasHandlerError::Internal(_)));
    let resp = map_err(err);
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn route_constant_uses_matchit_0_8_brace_syntax_not_colon() {
    // DEBT-029-cas regression net (post axum-0.8): matchit 0.8 parses
    // `{name}` as the capture and treats the legacy `:name` form as
    // LITERAL path bytes. Any reintroduction of `:name` would silently
    // route every real request to a router-level 404. Pin both the
    // absence of `:` and the presence of `{tenant}` + `{hash}`.
    assert!(
        !CAS_READ_ROUTE.contains(':'),
        "CAS_READ_ROUTE must use matchit-0.8 `{{name}}` syntax, not `:name`"
    );
    assert!(CAS_READ_ROUTE.contains("{tenant}"));
    assert!(CAS_READ_ROUTE.contains("{hash}"));
}

#[test]
fn batch_parser_rejects_line_and_hash_clone_amplification() {
    let oversized_line = format!(
        r#"{{"hash":"{}","padding":"{}"}}"#,
        "a".repeat(64),
        "p".repeat(BATCH_MAX_LINE_BYTES)
    );
    assert_eq!(
        parse_ndjson_hashes(oversized_line.as_bytes())
            .unwrap_err()
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let oversized_hash = format!(r#"{{"hash":"{}"}}"#, "a".repeat(BATCH_MAX_HASH_BYTES + 1));
    assert_eq!(
        parse_ndjson_hashes(oversized_hash.as_bytes())
            .unwrap_err()
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[test]
fn batch_parser_rejects_more_than_the_frozen_object_cap_before_allocating_all_lines() {
    let line = serde_json::json!({"hash": "short"}).to_string();
    let body = std::iter::repeat_n(line, BATCH_MAX_OBJECTS + 1)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parse_ndjson_hashes(body.as_bytes()).unwrap_err().0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

/// F7 (2026-06-13 CAA-360 audit) — CAS data-residency invariant.
///
/// Every non-IAD regional prod env (`[env.prod-sam|lhr|nrt|syd]`) MUST set
/// `R2_CAS_REGION` in its `.vars` so the container keys that region's CAS
/// objects under its OWN region instead of silently defaulting to the US
/// `"iad"` (the Schrems-II / GDPR Art. 44 gap). This test fails the build if
/// any regional env omits the binding — the fail-CLOSED, fail-LOUD guard
/// that stops a future env-block edit from re-opening the residency hole.
///
/// We assert the binding *exists* per region (the F7 contract). The bucket
/// stays `corelink-cas-prod` until per-region CAS buckets are provisioned
/// (infra follow-up), so we do not assert the bucket here.
#[test]
fn residency_invariant_every_regional_env_sets_cas_region() {
    // wrangler.toml lives at the repo root; this crate is at
    // crates/corelink-container, so climb two parents.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let wrangler_path = std::path::Path::new(manifest_dir)
        .join("..")
        .join("..")
        .join("wrangler.toml");
    let toml = std::fs::read_to_string(&wrangler_path).unwrap_or_else(|e| {
        panic!("cannot read {}: {e}", wrangler_path.display());
    });

    // Each non-IAD regional env must declare its CAS region. The IAD env is
    // exempt: its `R2_CAS_REGION` default ("iad") is the residency-correct
    // value, so an absent binding there is not a cross-border leak.
    for region in ["sam", "lhr", "nrt", "syd"] {
        let header = format!("[env.prod-{region}.vars]");
        let start = toml.find(&header).unwrap_or_else(|| {
            panic!("wrangler.toml missing `{header}` env block");
        });
        // Bound the search to this env block: from its header to the next
        // top-level `[` section after the header line.
        let after_header = start + header.len();
        let block_end = toml[after_header..]
            .find("\n[")
            .map_or(toml.len(), |rel| after_header + rel);
        let block = &toml[start..block_end];
        // Every residency-bearing key space must be pinned to this region.
        // CAS was the original F7 invariant; AC objects (which embed output
        // digests + command metadata) and chunk objects are equally
        // residency-bearing, and key under R2_AC_REGION / R2_CHUNK_REGION
        // (both default to "iad" in cas.rs/ac.rs `env_or`). A regional env
        // that drops ANY of the three silently keys that space under the US
        // default — the same cross-border leak the CAS check guards. Assert
        // all three so the build-time guard covers the full key surface.
        for var in ["R2_CAS_REGION", "R2_AC_REGION", "R2_CHUNK_REGION"] {
            let expected = format!("{var} = \"{region}\"");
            assert!(
                block.contains(&expected),
                "[env.prod-{region}] must set `{expected}` (residency \
                     invariant): a regional env without {var} keys that object \
                     space under the US default \"iad\" — a cross-border leak. \
                     Add the binding to wrangler.toml."
            );
        }
    }
}

#[test]
fn build_handlers_returns_usable_pair() {
    // Smoke test the build_handlers shape on the native target.
    let (read, _write, _delete, _list) = build_handlers();
    let bytes = b"hello".to_vec();
    let hash = fake_hash(&bytes);
    // We don't have a seed entry point on the trait alone, so
    // we drive the read against a known-empty handler and assert
    // it returns NotFound. The full wire-up is exercised via
    // the handler-crate's own unit tests.
    let res = read.read(CasReadRequest::new("t1", hash, "anon", "t1", 0));
    assert!(matches!(res, Err(CasHandlerError::NotFound { .. })));
    // Build state to satisfy the router constructor.
    let _router = router(fixture());
}

/// PUT then GET round-trip via the route state drives both trait
/// objects against the shared InMemory backing store — pins that
/// the new write trait object writes to the SAME backing store the
/// read trait object reads from (regression net for a future
/// refactor that accidentally splits the backing store).
#[test]
fn put_then_get_round_trip_through_route_state() {
    let st = fixture();
    let bytes = b"corelink-cas-put-then-get".to_vec();
    let hash = fake_hash(&bytes);

    // PUT
    let upd = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
    let upd_resp = st.write.write(upd).expect("write");
    assert!(upd_resp.durable, "fresh insert must be durable=true");

    // GET — must return the same bytes
    let rd = CasReadRequest::new("t1", hash.clone(), "anon@t1", "t1", 2);
    let rd_resp = st.read.read(rd).expect("read hit");
    assert_eq!(rd_resp.bytes, bytes);
    assert_eq!(rd_resp.content_hash, hash);
}

/// Idempotent retry on the write path: a second PUT with the same
/// bytes returns durable=false (mirrors AC + handler-crate
/// semantics).
#[test]
fn idempotent_write_retry_returns_durable_false() {
    let st = fixture();
    let bytes = b"r".to_vec();
    let hash = fake_hash(&bytes);
    let req1 = CasWriteRequest::new("t1", hash.clone(), bytes.clone(), "anon@t1", "t1", 1);
    assert!(st.write.write(req1).expect("first").durable);
    let req2 = CasWriteRequest::new("t1", hash, bytes, "anon@t1", "t1", 2);
    assert!(!st.write.write(req2).expect("retry").durable);
}

// ── scope enforcement (x-corelink-scope) ─────────────────────────────────

use axum::{
    body::Body,
    http::{Method, Request},
};
use tower::ServiceExt; // for `.oneshot()`

/// Authenticated tenant injected via `x-corelink-tenant-id` so the
/// `AuthTenant` extractor admits the request; the scope header then
/// gates read vs write.
const TEST_TENANT: &str = "00000000-0000-0000-0000-000000000001";

/// A `cas:rw` PUT writes successfully (current prod scope — happy path).
#[tokio::test]
async fn put_with_rw_scope_succeeds() {
    let app = router(fixture());
    let bytes = b"cas-scope-rw".to_vec();
    let hash = fake_hash(&bytes);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::from(bytes))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// B-093: the native CAS PUT override is 64 MiB even though the composed
/// container router has a 10 MiB outer default. A body just over 10 MiB must
/// reach the handler (and fail content verification with 422); if the
/// per-route override is removed or narrowed, the outer layer returns 413.
#[tokio::test]
async fn native_cas_put_uses_64_mib_route_limit() {
    let app = router(fixture()).layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024));
    let body = vec![0x5au8; 10 * 1024 * 1024 + 1];
    let hash = "0".repeat(64);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "a body above the outer 10 MiB default must reach native CAS PUT via its 64 MiB override"
    );
}

// ── cf-multitenant WP5b: runner-job PAT deny-DELETE ──────────────────────

/// A canonical 64-lowercase-hex CAS hash for the runner-job DELETE tests.
const WP5B_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// runner-job marker present ⇒ CAS DELETE is denied 403 even with a
/// write-capable scope (a per-job credential must not evict the cache).
#[tokio::test]
async fn runner_job_cas_delete_returns_403() {
    let app = router(fixture());
    let req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
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

/// runner-job + wildcard key (`*`) ⇒ CAS DELETE still denied (deny-DELETE is
/// unconditional for a runner-job; the key pin only governs AC writes).
#[tokio::test]
async fn runner_job_cas_delete_wildcard_still_403() {
    let app = router(fixture());
    let req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(crate::scope::RUNNER_JOB_HEADER, "1")
        .header(crate::scope::RUNNER_JOB_AC_KEY_ALLOW_HEADER, "*")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// NO runner-job marker (normal PAT): CAS DELETE with a write scope behaves
/// EXACTLY as before — 204 (idempotent), proving the WP5b check is a no-op.
#[tokio::test]
async fn no_runner_job_marker_cas_delete_is_204() {
    let app = router(fixture());
    let req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "normal delete unchanged"
    );
}

/// A present-but-non-`"1"` marker is NOT a runner-job: CAS DELETE behaves as
/// a normal write (204), proving the exact-`"1"` rule at the route.
#[tokio::test]
async fn runner_job_cas_marker_non_one_is_not_narrowed() {
    let app = router(fixture());
    let req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cas/{TEST_TENANT}/{WP5B_HASH}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(crate::scope::RUNNER_JOB_HEADER, "true")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "marker \"true\" ⇒ not narrowed"
    );
}

/// cluster F: with the tenant AT `CAS_WRITE_CONCURRENCY_LIMIT` in-flight
/// writes, the next CAS write is rejected 429 BEFORE its body is buffered —
/// the `CasPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
/// Proven deterministically by sending a body LARGER than the 10 MiB global
/// limit: if the body were buffered first, the body-limit layer would reject
/// it; because the concurrency guard runs first, we get 429 and the oversized
/// body is never read.
#[tokio::test]
async fn cas_write_at_concurrency_limit_returns_429_before_body() {
    let state = fixture();
    {
        let mut g = state.put_inflight.lock().expect("lock");
        g.insert(TEST_TENANT.to_owned(), CAS_WRITE_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]); // > 10 MiB
    let hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(oversized)
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an at-limit native CAS write must be rejected 429 by the pre-body concurrency guard"
    );
}

/// A write BELOW the limit succeeds and releases its slot (counter back to 0).
#[tokio::test]
async fn cas_write_below_limit_releases_slot() {
    let state = fixture();
    let app = router(state.clone());
    let bytes = b"cas-slot-release".to_vec();
    let hash = fake_hash(&bytes);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::from(bytes))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let g = state.put_inflight.lock().expect("lock");
    assert_eq!(
        g.get(TEST_TENANT),
        None,
        "the concurrency slot must be released after the write completes"
    );
}

/// A `cas:r` (read-only) PUT is rejected 403 "insufficient scope"
/// BEFORE storage.
#[tokio::test]
async fn put_with_read_only_scope_returns_403_insufficient_scope() {
    let app = router(fixture());
    let bytes = b"cas-scope-ro".to_vec();
    let hash = fake_hash(&bytes);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::from(bytes))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), b"insufficient scope");
}

/// F1 (CAA-360) end-to-end: a route mounted on the fail-CLOSED
/// `UnavailableCasHandler` (creds present, `R2_TDK_HEX` missing) returns
/// HTTP 503 for an in-scope GET — the cache refuses to serve, not a silent
/// 200/404 from a non-durable InMemory fallback.
#[tokio::test]
async fn get_on_unavailable_handler_returns_503() {
    let app = router(fixture_unavailable());
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// F1 (CAA-360) end-to-end: an in-scope PUT against the fail-CLOSED
/// `UnavailableCasHandler` returns 503 — writes are refused (never a silent
/// non-durable commit) until `R2_TDK_HEX` is set.
#[tokio::test]
async fn put_on_unavailable_handler_returns_503() {
    let app = router(fixture_unavailable());
    let bytes = b"cas-unavailable".to_vec();
    let hash = fake_hash(&bytes);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::from(bytes))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// A `cas:r` GET is allowed (read-only token reads). The artifact is
/// absent so the route returns 404 — proving the scope gate PASSED
/// (a denied scope would 403 before storage).
#[tokio::test]
async fn get_with_read_only_scope_passes_gate_then_404() {
    let app = router(fixture());
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Fail-CLOSED: a GET with NO scope header is rejected 403.
#[tokio::test]
async fn get_with_missing_scope_returns_403() {
    let app = router(fixture());
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_malformed_hash_returns_400() {
    // CAA-360 #9: a non-canonical :hash is rejected with 400 BEFORE storage,
    // even WITH valid auth + scope (the digest gate runs before the scope
    // gate). Covers the gate against a mutant that disables/inverts it.
    let app = router(fixture());
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/cas/{TEST_TENANT}/not-a-canonical-hash"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── 410-Gone tombstone read gate (hugit-P2 seam B, WP-B) ─────────────────

/// A GET for an ERASED `(tenant, hash)` returns HTTP 410 Gone — NOT 404,
/// NOT 200. The tombstone gate runs after the scope gate and before the R2
/// read.
#[tokio::test]
async fn get_erased_hash_returns_410_gone() {
    const ERASED: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/cas/{TEST_TENANT}/{ERASED}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::GONE);
}

/// With a tombstone store wired, a NON-erased hash still 404s (the gate is
/// per-hash, not a blanket block).
#[tokio::test]
async fn get_non_erased_hash_with_store_still_404() {
    let app = router(fixture_with_tombstone(TEST_TENANT, "some-other-hash"));
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Tombstone store whose `is_tombstoned` always errors — simulates a
/// transient D1 transport fault on the read gate.
#[derive(Debug, Default)]
struct ErroringTombstoneStore;
#[async_trait::async_trait]
impl crate::routes::cas_erase::TombstoneStore for ErroringTombstoneStore {
    async fn is_tombstoned(&self, _tenant: &str, _digest: &str) -> Result<bool, String> {
        Err("d1 transport fault".to_owned())
    }
    async fn upsert(
        &self,
        _tenant: &str,
        _digest: &str,
        _reason: &str,
        _erased_at_ms: i64,
    ) -> Result<bool, String> {
        Ok(false)
    }
    async fn list_tenant_tombstones(&self, _tenant: &str) -> Result<Vec<String>, String> {
        Err("d1 transport fault".to_owned())
    }
}

/// Route state whose tombstone gate always errors (D1 fault).
fn fixture_erroring_tombstone() -> CasRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    let tombstones: Arc<dyn crate::routes::cas_erase::TombstoneStore> =
        Arc::new(ErroringTombstoneStore);
    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: Some(tombstones),
        quota: None,
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// PEN-2 / REV-S1 regression: when the tombstone (GDPR-erasure) gate lookup
/// ERRORS (transient D1 fault), the read fails CLOSED with 503 — it must
/// NEVER fall through to the R2 read and risk resurrecting an erased blob.
#[tokio::test]
async fn get_with_tombstone_gate_error_fails_closed_503() {
    const HASH: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    let app = router(fixture_erroring_tombstone());
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/cas/{TEST_TENANT}/{HASH}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ── per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) ─────────

/// Fixture whose state carries a `QuotaGate` backed by an in-memory quota
/// store seeded so the NEXT billable op trips the ceiling. The flat per-op
/// cost is `1` micro-USD; the seeded tenant is already AT its budget, so any
/// charge projects over the cap.
fn fixture_over_ceiling(tenant: &str) -> CasRouteState {
    use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
    use crate::wall_clock::InMemoryFakeWallClock;

    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;

    let store = InMemoryQuotaStore::new();
    store.seed(
        tenant,
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 5_000_000, // already at the $5 cap
            cycle_anchor_ms: 1_700_000_000_000,
        },
    );
    let store: Arc<dyn QuotaStore> = Arc::new(store);
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
    let guard = Arc::new(QuotaGuard::new(store, clock));
    let gate = crate::routes::QuotaGate::new_for_test(guard, 1);

    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: Some(gate),
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

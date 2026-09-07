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

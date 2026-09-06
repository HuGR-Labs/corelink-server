/// Once accrued spend has reached the monthly ceiling, a billable CAS read
/// is rejected with HTTP 402 Payment Required — BEFORE storage. This is the
/// load-bearing wiring assertion: the previously-dead `QuotaGuard` is now
/// mounted and trips on a real HTTP request.
#[tokio::test]
async fn billable_request_over_ceiling_returns_402() {
    let app = router(fixture_over_ceiling(TEST_TENANT));
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

/// Control: with room under the ceiling the same request passes the gate
/// and proceeds to storage (404 for the absent blob — NOT 402). Proves the
/// gate is not a blanket block.
#[tokio::test]
async fn billable_request_under_ceiling_proceeds() {
    use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaStore};
    use crate::wall_clock::InMemoryFakeWallClock;

    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    // Empty store ⇒ fresh tenant at the $5 tripwire with 0 accrued.
    let store: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
    let guard = Arc::new(QuotaGuard::new(store, clock));
    let st = CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: Some(crate::routes::QuotaGate::new_for_test(guard, 1_000)),
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    };
    let app = router(st);
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

// ── finding #1: storage byte accounting (cap enforcement) ────────────────

/// Fixture whose `write`/`delete` trait objects are wrapped in the
/// [`AccountingCasHandler`] decorator over an in-memory byte store seeded AT
/// a tiny cap, so the next write trips the cap (mirrors the production wiring
/// in `routes::build_with_factory`).
fn fixture_over_storage_cap(tenant: &str) -> CasRouteState {
    use crate::byte_accounting::{
        testing::InMemoryByteStore, testing::Row, AccountingCasHandler, ByteAccountant, ByteStore,
    };

    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let raw_write: Arc<dyn CasWriteHandler> = shared.clone();
    let raw_delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;

    let store = Arc::new(InMemoryByteStore::new());
    // Cap of 4 bytes, already at 4 ⇒ ANY further byte is over-cap.
    store.seed(tenant, "iad", Row { used: 4, quota: 4 });
    let store_dyn: Arc<dyn ByteStore> = store;
    let acc = Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned()));
    let acct = Arc::new(AccountingCasHandler::new(raw_write, raw_delete, acc));
    let write: Arc<dyn CasWriteHandler> = acct.clone();
    let delete: Arc<dyn CasDeleteHandler> = acct;

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

/// The KILLING finding-#1 test: with the storage counter AT the cap, a PUT
/// that would store new bytes is rejected 402 — the previously-inert storage
/// cap now trips on a real HTTP write.
// multi_thread: the `AccountingCasHandler` decorator bridges the async
// accountant to the sync write trait with `block_in_place`, which requires a
// multi-thread runtime (matches production `#[tokio::main]`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_over_storage_cap_returns_402() {
    let app = router(fixture_over_storage_cap(TEST_TENANT));
    let bytes = b"more-bytes-than-the-cap-allows".to_vec();
    let hash = fake_hash(&bytes);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .body(Body::from(bytes))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

/// Control: an uncapped tenant's PUT accrues its bytes and succeeds (201) —
/// proving the counter MOVES (it never did before finding #1) and the gate
/// is not a blanket block.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_under_storage_cap_accrues_and_succeeds() {
    use crate::byte_accounting::{
        testing::InMemoryByteStore, AccountingCasHandler, ByteAccountant, ByteStore,
    };

    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let raw_write: Arc<dyn CasWriteHandler> = shared.clone();
    let raw_delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    let store = Arc::new(InMemoryByteStore::new()); // empty ⇒ uncapped fresh row
    let store_dyn: Arc<dyn ByteStore> = store.clone();
    let acct = Arc::new(AccountingCasHandler::new(
        raw_write,
        raw_delete,
        Arc::new(ByteAccountant::new(store_dyn, "iad".to_owned())),
    ));
    let write: Arc<dyn CasWriteHandler> = acct.clone();
    let delete: Arc<dyn CasDeleteHandler> = acct;
    let st = CasRouteState {
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
    };
    let app = router(st);
    let body = b"hello-cas".to_vec();
    let body_len = body.len() as i64;
    let hash = fake_hash(&body);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        // The Worker injects the resolved per-tier storage cap; a fresh row
        // (empty store) is seeded from it. Without it the reservation fails
        // CLOSED (no cap ⇒ 503) — see byte_accounting::storage_quota_from_headers.
        .header(crate::byte_accounting::STORAGE_QUOTA_HEADER, "1000000")
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(
        store.used(TEST_TENANT, "iad"),
        body_len,
        "a durable write must accrue its bytes into the storage counter"
    );
}

// ── finding #4: native PAT possession gate ───────────────────────────────

/// A request with the PAT gate wired but NO bearer Authorization header is
/// rejected 401 — the native gate fails CLOSED on a missing token even when
/// the Worker-set tenant header is present (defense-in-depth).
#[tokio::test]
async fn pat_gate_missing_bearer_returns_401() {
    use crate::adapter_pat::PatRow;
    use crate::native_pat_gate::testing::verifier_with_row;
    use crate::native_pat_gate::NativePatGate;
    use corelink_pat::PatSigningKey;

    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let read: Arc<dyn CasReadHandler> = shared.clone();
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    let key = Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("key"));
    let verifier = verifier_with_row(
        "no-such-token".to_owned(),
        PatRow {
            tenant_id: TEST_TENANT.to_owned(),
            pat_hash: String::new(),
            scope: "cas:rw".to_owned(),
            find_only: false,
            runner_job: false,
        },
        key,
    );
    let st = CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: None,
        pat_gate: Some(Arc::new(NativePatGate::new_for_test(verifier))),
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    };
    let app = router(st);
    let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/cas/{TEST_TENANT}/0000000000000000000000000000000000000000000000000000000000000000"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            // No Authorization header — the gate must reject.
            .body(Body::empty())
            .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── bulk CAS batch endpoints (hugit-P2: git-ingest fast path) ────────────

/// Build a length-framed `/batch` upload body from `(hash, bytes)` pairs:
/// the NDJSON manifest, a blank-line terminator, then the concatenated raw
/// bytes — the FROZEN wire format.
fn build_batch_upload(objs: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, bytes) in objs {
        out.extend_from_slice(
            format!(
                "{}\n",
                serde_json::json!({"hash": hash, "len": bytes.len()})
            )
            .as_bytes(),
        );
    }
    out.push(b'\n'); // blank-line terminator
    for (_h, bytes) in objs {
        out.extend_from_slice(bytes);
    }
    out
}

/// (a) Happy path: a 3-object batch upload returns 200 with every object
/// `created`.
#[tokio::test]
async fn batch_upload_all_created() {
    let app = router(fixture());
    let objs: Vec<(String, Vec<u8>)> = (0u8..3)
        .map(|i| {
            let b = vec![i; (i as usize) + 1];
            (fake_hash(&b), b)
        })
        .collect();
    let body = build_batch_upload(&objs);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(arr.len(), 3);
    for entry in &arr {
        assert_eq!(entry["status"], "created");
        assert!(entry["error"].is_null());
    }
}

/// finding #2 (HIGH DoS) on the BULK path: with the tenant AT
/// `CAS_WRITE_CONCURRENCY_LIMIT` in-flight writes, the next `/batch` upload
/// is rejected 429 BEFORE its body is buffered — the `_concurrency:
/// CasPutGuard` `FromRequestParts` extractor on `handle_batch_write` runs
/// ahead of `body: Bytes` and SHARES the same per-tenant pool as the single
/// PUT (so single+batch uploads count together). Proven deterministically by
/// sending a body LARGER than the limit: if the guard were missing, the body
/// would be buffered and the request would 413 (batch byte-cap / body-limit)
/// or otherwise parse — anything but 429. So a 429 here is a witness that the
/// pre-body concurrency reservation ran first; DELETING the `CasPutGuard`
/// extractor from `handle_batch_write` flips this to 413 and FAILS the test.
#[tokio::test]
async fn batch_write_at_concurrency_limit_returns_429_before_body() {
    let state = fixture();
    {
        let mut g = state.put_inflight.lock().expect("lock");
        g.insert(TEST_TENANT.to_owned(), CAS_WRITE_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    // > 10 MiB: if the guard is absent the body-limit/byte-cap layer rejects
    // it (413), never 429. Body content is irrelevant — the guard runs in
    // `FromRequestParts`, before the body is read.
    let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(oversized)
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an at-limit /batch upload must be rejected 429 by the pre-body concurrency guard \
             (the CasPutGuard extractor on handle_batch_write)"
    );
}

/// Companion to the at-limit case: a `/batch` upload BELOW the limit succeeds
/// and RELEASES its shared concurrency slot (counter back to 0). Together with
/// the 429 test this pins that `handle_batch_write` both reserves AND releases
/// the same per-tenant pool — a leaked slot would wedge the tenant's writes.
#[tokio::test]
async fn batch_write_below_limit_releases_slot() {
    let state = fixture();
    let app = router(state.clone());
    let b = b"batch-slot-release".to_vec();
    let body = build_batch_upload(&[(fake_hash(&b), b)]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let g = state.put_inflight.lock().expect("lock");
    assert_eq!(
        g.get(TEST_TENANT),
        None,
        "the batch upload's concurrency slot must be released after it completes"
    );
}

/// brutal-fleet M2 (MED DoS) on the BULK-READ path: with the tenant AT
/// `CAS_READ_CONCURRENCY_LIMIT` in-flight bulk reads, the next `/batch-read`
/// is rejected 429 BEFORE its body is buffered — the `_read_concurrency:
/// CasReadConcurrencyGuard` `FromRequestParts` extractor on `handle_batch_read`
/// runs ahead of `body: Bytes`. Proven deterministically by sending a body
/// LARGER than the 10 MiB body limit: if the guard were missing, the body
/// would be buffered and the request would 413 (body-limit) — anything but
/// 429. So a 429 here is a witness that the pre-body read reservation ran
/// first; DELETING the extractor flips this to 413 and FAILS the test. The
/// reservation is on the SEPARATE `read_inflight` pool — pre-loading
/// `put_inflight` would NOT trip it (asserted implicitly: the fixture's write
/// pool stays empty).
#[tokio::test]
async fn batch_read_at_concurrency_limit_returns_429_before_body() {
    let state = fixture();
    {
        let mut g = state.read_inflight.lock().expect("lock");
        g.insert(TEST_TENANT.to_owned(), CAS_READ_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    // > 10 MiB: absent the guard the body-limit layer rejects it (413), never
    // 429. Body content is irrelevant — the guard runs in `FromRequestParts`.
    let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(oversized)
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an at-limit /batch-read must be rejected 429 by the pre-body read-concurrency guard \
             (the CasReadConcurrencyGuard extractor on handle_batch_read)"
    );
}

/// Same guard covers the sibling `/batch-exists` route (it shares the
/// `CasReadConcurrencyGuard` read pool): at the cap ⇒ 429 before the body.
#[tokio::test]
async fn batch_exists_at_concurrency_limit_returns_429_before_body() {
    let state = fixture();
    {
        let mut g = state.read_inflight.lock().expect("lock");
        g.insert(TEST_TENANT.to_owned(), CAS_READ_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(oversized)
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an at-limit /batch-exists must be rejected 429 by the pre-body read-concurrency guard"
    );
}

/// Companion: a `/batch-read` BELOW the limit succeeds and RELEASES its read
/// slot (read pool back to empty), pinning the RAII release on the read axis.
#[tokio::test]
async fn batch_read_below_limit_releases_slot() {
    let state = fixture();
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(format!(
            "{}\n",
            serde_json::json!({ "hash": fake_hash(b"absent-blob") })
        )))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    // The slot is deliberately still held while the response body is
    // outstanding; consuming the stream is the release boundary.
    {
        let g = state.read_inflight.lock().expect("lock");
        assert_eq!(g.get(TEST_TENANT), Some(&1));
    }
    let _ = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let g = state.read_inflight.lock().expect("lock");
    assert_eq!(
        g.get(TEST_TENANT),
        None,
        "the batch-read's read-concurrency slot must be released after its stream is consumed"
    );
}

/// B-077 HOLD: the single GET keeps its tenant slot until its response
/// stream is consumed, not merely until the handler returns.
#[tokio::test]
async fn single_read_keeps_slot_until_response_consumed() {
    let state = fixture();
    let bytes = b"stream-lifetime".to_vec();
    let hash = fake_hash(&bytes);
    state
        .write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            hash.clone(),
            bytes,
            format!("anon@{TEST_TENANT}"),
            TEST_TENANT,
            0,
        ))
        .expect("seed CAS object");
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    {
        let g = state.read_inflight.lock().expect("lock");
        assert_eq!(g.get(TEST_TENANT), Some(&1));
    }
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&body[..], b"stream-lifetime");
    let g = state.read_inflight.lock().expect("lock");
    assert_eq!(g.get(TEST_TENANT), None);
}

/// (b) Re-upload of already-present objects returns `exists` (idempotent).
#[tokio::test]
async fn batch_reupload_returns_exists() {
    let st = fixture();
    let b = b"reupload-me".to_vec();
    let objs = vec![(fake_hash(&b), b)];
    let body = build_batch_upload(&objs);
    // First upload.
    let req1 = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body.clone()))
        .expect("request");
    let r1 = router(st.clone()).oneshot(req1).await.expect("oneshot");
    assert_eq!(r1.status(), StatusCode::OK);
    // Second upload — same bytes ⇒ exists.
    let req2 = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let r2 = router(st).oneshot(req2).await.expect("oneshot");
    let bytes = axum::body::to_bytes(r2.into_body(), usize::MAX)
        .await
        .expect("body");
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(arr[0]["status"], "exists");
}

/// (c) One object with WRONG bytes (claimed hash != actual) is reported
/// `error`; the OTHER objects in the same batch still `created`.
#[tokio::test]
async fn batch_upload_one_bad_object_others_commit() {
    let app = router(fixture());
    let good = b"good-object".to_vec();
    let bad_bytes = b"actual-bytes".to_vec();
    // Claim a hash that does not match bad_bytes ⇒ HashMismatch in the
    // write handler. fake_hash of DIFFERENT bytes gives a wrong claim of the
    // correct len, so the framing still holds but the content-verify fails.
    let wrong_claim = fake_hash(b"different");
    let objs = vec![
        (fake_hash(&good), good.clone()),
        (wrong_claim, bad_bytes.clone()),
    ];
    // Hand-build so the manifest len matches the ACTUAL payload bytes (the
    // framing is correct; only the per-object content-hash is wrong).
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "{}\n",
            serde_json::json!({"hash": fake_hash(&good), "len": good.len()})
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "{}\n",
            serde_json::json!({"hash": fake_hash(b"different"), "len": bad_bytes.len()})
        )
        .as_bytes(),
    );
    body.push(b'\n');
    body.extend_from_slice(&good);
    body.extend_from_slice(&bad_bytes);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "one bad object must NOT 4xx the whole batch"
    );
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(arr[0]["status"], "created");
    assert_eq!(arr[1]["status"], "error");
    assert_eq!(arr[1]["error"], "content hash mismatch");
    let _ = objs; // keep the descriptive binding alive for readers
}

/// (d1) Over-cap on object COUNT (2001 objects) ⇒ 413 batch_too_large.
#[tokio::test]
async fn batch_upload_over_object_cap_returns_413() {
    let app = router(fixture());
    // 2001 zero-length objects: framing trivially holds (empty payload), so
    // the ONLY thing that can trip is the object-count cap.
    let mut manifest = String::new();
    let zero_hash = "0".repeat(64);
    for _ in 0..(BATCH_MAX_OBJECTS + 1) {
        manifest.push_str(&format!(
            "{}\n",
            serde_json::json!({"hash": zero_hash, "len": 0})
        ));
    }
    let mut body = manifest.into_bytes();
    body.push(b'\n'); // terminator; empty payload follows
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["error"], "batch_too_large");
    assert_eq!(v["limit_objects"], BATCH_MAX_OBJECTS);
    assert_eq!(v["limit_bytes"], BATCH_MAX_BYTES);
}

/// (d2) Over-cap on declared BYTES (> 8 MiB sum(len)) ⇒ 413. We assert on
/// the manifest's declared length so we never need to ship 8 MiB of body.
#[tokio::test]
async fn batch_upload_over_byte_cap_returns_413() {
    let app = router(fixture());
    // Two objects whose DECLARED lengths sum just over 8 MiB. The byte-cap
    // check runs on sum(len) BEFORE the payload-length framing check, so an
    // over-cap request is rejected without shipping the bytes.
    let zero_hash = "0".repeat(64);
    let half = (BATCH_MAX_BYTES / 2) as u64 + 1;
    let mut body = Vec::new();
    for _ in 0..2 {
        body.extend_from_slice(
            format!("{}\n", serde_json::json!({"hash": zero_hash, "len": half})).as_bytes(),
        );
    }
    body.push(b'\n');
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// B-077 HOLD: a malformed manifest hash is length-bounded before the
/// entries vector can retain attacker-sized strings.
#[tokio::test]
async fn batch_upload_oversized_hash_is_rejected_before_storage() {
    let app = router(fixture());
    let manifest = serde_json::json!({
        "hash": "a".repeat(BATCH_MAX_HASH_BYTES + 1),
        "len": 0,
    });
    let body = format!("{manifest}\n\n");
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

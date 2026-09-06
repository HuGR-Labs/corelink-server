/// (e) Wrong Content-Type ⇒ 415 BEFORE the body is parsed.
#[tokio::test]
async fn batch_upload_wrong_content_type_returns_415() {
    let app = router(fixture());
    let b = b"x".to_vec();
    let body = build_batch_upload(&[(fake_hash(&b), b)]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

/// (f) Framing mismatch: declared sum(len) != payload bytes ⇒ 400 for the
/// whole request.
#[tokio::test]
async fn batch_upload_framing_mismatch_returns_400() {
    let app = router(fixture());
    let bytes = b"five!".to_vec(); // 5 bytes
                                   // Declare len=4 (one short) ⇒ sum(len) != payload.len().
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "{}\n",
            serde_json::json!({"hash": fake_hash(&bytes), "len": 4})
        )
        .as_bytes(),
    );
    body.push(b'\n');
    body.extend_from_slice(&bytes);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// (i) Cross-tenant (path tenant != auth tenant) ⇒ 403 BEFORE any parse.
#[tokio::test]
async fn batch_upload_cross_tenant_returns_403() {
    let app = router(fixture());
    let b = b"x".to_vec();
    let body = build_batch_upload(&[(fake_hash(&b), b)]);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/cas/other-tenant/batch")
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A read-only (`cas:r`) batch upload is rejected 403 (write scope required).
#[tokio::test]
async fn batch_upload_read_only_scope_returns_403() {
    let app = router(fixture());
    let b = b"x".to_vec();
    let body = build_batch_upload(&[(fake_hash(&b), b)]);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// (g) batch-read round-trip: seed two blobs, request three hashes (two
/// present + one absent), assert the manifest statuses and that the
/// concatenated payload is exactly the present blobs' bytes in order.
#[tokio::test]
async fn batch_read_round_trip_with_absent() {
    let st = fixture();
    let a = b"first-blob".to_vec();
    let bb = b"second-blob-longer".to_vec();
    let ha = fake_hash(&a);
    let hb = fake_hash(&bb);
    // Seed via the write trait object (the shared backing store).
    st.write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            ha.clone(),
            a.clone(),
            "anon@t1",
            TEST_TENANT,
            1,
        ))
        .expect("seed a");
    st.write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            hb.clone(),
            bb.clone(),
            "anon@t1",
            TEST_TENANT,
            2,
        ))
        .expect("seed b");
    let missing = "0".repeat(64);
    let ndjson = format!(
        "{}\n{}\n{}\n",
        serde_json::json!({"hash": ha}),
        serde_json::json!({"hash": missing}),
        serde_json::json!({"hash": hb}),
    );
    let app = router(st);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(ndjson))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    // Split manifest / payload at the blank-line terminator.
    let (manifest, payload) = split_manifest(&bytes).expect("framed response");
    let manifest = std::str::from_utf8(manifest).expect("utf8");
    let lines: Vec<&str> = manifest.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3);
    let l0: serde_json::Value = serde_json::from_str(lines[0]).expect("l0");
    let l1: serde_json::Value = serde_json::from_str(lines[1]).expect("l1");
    let l2: serde_json::Value = serde_json::from_str(lines[2]).expect("l2");
    assert_eq!(l0["status"], "ok");
    assert_eq!(l1["status"], "absent");
    assert_eq!(l2["status"], "ok");
    // Payload = a || bb (present objects, in manifest order).
    let mut expected = a.clone();
    expected.extend_from_slice(&bb);
    assert_eq!(payload, expected.as_slice());
}

/// Parallel-fan-out invariant: a large mixed batch (present + absent,
/// interleaved) must come back in EXACT request order despite the reads now
/// running concurrently (BATCH_READ_FANOUT-way). Asserts, per hash: the
/// manifest line order == request order, the reported `len`s, the per-hash
/// status, and that the concatenated payload slices back to the right bytes
/// for each present object. multi_thread so the sync `read()`'s
/// `block_in_place` (in prod) is exercised on a real worker thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn batch_read_parallel_preserves_order_and_slicing() {
    let st = fixture();
    // 30 hashes: even index = present (seeded, unique bytes), odd = absent.
    // Present bytes deliberately vary in length so a mis-ordered slice would
    // corrupt the reassembly and fail the per-hash byte assertion.
    let n = 30usize;
    let mut req_hashes: Vec<String> = Vec::with_capacity(n);
    let mut expected_present: Vec<Option<Vec<u8>>> = Vec::with_capacity(n);
    for i in 0..n {
        if i % 2 == 0 {
            let bytes = format!("blob-{i}-{}", "x".repeat(i)).into_bytes();
            let h = fake_hash(&bytes);
            st.write
                .write(CasWriteRequest::new(
                    TEST_TENANT,
                    h.clone(),
                    bytes.clone(),
                    "anon@t1",
                    TEST_TENANT,
                    i as u64 + 1,
                ))
                .expect("seed present blob");
            req_hashes.push(h);
            expected_present.push(Some(bytes));
        } else {
            // Distinct absent hash per slot (canonical but never stored).
            let h = fake_hash(format!("never-stored-{i}").as_bytes());
            req_hashes.push(h);
            expected_present.push(None);
        }
    }
    let ndjson: String = req_hashes
        .iter()
        .map(|h| format!("{}\n", serde_json::json!({ "hash": h })))
        .collect();

    let app = router(st);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(ndjson))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let (manifest, payload) = split_manifest(&body).expect("framed response");
    let manifest = std::str::from_utf8(manifest).expect("utf8");
    let lines: Vec<&str> = manifest.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), n, "one manifest line per requested hash");

    // Walk manifest + payload in lock-step, asserting request order, per-hash
    // status/len, and that each present segment slices back to its bytes.
    let mut cursor = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line).expect("manifest json");
        assert_eq!(
            v["hash"],
            serde_json::Value::String(req_hashes[i].clone()),
            "manifest line {i} must be the i-th requested hash (order preserved)"
        );
        match &expected_present[i] {
            Some(bytes) => {
                assert_eq!(v["status"], "ok", "line {i} should be present");
                let len = v["len"].as_u64().expect("len") as usize;
                assert_eq!(len, bytes.len(), "line {i} reported len mismatch");
                assert_eq!(
                    &payload[cursor..cursor + len],
                    bytes.as_slice(),
                    "payload segment {i} must slice back to the seeded bytes"
                );
                cursor += len;
            }
            None => {
                assert_eq!(v["status"], "absent", "line {i} should be absent");
                assert_eq!(v["len"].as_u64(), Some(0), "absent line {i} has len 0");
            }
        }
    }
    assert_eq!(
        cursor,
        payload.len(),
        "the payload is exactly the concatenation of the present segments"
    );
}

/// batch-read reports a tombstoned hash as `gone` (mirrors handle_read's
/// 410 gate), not `absent`.
#[tokio::test]
async fn batch_read_tombstoned_is_gone() {
    const ERASED: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    let app = router(fixture_with_tombstone(TEST_TENANT, ERASED));
    let ndjson = format!("{}\n", serde_json::json!({"hash": ERASED}));
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(ndjson))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let (manifest, _payload) = split_manifest(&bytes).expect("framed");
    let line = std::str::from_utf8(manifest)
        .expect("utf8")
        .lines()
        .next()
        .expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).expect("json");
    assert_eq!(v["status"], "gone");
}

/// (h) batch-exists reports present/absent correctly (HEAD-class).
#[tokio::test]
async fn batch_exists_present_and_absent() {
    let st = fixture();
    let a = b"exists-me".to_vec();
    let ha = fake_hash(&a);
    st.write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            ha.clone(),
            a.clone(),
            "anon@t1",
            TEST_TENANT,
            1,
        ))
        .expect("seed");
    let missing = "0".repeat(64);
    let ndjson = format!(
        "{}\n{}\n",
        serde_json::json!({"hash": ha}),
        serde_json::json!({"hash": missing}),
    );
    let app = router(st);
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(ndjson))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["present"], true);
    assert_eq!(arr[1]["present"], false);
}

/// batch-exists over the 2000-hash cap ⇒ 413.
#[tokio::test]
async fn batch_exists_over_cap_returns_413() {
    let app = router(fixture());
    let zero_hash = "0".repeat(64);
    let mut body = String::new();
    for _ in 0..(BATCH_MAX_OBJECTS + 1) {
        body.push_str(&format!("{}\n", serde_json::json!({"hash": zero_hash})));
    }
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-exists"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// The batch quota charge is ONE call of `n` (the object count), NOT one per
/// object. We seed a quota store with EXACTLY enough budget for n charges at
/// cost-per-op=1 and assert a 3-object batch passes; a per-object loop that
/// also added an extra implicit charge, or a wrong `n`, would mis-bill. The
/// companion assertion: with budget for only n-1, the SAME batch trips 402.
#[tokio::test]
async fn batch_upload_charges_quota_once_for_n() {
    use crate::tenant_quota::{InMemoryQuotaStore, QuotaGuard, QuotaState, QuotaStore};
    use crate::wall_clock::InMemoryFakeWallClock;

    fn state_with_budget(budget_micros: i64) -> CasRouteState {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
        let read: Arc<dyn CasReadHandler> = shared.clone();
        let write: Arc<dyn CasWriteHandler> = shared.clone();
        let delete: Arc<dyn CasDeleteHandler> = shared.clone();
        let list: Arc<dyn CasListHandler> = shared;
        let store = InMemoryQuotaStore::new();
        store.seed(
            TEST_TENANT,
            QuotaState {
                monthly_budget_usd_micros: budget_micros,
                accrued_usd_micros: 0,
                cycle_anchor_ms: 1_700_000_000_000,
            },
        );
        let store: Arc<dyn QuotaStore> = Arc::new(store);
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
        let guard = Arc::new(QuotaGuard::new(store, clock));
        CasRouteState::new(
            read,
            write,
            delete,
            list,
            None,
            Some(crate::routes::QuotaGate::new_for_test(guard, 1)),
            None,
        )
    }

    let objs: Vec<(String, Vec<u8>)> = (0u8..3)
        .map(|i| {
            let b = vec![i + 1; (i as usize) + 1];
            (fake_hash(&b), b)
        })
        .collect();
    let body = build_batch_upload(&objs);

    // Budget for exactly 3 charges (cost=1 each) ⇒ the single check_batch(_,3)
    // fits ⇒ 200.
    let req_ok = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body.clone()))
        .expect("request");
    let r_ok = router(state_with_budget(3))
        .oneshot(req_ok)
        .await
        .expect("oneshot");
    assert_eq!(
        r_ok.status(),
        StatusCode::OK,
        "budget for n=3 must admit the batch"
    );

    // Budget for only 2 ⇒ a single check_batch(_,3) over-projects ⇒ 402.
    let req_402 = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:rw")
        .header(axum::http::header::CONTENT_TYPE, BATCH_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request");
    let r_402 = router(state_with_budget(2))
        .oneshot(req_402)
        .await
        .expect("oneshot");
    assert_eq!(
        r_402.status(),
        StatusCode::PAYMENT_REQUIRED,
        "the batch charge must be n (=3); budget for 2 must trip 402"
    );
}

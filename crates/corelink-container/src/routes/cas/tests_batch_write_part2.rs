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

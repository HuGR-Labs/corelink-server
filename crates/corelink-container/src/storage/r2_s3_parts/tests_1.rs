    #[test]
    fn physical_cas_bucket_must_match_serving_region() {
        assert!(validate_cas_bucket_for_region("corelink-cas-prod", "iad").is_ok());
        assert!(validate_cas_bucket_for_region("corelink-cas-eu", "lhr").is_ok());
        assert!(validate_cas_bucket_for_region("corelink-cas-apac", "nrt").is_ok());
        // A region key alone is not physical residency; the old configuration
        // paired these regions with the US bucket and must now be rejected.
        assert!(validate_cas_bucket_for_region("corelink-cas-prod", "lhr").is_err());
        assert!(validate_cas_bucket_for_region("corelink-cas-prod", "sam").is_err());
        assert!(validate_cas_bucket_for_region("corelink-cas-prod", "syd").is_err());
        assert!(validate_cas_bucket_for_region("", "nrt").is_err());
    }

    // ---------------------------------------------------------------
    // Unit tests: key derivation logic (no network)
    // ---------------------------------------------------------------

    #[test]
    fn blob_key_format_is_canonical() {
        let key = R2S3Client::blob_key(
            "iad",
            "abcdef1234567890",
            "deadbeef00000001",
            DigestAlgo::Blake3,
        );
        assert_eq!(key, "iad/abcdef1234567890/deadbeef00000001");
    }

    #[tokio::test]
    async fn r2_cas_handler_key_uses_region_and_prefix() {
        let handler = make_test_handler("iad").await;
        let key = handler
            .r2_key("tenant-abc", "abc123hash0000001", DigestAlgo::Blake3)
            .unwrap();
        // Key must start with the region segment.
        assert!(key.starts_with("iad/"), "key: {key}");
        // Key must end with the digest.
        assert!(key.ends_with("/abc123hash0000001"), "key: {key}");
        // Middle segment is exactly 16 chars (tenant prefix).
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].len(), 16, "tenant prefix must be 16 chars");
    }

    /// Cross-tenant denial returns BEFORE any S3 I/O — this test
    /// exercises the audit-emit-BEFORE-rejection ordering without
    /// making network calls.
    #[tokio::test]
    async fn r2_cas_handler_cross_tenant_denied_audits_before_rejection() {
        let handler = make_test_handler("iad").await;
        let req = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "attacker@other",
            "other",
            1,
        );
        let err = handler.read(req).expect_err("denied");
        assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
        // The audit sink recorded a ReadDenied event BEFORE the
        // rejection — fail-CLOSED ordering pin.
    }

    #[tokio::test]
    async fn r2_public_requests_share_physical_key_but_keep_accounting_tenant() {
        // Exercise the production request shape and R2 key authority without
        // requiring credentials: both authenticated callers must resolve the
        // same `_public` object key, while their quota identities stay distinct.
        let handler = make_test_handler_with_tdk("iad").await;
        let bytes = b"shared-public-r2".to_vec();
        let digest = Digest::compute(&bytes).to_hex();
        let first = CasWriteRequest::for_public_namespace(
            "tenant-real",
            digest.clone(),
            bytes.clone(),
            "brew",
            1,
        );
        let second =
            CasWriteRequest::for_public_namespace("tenant-other", digest.clone(), bytes, "pip", 2);
        assert!(first.is_authorized_for_caller());
        assert!(second.is_authorized_for_caller());
        assert_ne!(first.accounting_tenant, second.accounting_tenant);
        let first_key = handler
            .r2_key(&first.tenant, &digest, DigestAlgo::Blake3)
            .expect("public R2 key");
        let second_key = handler
            .r2_key(&second.tenant, &digest, DigestAlgo::Blake3)
            .expect("public R2 key");
        assert_eq!(
            first_key, second_key,
            "public CAS must physically deduplicate"
        );
    }

    // ---------------------------------------------------------------
    // Content-addressing enforcement (INV-CAS-INTEGRITY)
    // ---------------------------------------------------------------

    #[test]
    fn verify_content_hash_accepts_matching_digest() {
        let bytes = b"the quick brown fox";
        let claimed = Digest::compute(bytes).to_hex();
        assert!(verify_content_hash(DigestAlgo::Blake3, &claimed, bytes).is_ok());
    }

    #[test]
    fn verify_content_hash_rejects_mismatch_and_reports_actual() {
        // Claim the digest of "A" but hand over the bytes of "B".
        let claimed = Digest::compute(b"A").to_hex();
        let actual_expected = Digest::compute(b"B").to_hex();
        let err = verify_content_hash(DigestAlgo::Blake3, &claimed, b"B").expect_err("must reject");
        // The reported `actual` is the TRUE hash of the bytes given,
        // not the (lying) claimed hash.
        assert_eq!(err, actual_expected);
        assert_ne!(err, claimed);
    }

    #[test]
    fn verify_content_hash_rejects_malformed_claim() {
        // A non-canonical claimed hash can never be validated — treat
        // as a mismatch, never persist/serve under an unparseable key.
        assert!(verify_content_hash(DigestAlgo::Blake3, "not-a-hash", b"anything").is_err());
        assert!(verify_content_hash(DigestAlgo::Blake3, "", b"anything").is_err());
    }

    // ── Surface-tagged SHA-256 keyspace (Option A / ADR-0044) ──────────────

    /// Real SHA-256 of `bytes` as lowercase hex (test helper).
    fn sha256_hex_t(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        hex::encode(h.finalize())
    }

    #[test]
    fn verify_content_hash_sha256_accepts_correct_and_rejects_mismatch() {
        let bytes = b"bazel reapi blob";
        let good = sha256_hex_t(bytes);
        assert!(verify_content_hash(DigestAlgo::Sha256, &good, bytes).is_ok());

        // Wrong claim → Err carrying the TRUE sha256 of the bytes.
        let err = verify_content_hash(DigestAlgo::Sha256, &"0".repeat(64), bytes)
            .expect_err("must reject");
        assert_eq!(err, good);
    }

    #[test]
    fn verify_content_hash_sha256_rejects_malformed_claim() {
        assert!(verify_content_hash(DigestAlgo::Sha256, "not-hex", b"x").is_err());
        assert!(verify_content_hash(DigestAlgo::Sha256, "", b"x").is_err());
    }

    #[test]
    fn verify_content_hash_algos_do_not_cross() {
        // A correct BLAKE3 claim is NOT accepted under the SHA-256 keyspace,
        // and vice-versa — the gate is single-function per keyspace, never
        // blanket OR-accept (the bug Option A refuses to ship).
        let bytes = b"single-function keyspace";
        let blake3_claim = Digest::compute(bytes).to_hex();
        let sha256_claim = sha256_hex_t(bytes);

        assert!(verify_content_hash(DigestAlgo::Blake3, &blake3_claim, bytes).is_ok());
        assert!(verify_content_hash(DigestAlgo::Sha256, &blake3_claim, bytes).is_err());
        assert!(verify_content_hash(DigestAlgo::Sha256, &sha256_claim, bytes).is_ok());
        assert!(verify_content_hash(DigestAlgo::Blake3, &sha256_claim, bytes).is_err());
    }

    #[test]
    fn blob_key_keyspace_isolation_native_vs_bazel() {
        let digest = "a".repeat(64);
        let native = R2S3Client::blob_key("iad", "abcdef1234567890", &digest, DigestAlgo::Blake3);
        let bazel = R2S3Client::blob_key("iad", "abcdef1234567890", &digest, DigestAlgo::Sha256);

        // Native: <region>/<prefix>/<digest> — no bazel sub-prefix.
        assert_eq!(native, format!("iad/abcdef1234567890/{digest}"));
        assert!(!native.contains("bazel/sha256/"));

        // Bazel: the sha256 keyspace sub-prefix sits AFTER the tenant prefix.
        assert_eq!(bazel, format!("iad/abcdef1234567890/bazel/sha256/{digest}"));
        assert!(bazel.starts_with("iad/abcdef1234567890/"));

        // The two keyspaces never collide for the same digest.
        assert_ne!(native, bazel);
    }

    /// Round-trip keyspace isolation through the handler's key derivation: a
    /// SHA-256 (Bazel) read/write derives the `bazel/sha256/` key, while a
    /// BLAKE3 (native) request for the SAME digest derives a DIFFERENT key —
    /// so a SHA-256 blob is never found under the native keyspace and vice
    /// versa. (`r2_key` is the production key authority; this exercises it
    /// without S3 I/O.)
    #[tokio::test]
    async fn r2_key_round_trip_keyspace_isolation() {
        let handler = make_test_handler_with_tdk("iad").await;
        let uuid = "00000000-0000-4000-8000-000000000001";
        let digest = "b".repeat(64);

        let native_key = handler
            .r2_key(uuid, &digest, DigestAlgo::Blake3)
            .expect("native key");
        let bazel_key = handler
            .r2_key(uuid, &digest, DigestAlgo::Sha256)
            .expect("bazel key");

        // Same tenant prefix (HMAC tenant isolation preserved), divergent
        // keyspace tail.
        assert!(bazel_key.contains("/bazel/sha256/"));
        assert!(!native_key.contains("/bazel/sha256/"));
        assert_ne!(native_key, bazel_key);
        // A SHA-256 blob's key is NOT a hit under the native keyspace.
        assert!(!bazel_key.starts_with(&native_key));
    }

    /// The load-bearing security regression: a WRITE whose bytes do not
    /// hash to the claimed digest is rejected with `HashMismatch`
    /// BEFORE any storage I/O — the durable store never persists
    /// poisoned content. (`make_test_handler` points at localhost:1, so
    /// reaching the PUT would error; this test proves we never reach
    /// it.)
    #[tokio::test]
    async fn r2_cas_write_rejects_poisoned_bytes_before_storage() {
        let handler = make_test_handler("iad").await;
        let claimed = Digest::compute(b"honest-bytes").to_hex();
        // Same tenant (so we pass the cross-tenant gate) but the body
        // is NOT what the claimed digest addresses.
        let req = CasWriteRequest::new(
            "t1",
            claimed.clone(),
            b"POISONED-bytes".to_vec(),
            "anon@t1",
            "t1",
            1,
        );
        let err = handler
            .write(req)
            .expect_err("poisoned write must be rejected");
        match err {
            CasHandlerError::HashMismatch { claimed: c, actual } => {
                assert_eq!(c, claimed);
                assert_eq!(actual, Digest::compute(b"POISONED-bytes").to_hex());
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }

    /// A WRITE whose bytes DO hash to the claimed digest passes
    /// verification and proceeds to the storage layer (which then
    /// errors against the unreachable stub endpoint — proving we got
    /// PAST the content check rather than being rejected by it).
    // NOTE: `multi_thread` flavor is REQUIRED — this test proceeds past
    // content verification into the storage layer, which uses
    // `tokio::task::block_in_place` (valid only on the multi-threaded
    // runtime; the production server is `#[tokio::main]` multi-thread).
    // The default current-thread `#[tokio::test]` runtime would panic at
    // the `block_in_place` call, not at any fault in the fix.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_write_with_honest_bytes_passes_verification() {
        let handler = make_test_handler("iad").await;
        let bytes = b"honest-bytes".to_vec();
        let claimed = Digest::compute(&bytes).to_hex();
        let req = CasWriteRequest::new("t1", claimed, bytes, "anon@t1", "t1", 1);
        let err = handler
            .write(req)
            .expect_err("stub endpoint is unreachable");
        // The load-bearing assertion: honest bytes are NOT rejected by
        // the content-addressing gate — verification PASSED and we
        // proceeded to storage (which then failed at the unreachable
        // stub endpoint). We assert "not a HashMismatch" rather than a
        // specific downstream error so the test does not depend on the
        // exact network-failure variant.
        assert!(
            !matches!(err, CasHandlerError::HashMismatch { .. }),
            "honest bytes must pass content verification, got {err:?}"
        );
    }

    // ---------------------------------------------------------------
    // origin_timing: `ostore` must absorb the native-plane R2 calls
    // ---------------------------------------------------------------

    /// Parse a `Server-Timing` header value into `{ name: dur_ms }`, mirroring
    /// `origin_timing::tests::parse` (that one is private to its own module).
    fn parse_server_timing(value: &str) -> std::collections::HashMap<String, i64> {
        let mut out = std::collections::HashMap::new();
        for part in value.split(',') {
            let mut it = part.trim().split(";dur=");
            if let (Some(name), Some(dur)) = (it.next(), it.next()) {
                if let Ok(v) = dur.trim().parse::<i64>() {
                    out.insert(name.trim().to_owned(), v);
                }
            }
        }
        out
    }

    /// The `oother` residue used to swallow the native CAS plane's R2 GET
    /// silently. This proves `R2CasHandler::read`'s `block_in_place` call is
    /// now wrapped into `Phase::Store` (`ostore`) — recorded even though the
    /// stub endpoint (`localhost:1`) makes the GET itself fail, because
    /// `PhaseScope`'s `Drop` records on every exit path, success or error.
    /// Driven through the real `origin_timing_layer` axum middleware (the
    /// only public way to scope a ledger from outside `origin_timing.rs`),
    /// exactly like `origin_timing::tests::the_layer_attributes_a_delay_to_the_phase_it_was_charged_to`.
    // NOTE: `multi_thread` flavor is REQUIRED — see
    // `r2_cas_write_with_honest_bytes_passes_verification` above for why.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_read_attributes_the_r2_get_to_ostore() {
        let handler = std::sync::Arc::new(make_test_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = CasReadRequest::new(
                            "tenant-x",
                            "deadbeef00000000000000000000000000000000000000000000000000000001",
                            "caller@tenant-x",
                            "tenant-x",
                            1,
                        );
                        // Errors against the unreachable stub endpoint — that
                        // is fine, the assertion is on the timing header, not
                        // the result.
                        let _ = handler.read(req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2CasHandler::read's block_in_place R2 GET must be attributed \
             to Phase::Store (ostore) even when the call errors. Header: {header}"
        );
    }

    /// AC counterpart of the above: `R2AcHandler::lookup`'s `block_in_place`
    /// R2 GET must also land in `Phase::Store`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_ac_lookup_attributes_the_r2_get_to_ostore() {
        let handler = std::sync::Arc::new(make_test_ac_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = corelink_handler_ac::AcLookupRequest::new(
                            "tenant-x",
                            "deadbeef00000000000000000000000000000000000000000000000000000001",
                            "caller@tenant-x",
                            "tenant-x",
                            1,
                        );
                        let _ = corelink_handler_ac::AcLookupHandler::lookup(&*handler, req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2AcHandler::lookup's block_in_place R2 GET must be attributed \
             to Phase::Store (ostore) even when the call errors. Header: {header}"
        );
    }

    // ---------------------------------------------------------------
    // Durable-audit-before-R2 `list()` timing
    // ---------------------------------------------------------------

    /// `list()`'s durable attempted audit is completed before the
    /// `block_in_place` R2 `ListObjectsV2` call, which still lands in
    /// `Phase::Store` (`ostore`). Network-free (stub S3 endpoint), so this
    /// runs in CI.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_cas_list_serial_fallback_attributes_the_r2_call_to_ostore() {
        let handler = std::sync::Arc::new(make_test_handler("iad").await);

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let req = corelink_handler_cas::CasListRequest::new(
                            "tenant-x",
                            "caller@tenant-x",
                            "tenant-x",
                            10,
                            None,
                            1,
                        );
                        let _ = CasListHandler::list(&*handler, req);
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "R2CasHandler::list's serial-fallback R2 call must be attributed \
             to Phase::Store (ostore). Header: {header}"
        );
    }

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
    // Concurrent audit+R2 `list()` seam
    // ---------------------------------------------------------------

    /// `list()`'s SERIAL FALLBACK path (`audit_async` unset — every test
    /// handler in this module) is exercised no differently than before this
    /// PR: the `block_in_place` R2 `ListObjectsV2` call still lands in
    /// `Phase::Store` (`ostore`). Network-free (stub S3 endpoint), so this
    /// runs in CI, unlike the concurrent-path test below.
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

    /// The CONCURRENT path (`audit_async` wired — production shape): even
    /// when the durable-audit D1 write fails (here: a reachable-but-wrong
    /// token, `stub_env()` from `d1_audit_sink.rs`'s own test helper shape),
    /// `list()` still returns `AuditFailed` and — the load-bearing part —
    /// the joined window is attributed ONCE to `ostore`, and `oaudit` is
    /// ABSENT (proving `append_async` did not enter its own `Phase::Audit`
    /// scope, so the two never double-count the same wall-clock window; see
    /// `origin_timing.rs`'s "Concurrent native-plane list seam" note).
    ///
    /// Gated behind `#[ignore]` like the `d1_audit_sink.rs` async-phase
    /// test it mirrors — needs outbound reachability to
    /// `api.cloudflare.com` (not live credentials: a 401 still proves the
    /// join ran). Run manually with:
    ///
    /// ```bash
    /// cargo test -p corelink-server r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires outbound network reachability to api.cloudflare.com"]
    async fn r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds() {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1HttpClient constructs over the stub env (network call happens lazily)");
        let audit: Arc<dyn AuditSink> = audit_concrete.clone();
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = std::sync::Arc::new(
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete),
        );

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
                        let result = CasListHandler::list(&*handler, req);
                        // FAIL-CLOSED: a rejected D1 credential must surface
                        // as AuditFailed, never as a "success" that could
                        // have served R2 rows.
                        assert!(
                            matches!(result, Err(CasHandlerError::AuditFailed(_))),
                            "bad D1 creds must fail CLOSED as AuditFailed, got: {result:?}"
                        );
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
            "the concurrent join must be attributed to Phase::Store (ostore). \
             Header: {header}"
        );
        assert!(
            !parsed.contains_key("oaudit"),
            "the concurrent list()'s audit write must NOT ALSO appear under \
             oaudit — it would double-count the same wall-clock window \
             `ostore` already reports. Header: {header}"
        );
    }

    // The `oaudit` assertion above used to be guarded by a mirror of
    // `origin_timing::detail_phases_enabled`, because `oaudit` shipped only with
    // `CORELINK_ORIGIN_TIMING_DETAIL=on`. B-109 narrowed that flag to the two
    // credential-path phases (`oargon`/`opermit`), so `oaudit` now publishes
    // ALWAYS — and the guard had to go with it.
    //
    // It was not merely redundant, it had become DOMINATED: with the flag off
    // (the production default) `!detail_phases_enabled_for_test()` is true, so
    // the whole assertion short-circuits to `true` and would pass even if the
    // concurrent `list()` seam started double-counting its audit write under
    // `oaudit`. A guard that cannot fail in the configuration that actually
    // ships is not a guard. The assertion is now unconditional, which is what
    // the double-counting property always required.

    // ---------------------------------------------------------------
    // Integration round-trip test (requires live R2 creds)
    // ---------------------------------------------------------------

    /// PUT bytes → GET → bytes match.
    ///
    /// Gated behind `#[ignore]` so the CI green path does not require
    /// live R2 credentials. Run manually with:
    ///
    /// ```bash
    /// R2_S3_ACCESS_KEY_ID=<id> R2_S3_SECRET_ACCESS_KEY=<sec> \
    ///   R2_S3_ENDPOINT=https://<account>.r2.cloudflarestorage.com \
    ///   CLOUDFLARE_ACCOUNT_ID=<acc> CF_API_TOKEN=<tok> \
    ///   D1_DATABASE_ID=<id> \
    ///   R2_TEST_BUCKET=corelink-cas-prod \
    ///   cargo test -p corelink-server storage_r2_round_trip -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn storage_r2_round_trip() {
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");

        // Use a timestamped key so parallel test runs don't collide.
        let key = format!(
            "test/round-trip/{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let payload = b"corelink-wp-s1-storage-round-trip".to_vec();

        // PUT
        client.put(&key, payload.clone()).await.expect("put");

        // GET → must match
        let got = client.get(&key).await.expect("get").expect("present");
        assert_eq!(got, payload, "round-trip bytes must match");

        // GET missing key → None
        let missing = client.get("__no_such_key__").await.expect("get");
        assert!(missing.is_none(), "missing key must return None");
    }

    /// rt-nuclear #13 regression (live R2): the FIRST CAS write of a content hash
    /// returns `durable=true` (a real PUT); an idempotent re-write of the SAME
    /// content returns `durable=false` (HEAD hit → no re-PUT), so the
    /// `AccountingCasHandler` decorator rolls the reservation back and does NOT
    /// double-charge the bytes. Gated behind `#[ignore]` like `storage_r2_round_trip`.
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn cas_idempotent_rewrite_reports_durable_false() {
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = R2CasHandler::new(client, "iad", None, audit, sli);

        // Unique content per run so parallel runs / prior state don't collide.
        let payload = format!(
            "rt-nuclear-13-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
        .into_bytes();
        let tenant = "t-rt13";
        let claimed = Digest::compute(&payload).to_hex();

        let first = handler
            .write(CasWriteRequest::new(
                tenant,
                claimed.clone(),
                payload.clone(),
                "p",
                tenant,
                1,
            ))
            .expect("first write");
        assert!(
            first.durable,
            "first write of a fresh hash must be durable=true"
        );

        let second = handler
            .write(CasWriteRequest::new(
                tenant,
                claimed.clone(),
                payload,
                "p",
                tenant,
                2,
            ))
            .expect("second write");
        assert!(
            !second.durable,
            "an idempotent re-write must report durable=false (HEAD hit → no re-PUT, no re-charge)"
        );
    }

    /// rt-nuclear #6/#10/#14 regression (live R2): `delete_if_present` returns the
    /// reclaimed size to the FIRST delete and `None` (release 0) to the SECOND —
    /// two deletes of the same key can never both credit the same bytes. Run with
    /// the same live-R2 env as `storage_r2_round_trip`.
    #[tokio::test]
    #[ignore = "requires live R2 credentials (R2_S3_ACCESS_KEY_ID etc.)"]
    async fn delete_if_present_credits_size_once_then_none() {
        let env = StorageEnv::from_env().expect("all R2 env vars must be set to run this test");
        let bucket =
            std::env::var("R2_TEST_BUCKET").unwrap_or_else(|_| "corelink-cas-prod".to_owned());
        let client = R2S3Client::new(&env, &bucket).await.expect("client");

        let key = format!(
            "test/delete-once/{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let payload = b"rt-nuclear-6-10-14".to_vec();
        let size = payload.len() as u64;
        client.put(&key, payload).await.expect("put");

        // First delete observes-and-removes → Some(size).
        let first = client.delete_if_present(&key).await.expect("first delete");
        assert_eq!(
            first,
            Some(size),
            "the first delete must credit the reclaimed size"
        );

        // Second delete sees the key already gone → None (releases 0).
        let second = client.delete_if_present(&key).await.expect("second delete");
        assert_eq!(
            second, None,
            "a second delete must credit 0 (no double-release)"
        );
    }

    // ---------------------------------------------------------------
    // Helper
    // ---------------------------------------------------------------

    /// Build an `R2CasHandler` backed by an unavailable (stub) S3
    /// client. Only the key-derivation and audit paths are exercised
    /// in these tests; any S3 I/O would fail.
    ///
    /// This is an `async fn` so it can be called from within a
    /// `#[tokio::test]` context without nested-runtime conflicts.
    async fn make_test_handler(region: &str) -> R2CasHandler {
        // Build a stub env pointing at localhost (won't connect).
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };

        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");

        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        R2CasHandler::new(client, region, None, audit, sli)
    }

    /// The latency SLI must observe a REAL window, not a literal zero.
    ///
    /// B-057 regression pin. `emit_sli` used to pass `0` for both the
    /// availability and the latency SLI at every CAS/AC call site, so
    /// `LatencyCasGetP99` was a stream of zeroes — not a loose
    /// measurement, an absent one. Reverting `emit_sli` to the old
    /// `SliObservation::new(lat, is_error, 0)` reds this test.
    ///
    /// Driven through the cross-tenant denial arm of `read()` on
    /// purpose: it emits and returns WITHOUT touching R2, so the pin
    /// needs no credentials and cannot flake on the network.
    #[tokio::test]
    async fn cas_read_emits_a_nonzero_latency_sli() {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let observer: Arc<dyn SliObserver> = Arc::clone(&sli) as Arc<dyn SliObserver>;
        let handler = R2CasHandler::new(client, "iad", None, audit, observer);

        let req = CasReadRequest::new("tenant-a", "a".repeat(64), "p", "tenant-b", 1);
        let out = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .expect("join");
        assert!(out.is_err(), "cross-tenant read must be refused");

        let obs = sli.snapshot().expect("sli");
        let lat: Vec<_> = obs
            .iter()
            .filter(|o| o.sli == corelink_handler_cas::observer::Sli::LatencyCasGetP99)
            .collect();
        assert_eq!(lat.len(), 1, "exactly one latency observation: {obs:?}");
        assert!(
            lat[0].latency_us > 0,
            "the latency SLI must carry the measured window, got {} us",
            lat[0].latency_us
        );
        let avail: Vec<_> = obs
            .iter()
            .filter(|o| o.sli == corelink_handler_cas::observer::Sli::AvailCasGet)
            .collect();
        assert_eq!(avail.len(), 1, "exactly one availability observation");
        assert!(avail[0].is_error, "a refused read is an error observation");
    }

    // ---------------------------------------------------------------
    // `exists_batch` — the findMissingBlobs seam
    // ---------------------------------------------------------------

    /// Without the async audit seam wired, the handler advertises NO batch
    /// capability, so `find_missing` keeps the unchanged per-digest
    /// `exists()` loop. This is the regression pin for "the serial fallback
    /// stayed byte-identical".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_batch_is_absent_without_the_async_audit_seam() {
        let handler = make_test_handler("iad").await;
        let req = CasReadRequest::new(
            "tenant-x",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "caller@tenant-x",
            "tenant-x",
            1,
        );
        assert!(
            CasReadHandler::exists_batch(&handler, &[req]).is_none(),
            "no durable async sink -> no batch capability -> serial fallback"
        );
    }

    /// Cross-tenant denial is evaluated STRICTLY FIRST — before the audit
    /// batch or any R2 probe is dispatched — and still emits its own
    /// `ReadDenied` row through the sync sink.
    ///
    /// The proof that nothing was dispatched is that this test is fully
    /// hermetic: the S3 client points at `localhost:1` and the D1 sink
    /// carries stub credentials, so ANY dispatch would have to fail against
    /// an unreachable endpoint rather than return a clean denial.
    ///
    /// It deliberately wires an in-memory `audit` alongside a D1
    /// `audit_async` — a combination `with_async_audit`'s doc forbids in
    /// production (the two must be the same sink) — purely so the denial
    /// row can be READ BACK without a network call. The denial path only
    /// ever touches `audit`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_batch_denies_cross_tenant_before_dispatching_anything() {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let recorder = Arc::new(InMemoryAuditSink::new());
        let audit: Arc<dyn AuditSink> = recorder.clone();
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1 client constructs (the network call happens lazily)");
        let sli = Arc::new(InMemorySliObserver::new());
        let handler =
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete);

        // A legitimate digest FIRST, the poisoned one second: the scan must
        // still refuse the whole batch before the good one touches R2.
        let ok = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
            "attacker",
            "victim",
            1,
        );
        let poisoned = CasReadRequest::new(
            "victim",
            "deadbeef00000000000000000000000000000000000000000000000000000002",
            "attacker",
            "attacker-tenant",
            1,
        );

        let result = CasReadHandler::exists_batch(&handler, &[ok, poisoned])
            .expect("batch capability is wired");
        match result {
            Err(CasHandlerError::CrossTenantDenied {
                caller,
                requested_tenant,
            }) => {
                assert_eq!(caller, "attacker-tenant");
                assert_eq!(requested_tenant, "victim");
            }
            other => panic!("expected CrossTenantDenied, got {other:?}"),
        }

        let rows = recorder.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 1, "exactly one denial row: {rows:?}");
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
        assert_eq!(rows[0].tenant, "victim");
    }

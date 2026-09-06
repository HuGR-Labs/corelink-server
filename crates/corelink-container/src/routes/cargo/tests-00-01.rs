    #[async_trait]
    impl PatRowLookup for CountingD1 {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.pat_err {
                return Err(e.clone());
            }
            if self.honours_coread {
                if let Some((cell, ns, key)) = crate::d1_coread::hint() {
                    cell.publish(self.lookup_map(&ns, &key));
                }
            }
            Ok(self.row.lock().unwrap().clone())
        }
    }

    #[async_trait]
    impl UrlMapStore for CountingD1 {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            self.map_reads.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.map_err {
                return Err(e.clone());
            }
            Ok(self.lookup_map(ns, url_hash))
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            self.map.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    /// A CAS that hands back FIXED bytes for any hash — the moat's own
    /// content-hash re-check decides whether they are served, so a test seeds
    /// the map with `canonical_hash_hex(bytes)` to model a real hit.
    #[derive(Debug)]
    struct FixedBytesCas(Vec<u8>);
    impl CasReadHandler for FixedBytesCas {
        fn read(
            &self,
            req: corelink_handler_cas::CasReadRequest,
        ) -> Result<corelink_handler_cas::CasReadResponse, CasHandlerError> {
            Ok(corelink_handler_cas::CasReadResponse::new(
                self.0.clone(),
                req.hash,
            ))
        }
    }
    impl CasWriteHandler for FixedBytesCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    fn coread_test_key() -> PatSigningKey {
        PatSigningKey::from_bytes(vec![0x42_u8; 32]).unwrap()
    }

    /// Mint a real PAT; returns `(plaintext, pat_hash, tenant_id)`.
    fn mint_cargo_pat(key: &PatSigningKey) -> (String, String, String) {
        let tenant = TenantId(Uuid::from_u128(0x5cca_c4e0_0000_0001));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant,
            PrincipalId(Uuid::from_u128(0x5cca_c4e0_0000_0002)),
            PatScopes::from_u64(corelink_pat::SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    /// The REAL cargo router (adapter + gate + co-read hint layer) over the
    /// counting D1 seam and the REAL `PatVerifier` — so the PAT is verified with
    /// real crypto and the only fakes are the two D1 ports.
    fn coread_router(d1: Arc<CountingD1>, key: &PatSigningKey, blob: Vec<u8>) -> Router {
        let verifier = crate::adapter_pat::PatVerifier::with_key_set(
            Arc::clone(&d1) as Arc<dyn PatRowLookup>,
            vec![key.clone()],
        );
        let cas = Arc::new(FixedBytesCas(blob));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::clone(&d1) as Arc<dyn UrlMapStore>,
            resolver_from_verifier(Arc::new(verifier)),
            None,
            None,
        )
    }

    /// A cargo GET exactly as the Worker forwards it: server-trusted scope +
    /// tenant headers, bearer PAT, `/cargo/<tenant>/<key>`.
    fn cargo_get(tenant: &str, key: &str, pat: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/{tenant}/{key}"))
            .header(SCOPE_HEADER, "cas:rw")
            .header(TENANT_HINT_HEADER, tenant)
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pat}"))
            .body(axum::body::Body::empty())
            .unwrap()
    }

    fn live_row(pat_hash: &str, tenant: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: "cas:rw".to_owned(),
            find_only: false,
            runner_job: false,
        }
    }

    /// ⭐ THE property. A cargo GET that MISSES — the exact request the prod
    /// probe measured at `opat` 72 + `ostore` 75 ms — must cross to D1 exactly
    /// ONCE, with the `pat` row and the url-map row in the same round trip.
    ///
    /// On the serial path this reads 2 (and `map_reads` 1): that is the
    /// negative-control signature.
    #[tokio::test]
    async fn cargo_get_miss_costs_exactly_one_d1_round_trip() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let d1 = Arc::new(CountingD1::new(Some(live_row(&hash, &tenant))));
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"a".repeat(64), &pat))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "cache miss ⇒ 404");
        assert_eq!(
            d1.trips(),
            1,
            "the cargo GET issued {} D1 round trips; expected exactly 1 carrying \
             BOTH the pat row and the url-map row (prod: opat 72ms + ostore 75ms \
             = 2 RTTs = 55% of origin)",
            d1.trips()
        );
        assert_eq!(
            d1.map_reads(),
            0,
            "the url-map must NOT be read separately — that second read IS the \
             round trip this change removes"
        );
    }

    /// The co-read is not just short-circuiting misses: a HIT serves the bytes
    /// the co-read row pointed at, still in one round trip.
    #[tokio::test]
    async fn cargo_get_hit_serves_the_coread_row_in_one_round_trip() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let blob = b"sccache-artifact-bytes".to_vec();
        let cache_key = "b".repeat(64);
        let d1 = Arc::new(
            CountingD1::new(Some(live_row(&hash, &tenant))).with_mapping(
                &tenant,
                &cache_key,
                &crate::adapter_cache::canonical_hash_hex(&blob),
            ),
        );
        let app = coread_router(Arc::clone(&d1), &key, blob.clone());

        let resp = app
            .oneshot(cargo_get(&tenant, &cache_key, &pat))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), blob.as_slice());
        assert_eq!(d1.trips(), 1, "a HIT is one round trip too");
        assert_eq!(d1.map_reads(), 0);
    }

    /// ⛔ IMMEDIATE REVOCATION SURVIVES. #1022 kept the per-request `pat` read
    /// for exactly this, and folding the url-map read into it must not turn it
    /// into a cache: revoke the row in D1 and the VERY NEXT request 401s.
    ///
    /// The round-trip count is asserted too — the second request must have made
    /// its own D1 read (2 total). A change that served the second request from
    /// anything remembered would show 1 here.
    #[tokio::test]
    async fn revoking_the_pat_row_401s_the_very_next_request() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let d1 = Arc::new(CountingD1::new(Some(live_row(&hash, &tenant))));
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let first = app
            .clone()
            .oneshot(cargo_get(&tenant, &"c".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::NOT_FOUND, "live PAT ⇒ served");

        // Revocation in D1: `revoked_at_ms IS NULL` stops matching, so the
        // statement returns no row — indistinguishable from absent, by design.
        *d1.row.lock().unwrap() = None;

        let second = app
            .oneshot(cargo_get(&tenant, &"c".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            second.status(),
            StatusCode::UNAUTHORIZED,
            "a PAT revoked in D1 must 401 on the very next request — the \
             co-read must not have cached the row"
        );
        assert_eq!(
            d1.trips(),
            2,
            "each request must make its OWN D1 read (no cross-request reuse)"
        );
    }

    /// ⚠️ AUTH BEFORE ACT. The co-read is keyed by the Worker's tenant HINT; the
    /// moat is keyed by the tenant the container derived from the PAT. When they
    /// disagree, the prefetched row MUST be discarded unread and the storage
    /// lookup re-issued under the PAT-derived tenant — never served across.
    ///
    /// Here the hint names a victim tenant that HAS the key; the PAT resolves to
    /// a different tenant that does NOT. The answer must be the PAT tenant's
    /// (404), and the map must have been read for real under it.
    #[tokio::test]
    async fn a_row_prefetched_under_a_spoofed_tenant_hint_is_never_served() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let cache_key = "d".repeat(64);
        let blob = b"victim-tenant-secret".to_vec();
        let d1 = Arc::new(
            CountingD1::new(Some(live_row(&hash, &tenant))).with_mapping(
                "victim-tenant",
                &cache_key,
                &crate::adapter_cache::canonical_hash_hex(&blob),
            ),
        );
        let app = coread_router(Arc::clone(&d1), &key, blob);

        // The URL + hint header both claim the victim tenant; the PAT does not.
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/victim-tenant/{cache_key}"))
            .header(SCOPE_HEADER, "cas:rw")
            .header(TENANT_HINT_HEADER, "victim-tenant")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pat}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the victim tenant's cached object must NOT be served to a PAT that \
             resolves to a different tenant"
        );
        assert_eq!(
            d1.map_reads(),
            1,
            "the mismatched prefetch must be discarded and the url-map re-read \
             under the PAT-derived tenant"
        );
    }

    /// D1-failure semantics on the `pat` read are unchanged: a backend fault is
    /// a SHED (503 + `Retry-After`), never a 401 — `VerifierOverloaded`, not
    /// `Auth` (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`).
    #[tokio::test]
    async fn a_d1_fault_on_the_coread_still_503s_never_401s() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let mut d1 = CountingD1::new(Some(live_row(&hash, &tenant)));
        d1.pat_err = Some("D1 HTTP 500: upstream".to_owned());
        let d1 = Arc::new(d1);
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"e".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a D1 fault must stay a 503 shed — a valid PAT is never told it is \
             invalid because the verifier could not reach D1"
        );
    }

    /// A backend that does NOT co-read (and the fallback a failed co-read takes)
    /// keeps the exact serial behaviour: two round trips, and a url-map fault
    /// still surfaces as the CAS-backend 502 — NOT the verifier's 503.
    #[tokio::test]
    async fn the_serial_fallback_keeps_both_reads_and_the_502_map_fault() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let mut d1 = CountingD1::new(Some(live_row(&hash, &tenant))).serial();
        d1.map_err = Some("D1 HTTP 500: adapter_cache_map".to_owned());
        let d1 = Arc::new(d1);
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"f".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_GATEWAY,
            "a url-map backend fault on the serial path is a CAS-dependency 502, \
             exactly as before the co-read existed"
        );
        assert_eq!(
            d1.trips(),
            2,
            "no co-read ⇒ the two reads stay serial (this is the pre-change \
             baseline the fallback preserves)"
        );
        assert_eq!(d1.map_reads(), 1);
    }

    /// What the handler saw: the `(namespace, url_hash)` hint in scope, or
    /// `None` when the layer published none.
    type ObservedHint = Arc<Mutex<Option<Option<(String, String)>>>>;

    /// Drive `req` through the co-read hint layer alone and report the hint the
    /// downstream handler observed.
    async fn observe_hint(req: axum::http::Request<axum::body::Body>) -> Option<(String, String)> {
        use tower::ServiceExt;
        let observed: ObservedHint = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&observed);
        let app = Router::new()
            .fallback(axum::routing::any(move || {
                let sink = Arc::clone(&sink);
                async move {
                    *sink.lock().unwrap() =
                        Some(crate::d1_coread::hint().map(|(_cell, ns, key)| (ns, key)));
                    StatusCode::OK
                }
            }))
            .layer(middleware::from_fn(cargo_coread_hint));
        let _ = app.oneshot(req).await.unwrap();
        let seen = observed.lock().unwrap().clone();
        seen.expect("the handler must have run")
    }

    /// A PUT publishes NO hint: its storage step is a map WRITE, so co-reading a
    /// map row would be a wasted index probe on every sccache upload.
    #[tokio::test]
    async fn a_write_verb_publishes_no_coread_hint() {
        let key = "1".repeat(64);
        let req = axum::http::Request::builder()
            .method(Method::PUT)
            .uri(format!("/cargo/tenant-abc/{key}"))
            .header(TENANT_HINT_HEADER, "tenant-abc")
            .body(axum::body::Body::from("bytes"))
            .unwrap();
        assert_eq!(
            observe_hint(req).await,
            None,
            "PUT must reach the handler with NO co-read hint in scope"
        );
    }

    /// A GET with no server-trusted tenant header (direct container access, or a
    /// Worker regression) publishes no hint and stays on the unchanged serial
    /// path — absence of the hint is never guessed at.
    #[tokio::test]
    async fn a_get_without_the_tenant_header_publishes_no_hint() {
        let key = "2".repeat(64);
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/tenant-abc/{key}"))
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(observe_hint(req).await, None);
    }

    /// A GET DOES publish the hint, keyed by the server-trusted tenant header
    /// and the SAME `key_from_path` normalization the moat will ask with (a
    /// divergence here would silently cost a wasted probe on every request).
    #[tokio::test]
    async fn a_get_publishes_the_hint_the_moat_will_ask_with() {
        // UPPERCASE hex on the wire — `key_from_path` lowercases 64-hex object
        // keys, so the hint must carry the lowercased form.
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/tenant-abc/{}", "AB".repeat(32)))
            .header(TENANT_HINT_HEADER, "  tenant-abc  ")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            observe_hint(req).await,
            Some(("tenant-abc".to_owned(), "ab".repeat(32))),
            "the hint must be the trimmed tenant + the normalized key"
        );
    }

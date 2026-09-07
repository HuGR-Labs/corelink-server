    /// PROPFIND on the collection root (trailing slash) → `207` collection.
    #[tokio::test]
    async fn propfind_collection_root_is_207_collection() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::MULTI_STATUS);
        let body = body_string(resp).await;
        assert!(
            body.contains("<D:collection/>"),
            "a collection stat must carry <D:collection/>; got: {body}"
        );
        // REGRESSION LOCK: the collection 207 is what opendal PROPFINDs on the
        // tenant/dir root during its write-check — it MUST also carry
        // <D:getlastmodified> or the deserialize fails and storage goes ReadOnly.
        assert!(
            body.contains("<D:getlastmodified>"),

            "collection 207 MUST carry <D:getlastmodified> (opendal stat parser); got: {body}"
        );
    }

    /// DELETE an EXISTING key → `204` and the key is GONE from the moat.
    #[tokio::test]
    async fn delete_existing_key_is_204_and_removes_it() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, "delobject", b"bytes".to_vec(), None)
            .await
            .unwrap();
        // Sanity: present before.
        assert!(moat.get(tenant, "delobject").await.unwrap().is_some());
        let check = Arc::clone(&moat);
        let app = webdav_router(moat, tenant, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            check.get(tenant, "delobject").await.unwrap().is_none(),
            "the key must be gone from the moat after DELETE"
        );
    }

    /// 0086 CONTAINMENT (audit F-1): a runner-job credential may NOT delete a
    /// build artifact, even holding a write scope AND the D1 write bit — the
    /// two layers this surface used to stop at. Mirrors the native plane's
    /// `runner_job_cas_delete_returns_403`.
    #[tokio::test]
    async fn runner_job_cannot_delete_an_artifact() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let key = "a".repeat(64); // a 64-hex object key = a real artifact
        let moat = in_memory_moat();
        moat.put(tenant, &key, b"bytes".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                &format!("/cargo/tenant-abc/{key}"),
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "a runner-job credential must not evict the tenant's cache"
        );
        assert!(
            check.get(tenant, &key).await.unwrap().is_some(),
            "the artifact must still be there after the refused DELETE"
        );
    }

    /// The other half of the containment, and the one that keeps the runner
    /// fleet alive: sccache's `.sccache_check` write-check MUST still round-trip
    /// under a runner-job credential. A blanket DELETE refusal would fail every
    /// runner box at startup — the same class of breakage as the 400 that once
    /// made the real client disable the backend entirely.
    #[tokio::test]
    async fn runner_job_can_still_delete_the_sccache_write_check() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, ".sccache_check", b"probe".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/.sccache_check",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "the write-check control key must stay deletable"
        );
        assert!(
            check.get(tenant, ".sccache_check").await.unwrap().is_none(),
            "the control key must actually be removed"
        );
    }

    /// A NORMAL (non-runner) credential deletes artifacts exactly as before —
    /// the containment must not widen into ordinary cache management.
    #[tokio::test]
    async fn normal_pat_can_still_delete_an_artifact() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let key = "b".repeat(64);
        let moat = in_memory_moat();
        moat.put(tenant, &key, b"bytes".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, false);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                &format!("/cargo/tenant-abc/{key}"),
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(check.get(tenant, &key).await.unwrap().is_none());
    }

    /// `is_object_key` decides the containment, so pin its edges: only exactly
    /// 64 hex chars is an artifact. Anything else is a control key and stays
    /// deletable — erring toward the runner fleet keeping working.
    #[test]
    fn is_object_key_accepts_only_64_hex() {
        assert!(is_object_key(&"a".repeat(64)));
        assert!(is_object_key(&"0123456789abcdef".repeat(4)));
        assert!(is_object_key(&"A".repeat(64)), "uppercase hex is still hex");
        assert!(!is_object_key(&"a".repeat(63)), "too short");
        assert!(!is_object_key(&"a".repeat(65)), "too long");
        assert!(!is_object_key(".sccache_check"), "the control key");
        assert!(!is_object_key(&"g".repeat(64)), "non-hex");
        assert!(!is_object_key(""), "empty");
    }

    /// DELETE with a read-ONLY scope → `403` (it is write-gated).
    #[tokio::test]
    async fn delete_readonly_scope_is_403() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// DELETE with a write scope but a PAT whose D1 record lacks write (F27) →
    /// `403` — the second layer blocks it even though the header said `cas:rw`.
    #[tokio::test]
    async fn delete_pat_without_write_is_403_f27() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", false); // PAT can_write = false
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// DELETE with no bearer token → `401` (F27 requires a PAT).
    #[tokio::test]
    async fn delete_without_bearer_is_401() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// DELETE of an ABSENT key is idempotent → still `204`.
    #[tokio::test]
    async fn delete_absent_key_is_204_idempotent() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/nothing-here",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // ---- The co-read: ONE D1 round trip for the pat row AND the url-map row ---
    //
    // Prod (`1ee76248-r1`, n=65, authed `/cargo` 404 miss): `opat` 62/72/81 and
    // `ostore` 66/75/84 ms — 147 ms, 55 % of a 267 ms `origin` — were two
    // container→D1-primary round trips issued back to back. These tests pin that
    // they are now ONE, and that collapsing them changed nothing about auth.
    //
    // The counter is on the two ports the request crosses D1 through
    // (`PatRowLookup` and `UrlMapStore`), so it counts round trips the way the
    // measurement does, not statements.

    use std::sync::atomic::{AtomicUsize, Ordering};

    use corelink_pat::{mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId};
    use uuid::Uuid;

    use crate::adapter_pat::{PatRow, PatRowLookup};

    /// The D1 seam of the cargo read path, counting round trips.
    ///
    /// `lookup` emulates the production [`crate::storage::d1_http::D1HttpClient`]
    /// contract exactly: when a co-read hint is in scope it satisfies the
    /// url-map lookup in the SAME call and publishes it. `honours_coread =
    /// false` reproduces the serial path (a backend that does not co-read), so
    /// the fallback is exercised by the same fixture.
    struct CountingD1 {
        /// Every D1 round trip the request made, over BOTH ports.
        round_trips: AtomicUsize,
        /// The subset that were SEPARATE url-map reads — the second hop this
        /// change exists to remove.
        map_reads: AtomicUsize,
        /// The live `pat` row, or `None` = absent / expired / REVOKED (the D1
        /// statement filters those out, so the verifier sees no row).
        row: Mutex<Option<PatRow>>,
        /// `(namespace, url_hash) → content_hash`.
        map: Mutex<HashMap<(String, String), String>>,
        /// Forced backend fault on the `pat` read (D1 unreachable).
        pat_err: Option<String>,
        /// Forced backend fault on the separate url-map read.
        map_err: Option<String>,
        honours_coread: bool,
    }

    impl CountingD1 {
        fn new(row: Option<PatRow>) -> Self {
            Self {
                round_trips: AtomicUsize::new(0),
                map_reads: AtomicUsize::new(0),
                row: Mutex::new(row),
                map: Mutex::new(HashMap::new()),
                pat_err: None,
                map_err: None,
                honours_coread: true,
            }
        }
        fn serial(mut self) -> Self {
            self.honours_coread = false;
            self
        }
        fn with_mapping(self, ns: &str, key: &str, content_hash: &str) -> Self {
            self.map
                .lock()
                .unwrap()
                .insert((ns.to_owned(), key.to_owned()), content_hash.to_owned());
            self
        }
        fn trips(&self) -> usize {
            self.round_trips.load(Ordering::SeqCst)
        }
        fn map_reads(&self) -> usize {
            self.map_reads.load(Ordering::SeqCst)
        }
        fn lookup_map(&self, ns: &str, key: &str) -> Option<String> {
            self.map
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), key.to_owned()))
                .cloned()
        }
    }

    impl std::fmt::Debug for CountingD1 {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CountingD1").finish_non_exhaustive()
        }
    }
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

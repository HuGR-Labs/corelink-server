    use std::collections::HashMap;
    use std::sync::Mutex;

    use axum::body::Body;
    use axum::http::{Method, Request as HttpRequest, StatusCode};
    use corelink_adapter_host::npm::metadata::kv_key_for_pkg;
    use corelink_adapter_host::npm::tarball::tarball_url_digest;
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId,
        SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_kv::NpmKvBackend;
    use crate::adapter_pat::{PatRow, PatRowLookup};

    use super::*;

    const SCOPE_RW: &str = "cas:rw";

    /// PatRowLookup that knows ONE token_id → row; everything else unknown.
    struct OneTokenLookup {
        token_id: String,
        row: PatRow,
    }
    #[async_trait]
    impl PatRowLookup for OneTokenLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            Ok((token_id == self.token_id).then(|| self.row.clone()))
        }
    }

    /// PatRowLookup that knows nothing (rejects every token).
    struct EmptyLookup;
    #[async_trait]
    impl PatRowLookup for EmptyLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            Ok(None)
        }
    }

    /// In-memory url→content-hash map (the tarball dedup level-2).
    #[derive(Default)]
    struct FakeMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeMap {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), url_hash.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    // `(ns, key) → (value, inserted_ms)` rows for the in-memory KV fake.
    type FakeKvRows = HashMap<(String, String), (Vec<u8>, u64)>;

    /// In-memory npm metadata KV backend: `(ns,key) → (value, inserted_ms)`.
    #[derive(Default, Debug)]
    struct FakeKv(Mutex<FakeKvRows>);
    #[async_trait]
    impl NpmKvBackend for FakeKv {
        async fn get(&self, ns: &str, key: &str) -> Result<Option<(Vec<u8>, u64)>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), key.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            key: &str,
            value: Vec<u8>,
            inserted_at_unix_ms: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), key.to_owned()),
                (value, inserted_at_unix_ms),
            );
            Ok(())
        }
    }

    /// Non-verifying CAS stub (accepts any claimed_hash) — `npm::router` wires
    /// the production `canonical_hash_hex`, which a verifying in-memory handler
    /// would reject. Keyed by `(namespace, hash)`.
    #[derive(Debug, Default)]
    struct StubCas(Mutex<HashMap<(String, String), Vec<u8>>>);
    impl CasReadHandler for StubCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .0
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
                None => Err(CasHandlerError::Internal("stub: absent".into())),
            }
        }
    }
    impl CasWriteHandler for StubCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            self.0
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    /// CAS fake that enforces the cap carried by each write request. This
    /// exercises the same reserve boundary the production accounting
    /// decorator consumes, rather than merely recording that a resolver was
    /// called.
    #[derive(Debug, Default)]
    struct CapCas {
        objects: Mutex<HashMap<(String, String), Vec<u8>>>,
        used: Mutex<HashMap<String, u64>>,
        writes: Mutex<Vec<(String, Option<i64>, usize)>>,
    }
    impl CasReadHandler for CapCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .objects
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(bytes) => Ok(CasReadResponse::new(bytes.clone(), req.hash)),
                None => Err(CasHandlerError::NotFound {
                    tenant: req.tenant,
                    hash: req.hash,
                }),
            }
        }
    }
    impl CasWriteHandler for CapCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            let cap = req.storage_quota_bytes.ok_or_else(|| {
                CasHandlerError::Internal("cap indeterminate: refusing write".into())
            })?;
            if cap < 0 {
                return Err(CasHandlerError::Internal("negative storage cap".into()));
            }
            let len = req.bytes.len();
            let mut used = self.used.lock().unwrap();
            let current = used.get(&req.accounting_tenant).copied().unwrap_or(0);
            let next = current.saturating_add(len as u64);
            if cap > 0 && next > cap as u64 {
                return Err(CasHandlerError::Internal("storage cap exceeded".into()));
            }
            used.insert(req.accounting_tenant.clone(), next);
            self.writes
                .lock()
                .unwrap()
                .push((req.accounting_tenant.clone(), Some(cap), len));
            self.objects
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    #[derive(Debug, Default)]
    struct StubCapResolver {
        cap: Mutex<Option<i64>>,
        requested: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl TenantCapResolver for StubCapResolver {
        async fn resolve_storage_cap(&self, tenant: &str) -> Option<i64> {
            self.requested.lock().unwrap().push(tenant.to_owned());
            *self.cap.lock().unwrap()
        }
    }

    fn npm_store_with_cap(
        cas: Arc<CapCas>,
        resolver: Arc<StubCapResolver>,
    ) -> (NpmMoatStore, Arc<FakeMap>) {
        let map = Arc::new(FakeMap::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::clone(&map) as Arc<dyn UrlMapStore>,
            NPM_SERVICE_PRINCIPAL,
        ));
        (
            NpmMoatStore {
                moat,
                cap_resolver: resolver,
            },
            map,
        )
    }

    fn npm_digest(byte: u8) -> Digest {
        Digest::from_bytes([byte; 32])
    }

    #[tokio::test]
    async fn npm_put_threads_resolved_cap_and_keeps_tenant_namespace() {
        let cas = Arc::new(CapCas::default());
        let resolver = Arc::new(StubCapResolver {
            cap: Mutex::new(Some(8)),
            requested: Mutex::new(Vec::new()),
        });
        let (store, map) = npm_store_with_cap(Arc::clone(&cas), Arc::clone(&resolver));
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xB110));

        CasStore::put(&store, &tenant, &npm_digest(1), b"four".to_vec())
            .await
            .unwrap();

        assert_eq!(
            resolver.requested.lock().unwrap().as_slice(),
            &[tenant.to_string()]
        );
        assert_eq!(
            cas.writes.lock().unwrap().as_slice(),
            &[(tenant.to_string(), Some(8), 4)]
        );
        let rows = map.0.lock().unwrap();
        assert!(rows.keys().all(|(namespace, _)| namespace == &tenant.to_string()));
        assert!(!rows.keys().any(|(namespace, _)| namespace == PUBLIC_NAMESPACE));
    }

    #[tokio::test]
    async fn npm_put_rejects_bytes_over_current_resolved_cap() {
        let cas = Arc::new(CapCas::default());
        let resolver = Arc::new(StubCapResolver {
            cap: Mutex::new(Some(4)),
            requested: Mutex::new(Vec::new()),
        });
        let (store, map) = npm_store_with_cap(Arc::clone(&cas), Arc::clone(&resolver));
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xB111));

        CasStore::put(&store, &tenant, &npm_digest(1), b"four".to_vec())
            .await
            .unwrap();
        let rejected = CasStore::put(&store, &tenant, &npm_digest(2), b"x".to_vec()).await;

        assert!(rejected.is_err(), "the second write must exceed the 4-byte cap");
        assert_eq!(cas.writes.lock().unwrap().len(), 1);
        assert_eq!(map.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn npm_put_uses_downgraded_cap_instead_of_stale_seed() {
        let cas = Arc::new(CapCas::default());
        let resolver = Arc::new(StubCapResolver {
            cap: Mutex::new(Some(10)),
            requested: Mutex::new(Vec::new()),
        });
        let (store, map) = npm_store_with_cap(Arc::clone(&cas), Arc::clone(&resolver));
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xB112));

        CasStore::put(&store, &tenant, &npm_digest(1), b"123456".to_vec())
            .await
            .unwrap();
        *resolver.cap.lock().unwrap() = Some(5);
        let rejected = CasStore::put(&store, &tenant, &npm_digest(2), b"x".to_vec()).await;

        assert!(rejected.is_err(), "the downgraded 5-byte cap must apply now");
        assert_eq!(cas.writes.lock().unwrap().len(), 1);
        assert_eq!(resolver.requested.lock().unwrap().len(), 2);
        assert_eq!(map.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn npm_put_fails_closed_when_cap_is_missing() {
        let cas = Arc::new(CapCas::default());
        let resolver = Arc::new(StubCapResolver {
            cap: Mutex::new(None),
            requested: Mutex::new(Vec::new()),
        });
        let (store, map) = npm_store_with_cap(Arc::clone(&cas), resolver);
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xB113));

        let rejected = CasStore::put(&store, &tenant, &npm_digest(1), b"x".to_vec()).await;

        assert!(rejected.is_err(), "indeterminate cap must fail closed");
        assert!(cas.writes.lock().unwrap().is_empty());
        assert!(map.0.lock().unwrap().is_empty());
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    fn npm_kv(backend: Arc<dyn NpmKvBackend>) -> Arc<NpmKvStore> {
        Arc::new(NpmKvStore::new(backend))
    }

    /// Router whose verifier rejects ALL PATs (empty lookup); cas/map/kv unused.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(Arc::new(FakeKv::default())),
            verifier,
            None,
        )
    }

    fn get(uri: &str, pat: Option<&str>, scope: Option<&str>) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder().method(Method::GET).uri(uri);
        if let Some(p) = pat {
            b = b.header("authorization", format!("Bearer {p}"));
        }
        if let Some(s) = scope {
            b = b.header(SCOPE_HEADER, s);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn missing_scope_is_403_at_the_gate() {
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/npm/t/lodash", Some("corelink_whatever"), None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unmapped_methods_are_403_even_with_rw_scope() {
        // Fail-CLOSED method gate: DELETE/PATCH carry a FULL cas:rw scope but
        // are not a mapped cache operation, so the gate must deny them (403)
        // rather than fall through to downstream routing.
        for method in [Method::DELETE, Method::PATCH] {
            let app = router_rejecting();
            let req = HttpRequest::builder()
                .method(method.clone())
                .uri("/npm/t/lodash")
                .header("authorization", "Bearer corelink_whatever")
                .header(SCOPE_HEADER, SCOPE_RW)
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "{method} with cas:rw must fail closed at the gate"
            );
        }
    }

    #[tokio::test]
    async fn missing_pat_is_401_reaches_adapter() {
        // scope present → gate passes → adapter authenticate() fails → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/npm/t/lodash", None, Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_prefix_pat_is_401() {
        // After the prefix-fix, npm's extract_pat enforces the `corelink_`
        // prefix, so a `ghp_`-style token is rejected at the adapter → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/npm/t/lodash", Some("ghp_github"), Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_pat_is_401_resolver_runs() {
        // corelink_-prefixed but HMAC-invalid → reaches the resolver (proves
        // nest_service routed to the adapter) → InvalidPat → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get(
                "/npm/t/lodash",
                Some("corelink_not-a-real-token"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Router whose verifier rejects ALL PATs but whose per-tenant `$`-ceiling
    /// quota gate is ACTIVE (hermetic in-memory store + fake clock). Used to
    /// prove the gate's cost-attribution does NOT fall open when no tenant id
    /// is available (REV-S3).
    fn router_with_quota() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        let store = Arc::new(crate::tenant_quota::InMemoryQuotaStore::new());
        let clock = Arc::new(crate::wall_clock::InMemoryFakeWallClock::at_unix_ms(
            1_700_000_000_000,
        ));
        let guard = Arc::new(crate::tenant_quota::QuotaGuard::new(store, clock));
        // $1/op flat cost — a fresh tenant (under the $5 tripwire) would be
        // ADMITTED, so a 503 here is unambiguously the no-tenant fail-CLOSED
        // path, not an over-ceiling 402.
        let gate = crate::routes::QuotaGate::new_for_test(guard, 1_000_000);
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(Arc::new(FakeKv::default())),
            verifier,
            Some(gate),
        )
    }

    #[tokio::test]
    async fn quota_gate_without_tenant_header_fails_closed_not_skipped() {
        // REV-S3 regression: a billable op (scope-valid GET) that reaches an
        // ACTIVE quota gate with NO `x-corelink-tenant-id` and no PAT-resolved
        // tenant must FAIL CLOSED (503) — the prior code silently skipped the
        // charge (fail-OPEN), an unmetered $-ceiling bypass.
        let app = router_with_quota();
        let resp = app
            .oneshot(get(
                "/npm/t/lodash",
                Some("corelink_whatever"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "no-tenant billable op must fail closed (503), not skip the charge"
        );
    }

    #[tokio::test]
    async fn metadata_cache_hit_round_trip_with_tenant_stripped() {
        // Mint a real PAT; seed the metadata KV (PUBLIC namespace, unscoped
        // package) so a GET /<pkg> is a cache HIT (no upstream). Proves
        // end-to-end: nest_service mount + tenant-strip + scope + Option-B
        // resolve + KV get + JSON passthrough.
        let key = test_key();
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(Uuid::from_u128(0xBEEF)),
            PrincipalId(Uuid::from_u128(0xF00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
                find_only: false,
                runner_job: false,
            },

include!("fragment-tests-01.rs");

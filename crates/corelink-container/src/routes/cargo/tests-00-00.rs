
    use std::collections::HashMap;
    use std::sync::Mutex;

    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };

    use crate::oci_cap::TenantCapResolver;

    use super::*;

    /// Hermetic in-memory url→content-hash map (the `MoatCache` map port).
    #[derive(Default)]
    struct FakeUrlMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeUrlMap {
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
        async fn delete(&self, ns: &str, url_hash: &str) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .remove(&(ns.to_owned(), url_hash.to_owned()));
            Ok(())
        }
    }

    /// `CasWriteHandler` that RECORDS the `storage_quota_bytes` of the last
    /// write request (proving the resolved cap is threaded all the way into
    /// `CasWriteRequest`), and accepts any claimed hash.
    #[derive(Debug, Default)]
    struct RecordingCasWrite {
        last_cap: Mutex<Option<Option<i64>>>,
    }
    impl CasWriteHandler for RecordingCasWrite {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            *self.last_cap.lock().unwrap() = Some(req.storage_quota_bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }
    impl CasReadHandler for RecordingCasWrite {
        fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            Err(CasHandlerError::Internal("not used".into()))
        }
    }

    /// `TenantCapResolver` that returns a FIXED cap for the configured tenant
    /// and `None` (indeterminate) for everyone else.
    #[derive(Debug)]
    struct StubCapResolver {
        tenant: String,
        cap: Option<i64>,
    }
    #[async_trait]
    impl TenantCapResolver for StubCapResolver {
        async fn resolve_storage_cap(&self, tenant_id: &str) -> Option<i64> {
            if tenant_id == self.tenant {
                self.cap
            } else {
                None
            }
        }
    }

    /// A resolver that panics if a routed request performs the old per-PUT D1
    /// lookup. This keeps the latency fix adversarial: forwarding the cap must
    /// bypass the resolver entirely.
    #[derive(Debug)]
    struct PanicCapResolver;
    #[async_trait]
    impl TenantCapResolver for PanicCapResolver {
        async fn resolve_storage_cap(&self, _tenant_id: &str) -> Option<i64> {
            panic!("forwarded cap must bypass the per-PUT D1 tier lookup");
        }
    }

    fn store_with_resolver(
        cap_resolver: Option<Arc<dyn TenantCapResolver>>,
    ) -> (CargoMoatStore, Arc<RecordingCasWrite>) {
        let rec = Arc::new(RecordingCasWrite::default());
        let read: Arc<dyn CasReadHandler> = rec.clone();
        let write: Arc<dyn CasWriteHandler> = rec.clone();
        let map: Arc<dyn UrlMapStore> = Arc::new(FakeUrlMap::default());
        let moat = Arc::new(MoatCache::production(
            read,
            write,
            map,
            CARGO_SERVICE_PRINCIPAL,
        ));
        (CargoMoatStore { moat, cap_resolver }, rec)
    }

    /// THE fix: a fresh tenant's cargo write carries the RESOLVED per-tier cap
    /// (so the byte-accounting reservation seeds the `tenant_storage_state` row
    /// instead of failing closed 502 on the indeterminate `None`).
    #[tokio::test]
    async fn put_threads_resolved_cap_into_cas_write() {
        let tenant = "fresh-tenant-xyz";
        let resolver: Arc<dyn TenantCapResolver> = Arc::new(StubCapResolver {
            tenant: tenant.to_owned(),
            cap: Some(50 * 1_073_741_824), // solo tier cap
        });
        let (store, rec) = store_with_resolver(Some(resolver));

        store
            .put(tenant, "url-key-1", b"sccache-artifact".to_vec())
            .await
            .expect("put must succeed");

        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded,
            Some(50 * 1_073_741_824),
            "the resolved per-tier cap must reach CasWriteRequest::storage_quota_bytes \
             so a fresh tenant_storage_state row seeds (no 502)"
        );
    }

    /// With NO resolver wired (dev/CI), the cap stays `None` — the previous
    /// fail-closed-on-fresh-row posture is preserved.
    #[tokio::test]
    async fn put_with_no_resolver_keeps_none_cap() {
        let (store, rec) = store_with_resolver(None);
        store
            .put("any-tenant", "url-key-2", b"x".to_vec())
            .await
            .expect("put must succeed");
        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded, None,
            "no resolver ⇒ None cap (fail-closed posture)"
        );
    }

    /// An INDETERMINATE cap from the resolver (D1 error → `None`) is threaded as
    /// `None` — absence is never upgraded to unlimited.
    #[tokio::test]
    async fn put_with_indeterminate_cap_threads_none() {
        // The resolver only knows "known-tenant"; everyone else ⇒ None.
        let resolver: Arc<dyn TenantCapResolver> = Arc::new(StubCapResolver {
            tenant: "known-tenant".to_owned(),
            cap: Some(10 * 1_073_741_824),
        });
        let (store, rec) = store_with_resolver(Some(resolver));
        store
            .put("unknown-tenant", "url-key-3", b"y".to_vec())
            .await
            .expect("put must succeed");
        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded, None,
            "indeterminate resolver cap must stay None (never unlimited)"
        );
    }

    /// A forwarded Worker cap skips the resolver, which otherwise performs the
    /// latency inducing D1 tier lookup on every cargo PUT.
    #[tokio::test]
    async fn forwarded_cap_skips_per_put_tier_lookup() {
        let resolver: Arc<dyn TenantCapResolver> = Arc::new(PanicCapResolver);
        let (store, rec) = store_with_resolver(Some(resolver));
        let cap = Some(50 * 1_073_741_824);

        CARGO_STORAGE_QUOTA_CAP
            .scope(cap, store.put("tenant-abc", "url-key-forwarded", b"x".to_vec()))
            .await
            .expect("put must succeed without consulting the D1 resolver");

        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(recorded, cap);
    }

    // ---- cargo_gate: WebDAV MKCOL no-op (sccache real-client fix) --------------

    /// A `TenantResolver` that MUST NOT be called: MKCOL short-circuits in the
    /// gate before any PAT verification, so reaching the resolver is a bug.
    #[derive(Debug)]
    struct UnusedResolver;
    #[async_trait]
    impl TenantResolver for UnusedResolver {
        async fn resolve(&self, _pat_plaintext: &str) -> Result<String, TenantResolveError> {
            panic!("MKCOL must short-circuit before the resolver is ever called");
        }
    }

    /// Router carrying ONLY the `cargo_gate` layer over a fallback that 200s if
    /// reached. MKCOL requests short-circuit in the gate and never hit the
    /// fallback, so this exercises `cargo_gate` in isolation (no adapter/CAS).
    fn gate_only_router() -> Router {
        use axum::routing::any;
        let resolver: SharedTenantResolver = Arc::new(UnusedResolver);
        let state = CargoGateState {
            quota: None,
            resolver,
            staging_admission: None,
            // MKCOL short-circuits before any moat access; a real (unused) store
            // keeps the state well-formed.
            moat: in_memory_moat(),
        };
        Router::new()
            .fallback(any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(state, cargo_gate))
    }

    /// A hermetic [`MoatCache`] over an in-memory CAS + fake map, wired with
    /// `fake_hash` (matching `InMemoryCasHandler`'s verification). Seed it via
    /// `moat.put(tenant, key, bytes, None)` and read it via `moat.get`.
    fn in_memory_moat() -> Arc<MoatCache> {
        use corelink_handler_cas::handler::fake_hash;
        use corelink_handler_cas::{InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver};
        let cas = Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ));
        let map: Arc<dyn UrlMapStore> = Arc::new(FakeUrlMap::default());
        Arc::new(MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            CARGO_SERVICE_PRINCIPAL,
        ))
    }

    fn mkcol_request(scope: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method(Method::from_bytes(b"MKCOL").unwrap())
            .uri("/cargo/tenant-abc/6/b/4")
            .header(SCOPE_HEADER, scope)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    /// MKCOL + a cache-WRITE scope → 201 CREATED (success no-op). This is the
    /// exact path opendal issues before PUTting a sharded key; without it the
    /// real `sccache` binary can never write.
    #[tokio::test]
    async fn mkcol_with_write_scope_is_201_noop() {
        use tower::ServiceExt;
        let resp = gate_only_router()
            .oneshot(mkcol_request("cas:rw"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "MKCOL with cache-write scope must succeed as a directory-creation no-op"
        );
    }

    /// MKCOL + a read-only scope → 403 FORBIDDEN. MKCOL is part of a write flow,
    /// so the surface stays fail-closed on a credential that lacks write.
    #[tokio::test]
    async fn mkcol_with_readonly_scope_is_403() {
        use tower::ServiceExt;
        let resp = gate_only_router()
            .oneshot(mkcol_request("cas:r"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "MKCOL with a read-only scope must be denied (still gated on cache-write)"
        );
    }

    // ---- cargo_gate: WebDAV PROPFIND + DELETE (real-sccache/opendal fix) --------

    /// A resolver that resolves ANY PAT to a fixed tenant with a fixed
    /// write-capability (so the PROPFIND read path + DELETE F27 path reach the
    /// moat). Distinct from `UnusedResolver`, which panics.
    #[derive(Debug)]
    struct FixedTenantResolver {
        tenant: String,
        can_write: bool,
        /// The 0086 narrowing marker. `false` for every case that predates the
        /// runner-job containment; the DELETE cells set it explicitly.
        runner_job: bool,
    }
    #[async_trait]
    impl TenantResolver for FixedTenantResolver {
        async fn resolve(&self, _pat: &str) -> Result<String, TenantResolveError> {
            Ok(self.tenant.clone())
        }
        async fn resolve_with_capability(
            &self,
            _pat: &str,
        ) -> Result<ResolvedTenant, TenantResolveError> {
            Ok(ResolvedTenant {
                tenant_id: self.tenant.clone(),
                can_write: self.can_write,
                runner_job: self.runner_job,
            })
        }
    }

    /// Router carrying the `cargo_gate` over the given moat + resolver. GET/PUT/
    /// HEAD would hit the 200 fallback; PROPFIND/DELETE short-circuit in the gate.
    fn webdav_router(moat: Arc<MoatCache>, tenant: &str, can_write: bool) -> Router {
        webdav_router_marked(moat, tenant, can_write, false)
    }

    /// [`webdav_router`] with the 0086 runner-job marker set explicitly, for the
    /// containment cells.
    fn webdav_router_marked(
        moat: Arc<MoatCache>,
        tenant: &str,
        can_write: bool,
        runner_job: bool,
    ) -> Router {
        use axum::routing::any;
        let resolver: SharedTenantResolver = Arc::new(FixedTenantResolver {
            tenant: tenant.to_owned(),
            can_write,
            runner_job,
        });
        let state = CargoGateState {
            quota: None,
            resolver,
            staging_admission: None,
            moat,
        };
        Router::new()
            .fallback(any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(state, cargo_gate))
    }

    #[tokio::test]
    async fn cargo_gate_scopes_forwarded_storage_cap_for_downstream_put() {
        use axum::routing::any;
        use tower::ServiceExt;

        let resolver: SharedTenantResolver = Arc::new(FixedTenantResolver {
            tenant: "tenant-abc".to_owned(),
            can_write: true,
            runner_job: false,
        });
        let state = CargoGateState {
            quota: None,
            resolver,
            staging_admission: None,
            moat: in_memory_moat(),
        };
        let app = Router::new()
            .fallback(any(|| async {
                match CARGO_STORAGE_QUOTA_CAP.try_with(|cap| *cap).ok().flatten() {
                    Some(123) => StatusCode::OK,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                }
            }))
            .layer(middleware::from_fn_with_state(state, cargo_gate));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::PUT)
                    .uri("/cargo/tenant-abc/key")
                    .header(SCOPE_HEADER, "cas:rw")
                    .header(axum::http::header::AUTHORIZATION, "Bearer pat")
                    .header(crate::byte_accounting::STORAGE_QUOTA_HEADER, "123")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "cargo_gate must propagate the Worker cap into the downstream PUT scope"
        );
    }

    fn webdav_request(
        method: &[u8],
        uri: &str,
        scope: &str,
        bearer: Option<&str>,
    ) -> axum::http::Request<axum::body::Body> {
        let mut b = axum::http::Request::builder()
            .method(Method::from_bytes(method).unwrap())
            .uri(uri)
            .header(SCOPE_HEADER, scope);
        if let Some(t) = bearer {
            b = b.header(axum::http::header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    async fn body_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// PROPFIND on an EXISTING key → `207` with the right `getcontentlength` and
    /// a `200 OK` propstat (the shape opendal's stat parser accepts).
    #[tokio::test]
    async fn propfind_existing_key_is_207_with_size() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        let bytes = b"sccache-artifact".to_vec(); // 16 bytes
        moat.put(tenant, "abc123object", bytes.clone(), None)
            .await
            .unwrap();
        let app = webdav_router(Arc::clone(&moat), tenant, true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/abc123object",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::MULTI_STATUS);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/xml")
        );
        let body = body_string(resp).await;
        assert!(
            body.contains("<D:getcontentlength>16</D:getcontentlength>"),
            "must report the stored blob's real byte length; got: {body}"
        );
        assert!(
            body.contains("HTTP/1.1 200 OK"),
            "must carry a 200 OK propstat; got: {body}"
        );
        assert!(
            body.contains("/cargo/tenant-abc/abc123object"),
            "href must echo the request path; got: {body}"
        );
        // REGRESSION LOCK: opendal's WebDAV stat deserializer treats
        // <D:getlastmodified> as REQUIRED — a 207 without it fails "missing field
        // getlastmodified", so the real sccache binary flags storage ReadOnly and
        // never writes (invisible to a 207-status check). Empirically proven.
        assert!(
            body.contains("<D:getlastmodified>"),
            "207 MUST carry <D:getlastmodified> or opendal/sccache treats storage as ReadOnly; got: {body}"
        );
    }

    /// PROPFIND on an ABSENT key → `404` (opendal treats it as not-found and
    /// proceeds to write).
    #[tokio::test]
    async fn propfind_absent_key_is_404() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/never-written",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// PROPFIND with a read-ONLY scope is ALLOWED (it is a read) — reaches the
    /// moat and returns `207` for an existing key.
    #[tokio::test]
    async fn propfind_readonly_scope_is_allowed() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, "roobject", b"z".to_vec(), None)
            .await
            .unwrap();
        let app = webdav_router(Arc::clone(&moat), tenant, false);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/roobject",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MULTI_STATUS,
            "PROPFIND is a read — a read-only scope must be sufficient"
        );
    }

    /// PROPFIND with NO cache scope → `403` (fail-closed, before any lookup).
    #[tokio::test]
    async fn propfind_without_scope_is_403() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/whatever",
                "",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }


include!("fragment-tests-00-00-01.rs");

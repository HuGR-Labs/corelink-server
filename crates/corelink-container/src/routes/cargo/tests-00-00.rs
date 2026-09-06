
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
            moat,
        };
        Router::new()
            .fallback(any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(state, cargo_gate))
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

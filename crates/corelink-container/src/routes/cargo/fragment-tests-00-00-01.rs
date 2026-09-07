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

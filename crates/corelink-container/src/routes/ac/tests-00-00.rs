
    use super::*;
    use corelink_handler_ac::{AuditEventKind, Sli};

    /// A canonical 64-lowercase-hex action digest for router-level tests — the
    /// HTTP handlers now reject non-canonical digests with 400 (CAA-360 #9), so
    /// router tests must use a realistic digest. (Store-level tests call the
    /// handler trait directly, bypass the route gate, and keep their short stubs.)
    const VALID_DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn is_canonical_digest_accepts_only_64_lowercase_hex() {
        // CAA-360 #9: the digest gate must accept exactly 64 lowercase hex and
        // reject everything else BEFORE it can become an R2 key.
        assert!(is_canonical_digest(&"0123456789abcdef".repeat(4))); // 64 lc hex
        assert!(is_canonical_digest(&"a".repeat(64)));
        assert!(!is_canonical_digest(&"a".repeat(63)), "too short");
        assert!(!is_canonical_digest(&"a".repeat(65)), "too long");
        assert!(
            !is_canonical_digest(&"A".repeat(64)),
            "uppercase not canonical"
        );
        assert!(!is_canonical_digest(&"g".repeat(64)), "non-hex char");
        assert!(
            !is_canonical_digest("../../../etc/passwd"),
            "path traversal"
        );
        assert!(!is_canonical_digest(""), "empty");
    }

    /// `map_err` must map an `Internal` error to 503 ONLY when it carries the
    /// storage-unavailable sentinel; any other `Internal` is a generic 500.
    /// Kills the cargo-mutants "replace match guard with true" mutant on the
    /// `if msg.starts_with(STORAGE_UNAVAILABLE_SENTINEL)` guard — with the guard
    /// forced to `true`, the non-sentinel case below would wrongly become 503.
    #[test]
    fn map_err_internal_is_503_only_for_storage_sentinel() {
        let storage = map_err(AcHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2_TDK_HEX unset"
        )));
        assert_eq!(
            storage.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "sentinel-tagged Internal must be 503"
        );

        let generic = map_err(AcHandlerError::Internal(
            "lock poisoned: unrelated failure".to_string(),
        ));
        assert_eq!(
            generic.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "non-sentinel Internal must be 500, not 503 (guard must not be `true`)"
        );
    }

    /// Test fixture: returns the route state plus the underlying
    /// audit + SLI sinks so each test can verify emit ordering.
    fn fixture() -> (
        Arc<InMemoryAuditSink>,
        Arc<InMemorySliObserver>,
        AcRouteState,
    ) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared = Arc::new(InMemoryAcHandler::new(audit.clone(), sli.clone()));
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared.clone();
        let delete: Arc<dyn AcDeleteHandler> = shared.clone();
        let list: Arc<dyn AcListHandler> = shared;
        (
            audit,
            sli,
            AcRouteState {
                lookup,
                update,
                delete,
                list,
                quota: None,
                pat_gate: None,
                put_inflight: Arc::new(Mutex::new(HashMap::new())),
            },
        )
    }

    /// Route state backed by the fail-CLOSED [`UnavailableAcHandler`] — the
    /// shape `build_handlers` returns when storage creds are present but the R2
    /// handler refuses to build (F1: `R2_TDK_HEX` unset/invalid).
    fn fixture_unavailable() -> AcRouteState {
        let shared = Arc::new(UnavailableAcHandler);
        let lookup: Arc<dyn AcLookupHandler> = shared.clone();
        let update: Arc<dyn AcUpdateHandler> = shared.clone();
        let delete: Arc<dyn AcDeleteHandler> = shared.clone();
        let list: Arc<dyn AcListHandler> = shared;
        AcRouteState {
            lookup,
            update,
            delete,
            list,
            quota: None,
            pat_gate: None,
            put_inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[test]
    fn route_constants_match_canonical_path() {
        assert_eq!(AC_LOOKUP_ROUTE, "/v1/ac/{tenant}/{action_digest}");
        assert_eq!(AC_UPDATE_ROUTE, "/v1/ac/{tenant}/{action_digest}");
    }

    /// F1 (CAA-360) fail-CLOSED: the `UnavailableAcHandler`'s error maps to
    /// HTTP 503 "storage unavailable" (NOT the generic 500) so a forgotten
    /// `R2_TDK_HEX` is loud, not a silent non-durable cache.
    #[test]
    fn unavailable_handler_maps_to_503() {
        let err = UnavailableAcHandler
            .lookup(AcLookupRequest::new("t1", "d1", "anon@t1", "t1", 0))
            .expect_err("unavailable");
        assert!(matches!(err, AcHandlerError::Internal(_)));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn build_handlers_returns_usable_pair() {
        let (lookup, _update, _delete, _list) = build_handlers();
        let res = lookup.lookup(AcLookupRequest::new("t1", "d1", "anon", "t1", 0));
        assert!(matches!(res, Err(AcHandlerError::Miss { .. })));
        // Router constructor smoke.
        let (_a, _s, st) = fixture();
        let _router = router(st);
    }

    /// Happy path: PUT then GET round-trip emits audit + Sli on both
    /// legs and returns the stored bytes.
    #[test]
    fn put_then_get_round_trip_emits_audit_and_sli() {
        let (audit, sli, st) = fixture();
        // Update path.
        let upd_req = AcUpdateRequest::new("t1", "d1", b"result".to_vec(), "anon@t1", "t1", 1);
        let upd_resp = st.update.update(upd_req).expect("update");
        assert!(upd_resp.durable);
        // Lookup path.
        let lk_req = AcLookupRequest::new("t1", "d1", "anon@t1", "t1", 2);
        let lk_resp = st.lookup.lookup(lk_req).expect("lookup hit");
        assert_eq!(lk_resp.action_digest, "d1");
        assert_eq!(lk_resp.result_payload, b"result".to_vec());
        // Audit rows: UpdateAttempted + UpdateCommitted + LookupAttempted + LookupHit.
        let rows = audit.snapshot().expect("audit");
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::UpdateAttempted));
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::UpdateCommitted));
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::LookupAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupHit));
        // SLI: at least one AvailAcLookup + one LatencyAcHitP99 non-error.
        let obs = sli.snapshot().expect("sli");
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::AvailAcLookup && !o.is_error));
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::LatencyAcHitP99 && !o.is_error));
    }

    /// Cross-tenant lookup MUST emit `LookupDenied` BEFORE the error.
    /// Pins `INV-TENANT-ISOLATION` at the route boundary.
    #[test]
    fn cross_tenant_lookup_denied_audits_before_rejection() {
        let (audit, _sli, st) = fixture();
        let req = AcLookupRequest::new("victim", "d1", "attacker@attacker_t", "attacker_t", 1);
        let err = st.lookup.lookup(req).expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        // First row MUST be LookupDenied (fail-CLOSED ordering pin).
        assert_eq!(rows[0].kind, AuditEventKind::LookupDenied);
        assert_eq!(rows[0].tenant, "victim");
        assert_eq!(rows[0].principal, "attacker@attacker_t");
    }

    /// Audit-fail aborts the update mutation (fail-CLOSED). Verifies
    /// the auth/audit-fail handling path used by `map_err` returns
    /// 503 semantically through the handler error variant.
    #[test]
    fn audit_failure_aborts_update_and_maps_to_audit_closed() {
        let (audit, _sli, st) = fixture();
        audit.inject_failure("audit pipeline down").expect("inject");
        let upd = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1);
        let err = st.update.update(upd).expect_err("audit closed");
        assert!(matches!(err, AcHandlerError::AuditFailed(_)));
        // Verify the route-layer `map_err` translates this to 503.
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Idempotent retry: a second PUT with the same bytes does NOT
    /// re-create the entry (`durable=false`) and audit + Sli still
    /// fire on the retry — pins R-prep retry idempotency.
    #[test]
    fn idempotent_retry_keeps_state_stable() {
        let (audit, sli, st) = fixture();
        let req1 = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 1);
        let r1 = st.update.update(req1).expect("first put");
        assert!(r1.durable, "fresh insert");
        let req2 = AcUpdateRequest::new("t1", "d1", b"r".to_vec(), "anon@t1", "t1", 2);
        let r2 = st.update.update(req2).expect("retry put");
        assert!(!r2.durable, "retry MUST NOT be durable=true");
        // Audit rows: two UpdateAttempted + two UpdateCommitted.
        let rows = audit.snapshot().expect("audit");
        let attempts = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateAttempted)
            .count();
        let commits = rows
            .iter()
            .filter(|r| r.kind == AuditEventKind::UpdateCommitted)
            .count();
        assert_eq!(attempts, 2);
        assert_eq!(commits, 2);
        // SLI emit on every entry.
        let obs_count = sli.count(Sli::AvailAcLookup).expect("count");
        assert!(obs_count >= 2);
    }

    /// Auth-fail surrogate: a missing-entry lookup returns Miss; the
    /// route layer maps Miss to HTTP 404. Tenant binding is now done by
    /// the `crate::auth_tenant::AuthTenant` extractor on the handlers
    /// (path `:tenant` must equal the authenticated tenant else 403);
    /// the remaining auth-fail surrogate here is the cross-tenant
    /// denial above.
    #[test]
    fn miss_maps_to_404() {
        let (_audit, _sli, st) = fixture();
        let lk = AcLookupRequest::new("t1", "ghost", "anon@t1", "t1", 1);
        let err = st.lookup.lookup(lk).expect_err("miss");
        assert!(matches!(err, AcHandlerError::Miss { .. }));
        let resp = map_err(err);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── scope enforcement (x-corelink-scope) ─────────────────────────────────

    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt; // for `.oneshot()`

    /// Authenticated tenant injected via `x-corelink-tenant-id`; the scope
    /// header then gates lookup (read) vs update (write).
    const TEST_TENANT: &str = "t1";

    /// A `cas:rw` AC update succeeds (current prod scope — happy path).
    #[tokio::test]
    async fn update_with_rw_scope_succeeds() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// cluster F: with the tenant AT `AC_WRITE_CONCURRENCY_LIMIT` in-flight
    /// writes, the next AC update is rejected 429 BEFORE its body is buffered —
    /// the `AcPutGuard` `FromRequestParts` extractor runs ahead of `body: Bytes`.
    /// Proven deterministically with a body LARGER than the 10 MiB global limit.
    #[tokio::test]
    async fn ac_write_at_concurrency_limit_returns_429_before_body() {
        let (_a, _s, st) = fixture();
        {
            let mut g = st.put_inflight.lock().expect("lock");
            g.insert(TEST_TENANT.to_owned(), AC_WRITE_CONCURRENCY_LIMIT);
        }
        let app = router(st);
        let oversized = Body::from(vec![0u8; 11 * 1024 * 1024]); // > 10 MiB
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(oversized)
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an at-limit native AC write must be rejected 429 by the pre-body concurrency guard"
        );
    }

    /// An AC update BELOW the limit succeeds and releases its slot.
    #[tokio::test]
    async fn ac_write_below_limit_releases_slot() {
        let (_a, _s, st) = fixture();
        let inflight = st.put_inflight.clone();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let g = inflight.lock().expect("lock");
        assert_eq!(
            g.get(TEST_TENANT),
            None,
            "the concurrency slot must be released after the AC write completes"
        );
    }

    /// A `cas:r` (read-only) AC update is rejected 403 "insufficient scope".
    #[tokio::test]
    async fn update_with_read_only_scope_returns_403_insufficient_scope() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"insufficient scope");
    }

    /// F1 (CAA-360) end-to-end: an in-scope AC lookup against the fail-CLOSED
    /// `UnavailableAcHandler` (creds present, `R2_TDK_HEX` missing) returns
    /// HTTP 503 — the cache refuses to serve, not a silent miss from a
    /// non-durable InMemory fallback.
    #[tokio::test]
    async fn lookup_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// F1 (CAA-360) end-to-end: an in-scope AC update against the fail-CLOSED
    /// `UnavailableAcHandler` returns 503 — updates are refused (never a silent
    /// non-durable commit) until `R2_TDK_HEX` is set.
    #[tokio::test]
    async fn update_on_unavailable_handler_returns_503() {
        let app = router(fixture_unavailable());
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:rw")
            .body(Body::from(b"result".to_vec()))
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A `cas:r` AC lookup passes the scope gate; the entry is absent so the
    /// route returns 404 (a denied scope would 403 before storage).
    #[tokio::test]
    async fn lookup_with_read_only_scope_passes_gate_then_404() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Fail-CLOSED: an AC lookup with NO scope header is rejected 403.
    #[tokio::test]
    async fn lookup_with_missing_scope_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── D-1 delete + D-7 list ────────────────────────────────────────────────

    #[test]
    fn list_route_constant_uses_axum_0_8_brace_syntax() {
        // axum 0.8 / matchit 0.8 use `{name}` captures; the legacy `:name`
        // form is now a literal path segment, so pin brace-presence + colon
        // absence to prevent drift back to the pre-0.8 syntax.
        assert_eq!(AC_LIST_ROUTE, "/v1/ac/{tenant}");
        assert!(AC_LIST_ROUTE.contains("{tenant}"));
        assert!(!AC_LIST_ROUTE.contains(':'));
    }

    #[test]
    fn clamp_limit_enforces_contract_window() {
        assert_eq!(clamp_limit(None), 200);
        assert_eq!(clamp_limit(Some(0)), 200);
        assert_eq!(clamp_limit(Some(50)), 50);
        assert_eq!(clamp_limit(Some(1000)), 1000);
        assert_eq!(clamp_limit(Some(5000)), 1000);
    }

    /// DELETE an existing ref → 204; a repeated DELETE → 204 (idempotent).
    #[tokio::test]
    async fn delete_existing_then_repeat_both_204() {
        let (_a, _s, st) = fixture();
        // Seed a ref via the write path.
        st.update
            .update(AcUpdateRequest::new(
                TEST_TENANT,
                "d1",
                b"r".to_vec(),
                "anon@t1",
                TEST_TENANT,
                1,
            ))
            .expect("seed");
        let app = router(st);
        let del = || {
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
                .header("x-corelink-tenant-id", TEST_TENANT)
                .header(crate::scope::SCOPE_HEADER, "cas:rw")
                .body(Body::empty())
                .expect("request")
        };
        let resp = app.clone().oneshot(del()).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        // Repeat delete (now absent) MUST also be 204.
        let resp2 = app.oneshot(del()).await.expect("oneshot");
        assert_eq!(resp2.status(), StatusCode::NO_CONTENT);
    }

    /// DELETE with a read-only PAT → 403 (write capability required).
    #[tokio::test]
    async fn delete_with_read_only_scope_returns_403() {
        let (_a, _s, st) = fixture();
        let app = router(st);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/ac/{TEST_TENANT}/{VALID_DIGEST}"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .body(Body::empty())
            .expect("request");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }


include!("fragment-tests-00-00-01.rs");

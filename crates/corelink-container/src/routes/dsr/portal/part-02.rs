/// Clamp a Unix-ms value to `i64` (D1 stores INTEGER; the `ms_to_iso8601` helper
/// + the params take `i64`).
fn clamp_i64(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    const TENANT_A: &str = "00000000-0000-7000-8000-00000000000a";
    const TENANT_B: &str = "00000000-0000-7000-8000-00000000000b";

    /// A fake pipeline that records calls and returns canned successes.
    #[derive(Debug, Default)]
    struct FakePipeline {
        calls: std::sync::Mutex<Vec<String>>,
        rectify_reject: bool,
        access_err: bool,
    }

    impl DsrPipeline for FakePipeline {
        fn access(&self, tenant: &str, _dsr: &str, _now: u64) -> Result<Value, String> {
            self.calls.lock().unwrap().push(format!("access:{tenant}"));
            if self.access_err {
                return Err("boom".to_owned());
            }
            Ok(json!({ "tenant": tenant, "tables": [] }))
        }
        fn portability(
            &self,
            tenant: &str,
            _dsr: &str,
            _now: u64,
        ) -> Result<(Value, Option<String>), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("portability:{tenant}"));
            Ok((
                json!({ "tenant": tenant }),
                Some("dsr_exports/x.json".to_owned()),
            ))
        }
        fn rectification(
            &self,
            tenant: &str,
            _dsr: &str,
            _email: &str,
            _now: u64,
        ) -> Result<Result<(), String>, String> {
            self.calls.lock().unwrap().push(format!("rectify:{tenant}"));
            if self.rectify_reject {
                return Ok(Err("not a valid email".to_owned()));
            }
            Ok(Ok(()))
        }
        fn erasure(&self, tenant: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("erasure:{tenant}"));
            Ok(())
        }
    }

    fn state_with(pipeline: FakePipeline) -> (PrivacyDsrRouteState, Arc<InMemoryDsrTicketStore>) {
        let tickets = Arc::new(InMemoryDsrTicketStore::new());
        let state = PrivacyDsrRouteState {
            pipeline: Some(Arc::new(pipeline)),
            tickets: tickets.clone(),
            pat_gate: None,
            receipt_key: Arc::new(b"test-key-test-key-test-key-test-key".to_vec()),
        };
        (state, tickets)
    }

    fn clerk_headers(tenant: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-corelink-tenant-id", tenant.parse().unwrap());
        h.insert("x-corelink-token-prefix", "clerk".parse().unwrap());
        h
    }

    fn with_mfa(mut h: HeaderMap) -> HeaderMap {
        h.insert(MFA_VERIFIED_HEADER, "1".parse().unwrap());
        h
    }

    fn status_of(resp: &Response) -> StatusCode {
        resp.status()
    }

    #[tokio::test]
    async fn access_drives_pipeline_and_completes() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let listed = tickets.list(TENANT_A, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, TicketStatus::Completed);
        assert_eq!(listed[0].action, DsrRequestKind::Access);
    }

    #[tokio::test]
    async fn portability_sets_download_handle() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Portability,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Completed);
        assert_eq!(t.data_download_url.as_deref(), Some("dsr_exports/x.json"));
    }

    #[tokio::test]
    async fn erasure_without_mfa_is_pending() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Pending);
        assert!(t.mfa_required);
    }

    #[tokio::test]
    async fn erasure_with_mfa_drives_pipeline() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            with_mfa(clerk_headers(TENANT_A)),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Completed);
    }

    #[tokio::test]
    async fn rectification_reject_persists_rejected_ticket() {
        let pipe = FakePipeline {
            rectify_reject: true,
            ..Default::default()
        };
        let (state, tickets) = state_with(pipe);
        let body = DsrSubmitBody {
            reason: None,
            rectification: Some(RectificationBody {
                email: Some("bad".to_owned()),
            }),
        };
        let resp = submit(
            state,
            with_mfa(clerk_headers(TENANT_A)),
            DsrRequestKind::Rectification,
            body,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNPROCESSABLE_ENTITY);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Rejected);
    }

    #[tokio::test]
    async fn access_pipeline_error_fails_closed_no_ticket() {
        let pipe = FakePipeline {
            access_err: true,
            ..Default::default()
        };
        let (state, tickets) = state_with(pipe);
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::INTERNAL_SERVER_ERROR);
        // Fail-CLOSED: no ticket persisted on a gather fault.
        assert!(tickets.list(TENANT_A, 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_tenant_is_unauthorized() {
        let (state, _t) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            HeaderMap::new(),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn pat_caller_is_forbidden() {
        let (state, _t) = state_with(FakePipeline::default());
        let mut h = HeaderMap::new();
        h.insert("x-corelink-tenant-id", TENANT_A.parse().unwrap());
        h.insert("x-corelink-token-prefix", "corelink_test".parse().unwrap());
        let resp = submit(state, h, DsrRequestKind::Access, DsrSubmitBody::default()).await;
        assert_eq!(status_of(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn sentinel_tenant_is_unauthorized() {
        let (state, _t) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers("_anonymous"),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_cross_tenant_is_not_found() {
        let (state, tickets) = state_with(FakePipeline::default());
        // Tenant A submits; tenant B must not see it.
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        let resp = handle_status(State(state), clerk_headers(TENANT_B), Path(rid)).await;
        assert_eq!(status_of(&resp), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn status_same_tenant_returns_detail() {
        let (state, tickets) = state_with(FakePipeline::default());
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        let resp = handle_status(State(state), clerk_headers(TENANT_A), Path(rid)).await;
        assert_eq!(status_of(&resp), StatusCode::OK);
    }

    #[tokio::test]
    async fn verify_mfa_completes_pending_erasure() {
        let (state, tickets) = state_with(FakePipeline::default());
        // Submit erasure WITHOUT mfa → pending.
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        // verify-mfa WITHOUT the freshness header → 401.
        let resp = handle_verify_mfa(
            State(state.clone()),
            clerk_headers(TENANT_A),
            Path(rid.clone()),
            None,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
        // verify-mfa WITH the freshness header → completes.
        let resp = handle_verify_mfa(
            State(state.clone()),
            with_mfa(clerk_headers(TENANT_A)),
            Path(rid.clone()),
            None,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = tickets.get(TENANT_A, &rid).unwrap().unwrap();
        assert_eq!(t.status, TicketStatus::Completed);
    }

    #[tokio::test]
    async fn rate_limit_after_daily_cap() {
        let (state, _t) = state_with(FakePipeline::default());
        for _ in 0..DSR_DAILY_LIMIT {
            let resp = submit(
                state.clone(),
                clerk_headers(TENANT_A),
                DsrRequestKind::Access,
                DsrSubmitBody::default(),
            )
            .await;
            assert_eq!(status_of(&resp), StatusCode::OK);
        }
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn pipeline_unconfigured_fails_closed() {
        let state = PrivacyDsrRouteState {
            pipeline: None,
            tickets: Arc::new(InMemoryDsrTicketStore::new()),
            pat_gate: None,
            receipt_key: Arc::new(b"test-key-test-key-test-key-test-key".to_vec()),
        };
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn receipt_is_three_part_jwt() {
        let r = issue_receipt(
            b"k",
            "rid",
            DsrRequestKind::Access,
            DsrJurisdiction::Gdpr,
            1000,
            0,
            TENANT_A,
        );
        assert_eq!(r.split('.').count(), 3, "compact JWS = header.claims.sig");
    }
}

use super::*;
use corelink_handler_admin::{AuditEventKind, Sli};

fn fixture() -> (
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
    Arc<InMemoryApprovalLedger>,
    Arc<InMemoryAdminHandler>,
    AdminRouteState,
) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let ledger = Arc::new(InMemoryApprovalLedger::new());
    let shared = Arc::new(InMemoryAdminHandler::new_with_ledger(
        audit.clone(),
        sli.clone(),
        ledger.clone() as Arc<dyn ApprovalLedger>,
    ));
    let read: Arc<dyn AdminReadHandler> = shared.clone();
    let mutate: Arc<dyn AdminMutateHandler> = shared.clone();
    let approval_writer: Arc<dyn ApprovalLedgerWriter> = ledger.clone();
    (
        audit,
        sli,
        ledger,
        shared,
        AdminRouteState {
            read,
            mutate,
            // Tests exercise the handler trait directly; a configured
            // key here keeps the router constructable. Gate behaviour
            // is covered by the internal_auth_ok unit tests below.
            internal_auth_key: Some(Arc::from("test-internal-auth-key-32-bytes-x")),
            approval_writer,
            approver_auth_key: Some(Arc::from("test-approver-auth-key-32-bytes-y")),
            approvals_durable: false,
        },
    )
}

#[test]
fn route_constants_match_canonical_paths() {
    assert_eq!(ADMIN_READ_ROUTE, "/v1/admin/read/{resource}");
    assert_eq!(ADMIN_MUTATE_ROUTE, "/v1/admin/mutate");
}

#[test]
fn build_handlers_returns_usable_pair() {
    let (read, _mutate) = build_handlers();
    let res = read.read(AdminReadRequest::new("ghost", "admin", true, 0));
    assert!(matches!(res, Err(AdminHandlerError::NotFound { .. })));
    let (_a, _s, _ledger, _shared, st) = fixture();
    let _router = router(st);
}

/// Happy path: admin read returns the seeded body + emits
/// `ReadAttempted` + `ReadServed` audit rows and the
/// `Sli::AvailControlPlane` observation.
#[test]
fn admin_read_happy_emits_audit_and_sli() {
    let (audit, sli, _ledger, shared, st) = fixture();
    shared.seed_read("tenant:t1", b"{}").expect("seed");
    let req = AdminReadRequest::new("tenant:t1", "admin@root", true, 1);
    let resp = st.read.read(req).expect("read");
    assert_eq!(resp.resource, "tenant:t1");
    assert_eq!(resp.body, b"{}".to_vec());
    let rows = audit.snapshot().expect("audit");
    assert_eq!(rows[0].kind, AuditEventKind::ReadAttempted);
    assert!(rows.iter().any(|r| r.kind == AuditEventKind::ReadServed));
    assert!(sli
        .snapshot()
        .expect("sli")
        .iter()
        .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
}

/// Auth-fail: a non-admin principal MUST be rejected with
/// `Forbidden` and the audit row `ReadDenied` MUST be emitted
/// BEFORE the rejection (fail-CLOSED ordering pin).
#[test]
fn admin_read_auth_fail_audits_before_denial() {
    let (audit, _sli, _ledger, _shared, st) = fixture();
    let req = AdminReadRequest::new("tenant:t1", "alice", false, 1);
    let err = st.read.read(req).expect_err("forbidden");
    assert!(matches!(err, AdminHandlerError::Forbidden { .. }));
    let rows = audit.snapshot().expect("audit");
    assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
}

/// Cross-tenant equivalent: admin reads are not tenant-scoped at
/// the handler boundary, so `INV-TENANT-ISOLATION` is enforced
/// through RBAC (Forbidden) for non-admin principals attempting
/// to read a tenant resource they don't own.
#[test]
fn admin_read_cross_tenant_via_rbac_denial() {
    let (audit, _sli, _ledger, _shared, st) = fixture();
    // Non-admin principal attempting to read another tenant's data.
    let req = AdminReadRequest::new("tenant:victim", "attacker", false, 1);
    let err = st.read.read(req).expect_err("denied");
    assert!(matches!(err, AdminHandlerError::Forbidden { .. }));
    let rows = audit.snapshot().expect("audit");
    // Audit row pins the principal + resource attempted —
    // analyst can reconstruct the cross-tenant probe.
    assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
    assert_eq!(rows[0].principal, "attacker");
    assert_eq!(rows[0].resource, "tenant:victim");
}

/// Dual-approval missing MUST be rejected with audit
/// `MutateDualApprovalRejected` BEFORE the response.
#[test]
fn admin_mutate_dual_approval_missing_rejected() {
    let (audit, _sli, _ledger, _shared, st) = fixture();
    let req = AdminMutateRequest::new(
        MutateOp::set_tenant_tier("t1", "Team"),
        "alice",
        true,
        None,
        1,
    );
    let err = st.mutate.mutate(req).expect_err("dual approval");
    assert!(matches!(err, AdminHandlerError::DualApprovalMissing));
    let rows = audit.snapshot().expect("audit");
    assert_eq!(rows[0].kind, AuditEventKind::MutateAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::MutateDualApprovalRejected);
}

/// Self-approval MUST be rejected — based on the LEDGER-recorded
/// approver, not the request body. The recorded approver for `a1` is the
/// initiator (`alice`), so it rejects even though the body could claim
/// anything.
#[test]
fn admin_mutate_self_approval_rejected() {
    let (audit, _sli, ledger, _shared, st) = fixture();
    ledger.record("a1", "alice", "tenant:t1").expect("record");
    let req = AdminMutateRequest::new(
        MutateOp::set_tenant_tier("t1", "Team"),
        "alice",
        true,
        Some(DualApprovalToken::new("a1", "alice")),
        1,
    );
    let err = st.mutate.mutate(req).expect_err("self approval");
    assert!(matches!(
        err,
        AdminHandlerError::DualApprovalSelfApproval { .. }
    ));
    let rows = audit.snapshot().expect("audit");
    assert!(rows
        .iter()
        .any(|r| r.kind == AuditEventKind::MutateDualApprovalRejected));
}

/// Happy mutate path: emits `MutateAttempted` + `MutateCommitted`,
/// records `Sli::AvailControlPlane` non-error.
#[test]
fn admin_mutate_happy_commits_and_audits() {
    let (audit, sli, ledger, shared, st) = fixture();
    // A genuine, distinct, recorded approval authorizes the mutation.
    ledger.record("a1", "bob", "tenant:t1").expect("record");
    let req = AdminMutateRequest::new(
        MutateOp::set_tenant_tier("t1", "Team"),
        "alice",
        true,
        Some(DualApprovalToken::new("a1", "bob")),
        1,
    );
    let resp = st.mutate.mutate(req).expect("commit");
    assert_eq!(resp.resource, "tenant:t1");
    assert_eq!(resp.approval_id, "a1");
    assert_eq!(shared.applied_snapshot().expect("snap").len(), 1);
    let rows = audit.snapshot().expect("audit");
    assert!(rows
        .iter()
        .any(|r| r.kind == AuditEventKind::MutateCommitted));
    assert!(sli
        .snapshot()
        .expect("sli")
        .iter()
        .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
}

/// Audit failure MUST abort the mutation — fail-CLOSED ordering.
#[test]
fn admin_mutate_audit_failure_aborts_before_state_change() {
    let (audit, _sli, _ledger, shared, st) = fixture();
    audit.inject_failure("audit pipeline down").expect("inject");
    let req = AdminMutateRequest::new(
        MutateOp::set_tenant_tier("t1", "Team"),
        "alice",
        true,
        Some(DualApprovalToken::new("a1", "bob")),
        1,
    );
    let err = st.mutate.mutate(req).expect_err("audit closed");
    assert!(matches!(err, AdminHandlerError::AuditFailed(_)));
    assert!(shared.applied_snapshot().expect("snap").is_empty());
    let resp = map_err(err);
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// Single-use: an approval authorizes EXACTLY one mutation. A replay with
/// the same approval-id is rejected `DualApprovalConsumed` and applies no
/// second mutation. (Anti-replay — an operator who wants to repeat the op
/// must obtain a fresh approval.)
#[test]
fn admin_mutate_replay_rejected_single_use() {
    let (audit, _sli, ledger, shared, st) = fixture();
    ledger.record("a1", "bob", "tenant:t1").expect("record");
    let mk = || {
        AdminMutateRequest::new(
            MutateOp::set_tenant_tier("t1", "Team"),
            "alice",
            true,
            Some(DualApprovalToken::new("a1", "bob")),
            1,
        )
    };
    st.mutate.mutate(mk()).expect("first commit");
    let err = st.mutate.mutate(mk()).expect_err("replay rejected");
    assert!(matches!(
        err,
        AdminHandlerError::DualApprovalConsumed { .. }
    ));
    // Exactly one applied resource; no double-spend.
    assert_eq!(shared.applied_snapshot().expect("snap").len(), 1);
    let rows = audit.snapshot().expect("audit");
    let commits = rows
        .iter()
        .filter(|r| r.kind == AuditEventKind::MutateCommitted)
        .count();
    assert_eq!(commits, 1, "only the first spend commits");
}

/// Body parsing: `set_tenant_tier` without `tier` MUST fail at
/// the route boundary before reaching the handler.
#[test]
fn admin_mutate_body_parse_rejects_missing_fields() {
    let body = AdminMutateBody {
        op_kind: "set_tenant_tier".into(),
        tenant: Some("t1".into()),
        tier: None,
        token_id: None,
        initiator: "alice".into(),
        initiator_is_admin: true,
        approval_id: Some("a1".into()),
        approver: Some("bob".into()),
    };
    let err = body.into_request(0).expect_err("missing tier");
    assert!(err.contains("tier"));
}

/// Build a `set_tenant_tier` body with the given raw tier string.
fn tier_body(tier: &str) -> AdminMutateBody {
    AdminMutateBody {
        op_kind: "set_tenant_tier".into(),
        tenant: Some("t1".into()),
        tier: Some(tier.into()),
        token_id: None,
        initiator: String::new(),
        initiator_is_admin: false,
        approval_id: Some("a1".into()),
        approver: Some("bob".into()),
    }
}

/// Extract the tier label from a parsed `set_tenant_tier` request.
fn parsed_tier(req: &AdminMutateRequest) -> &str {
    match &req.op {
        MutateOp::SetTenantTier { tier, .. } => tier.as_str(),
        other => panic!("expected SetTenantTier, got {other:?}"),
    }
}

/// #35: the operator's own admin tests send capitalized labels
/// (`"Solo"`). The parser MUST normalize them to the lower-case
/// `tier_selections` label so the D1 CHECK accepts the write — on
/// BOTH the legacy and the operator-gated path.
#[test]
fn set_tenant_tier_normalizes_capitalized_tier() {
    let req = tier_body("Solo")
        .into_request(0)
        .expect("capitalized tier accepted + normalized");
    assert_eq!(parsed_tier(&req), "solo");

    let gated = tier_body("  PRO  ")
        .into_request_gated("operator@internal", 0)
        .expect("capitalized + padded tier accepted + normalized");
    assert_eq!(parsed_tier(&gated), "pro");
}

/// #35: every canonical `tier_selections` tier is accepted in its
/// lower-case form and passes through unchanged.
#[test]
fn set_tenant_tier_accepts_valid_lowercase_tiers() {
    for tier in ["free", "solo", "starter", "pro", "max", "enterprise"] {
        let req = tier_body(tier)
            .into_request(0)
            .unwrap_or_else(|e| panic!("tier {tier} should be accepted: {e}"));
        assert_eq!(parsed_tier(&req), tier);
    }
}

/// #35: an unknown tier is rejected with the `invalid_tier` 400
/// marker BEFORE reaching the D1 CHECK (no opaque 500).
#[test]
fn set_tenant_tier_rejects_invalid_tier() {
    let err = tier_body("platinum")
        .into_request(0)
        .expect_err("unknown tier rejected");
    assert_eq!(err, "invalid_tier");

    let gated_err = tier_body("platinum")
        .into_request_gated("operator@internal", 0)
        .expect_err("unknown tier rejected on gated path");
    assert_eq!(gated_err, "invalid_tier");
}

/// #35 divergence: `org` is valid in migration 0057's `tenant.tier`
/// CHECK but NOT in 0039's `tier_selections.tier`. It MUST be rejected
/// cleanly here (additive-only migration follow-up tracked in the PR),
/// not flowed to the D1 CHECK. (`solo` is now a canonical
/// `tier_selections` tier and is accepted — covered above.)
#[test]
fn set_tenant_tier_rejects_org_divergence() {
    for tier in ["org", "ORG"] {
        let err = tier_body(tier)
            .into_request_gated("operator@internal", 0)
            .expect_err("org rejected (0039 vs 0057 divergence)");
        assert_eq!(err, "invalid_tier", "tier {tier} must be rejected");
    }
}

/// serde-default: a wire body WITHOUT `initiator` / `initiator_is_admin`
/// fields MUST deserialize successfully and the gated path MUST still
/// derive admin authority from the operator principal, not the body.
///
/// This pins the PR #152 change: the fields are no longer required on the
/// wire, so callers that omit them don't get a 400.
#[test]
fn admin_mutate_body_serde_default_no_initiator_fields() {
    // JSON that intentionally omits initiator and initiator_is_admin.
    let json = r#"{
            "op_kind": "set_tenant_tier",
            "tenant": "t1",
            "tier": "Pro",
            "approval_id": "a1",
            "approver": "bob"
        }"#;
    let body: AdminMutateBody =
        serde_json::from_str(json).expect("deserialize without initiator fields");
    // Defaults: empty string + false.
    assert_eq!(body.initiator, "");
    assert!(!body.initiator_is_admin);
    // The gated path ignores the body fields and derives admin from the gate.
    let req = body
        .into_request_gated("operator@internal", 0)
        .expect("gated request from minimal body");
    assert_eq!(req.initiator, "operator@internal");
    assert!(req.initiator_is_admin);
}

/// The operator gate fails CLOSED when the key is unconfigured,
/// regardless of any header the client supplies.
#[test]
fn internal_auth_fails_closed_when_key_unset() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "anything".parse().expect("header"),
    );
    assert!(!internal_auth_ok(None, &headers));
}

/// Absent / wrong / right header behaviour against a configured key.
#[test]
fn internal_auth_matches_only_exact_secret() {
    let key: Arc<str> = Arc::from("test-internal-auth-key-32-bytes-x");
    // Absent header → reject.
    let empty = HeaderMap::new();
    assert!(!internal_auth_ok(Some(&key), &empty));
    // Wrong value → reject.
    let mut wrong = HeaderMap::new();
    wrong.insert(ADMIN_INTERNAL_AUTH_HEADER, "wrong".parse().expect("header"));
    assert!(!internal_auth_ok(Some(&key), &wrong));
    // Prefix of the real key (length differs) → reject (no length leak).
    let mut prefix = HeaderMap::new();
    prefix.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes".parse().expect("header"),
    );
    assert!(!internal_auth_ok(Some(&key), &prefix));
    // Exact value → accept.
    let mut right = HeaderMap::new();
    right.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    assert!(internal_auth_ok(Some(&key), &right));
}

/// The gated body parser derives `is_admin = true` + the operator
/// principal from the gate, IGNORING the body's self-asserted
/// `initiator` / `initiator_is_admin`.
#[test]
fn into_request_gated_ignores_body_admin_assertion() {
    let body = AdminMutateBody {
        op_kind: "set_tenant_tier".into(),
        tenant: Some("t1".into()),
        tier: Some("Pro".into()),
        token_id: None,
        // Attacker-controlled body fields — must be ignored.
        initiator: "attacker".into(),
        initiator_is_admin: false,
        approval_id: Some("a1".into()),
        approver: Some("bob".into()),
    };
    let req = body
        .into_request_gated("operator@internal", 7)
        .expect("gated request");
    // The handler request carries the initiator + admin flag as
    // public fields; pin that the operator principal — not
    // "attacker" — is recorded and is_admin is forced true.
    assert_eq!(req.initiator, "operator@internal");
    assert!(req.initiator_is_admin);
}

// ── F16: auth-before-body-parse (M3 pattern) ──────────────────────────────

/// Build an axum router from the fixture state for end-to-end handler tests.
fn fixture_router() -> (axum::Router, Arc<str>) {
    let (_audit, _sli, _ledger, _shared, state) = fixture();
    let key = state
        .internal_auth_key
        .clone()
        .expect("fixture always sets a key");
    (router(state), key)
}

/// F16: an unauthenticated caller sending a LARGE non-JSON body must receive
/// 403 (auth gate fires first), NOT a 400 from body deserialization.
/// This pins the M3 pattern: `Bytes` extractor defers parse until after auth.
#[tokio::test]
async fn handle_mutate_rejects_unauthenticated_before_parsing_body() {
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    let (app, _key) = fixture_router();
    // 2 MiB of garbage — not valid JSON.
    let big_garbage = "Z".repeat(2 * 1024 * 1024);
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(ADMIN_MUTATE_ROUTE)
        .header("content-type", "application/json")
        // NO x-corelink-internal-auth header.
        .body(Body::from(big_garbage))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "auth must fail (403) BEFORE the body is parsed (M3 / F16)"
    );
}

/// F16: a caller with a WRONG auth header and an invalid body must also get
/// 403, not 400 — the parse never runs when auth fails.
#[tokio::test]
async fn handle_mutate_wrong_auth_returns_403_not_400() {
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    let (app, _key) = fixture_router();
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(ADMIN_MUTATE_ROUTE)
        .header("content-type", "application/json")
        .header("x-corelink-internal-auth", "wrong-secret")
        .body(Body::from("{not-json}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

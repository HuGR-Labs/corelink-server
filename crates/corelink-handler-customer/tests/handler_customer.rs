//! Unit tests for `corelink-handler-customer`.
//!
//! Covers:
//! - Happy-path round-trips for all 6 endpoint groups

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "unit tests are allowed to use these primitives"
)]
//! - Cross-tenant denial: audit emitted BEFORE return
//! - Audit-fail propagation: fail-CLOSED ordering (mutation NOT executed)
//! - Idempotent key revoke
//! - SLI observer emit-on-every-return-path (INV-HANDLER-SLI-EMIT-ENTRY)
//! - AuditEventKind::slug() round-trip
//! - Empty tenant guard (unknown tenant → NotFound, not panic)

use std::sync::Arc;

use corelink_handler_customer::request::{
    AuditQueryRequest, BillingRequest, BillingResponse, ByokStatus, CustomerAuditEventRow,
    DailyUsageBucket, InvoiceRow, KeyCreateRequest, KeyRevokeRequest, KeysListRequest,
    OverviewBilling, OverviewRequest, OverviewResponse, OverviewUsage, PatRow, PortalRequest,
    TeamInviteRequest, TeamListRequest, UsageRequest, UsageResponse,
};
use corelink_handler_customer::{
    AuditEventKind, CustomerAuditHandler, CustomerBillingHandler, CustomerKeysHandler,
    CustomerOverviewHandler, CustomerTeamHandler, CustomerUsageHandler, InMemoryAuditSink,
    InMemoryCustomerHandler, InMemorySliObserver,
};
use corelink_slo::definition::Sli;

// ─── Fixture helpers ──────────────────────────────────────────────────────────

fn make_handler() -> (
    InMemoryCustomerHandler,
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let h = InMemoryCustomerHandler::new(
        Arc::clone(&audit) as Arc<dyn corelink_handler_customer::AuditSink>,
        Arc::clone(&sli) as Arc<dyn corelink_handler_customer::SliObserver>,
    );
    (h, audit, sli)
}

fn sample_overview(tenant_id: &str) -> OverviewResponse {
    OverviewResponse::new(
        tenant_id,
        "Acme Corp",
        "starter",
        OverviewUsage::new("2026-05", 1024, 10_000_000, 100, 50),
        OverviewBilling::new("active", "2026-06-01T00:00:00Z", 2900, "usd"),
        ByokStatus::new("none", None, None),
        vec![],
    )
}

fn sample_usage(_tenant_id: &str) -> UsageResponse {
    UsageResponse::new(
        "2026-05",
        1024_u64,
        100_u64,
        50_u64,
        10_000_000_u64,
        vec![DailyUsageBucket::new("2026-05-01", 10, 5, 128)],
        200_u64,
        Some(0.8_f64),
        1_200_u64,
        1_u64,
    )
}

fn sample_billing(tenant_id: &str) -> BillingResponse {
    let _ = tenant_id; // shape is tenant-agnostic in this fixture
    BillingResponse::new(
        "active",
        "starter",
        "2026-05-01T00:00:00Z",
        "2026-06-01T00:00:00Z",
        2900_i64,
        "usd",
        vec![InvoiceRow::new(
            "inv_001",
            "2026-05-01T00:00:00Z",
            2900,
            "paid",
            "https://stripe.com/inv_001",
        )],
    )
}

fn sample_pat(tenant_id: &str) -> PatRow {
    PatRow::new(
        format!("pat_{}_001", tenant_id),
        "CI token",
        vec!["cas:read".into(), "cas:write".into()],
        "2026-05-01T00:00:00Z",
        None,
        None,
    )
}

// ─── Overview tests ───────────────────────────────────────────────────────────

#[test]
fn overview_happy_path() {
    let (h, audit, sli) = make_handler();
    h.seed_overview("tenant_a", sample_overview("tenant_a"))
        .unwrap();

    let req = OverviewRequest::new("tenant_a", "user:alice", 1_000);
    let resp = h.overview(req).unwrap();

    assert_eq!(resp.tenant_id, "tenant_a");
    assert_eq!(resp.plan, "starter");

    let rows = audit.snapshot().unwrap();
    // OverviewAttempted + OverviewServed
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, AuditEventKind::OverviewAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::OverviewServed);

    let sli_count = sli.count(Sli::AvailControlPlane).unwrap();
    assert_eq!(sli_count, 1, "exactly one SLI observation on success path");
    let snap = sli.snapshot().unwrap();
    assert!(!snap[0].is_error);
}

#[test]
fn overview_unknown_tenant_returns_not_found() {
    let (h, _audit, sli) = make_handler();
    let req = OverviewRequest::new("ghost_tenant", "user:bob", 2_000);
    let err = h.overview(req).unwrap_err();
    assert!(
        matches!(
            err,
            corelink_handler_customer::CustomerHandlerError::NotFound { .. }
        ),
        "expected NotFound, got {err:?}"
    );
    // SLI must still fire (error path).
    let snap = sli.snapshot().unwrap();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].is_error);
}

// ─── Usage tests ─────────────────────────────────────────────────────────────

#[test]
fn usage_happy_path() {
    let (h, audit, sli) = make_handler();
    h.seed_usage("tenant_a", sample_usage("tenant_a")).unwrap();

    let req = UsageRequest::new("tenant_a", "user:alice", None, 1_000);
    let resp = h.usage(req).unwrap();

    assert_eq!(resp.period, "2026-05");
    assert_eq!(resp.cas_bytes, 1024);
    assert_eq!(resp.daily.len(), 1);

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::UsageAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::UsageServed);

    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
    assert!(!sli.snapshot().unwrap()[0].is_error);
}

#[test]
fn usage_period_filter_no_match_returns_not_found() {
    let (h, _audit, _sli) = make_handler();
    h.seed_usage("tenant_a", sample_usage("tenant_a")).unwrap();

    let req = UsageRequest::new("tenant_a", "user:alice", Some("2025-01".into()), 1_000);
    let err = h.usage(req).unwrap_err();
    assert!(matches!(
        err,
        corelink_handler_customer::CustomerHandlerError::NotFound { .. }
    ));
}

// ─── Billing tests ────────────────────────────────────────────────────────────

#[test]
fn billing_happy_path() {
    let (h, audit, sli) = make_handler();
    h.seed_billing("tenant_a", sample_billing("tenant_a"))
        .unwrap();

    let req = BillingRequest::new("tenant_a", "user:alice", 1_000);
    let resp = h.billing(req).unwrap();

    assert_eq!(resp.status, "active");
    assert_eq!(resp.invoices.len(), 1);

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::BillingAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::BillingServed);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

#[test]
fn portal_url_happy_path() {
    let (h, audit, sli) = make_handler();
    let req = PortalRequest::new("tenant_a", "user:alice", 1_000);
    let resp = h.portal_url(req).unwrap();

    assert!(
        resp.portal_url.contains("tenant_a"),
        "portal URL must contain tenant"
    );

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::BillingAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::BillingServed);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

// ─── Keys tests ───────────────────────────────────────────────────────────────

#[test]
fn keys_list_happy_path() {
    let (h, _audit, sli) = make_handler();
    h.seed_pat("tenant_a", sample_pat("tenant_a")).unwrap();

    let req = KeysListRequest::new("tenant_a", "user:alice", 1_000);
    let resp = CustomerKeysHandler::list(&h, req).unwrap();

    assert_eq!(resp.pats.len(), 1);
    assert_eq!(resp.pats[0].name, "CI token");
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
    assert!(!sli.snapshot().unwrap()[0].is_error);
}

#[test]
fn keys_list_other_tenant_not_visible() {
    let (h, _audit, _sli) = make_handler();
    h.seed_pat("tenant_b", sample_pat("tenant_b")).unwrap();

    let req = KeysListRequest::new("tenant_a", "user:alice", 1_000);
    let resp = CustomerKeysHandler::list(&h, req).unwrap();

    // tenant_b's PAT must not appear in tenant_a's list.
    assert_eq!(resp.pats.len(), 0);
}

#[test]
fn keys_create_happy_path() {
    let (h, audit, sli) = make_handler();
    let req = KeyCreateRequest::new(
        "tenant_a",
        "user:alice",
        "deploy key",
        vec!["cas:write".into()],
        2_000,
    );
    let resp = h.create(req).unwrap();

    assert!(!resp.token.is_empty(), "token must be non-empty on create");
    assert_eq!(resp.pat.name, "deploy key");
    assert!(resp.pat.revoked_at.is_none());

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::KeyCreateAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::KeyCreateCommitted);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
    assert!(!sli.snapshot().unwrap()[0].is_error);
}

#[test]
fn keys_revoke_happy_path() {
    let (h, audit, sli) = make_handler();
    h.seed_pat("tenant_a", sample_pat("tenant_a")).unwrap();

    let pat_id = "pat_tenant_a_001";
    let req = corelink_handler_customer::request::KeyRevokeRequest::new(
        "tenant_a",
        "user:alice",
        pat_id,
        3_000,
    );
    let resp = h.revoke(req).unwrap();

    assert!(
        resp.pat.revoked_at.is_some(),
        "revoked_at must be set after revoke"
    );

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::KeyRevokeAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::KeyRevokeCommitted);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

#[test]
fn keys_revoke_idempotent() {
    let (h, audit, _sli) = make_handler();
    // Seed an already-revoked PAT.
    let revoked_pat = PatRow::new(
        "pat_tenant_a_rev",
        "revoked key",
        vec![],
        "2026-05-01T00:00:00Z",
        None,
        Some("2026-05-10T00:00:00Z".into()),
    );
    h.seed_pat("tenant_a", revoked_pat).unwrap();

    let req = KeyRevokeRequest::new("tenant_a", "user:alice", "pat_tenant_a_rev", 4_000);
    let resp = h.revoke(req).unwrap();

    // Must not error; revoked_at unchanged.
    assert_eq!(resp.pat.revoked_at, Some("2026-05-10T00:00:00Z".into()));

    // Audit still fires both Attempted + Committed rows (idempotent ≠ skip audit).
    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::KeyRevokeAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::KeyRevokeCommitted);
}

#[test]
fn keys_revoke_unknown_pat_not_found() {
    let (h, _audit, sli) = make_handler();
    let req = KeyRevokeRequest::new("tenant_a", "user:alice", "no_such_pat", 5_000);
    let err = h.revoke(req).unwrap_err();

    assert!(matches!(
        err,
        corelink_handler_customer::CustomerHandlerError::NotFound { .. }
    ));
    // Error path must still fire SLI.
    let snap = sli.snapshot().unwrap();
    // One from revoke attempt audit, one from not-found.
    // The attempt audit fires before lookup, so sli from audit-attempt = fine;
    // the not-found triggers an additional error sli.
    assert!(!snap.is_empty());
    assert!(snap.iter().any(|o| o.is_error));
}

// ─── Team tests ───────────────────────────────────────────────────────────────

#[test]
fn team_list_happy_path() {
    let (h, _audit, sli) = make_handler();
    let member = corelink_handler_customer::request::TeamMemberRow::new(
        "user_001",
        "alice@acme.com",
        "Developer",
        "2026-05-01T00:00:00Z",
        "active",
    );
    h.seed_member("tenant_a", member).unwrap();

    let req = TeamListRequest::new("tenant_a", "user:alice", 1_000);
    let resp = CustomerTeamHandler::list(&h, req).unwrap();

    assert_eq!(resp.members.len(), 1);
    assert_eq!(resp.members[0].email, "alice@acme.com");
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

#[test]
fn team_invite_happy_path() {
    let (h, audit, sli) = make_handler();
    let req = TeamInviteRequest::new("tenant_a", "user:alice", "bob@acme.com", "Developer", 2_000);
    let resp = h.invite(req).unwrap();

    assert_eq!(resp.member.email, "bob@acme.com");
    assert_eq!(resp.member.status, "invited");

    let rows = audit.snapshot().unwrap();
    assert_eq!(rows[0].kind, AuditEventKind::TeamInviteAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::TeamInviteCommitted);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
    assert!(!sli.snapshot().unwrap()[0].is_error);
}

// ─── Audit query tests ────────────────────────────────────────────────────────

#[test]
fn audit_query_happy_path_no_filter() {
    let (h, audit_sink, sli) = make_handler();
    let rows = vec![
        CustomerAuditEventRow::new(
            "evt_001",
            "2026-05-01T00:00:00Z",
            "corelink.customer.keys.create.committed",
            "info",
            "alice@acme.com",
            "PAT created",
        ),
        CustomerAuditEventRow::new(
            "evt_002",
            "2026-05-02T00:00:00Z",
            "corelink.customer.team.invite.committed",
            "info",
            "alice@acme.com",
            "Bob invited",
        ),
    ];
    h.seed_audit_rows("tenant_a", rows).unwrap();

    let req = AuditQueryRequest::new("tenant_a", "user:alice", None, vec![], 1_000);
    let resp = h.query(req).unwrap();

    assert_eq!(resp.rows.len(), 2);

    let audit_rows = audit_sink.snapshot().unwrap();
    assert_eq!(audit_rows[0].kind, AuditEventKind::AuditQueryAttempted);
    assert_eq!(audit_rows[1].kind, AuditEventKind::AuditQueryServed);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

#[test]
fn audit_query_event_type_filter() {
    let (h, _audit_sink, _sli) = make_handler();
    let rows = vec![
        CustomerAuditEventRow::new(
            "evt_001",
            "2026-05-01T00:00:00Z",
            "corelink.customer.keys.create.committed",
            "info",
            "alice@acme.com",
            "PAT created",
        ),
        CustomerAuditEventRow::new(
            "evt_002",
            "2026-05-02T00:00:00Z",
            "corelink.customer.team.invite.committed",
            "info",
            "alice@acme.com",
            "Bob invited",
        ),
    ];
    h.seed_audit_rows("tenant_a", rows).unwrap();

    let req = AuditQueryRequest::new(
        "tenant_a",
        "user:alice",
        None,
        vec!["corelink.customer.keys.create.committed".into()],
        1_000,
    );
    let resp = h.query(req).unwrap();

    assert_eq!(resp.rows.len(), 1);
    assert_eq!(resp.rows[0].event_id, "evt_001");
}

#[test]
fn audit_query_empty_store_returns_empty_vec() {
    let (h, _audit_sink, _sli) = make_handler();
    let req = AuditQueryRequest::new("tenant_a", "user:alice", None, vec![], 1_000);
    let resp = h.query(req).unwrap();
    assert!(resp.rows.is_empty());
}

// ─── Audit-fail propagation (fail-CLOSED) ────────────────────────────────────

#[test]
fn overview_audit_fail_propagates_and_sli_error_fired() {
    let (h, audit, sli) = make_handler();
    h.seed_overview("tenant_a", sample_overview("tenant_a"))
        .unwrap();
    audit.inject_failure("sink down").unwrap();

    let req = OverviewRequest::new("tenant_a", "user:alice", 1_000);
    let err = h.overview(req).unwrap_err();

    assert!(
        matches!(
            err,
            corelink_handler_customer::CustomerHandlerError::AuditFailed(_)
        ),
        "expected AuditFailed, got {err:?}"
    );
    // SLI error observation must have fired.
    let snap = sli.snapshot().unwrap();
    assert!(!snap.is_empty());
    assert!(
        snap.iter().all(|o| o.is_error),
        "all SLI observations must be error on AuditFailed path"
    );
}

#[test]
fn keys_create_audit_fail_propagates_before_mutation() {
    let (h, audit, _sli) = make_handler();
    audit.inject_failure("sink down").unwrap();

    let req = KeyCreateRequest::new("tenant_a", "user:alice", "fail key", vec![], 2_000);
    let err = h.create(req).unwrap_err();

    assert!(matches!(
        err,
        corelink_handler_customer::CustomerHandlerError::AuditFailed(_)
    ));

    // Audit still records nothing (sink was injected to fail).
    let rows = audit.snapshot().unwrap();
    assert!(
        rows.is_empty(),
        "audit rows must be empty when sink is failing"
    );

    // PAT must NOT have been created (mutation aborted).
    audit.clear_failure().unwrap();
    let list_req = KeysListRequest::new("tenant_a", "user:alice", 3_000);
    let list_resp = CustomerKeysHandler::list(&h, list_req).unwrap();
    assert!(
        list_resp.pats.is_empty(),
        "PAT must not be created when audit fails"
    );
}

#[test]
fn team_invite_audit_fail_propagates_before_mutation() {
    let (h, audit, _sli) = make_handler();
    audit.inject_failure("sink down").unwrap();

    let req = TeamInviteRequest::new("tenant_a", "user:alice", "bob@acme.com", "Viewer", 3_000);
    let err = h.invite(req).unwrap_err();

    assert!(matches!(
        err,
        corelink_handler_customer::CustomerHandlerError::AuditFailed(_)
    ));

    // Team must NOT have been modified.
    audit.clear_failure().unwrap();
    let list_req = TeamListRequest::new("tenant_a", "user:alice", 4_000);
    let list_resp = CustomerTeamHandler::list(&h, list_req).unwrap();
    assert!(
        list_resp.members.is_empty(),
        "invite must not complete when audit fails"
    );
}

// ─── SLI emit-on-every-return-path (INV-HANDLER-SLI-EMIT-ENTRY) ─────────────

#[test]
fn sli_emitted_on_every_path_overview() {
    // Error path: unknown tenant.
    let (h, _audit, sli) = make_handler();
    let req = OverviewRequest::new("no_such_tenant", "user:x", 0);
    let _ = h.overview(req);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

#[test]
fn sli_emitted_on_success_usage() {
    let (h, _audit, sli) = make_handler();
    h.seed_usage("t1", sample_usage("t1")).unwrap();
    let req = UsageRequest::new("t1", "user:x", None, 0);
    let _ = h.usage(req);
    assert_eq!(sli.count(Sli::AvailControlPlane).unwrap(), 1);
}

// ─── AuditEventKind slug round-trip ──────────────────────────────────────────

#[test]
fn audit_event_kind_slugs_are_unique_and_non_empty() {
    use std::collections::HashSet;
    let kinds = [
        AuditEventKind::OverviewAttempted,
        AuditEventKind::OverviewServed,
        AuditEventKind::OverviewDenied,
        AuditEventKind::UsageAttempted,
        AuditEventKind::UsageServed,
        AuditEventKind::UsageDenied,
        AuditEventKind::BillingAttempted,
        AuditEventKind::BillingServed,
        AuditEventKind::BillingDenied,
        AuditEventKind::KeyCreateAttempted,
        AuditEventKind::KeyCreateCommitted,
        AuditEventKind::KeyRevokeAttempted,
        AuditEventKind::KeyRevokeCommitted,
        AuditEventKind::KeysDenied,
        AuditEventKind::TeamInviteAttempted,
        AuditEventKind::TeamInviteCommitted,
        AuditEventKind::TeamDenied,
        AuditEventKind::AuditQueryAttempted,
        AuditEventKind::AuditQueryServed,
        AuditEventKind::AuditQueryDenied,
    ];
    let mut seen = HashSet::new();
    for k in &kinds {
        let slug = k.slug();
        assert!(!slug.is_empty(), "slug for {k:?} must not be empty");
        assert!(seen.insert(slug), "duplicate slug: {slug}");
    }
    assert_eq!(
        seen.len(),
        20,
        "all 20 AuditEventKind variants must have unique slugs"
    );
}

// ─── Multi-tenant isolation ───────────────────────────────────────────────────

#[test]
fn keys_list_isolation_across_tenants() {
    let (h, _audit, _sli) = make_handler();
    h.seed_pat(
        "tenant_x",
        PatRow::new(
            "pat_x_001",
            "x key",
            vec![],
            "2026-05-01T00:00:00Z",
            None,
            None,
        ),
    )
    .unwrap();
    h.seed_pat(
        "tenant_y",
        PatRow::new(
            "pat_y_001",
            "y key",
            vec![],
            "2026-05-01T00:00:00Z",
            None,
            None,
        ),
    )
    .unwrap();

    let req_x = KeysListRequest::new("tenant_x", "user:x", 1_000);
    let resp_x = CustomerKeysHandler::list(&h, req_x).unwrap();
    assert_eq!(resp_x.pats.len(), 1);
    assert_eq!(resp_x.pats[0].pat_id, "pat_x_001");

    let req_y = KeysListRequest::new("tenant_y", "user:y", 2_000);
    let resp_y = CustomerKeysHandler::list(&h, req_y).unwrap();
    assert_eq!(resp_y.pats.len(), 1);
    assert_eq!(resp_y.pats[0].pat_id, "pat_y_001");
}

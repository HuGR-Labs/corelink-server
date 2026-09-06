//! Mutation-backed B-255 tests.
//!
//! These calls intentionally use a populated target tenant while the
//! authenticated caller is `tenant_a`.  Mutating the shared tenant predicate,
//! moving the denial audit after the return, or dropping one of the six event
//! variants makes this test fail before any state backend is reached.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use corelink_handler_customer::request::{
    AuditQueryRequest, BillingRequest, KeyCreateRequest, KeyRevokeRequest, KeysListRequest,
    OverviewRequest, PortalRequest, TeamInviteRequest, TeamListRequest, TeamRemoveRequest,
    UsageRequest,
};
use corelink_handler_customer::{
    AuditEventKind, CustomerAuditHandler, CustomerBillingHandler, CustomerKeysHandler,
    CustomerOverviewHandler, CustomerTeamHandler, CustomerUsageHandler, InMemoryAuditSink,
    InMemoryCustomerHandler, InMemorySliObserver,
};

#[test]
fn mutation_kills_cross_tenant_guard_and_audit_ordering() {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let handler = InMemoryCustomerHandler::new(
        Arc::clone(&audit) as Arc<dyn corelink_handler_customer::AuditSink>,
        Arc::clone(&sli) as Arc<dyn corelink_handler_customer::SliObserver>,
    );

    assert!(matches!(
        handler.overview(OverviewRequest::new("tenant_a", "p", 1).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.usage(UsageRequest::new("tenant_a", "p", None, 2).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.billing(BillingRequest::new("tenant_a", "p", 3).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.portal_url(PortalRequest::new("tenant_a", "p", 4).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        CustomerKeysHandler::list(
            &handler,
            KeysListRequest::new("tenant_a", "p", 5).for_tenant("tenant_b")
        ),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler
            .create(KeyCreateRequest::new("tenant_a", "p", "x", vec![], 6).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.revoke(KeyRevokeRequest::new("tenant_a", "p", "pat", 7).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        CustomerTeamHandler::list(
            &handler,
            TeamListRequest::new("tenant_a", "p", 8).for_tenant("tenant_b")
        ),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.invite(
            TeamInviteRequest::new("tenant_a", "p", "x@example.com", "member", 9)
                .for_tenant("tenant_b")
        ),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler
            .remove(TeamRemoveRequest::new("tenant_a", "p", "member", 10).for_tenant("tenant_b")),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));
    assert!(matches!(
        handler.query(
            AuditQueryRequest::new("tenant_a", "p", None, vec![], 11).for_tenant("tenant_b")
        ),
        Err(corelink_handler_customer::CustomerHandlerError::CrossTenantDenied { .. })
    ));

    let kinds: Vec<_> = audit
        .snapshot()
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::OverviewDenied,
            AuditEventKind::UsageDenied,
            AuditEventKind::BillingDenied,
            AuditEventKind::BillingDenied,
            AuditEventKind::KeysDenied,
            AuditEventKind::KeysDenied,
            AuditEventKind::KeysDenied,
            AuditEventKind::TeamDenied,
            AuditEventKind::TeamDenied,
            AuditEventKind::TeamDenied,
            AuditEventKind::AuditQueryDenied,
        ]
    );
    assert_eq!(sli.snapshot().unwrap().len(), kinds.len());
}

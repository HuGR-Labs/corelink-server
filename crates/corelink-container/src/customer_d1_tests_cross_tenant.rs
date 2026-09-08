// ── Cross-tenant D1 handler boundary ─────────────────────────────────────

#[test]
fn d1_cross_tenant_denials_cover_all_six_groups_before_any_query() {
    let f = fixture_with(MockD1::with(vec![]), None);

    let err = f
        .handler
        .overview(OverviewRequest::new(TENANT, "attacker", NOW_MS).for_tenant(OTHER_TENANT))
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .usage(UsageRequest::new(TENANT, "attacker", None, NOW_MS + 1).for_tenant(OTHER_TENANT))
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .billing(BillingRequest::new(TENANT, "attacker", NOW_MS + 2).for_tenant(OTHER_TENANT))
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = CustomerKeysHandler::list(
        &f.handler,
        KeysListRequest::new(TENANT, "attacker", NOW_MS + 3).for_tenant(OTHER_TENANT),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = CustomerTeamHandler::list(
        &f.handler,
        TeamListRequest::new(TENANT, "attacker", NOW_MS + 4).for_tenant(OTHER_TENANT),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .query(
            AuditQueryRequest::new(TENANT, "attacker", None, vec![], NOW_MS + 5)
                .for_tenant(OTHER_TENANT),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let kinds: Vec<_> = f
        .audit
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
            AuditEventKind::KeysDenied,
            AuditEventKind::TeamDenied,
            AuditEventKind::AuditQueryDenied,
        ]
    );
    assert!(f
        .audit
        .snapshot()
        .unwrap()
        .iter()
        .all(|event| event.tenant == OTHER_TENANT));
    assert_eq!(
        f.db.calls().len(),
        0,
        "CrossTenantDenied is the D1 boundary: no lookup is allowed before it"
    );
    assert_eq!(f.sli.snapshot().unwrap().len(), kinds.len());
    assert!(f.sli.snapshot().unwrap().iter().all(|obs| obs.is_error));
}

#[test]
fn d1_cross_tenant_mutations_and_portal_are_denied_before_d1_write() {
    let f = fixture_with(
        MockD1::with(vec![]),
        Some(Arc::new(MockPortal {
            url: "https://unused",
            fail: false,
        })),
    );

    let err = f
        .handler
        .create(
            KeyCreateRequest::new(TENANT, "attacker", "forged", vec![], NOW_MS)
                .for_tenant(OTHER_TENANT),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .revoke(
            KeyRevokeRequest::new(TENANT, "attacker", "forged-pat", NOW_MS + 1)
                .for_tenant(OTHER_TENANT),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .invite(
            TeamInviteRequest::new(
                TENANT,
                "attacker",
                "attacker@example.com",
                "member",
                NOW_MS + 2,
            )
            .for_tenant(OTHER_TENANT),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .remove(
            TeamRemoveRequest::new(TENANT, "attacker", "victim", NOW_MS + 3)
                .for_tenant(OTHER_TENANT),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    let err = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "attacker", NOW_MS + 4).for_tenant(OTHER_TENANT))
        .unwrap_err();
    assert!(matches!(
        err,
        CustomerHandlerError::CrossTenantDenied { .. }
    ));

    assert_eq!(
        f.db.calls().len(),
        0,
        "cross-tenant mutations and portal access must not reach D1"
    );
    assert_eq!(f.audit.snapshot().unwrap().len(), 5);
    assert_eq!(f.sli.snapshot().unwrap().len(), 5);
}

#[test]
fn d1_cross_tenant_audit_failure_is_fail_closed_before_query() {
    let f = fixture_with(MockD1::with(vec![]), None);
    f.audit.inject_failure("audit unavailable").unwrap();

    let err = f
        .handler
        .overview(OverviewRequest::new(TENANT, "attacker", NOW_MS).for_tenant(OTHER_TENANT))
        .unwrap_err();
    assert!(matches!(err, CustomerHandlerError::AuditFailed(_)));
    assert!(f.audit.snapshot().unwrap().is_empty());
    assert!(f.db.calls().is_empty());
    assert!(f.sli.snapshot().unwrap().iter().any(|obs| obs.is_error));
}

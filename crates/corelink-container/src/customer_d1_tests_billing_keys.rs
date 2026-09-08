// ── Billing ──────────────────────────────────────────────────────────────

#[test]
fn billing_happy_path_maps_status_and_tier() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_billing",
                vec![row(&[
                    ("status", json!("past_due")),
                    ("plan", json!("price_123")),
                    ("stripe_customer_id", json!("cus_42")),
                    ("current_period_end_ms", json!(1_700_500_000_000_i64)),
                ])],
            ),
            (
                "FROM tier_selections",
                vec![row(&[("tier", json!("starter"))])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .billing(BillingRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(resp.status, "past_due");
    assert_eq!(
        resp.plan, "starter",
        "tier_selections is the canonical tier"
    );
    assert_eq!(
        resp.current_period_start, "",
        "period start not tracked: honest empty"
    );
    assert_eq!(resp.current_period_end, ms_to_iso8601(1_700_500_000_000));
    assert_eq!(
        resp.amount_due_cents, 0,
        "amount due not materialized: honest 0"
    );
    assert!(
        resp.invoices.is_empty(),
        "no invoice surface yet: honest []"
    );
}

#[test]
fn billing_no_rows_is_inactive_free() {
    let f = fixture_with(
        MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
        None,
    );
    let resp = f
        .handler
        .billing(BillingRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(
        resp.status, "inactive",
        "no tenant_billing row → inactive (frozen)"
    );
    assert_eq!(
        resp.plan, "pro",
        "tenant.tier fallback when no tier_selections row"
    );
}

// ── Billing portal ───────────────────────────────────────────────────────

#[test]
fn portal_happy_path_returns_stripe_url() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM tenant_billing",
            vec![row(&[
                ("status", json!("paid")),
                ("stripe_customer_id", json!("cus_42")),
            ])],
        )]),
        Some(Arc::new(MockPortal {
            url: "https://billing.stripe.com/p/session/live_123",
            fail: false,
        })),
    );
    let resp = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(
        resp.portal_url,
        "https://billing.stripe.com/p/session/live_123"
    );
}

#[test]
fn portal_without_billing_account_is_404() {
    // No tenant_billing row at all.
    let f = fixture_with(
        MockD1::with(vec![]),
        Some(Arc::new(MockPortal {
            url: "https://unused",
            fail: false,
        })),
    );
    let err = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(
        matches!(&err, CustomerHandlerError::NotFound { what } if what.contains("no billing account")),
        "{err:?}"
    );

    // A row whose stripe_customer_id is NULL → same 404.
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM tenant_billing",
            vec![row(&[
                ("status", json!("inactive")),
                ("stripe_customer_id", Value::Null),
            ])],
        )]),
        Some(Arc::new(MockPortal {
            url: "https://unused",
            fail: false,
        })),
    );
    let err = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::NotFound { .. }),
        "{err:?}"
    );
}

#[test]
fn portal_without_stripe_client_fails_closed() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM tenant_billing",
            vec![row(&[("stripe_customer_id", json!("cus_42"))])],
        )]),
        None,
    );
    let err = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
}

#[test]
fn portal_stripe_error_fails_closed() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM tenant_billing",
            vec![row(&[("stripe_customer_id", json!("cus_42"))])],
        )]),
        Some(Arc::new(MockPortal {
            url: "https://unused",
            fail: true,
        })),
    );
    let err = f
        .handler
        .portal_url(PortalRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
}

// ── Keys ─────────────────────────────────────────────────────────────────

#[test]
fn keys_list_maps_pat_rows() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM pat WHERE tenant_id",
            vec![
                row(&[
                    ("pat_id", json!("pat_1")),
                    ("name", json!("ci-key")),
                    ("scope", json!("read-write")),
                    ("created_ms", json!(1_690_000_000_000_i64)),
                    ("revoked_at_ms", Value::Null),
                ]),
                row(&[
                    ("pat_id", json!("pat_0")),
                    // Pre-0063 row: NULL name.
                    ("name", Value::Null),
                    ("scope", json!("read-only")),
                    ("created_ms", json!(1_680_000_000_000_i64)),
                    ("revoked_at_ms", json!(1_695_000_000_000_i64)),
                ]),
            ],
        )]),
        None,
    );
    let resp =
        CustomerKeysHandler::list(&f.handler, KeysListRequest::new(TENANT, "clpat_x", 0)).unwrap();
    assert_eq!(resp.pats.len(), 2);
    assert_eq!(resp.pats[0].pat_id, "pat_1");
    assert_eq!(resp.pats[0].name, "ci-key");
    assert_eq!(
        resp.pats[0].scopes,
        vec!["cache:read".to_owned(), "cache:write".to_owned()]
    );
    assert_eq!(resp.pats[0].created_at, ms_to_iso8601(1_690_000_000_000));
    assert_eq!(
        resp.pats[0].last_used_at, None,
        "last-used not tracked: honest None"
    );
    assert_eq!(resp.pats[0].revoked_at, None);
    assert_eq!(
        resp.pats[1].name, "",
        "NULL name (pre-0063 row) → honest empty"
    );
    assert_eq!(
        resp.pats[1].revoked_at,
        Some(ms_to_iso8601(1_695_000_000_000))
    );
    assert_eq!(resp.byok.status, "none");
}

#[test]
fn keys_create_mints_inserts_and_returns_token_once() {
    let f = fixture_with(
        MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
        None,
    );
    let resp = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "deploy-key",
            vec!["cache:read".to_owned(), "cache:write".to_owned()],
            0,
        ))
        .unwrap();
    assert!(
        resp.token.starts_with("corelink_pat_"),
        "real minted plaintext"
    );
    assert_eq!(resp.pat.name, "deploy-key");
    assert_eq!(
        resp.pat.scopes,
        vec!["cache:read".to_owned(), "cache:write".to_owned()]
    );
    assert_eq!(
        resp.pat.created_at,
        ms_to_iso8601(i64::try_from(NOW_MS).unwrap())
    );
    assert_eq!(resp.pat.revoked_at, None);

    // The INSERT carried the frozen 'read-write' scope + the name,
    // and the plaintext was NEVER bound into SQL.
    let calls = f.db.calls();
    let insert = calls
        .iter()
        .find(|(sql, _)| sql.contains("INSERT INTO pat"))
        .expect("INSERT executed");
    assert_eq!(insert.1[3], json!("read-write"));
    assert_eq!(insert.1[8], json!("deploy-key"));
    assert!(
        !insert
            .1
            .iter()
            .any(|b| b.as_str().is_some_and(|s| s.contains(resp.token.as_str()))),
        "the PAT plaintext must never reach D1"
    );
    // Audit ordering: Attempted BEFORE the INSERT, then Committed.
    let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::KeyCreateAttempted,
            AuditEventKind::KeyCreateCommitted
        ]
    );
}

/// Item-4 fail-CLOSED: when the UNSKIPPABLE `customer_audit_events` insert
/// faults, `create` MUST surface `AuditFailed` (→ 503) AND must NOT have run
/// the `INSERT INTO pat` mutation (emit-before-mutate: the key is never
/// minted without its customer-visible audit row). This is the marketing
/// "unskippable audit trail" made true for the key-create control-plane op.
#[test]
fn keys_create_fails_closed_when_customer_audit_insert_faults() {
    let f = fixture_with(
        MockD1::failing_on(
            "customer_audit_events",
            vec![("FROM tenant WHERE", vec![tenant_row_fixture()])],
        ),
        None,
    );
    let err = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "deploy-key",
            vec!["cache:read".to_owned()],
            0,
        ))
        .expect_err("audit-insert fault must fail the create CLOSED");
    assert!(
        matches!(err, CustomerHandlerError::AuditFailed(_)),
        "expected AuditFailed, got {err:?}"
    );
    // Emit-before-mutate: the pat INSERT must NEVER have run.
    let calls = f.db.calls();
    assert!(
        !calls.iter().any(|(sql, _)| sql.contains("INSERT INTO pat")),
        "the pat mint must not commit when the unskippable audit row fails"
    );
    // The audit row WAS attempted (proving it is on the fail-CLOSED path).
    assert!(
        calls
            .iter()
            .any(|(sql, _)| sql.contains("customer_audit_events")),
        "the customer audit insert must be attempted before the mutation"
    );
    assert_eq!(f.db.atomic_rows(), AtomicRows::default());
}

#[test]
fn keys_create_mutation_failure_rolls_back_audit_row() {
    let f = fixture_with(MockD1::failing_on("INSERT INTO pat", vec![]), None);
    let err = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "deploy-key",
            vec!["cache:read".to_owned()],
            0,
        ))
        .expect_err("PAT mutation failure must fail closed");
    assert!(matches!(err, CustomerHandlerError::Internal(_)));
    assert_eq!(
        f.db.atomic_rows(),
        AtomicRows::default(),
        "D1 rollback removes the staged customer audit row"
    );
}

/// Item-4 fail-CLOSED sibling for team invite: an audit-insert fault must
/// surface `AuditFailed` and leave NO `team_member` seat behind.
#[test]
fn team_invite_fails_closed_when_customer_audit_insert_faults() {
    let _env = crate::email_hash::EnvGuard::acquire();
    let f = fixture_with(MockD1::failing_on("customer_audit_events", vec![]), None);
    let err = f
        .handler
        .invite(TeamInviteRequest::new(
            TENANT,
            "clpat_x",
            "Alice@Example.com",
            "Member",
            0,
        ))
        .expect_err("audit-insert fault must fail the invite CLOSED");
    assert!(
        matches!(err, CustomerHandlerError::AuditFailed(_)),
        "expected AuditFailed, got {err:?}"
    );
    let calls = f.db.calls();
    assert!(
        !calls
            .iter()
            .any(|(sql, _)| sql.contains("INSERT INTO team_member")),
        "no seat may be written when the unskippable audit row fails"
    );
    assert_eq!(f.db.atomic_rows(), AtomicRows::default());
}

#[test]
fn team_invite_mutation_failure_rolls_back_audit_row() {
    let _env = crate::email_hash::EnvGuard::acquire();
    let f = fixture_with(MockD1::failing_on("INSERT INTO team_member", vec![]), None);
    let err = f
        .handler
        .invite(TeamInviteRequest::new(
            TENANT,
            "clpat_x",
            "Alice@Example.com",
            "Member",
            0,
        ))
        .expect_err("team-member mutation failure must fail closed");
    assert!(matches!(err, CustomerHandlerError::Internal(_)));
    assert_eq!(
        f.db.atomic_rows(),
        AtomicRows::default(),
        "D1 rollback removes the staged customer audit row"
    );
}

#[test]
fn keys_create_read_only_scope_for_cache_read() {
    let f = fixture_with(MockD1::with(vec![]), None);
    let resp = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "ro-key",
            vec!["cache:read".to_owned()],
            0,
        ))
        .unwrap();
    assert_eq!(resp.pat.scopes, vec!["cache:read".to_owned()]);
    let calls = f.db.calls();
    let insert = calls
        .iter()
        .find(|(sql, _)| sql.contains("INSERT INTO pat"))
        .expect("INSERT executed");
    assert_eq!(insert.1[3], json!("read-only"));
    // A normal read PAT is NOT find-only (?10 = find_only = 0).
    assert_eq!(insert.1[9], json!(0));
}

/// ADR-0071: a find-only request stores the CHECK-safe base `read-only` +
/// `find_only = 1` (NOT a 4th `pat.scope` value the 0037 CHECK would reject),
/// and surfaces as the `cache:find-missing` capability it actually grants.
#[test]
fn keys_create_find_only_stores_read_only_base_plus_marker() {
    let f = fixture_with(MockD1::with(vec![]), None);
    let resp = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "find-key",
            vec!["cache:find-missing".to_owned()],
            0,
        ))
        .unwrap();
    // Displayed as find-missing (via the marker), never cache:read.
    assert_eq!(resp.pat.scopes, vec!["cache:find-missing".to_owned()]);
    let calls = f.db.calls();
    let insert = calls
        .iter()
        .find(|(sql, _)| sql.contains("INSERT INTO pat"))
        .expect("INSERT executed");
    // Base scope is CHECK-safe `read-only` (?4); the `find_only` marker (?10) = 1.
    assert_eq!(insert.1[3], json!("read-only"));
    assert_eq!(insert.1[9], json!(1));
}

#[test]
fn keys_create_admin_scope_is_never_grantable() {
    let f = fixture_with(MockD1::with(vec![]), None);
    let err = f
        .handler
        .create(KeyCreateRequest::new(
            TENANT,
            "clpat_x",
            "evil",
            vec!["admin".to_owned()],
            0,
        ))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::Unauthorized(_)),
        "{err:?}"
    );
    assert!(
        !f.db.calls().iter().any(|(sql, _)| sql.contains("INSERT")),
        "no INSERT may run for a rejected scope"
    );
}

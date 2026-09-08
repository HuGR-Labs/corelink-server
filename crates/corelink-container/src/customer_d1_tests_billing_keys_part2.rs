#[test]
fn keys_create_without_signing_key_fails_closed() {
    let db = Arc::new(MockD1::with(vec![]));
    let handler = D1CustomerHandler::new(
        db,
        None,
        String::new(),
        None, // no signing key
        Arc::new(InMemoryAuditSink::new()),
        Arc::new(InMemorySliObserver::new()),
        Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
    );
    let err = handler
        .create(KeyCreateRequest::new(TENANT, "clpat_x", "k", vec![], 0))
        .unwrap_err();
    assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
}

#[test]
fn keys_revoke_happy_path_updates_and_returns_revoked_at() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM pat WHERE pat_id",
            vec![row(&[
                ("pat_id", json!("pat_1")),
                ("token_id", json!("0000000000000001")),
                ("name", json!("ci-key")),
                ("scope", json!("read-only")),
                ("created_ms", json!(1_690_000_000_000_i64)),
                ("revoked_at_ms", Value::Null),
            ])],
        )]),
        None,
    );
    let resp = f
        .handler
        .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
        .unwrap();
    assert_eq!(
        resp.pat.revoked_at,
        Some(ms_to_iso8601(i64::try_from(NOW_MS).unwrap()))
    );
    // The UPDATE ran, tenant-scoped + only-if-unrevoked.
    let calls = f.db.calls();
    let update = calls
        .iter()
        .find(|(sql, _)| sql.contains("UPDATE pat SET revoked_at_ms"))
        .expect("UPDATE executed");
    assert!(update.0.contains("tenant_id = ?3"));
    assert!(update.0.contains("revoked_at_ms IS NULL"));
    assert_eq!(update.1[1], json!("pat_1"));
    assert_eq!(update.1[2], json!(TENANT));
}

#[test]
fn keys_revoke_is_idempotent_no_second_update() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM pat WHERE pat_id",
            vec![row(&[
                ("pat_id", json!("pat_1")),
                ("token_id", json!("0000000000000002")),
                ("name", json!("ci-key")),
                ("scope", json!("read-only")),
                ("created_ms", json!(1_690_000_000_000_i64)),
                ("revoked_at_ms", json!(1_695_000_000_000_i64)),
            ])],
        )]),
        None,
    );
    let resp = f
        .handler
        .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
        .unwrap();
    // Original timestamp preserved.
    assert_eq!(resp.pat.revoked_at, Some(ms_to_iso8601(1_695_000_000_000)));
    assert!(
        !f.db
            .calls()
            .iter()
            .any(|(sql, _)| sql.contains("UPDATE pat")),
        "idempotent re-revoke must not re-UPDATE"
    );
}

#[test]
fn keys_revoke_admin_cannot_revoke_owner_pat() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM pat WHERE pat_id",
            vec![row(&[
                ("pat_id", json!("pat_owner")),
                ("token_id", json!("0000000000000003")),
                ("name", json!("owner-key")),
                ("scope", json!("read-write")),
                ("created_ms", json!(1_690_000_000_000_i64)),
                // Migration 0075: NULL principal_id denotes owner/legacy.
                ("principal_id", Value::Null),
                ("revoked_at_ms", Value::Null),
            ])],
        )]),
        None,
    );
    let err = f
        .handler
        .revoke(KeyRevokeRequest::with_role(
            TENANT,
            "clerk_admin",
            "admin",
            "pat_owner",
            0,
        ))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::Unauthorized(_)),
        "admin owner-target denial must be explicit: {err:?}"
    );
    assert!(
        !f.db
            .calls()
            .iter()
            .any(|(sql, _)| sql.contains("UPDATE pat")),
        "an admin denial must never mutate the owner PAT"
    );
    let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::KeyRevokeAttempted,
            AuditEventKind::KeysDenied
        ]
    );
}

#[test]
fn keys_revoke_admin_can_revoke_member_pat_within_tenant() {
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM pat WHERE pat_id",
            vec![row(&[
                ("pat_id", json!("pat_member")),
                ("token_id", json!("0000000000000004")),
                ("name", json!("member-key")),
                ("scope", json!("read-only")),
                ("principal_id", json!("member-uuid")),
                ("created_ms", json!(1_690_000_000_000_i64)),
                ("revoked_at_ms", Value::Null),
            ])],
        )]),
        None,
    );
    let response = f
        .handler
        .revoke(KeyRevokeRequest::with_role(
            TENANT,
            "clerk_admin",
            "admin",
            "pat_member",
            0,
        ))
        .expect("admin may revoke a non-owner PAT in its tenant");
    assert!(response.pat.revoked_at.is_some());
    assert!(f
        .db
        .calls()
        .iter()
        .any(|(sql, _)| sql.contains("UPDATE pat") && sql.contains("tenant_id = ?3")));
}

#[test]
fn keys_revoke_cross_tenant_is_not_found() {
    // The tenant-scoped SELECT finds nothing for another tenant's
    // PAT (the mock returns rows only when binds match TENANT).
    #[derive(Debug, Default)]
    struct TenantScopedMock {
        calls: Mutex<Vec<(String, Vec<Value>)>>,
    }
    impl CustomerD1 for TenantScopedMock {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            self.calls
                .lock()
                .unwrap()
                .push((sql.to_owned(), binds.clone()));
            if sql.contains("FROM pat WHERE pat_id") && binds.get(1) == Some(&json!(TENANT)) {
                return Ok(vec![row(&[
                    ("pat_id", json!("pat_1")),
                    ("token_id", json!("0000000000000005")),
                    ("name", json!("ci-key")),
                    ("scope", json!("read-only")),
                    ("created_ms", json!(1_690_000_000_000_i64)),
                    ("revoked_at_ms", Value::Null),
                ])]);
            }
            Ok(Vec::new())
        }
    }
    let db = Arc::new(TenantScopedMock::default());
    let handler = D1CustomerHandler::new(
        db.clone(),
        None,
        String::new(),
        None,
        Arc::new(InMemoryAuditSink::new()),
        Arc::new(InMemorySliObserver::new()),
        Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
    );
    // The OWNING tenant revokes fine…
    assert!(handler
        .revoke(KeyRevokeRequest::new(TENANT, "clpat_x", "pat_1", 0))
        .is_ok());
    // …another tenant naming the same pat_id gets NotFound, and no
    // UPDATE ever ran under the foreign tenant.
    let err = handler
        .revoke(KeyRevokeRequest::new(OTHER_TENANT, "clpat_y", "pat_1", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::NotFound { .. }),
        "{err:?}"
    );
    assert!(
        !db.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(sql, binds)| sql.contains("UPDATE pat") && binds.contains(&json!(OTHER_TENANT))),
        "cross-tenant revoke must never UPDATE"
    );
}

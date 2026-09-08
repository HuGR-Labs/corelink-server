// ── Team ─────────────────────────────────────────────────────────────────

#[test]
fn team_list_synthesizes_single_owner_row() {
    let f = fixture_with(
        MockD1::with(vec![("FROM tenant WHERE", vec![tenant_row_fixture()])]),
        None,
    );
    let resp =
        CustomerTeamHandler::list(&f.handler, TeamListRequest::new(TENANT, "clpat_x", 0)).unwrap();
    assert_eq!(resp.members.len(), 1);
    assert_eq!(resp.members[0].user_id, "user_2abc");
    assert_eq!(
        resp.members[0].email, "—",
        "email is hashed in D1: honest dash"
    );
    assert_eq!(resp.members[0].role, "owner");
    assert_eq!(resp.members[0].status, "active");
    assert_eq!(resp.members[0].joined_at, ms_to_iso8601(1_690_000_000_000));
}

#[test]
fn team_invite_inserts_invited_member_row() {
    // B-073: invite() INSERTs an `invited` row with pseudonymized email and
    // token digests, returning the opaque token exactly once.
    //
    // EMAIL_HASH_SALT is process-global: hold the shared lock (forces salt
    // UNSET) so a concurrent salted test can't perturb the bind below.
    let _env = crate::email_hash::EnvGuard::acquire();
    let f = fixture_with(MockD1::with(vec![]), None);
    let resp = f
        .handler
        .invite(TeamInviteRequest::new(
            TENANT,
            "clpat_x",
            "Alice@Example.com",
            "Member",
            0,
        ))
        .expect("invite must succeed");
    assert_eq!(resp.member.status, "invited");
    // Response echoes the caller-supplied email + requested role (InMemory shape).
    assert_eq!(resp.member.email, "Alice@Example.com");
    assert_eq!(resp.member.role, "member");

    // Exactly one D1 write: the team_member INSERT (status='invited').
    let calls = f.db.calls();
    let insert = calls
        .iter()
        .find(|(sql, _)| sql.contains("INSERT INTO team_member"))
        .expect("an INSERT INTO team_member must have run");
    assert!(insert.0.contains("'invited'"), "row must be status=invited");
    // binds: tenant, user_id(placeholder UUID), email_hash, token_hash, role,
    // invited_by, invited_at_ms.
    assert_eq!(insert.1[0], json!(TENANT));
    // CTRL-PRIV-001: the raw email is NEVER a bind value — only the canonical
    // pseudonymized hash of the NORMALIZED (trim+lowercase) email, computed by
    // the ONE shared helper (the exact join key the rectification + signup-worker
    // accept side match on — C-ACCEPT parity). Asserting against the helper proves
    // the WRITE site routes through the single source of truth (matching invariant).
    let expected_hash = crate::email_hash::hash_email("alice@example.com");
    assert_eq!(insert.1[2], json!(expected_hash));
    let token = resp
        .invitation_token
        .as_deref()
        .expect("D1 invite returns a one-time token");
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(insert.1[3].as_str().map(str::len), Some(64));
    for bind in &insert.1 {
        assert_ne!(
            bind.as_str(),
            Some("Alice@Example.com"),
            "raw invitee email must never be persisted (CTRL-PRIV-001)"
        );
    }
    // The canonical `member` role is persisted in the CHECK domain.
    assert_eq!(insert.1[4], json!("member"));
    assert_eq!(
        insert.1[5],
        json!("clpat_x"),
        "invited_by = caller principal"
    );

    // Audit: Attempted BEFORE the write, Committed AFTER.
    let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::TeamInviteAttempted,
            AuditEventKind::TeamInviteCommitted
        ]
    );
}

#[test]
fn team_invite_rejects_unsupported_role_before_audit_or_mutation() {
    // Production D1 must reject UI-era aliases (Developer) and arbitrary
    // values before even the attempted audit. This protects the audit
    // contract and guarantees that an invalid request cannot create a seat.
    let _env = crate::email_hash::EnvGuard::acquire();
    for role in ["Developer", "unknown"] {
        let f = fixture_with(MockD1::with(vec![]), None);
        let err = f
            .handler
            .invite(TeamInviteRequest::new(
                TENANT,
                "clpat_x",
                "attacker@example.com",
                role,
                0,
            ))
            .expect_err("unsupported role must be rejected");
        assert_eq!(
            err,
            CustomerHandlerError::InvalidRequest("unsupported team invite role".to_owned())
        );
        assert!(
            f.audit.snapshot().unwrap().is_empty(),
            "{role} must not emit TeamInviteAttempted or any audit"
        );
        assert!(
            f.db.calls().is_empty(),
            "{role} must not query D1 or mutate team_member"
        );
    }
}

#[test]
fn normalize_invite_role_maps_to_check_domain() {
    // RBAC hardening: `owner` is NEVER mintable via a self-serve invite.
    assert_eq!(
        normalize_invite_role("Owner").unwrap_err(),
        CustomerHandlerError::InvalidRequest("unsupported team invite role".to_owned())
    );
    assert_eq!(normalize_invite_role("ADMIN").unwrap(), "admin");
    assert_eq!(normalize_invite_role("Viewer").unwrap(), "viewer");
    assert_eq!(
        normalize_invite_role("Developer").unwrap_err(),
        CustomerHandlerError::InvalidRequest("unsupported team invite role".to_owned())
    );
    assert_eq!(normalize_invite_role("  member ").unwrap(), "member");
    assert_eq!(
        normalize_invite_role("anything-else").unwrap_err(),
        CustomerHandlerError::InvalidRequest("unsupported team invite role".to_owned())
    );
}

#[test]
fn team_list_includes_active_members() {
    // Owner (synthesized from tenant) + one active `team_member` seat.
    let member = row(&[
        ("user_id", json!("user_member1")),
        ("role", json!("member")),
        ("status", json!("active")),
        ("joined_at_ms", json!(1_690_000_500_000_i64)),
        ("invited_at_ms", json!(1_690_000_400_000_i64)),
    ]);
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            ("status IN ('active','invited')", vec![member]),
        ]),
        None,
    );
    let resp =
        CustomerTeamHandler::list(&f.handler, TeamListRequest::new(TENANT, "clpat_x", 0)).unwrap();
    assert_eq!(resp.members.len(), 2, "owner + 1 member");
    assert_eq!(resp.members[0].role, "owner");
    assert_eq!(resp.members[1].user_id, "user_member1");
    assert_eq!(resp.members[1].status, "active");
}

#[test]
fn team_remove_flips_seat_and_revokes_member_pats() {
    // Member exists (not owner) + holds 2 live PATs → removal revokes both.
    let f = fixture_with(
        MockD1::with(vec![
            (
                "role, status FROM team_member",
                vec![row(&[
                    ("role", json!("member")),
                    ("status", json!("active")),
                ])],
            ),
            (
                "pat_id FROM pat",
                vec![
                    row(&[("pat_id", json!("pat_a"))]),
                    row(&[("pat_id", json!("pat_b"))]),
                ],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .remove(TeamRemoveRequest::new(
            TENANT,
            "clpat_owner",
            "user_member1",
            0,
        ))
        .unwrap();
    assert_eq!(
        resp.revoked_pats, 2,
        "both of the member's live PATs revoked"
    );
    assert_eq!(resp.member.status, "removed");

    // The load-bearing effects must both have been issued to D1.
    let sqls: Vec<String> = f.db.calls().into_iter().map(|(s, _)| s).collect();
    assert!(
        sqls.iter()
            .any(|s| s.contains("UPDATE pat SET revoked_at_ms")),
        "must revoke the member's PATs"
    );
    assert!(
        sqls.iter()
            .any(|s| s.contains("UPDATE team_member SET status = 'removed'")),
        "must flip the seat to removed"
    );
    let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::TeamRemoveAttempted,
            AuditEventKind::TeamRemoveCommitted
        ]
    );
}

#[test]
fn team_remove_owner_is_rejected() {
    let f = fixture_with(
        MockD1::with(vec![(
            "role, status FROM team_member",
            vec![row(&[
                ("role", json!("owner")),
                ("status", json!("active")),
            ])],
        )]),
        None,
    );
    let err = f
        .handler
        .remove(TeamRemoveRequest::new(TENANT, "clpat_x", "user_owner", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::Unauthorized(_)),
        "{err:?}"
    );
    // No PAT revocation must have been attempted for an owner-removal reject.
    let sqls: Vec<String> = f.db.calls().into_iter().map(|(s, _)| s).collect();
    assert!(!sqls.iter().any(|s| s.contains("UPDATE pat")));
}

#[test]
fn team_remove_absent_member_is_not_found() {
    // No canned team_member row → lookup returns empty → NotFound.
    let f = fixture_with(MockD1::with(vec![]), None);
    let err = f
        .handler
        .remove(TeamRemoveRequest::new(TENANT, "clpat_x", "user_ghost", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::NotFound { .. }),
        "{err:?}"
    );
}

// ── Audit query ──────────────────────────────────────────────────────────

#[test]
fn audit_query_empty_source_returns_empty_rows() {
    // No rows in customer_audit_events → honest empty page (never fabricated).
    let f = fixture_with(MockD1::with(vec![]), None);
    let resp = f
        .handler
        .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
        .unwrap();
    assert!(resp.rows.is_empty());
}

#[test]
fn audit_query_maps_customer_audit_events_rows() {
    // Two seeded events (migration 0077). The read maps each row to a
    // `CustomerAuditEventRow` and preserves the SQL's newest-first order;
    // `ts` is rendered ISO-8601 and `severity` is the constant `info`.
    let f = fixture_with(
        MockD1::with(vec![(
            "FROM customer_audit_events",
            vec![
                row(&[
                    ("id", json!(2)),
                    ("event_type", json!("team.invited")),
                    ("actor", json!("clpat_admin")),
                    ("target", json!("inv-2")),
                    ("ts_ms", json!(1_700_000_000_000_i64)),
                    ("detail", json!("Invited a team member with role member")),
                ]),
                row(&[
                    ("id", json!(1)),
                    ("event_type", json!("pat.created")),
                    ("actor", json!("clpat_admin")),
                    ("target", json!("pat-1")),
                    ("ts_ms", json!(1_699_999_999_000_i64)),
                    ("detail", json!("Created API key \"ci\" (read-only)")),
                ]),
            ],
        )]),
        None,
    );
    let resp = f
        .handler
        .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
        .unwrap();
    assert_eq!(resp.rows.len(), 2);
    assert_eq!(resp.rows[0].event_id, "2");
    assert_eq!(resp.rows[0].event_type, "team.invited");
    assert_eq!(resp.rows[0].severity, "info");
    assert_eq!(resp.rows[0].actor, "clpat_admin");
    assert!(
        resp.rows[0].ts.ends_with('Z'),
        "ISO-8601 ts: {}",
        resp.rows[0].ts
    );
    assert_eq!(resp.rows[1].event_id, "1");
    assert_eq!(resp.rows[1].event_type, "pat.created");
}

#[test]
fn iso8601_to_ms_round_trips_and_rejects_garbage() {
    // Exact inverse of `ms_to_iso8601` on the canonical (second-precision)
    // form, plus the lenient variants the contract may receive.
    assert_eq!(iso8601_to_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        iso8601_to_ms("2023-11-14T22:13:20Z"),
        Some(1_700_000_000_000)
    );
    assert_eq!(iso8601_to_ms("2023-11-14"), Some(1_699_920_000_000)); // bare date → 00:00:00Z
    assert_eq!(
        iso8601_to_ms("2023-11-14T22:13:20.999Z"), // fractional dropped
        Some(1_700_000_000_000)
    );
    assert_eq!(
        iso8601_to_ms("2023-11-14 22:13:20"),
        Some(1_700_000_000_000)
    ); // space sep, no Z
       // Garbage → None (caller then applies no filter).
    assert_eq!(iso8601_to_ms("not-a-date"), None);
    assert_eq!(iso8601_to_ms("2023-13-01"), None); // month out of range
    assert_eq!(iso8601_to_ms("2023-11-14T25:00:00Z"), None); // hour out of range
    assert_eq!(iso8601_to_ms(""), None);
}

#[test]
fn audit_query_applies_since_filter_in_sql_and_binds() {
    // `?from=` must reach the SQL as `ts_ms >= ?` with the ISO-8601 value
    // parsed into the integer ts_ms domain — not silently ignored.
    let f = fixture_with(MockD1::with(vec![]), None);
    let _ = f
        .handler
        .query(AuditQueryRequest::new(
            TENANT,
            "clpat_x",
            Some("2023-11-14T22:13:20Z".to_owned()),
            vec![],
            0,
        ))
        .unwrap();
    let calls = f.db.calls();
    let (sql, binds) = calls
        .iter()
        .find(|(s, _)| s.contains("FROM customer_audit_events"))
        .expect("the audit SELECT must have run");
    assert!(sql.contains("ts_ms >= ?2"), "since filter missing: {sql}");
    // tenant (?1), since-ms (?2), LIMIT (?3) — parameterized, no interpolation.
    assert_eq!(binds.len(), 3, "binds: {binds:?}");
    assert_eq!(binds[0], json!(TENANT));
    assert_eq!(binds[1], json!(1_700_000_000_000_i64));
    assert_eq!(binds[2], json!(AUDIT_QUERY_LIMIT));
    assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?3"), "{sql}");
}

#[test]
fn audit_query_applies_event_types_filter_in_sql_and_binds() {
    // `?kind=` must reach the SQL as a parameterized `event_type IN (…)`
    // with one BOUND placeholder per type (never string-interpolated).
    let f = fixture_with(MockD1::with(vec![]), None);
    let _ = f
        .handler
        .query(AuditQueryRequest::new(
            TENANT,
            "clpat_x",
            None,
            vec!["pat.created".to_owned(), "team.invited".to_owned()],
            0,
        ))
        .unwrap();
    let calls = f.db.calls();
    let (sql, binds) = calls
        .iter()
        .find(|(s, _)| s.contains("FROM customer_audit_events"))
        .expect("the audit SELECT must have run");
    assert!(
        sql.contains("event_type IN (?2, ?3)"),
        "event_types filter missing/not parameterized: {sql}"
    );
    // The values are BOUND, not embedded in the SQL string (no injection).
    assert!(!sql.contains("pat.created"), "value interpolated: {sql}");
    // tenant (?1), two event types (?2,?3), LIMIT (?4).
    assert_eq!(binds.len(), 4, "binds: {binds:?}");
    assert_eq!(binds[0], json!(TENANT));
    assert_eq!(binds[1], json!("pat.created"));
    assert_eq!(binds[2], json!("team.invited"));
    assert_eq!(binds[3], json!(AUDIT_QUERY_LIMIT));
    assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?4"), "{sql}");
}

#[test]
fn audit_query_combines_since_and_event_types_filters() {
    // Both filters together: placeholders stay correctly numbered and the
    // tenant scope stays fail-CLOSED at ?1.
    let f = fixture_with(MockD1::with(vec![]), None);
    let _ = f
        .handler
        .query(AuditQueryRequest::new(
            TENANT,
            "clpat_x",
            Some("2023-11-14T22:13:20Z".to_owned()),
            vec!["pat.created".to_owned()],
            0,
        ))
        .unwrap();
    let calls = f.db.calls();
    let (sql, binds) = calls
        .iter()
        .find(|(s, _)| s.contains("FROM customer_audit_events"))
        .expect("the audit SELECT must have run");
    assert!(sql.contains("WHERE tenant_id = ?1"), "{sql}");
    assert!(sql.contains("ts_ms >= ?2"), "{sql}");
    assert!(sql.contains("event_type IN (?3)"), "{sql}");
    assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?4"), "{sql}");
    assert_eq!(
        binds,
        &vec![
            json!(TENANT),
            json!(1_700_000_000_000_i64),
            json!("pat.created"),
            json!(AUDIT_QUERY_LIMIT),
        ]
    );
}

#[test]
fn audit_query_unparseable_since_applies_no_filter() {
    // A malformed `from=` is lenient: NO `ts_ms` clause, behaves as before.
    let f = fixture_with(MockD1::with(vec![]), None);
    let _ = f
        .handler
        .query(AuditQueryRequest::new(
            TENANT,
            "clpat_x",
            Some("garbage".to_owned()),
            vec![],
            0,
        ))
        .unwrap();
    let calls = f.db.calls();
    let (sql, binds) = calls
        .iter()
        .find(|(s, _)| s.contains("FROM customer_audit_events"))
        .expect("the audit SELECT must have run");
    assert!(
        !sql.contains("ts_ms >="),
        "bad since must not filter: {sql}"
    );
    assert_eq!(binds.len(), 2, "tenant + LIMIT only: {binds:?}");
    assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?2"), "{sql}");
}

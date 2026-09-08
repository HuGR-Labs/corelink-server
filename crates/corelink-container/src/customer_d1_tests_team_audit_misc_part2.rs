#[test]
fn audit_query_no_filters_matches_prior_shape() {
    // Regression guard: with neither param the SQL is the original
    // tenant-only, bounded, newest-first form.
    let f = fixture_with(MockD1::with(vec![]), None);
    let _ = f
        .handler
        .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0))
        .unwrap();
    let calls = f.db.calls();
    let (sql, binds) = calls
        .iter()
        .find(|(s, _)| s.contains("FROM customer_audit_events"))
        .expect("the audit SELECT must have run");
    assert!(!sql.contains("ts_ms >="), "{sql}");
    assert!(!sql.contains("event_type IN"), "{sql}");
    assert_eq!(binds, &vec![json!(TENANT), json!(AUDIT_QUERY_LIMIT)]);
    assert!(sql.ends_with("ORDER BY ts_ms DESC LIMIT ?2"), "{sql}");
}

// ── Misc plumbing ────────────────────────────────────────────────────────

#[test]
fn debug_is_redacted() {
    let f = fixture_with(MockD1::with(vec![]), None);
    let dbg = format!("{:?}", f.handler);
    assert!(dbg.contains("[CustomerD1]"), "{dbg}");
    assert!(!dbg.contains("0x42"), "no key material in Debug: {dbg}");
}

#[test]
fn audit_events_use_wall_clock_not_request_timestamp() {
    let f = fixture_with(MockD1::with(vec![]), None);
    // Request carries the routes' pinned at_unix_ms = 0.
    let _ = f
        .handler
        .query(AuditQueryRequest::new(TENANT, "clpat_x", None, vec![], 0));
    let rows = f.audit.snapshot().unwrap();
    assert!(!rows.is_empty());
    assert!(
        rows.iter().all(|e| e.at_unix_ms == NOW_MS),
        "audit timestamps must come from the wall clock, never the request"
    );
}

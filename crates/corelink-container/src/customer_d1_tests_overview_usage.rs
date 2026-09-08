#[test]
fn overview_happy_path_real_data() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(123_456_i64)),
                    ("bytes_quota", json!(10_000_000_i64)),
                ])],
            ),
            (
                "FROM tenant_billing",
                vec![row(&[
                    ("status", json!("paid")),
                    ("plan", json!("price_123")),
                    ("stripe_customer_id", json!("cus_42")),
                    ("current_period_end_ms", json!(1_700_500_000_000_i64)),
                ])],
            ),
            // byok_envelope: no rows → "none".
        ]),
        None,
    );
    let resp = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(resp.tenant_id, TENANT);
    assert_eq!(
        resp.tenant_name, TENANT,
        "tenant_name = tenant_id (HONEST v1)"
    );
    assert_eq!(resp.plan, "pro");
    assert_eq!(resp.usage.cas_bytes, 123_456);
    assert_eq!(resp.usage.quota_bytes, 10_000_000);
    assert_eq!(resp.usage.period, "2023-11");
    assert_eq!(resp.usage.reads, 0, "reads not tracked: honest 0");
    assert_eq!(resp.usage.writes, 0, "writes not tracked: honest 0");
    assert_eq!(resp.billing.status, "active", "paid → active (frozen map)");
    assert_eq!(
        resp.billing.next_invoice_at,
        ms_to_iso8601(1_700_500_000_000)
    );
    assert_eq!(resp.byok.status, "none", "no envelope row → none");
    assert!(
        resp.recent_activity.is_empty(),
        "no seeded customer_audit_events → honestly empty feed (BE-3)"
    );
    // Audit ordering: Attempted then Served.
    let kinds: Vec<_> = f.audit.snapshot().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AuditEventKind::OverviewAttempted,
            AuditEventKind::OverviewServed
        ]
    );
    // SLI emitted, success.
    let obs = f.sli.snapshot().unwrap();
    assert!(obs.iter().any(|o| !o.is_error));
}

#[test]
fn overview_recent_activity_reads_seeded_audit_events() {
    // BE-3: the overview snapshot feed reads the same `customer_audit_events`
    // (0077) surface the audit endpoint serves — newest-first, mapped to
    // `CustomerAuditEventRow`, `severity` = constant `info`. A seeded event
    // surfaces on the snapshot; it is NOT fabricated and NOT hard-empty.
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(1_i64)),
                    ("bytes_quota", json!(10_000_000_i64)),
                ])],
            ),
            (
                "FROM customer_audit_events",
                vec![row(&[
                    ("id", json!(7)),
                    ("event_type", json!("pat.created")),
                    ("actor", json!("clpat_admin")),
                    ("target", json!("pat-7")),
                    ("ts_ms", json!(1_700_000_000_000_i64)),
                    ("detail", json!("Created API key \"ci\" (read-only)")),
                ])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(
        resp.recent_activity.len(),
        1,
        "seeded customer_audit_events row surfaces on the snapshot (BE-3)"
    );
    assert_eq!(resp.recent_activity[0].event_id, "7");
    assert_eq!(resp.recent_activity[0].event_type, "pat.created");
    assert_eq!(resp.recent_activity[0].severity, "info");
    assert_eq!(resp.recent_activity[0].actor, "clpat_admin");
    assert!(
        resp.recent_activity[0].ts.ends_with('Z'),
        "ISO-8601 ts: {}",
        resp.recent_activity[0].ts
    );
}

#[test]
fn overview_unknown_tenant_is_not_found() {
    let f = fixture_with(MockD1::with(vec![]), None);
    let err = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::NotFound { .. }),
        "{err:?}"
    );
}

#[test]
fn overview_byok_envelope_present_surfaces_kill_switch_state() {
    let mut tenant = tenant_row_fixture();
    tenant.insert("byok_status".to_owned(), json!("degraded_read_only"));
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant]),
            ("FROM byok_envelope", vec![row(&[("present", json!(1))])]),
        ]),
        None,
    );
    let resp = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap();
    assert_eq!(resp.byok.status, "degraded_read_only");
}

#[test]
fn overview_fails_closed_on_d1_transport_error() {
    let f = fixture_with(MockD1::failing(), None);
    let err = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(matches!(err, CustomerHandlerError::Internal(_)), "{err:?}");
    // Error SLI emitted (fail-CLOSED observation).
    assert!(f.sli.snapshot().unwrap().iter().any(|o| o.is_error));
}

#[test]
fn overview_audit_failure_aborts_before_any_d1_query() {
    let f = fixture_with(MockD1::with(vec![]), None);
    f.audit.inject_failure("sink down").unwrap();
    let err = f
        .handler
        .overview(OverviewRequest::new(TENANT, "clpat_x", 0))
        .unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::AuditFailed(_)),
        "{err:?}"
    );
    assert!(
        f.db.calls().is_empty(),
        "fail-CLOSED: audit emit failure must abort BEFORE any lookup"
    );
}

// ── Usage ────────────────────────────────────────────────────────────────

#[test]
fn usage_happy_path_real_bytes_zero_ops_empty_daily() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
        .unwrap();
    assert_eq!(resp.period, "2023-11", "current period from the wall clock");
    assert_eq!(resp.cas_bytes, 512);
    assert_eq!(resp.quota_bytes, 1_000_000);
    assert_eq!(resp.reads, 0);
    assert_eq!(resp.writes, 0);
    assert_eq!(
        resp.request_count, 0,
        "no monthly_request_counts row → honest 0"
    );
    assert!(resp.daily.is_empty(), "no per-day table: honest []");
}

#[test]
fn usage_request_count_reads_monthly_counter() {
    // BE-1a: the running request counter the quota gate maintains
    // (monthly_request_counts, 0071) surfaces as a real usage-vs-quota
    // signal — a READ, never a hot-path write. Keyed on the same `YYYY-MM`
    // period as the storage read.
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
            (
                "FROM monthly_request_counts",
                vec![row(&[("request_count", json!(4_211_i64))])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
        .unwrap();
    assert_eq!(
        resp.request_count, 4_211,
        "seeded monthly_request_counts row surfaces (BE-1a)"
    );
}

#[test]
fn usage_historical_period_reports_honest_zero_bytes() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .usage(UsageRequest::new(
            TENANT,
            "clpat_x",
            Some("2023-01".to_owned()),
            0,
        ))
        .unwrap();
    assert_eq!(resp.period, "2023-01");
    assert_eq!(
        resp.cas_bytes, 0,
        "historical data is not retained — never relabel today's counter"
    );
    assert_eq!(
        resp.quota_bytes, 1_000_000,
        "the quota ceiling is still real"
    );
}

/// BE-1 + BE-2: `usage_daily` (0089) rollup — reads/writes/daily are the
/// real period sums, hit_rate = hits/(hits+misses), and the time/$ estimates
/// follow the frozen formulas.
#[test]
fn usage_daily_rollup_sums_days_and_computes_roi() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
            (
                "FROM usage_daily",
                vec![
                    row(&[
                        ("day", json!("2023-11-01")),
                        ("reads", json!(5_000_i64)),
                        ("writes", json!(1_000_i64)),
                        ("hits", json!(4_000_i64)),
                        ("misses", json!(1_000_i64)),
                    ]),
                    row(&[
                        ("day", json!("2023-11-02")),
                        ("reads", json!(3_000_i64)),
                        ("writes", json!(500_i64)),
                        ("hits", json!(2_000_i64)),
                        ("misses", json!(1_000_i64)),
                    ]),
                ],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
        .unwrap();
    // Real period sums (were hardcoded 0).
    assert_eq!(resp.reads, 8_000, "reads summed across days");
    assert_eq!(resp.writes, 1_500, "writes summed across days");
    // Daily series, oldest-first, cas_bytes always 0 (no per-day byte history).
    assert_eq!(resp.daily.len(), 2);
    assert_eq!(
        resp.daily[0],
        DailyUsageBucket::new("2023-11-01", 5_000, 1_000, 0)
    );
    assert_eq!(
        resp.daily[1],
        DailyUsageBucket::new("2023-11-02", 3_000, 500, 0)
    );
    // hit_rate = hits/(hits+misses) = 6000/8000 = 0.75.
    assert_eq!(resp.hit_rate, Some(0.75));
    // time_saved_seconds = hits * 15 = 6000 * 15 = 90_000.
    assert_eq!(resp.time_saved_seconds, 90_000);
    // dollars_saved_cents = round(90_000 * 0.0000111 * 100) = round(99.9) = 100.
    assert_eq!(resp.dollars_saved_cents, 100);
}

/// hit_rate is `None` (not a fabricated 0.0/1.0) when there were no cache
/// reads at all (hits + misses == 0) — even if writes happened. With no
/// hits, the time/$ estimates are honest 0.
#[test]
fn usage_hit_rate_none_when_no_cache_reads() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
            (
                "FROM usage_daily",
                vec![row(&[
                    ("day", json!("2023-11-01")),
                    ("reads", json!(0_i64)),
                    ("writes", json!(42_i64)),
                    ("hits", json!(0_i64)),
                    ("misses", json!(0_i64)),
                ])],
            ),
        ]),
        None,
    );
    let resp = f
        .handler
        .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
        .unwrap();
    assert_eq!(resp.writes, 42, "writes still surface");
    assert_eq!(
        resp.hit_rate, None,
        "no cache reads → honest null, never a fabricated rate"
    );
    assert_eq!(resp.time_saved_seconds, 0);
    assert_eq!(resp.dollars_saved_cents, 0);
}

/// The rollup is a READ-ONLY period `SELECT` bound to the tenant + period
/// (`day LIKE 'YYYY-MM-%'`) — never a hot-path write.
#[test]
fn usage_daily_rollup_is_read_only_and_period_scoped() {
    let f = fixture_with(
        MockD1::with(vec![
            ("FROM tenant WHERE", vec![tenant_row_fixture()]),
            (
                "FROM tenant_storage_state",
                vec![row(&[
                    ("bytes_used", json!(512_i64)),
                    ("bytes_quota", json!(1_000_000_i64)),
                ])],
            ),
        ]),
        None,
    );
    f.handler
        .usage(UsageRequest::new(TENANT, "clpat_x", None, 0))
        .unwrap();
    let rollup_call =
        f.db.calls()
            .into_iter()
            .find(|(sql, _)| sql.contains("FROM usage_daily"))
            .expect("usage_daily rollup query issued");
    let (sql, binds) = rollup_call;
    assert!(sql.trim_start().starts_with("SELECT"), "READ-ONLY select");
    assert!(!sql.contains("INSERT") && !sql.contains("UPDATE"));
    assert!(sql.contains("day LIKE ?2"));
    assert_eq!(binds[0], json!(TENANT), "bound to the caller tenant");
    assert_eq!(
        binds[1],
        json!("2023-11-%"),
        "period-scoped LIKE for the current period"
    );
}

#[test]
fn build_handlers_returns_usable_pair() {
    let (_store, audit) = build_handlers();
    let err = audit.emit(PilotAuditRow {
        event_type: "x".into(),
        principal: "p".into(),
        tenant_id: None,
        at_unix_ms: 0,
        exit_status: "ok".into(),
        payload: serde_json::Value::Null,
    });
    assert!(err.is_ok());
}

// ── create (in-memory store) ─────────────────────────────────────────────

#[test]
fn in_memory_store_create_inserts_new_tenant_in_new_state() {
    let (_st, store, _au, _c) = fixture();
    let id = Uuid::now_v7();
    let created = store.create(id, "acme-builds", 0, 1_000).expect("create");
    assert_eq!(created.tenant_id, id);
    assert_eq!(created.slug, "acme-builds");
    assert_eq!(created.tier, "free");
    assert_eq!(created.pilot_state, PilotState::New);
    assert_eq!(created.signup_at_ms, 1_000);
    assert!(created.tier_granted_at_ms.is_none());
    assert!(created.first_blob_at_ms.is_none());
    // The row is now durable in the store + grant-eligible.
    let fetched = store.get(id).expect("get").expect("present");
    assert_eq!(fetched, created);
}

#[test]
fn in_memory_store_create_rejects_duplicate_id() {
    let (_st, store, _au, _c) = fixture();
    let id = Uuid::now_v7();
    store.create(id, "a", 0, 1).expect("first");
    let err = store.create(id, "b", 0, 2).expect_err("dup");
    assert_eq!(err, "tenant already exists");
}

// ── create handler (router) ──────────────────────────────────────────────

/// Serialise a JSON request body to the raw `Bytes` the handlers now
/// accept (M3: body is parsed only AFTER the auth gate, so the handlers
/// take `axum::body::Bytes` rather than `Json<T>`). Takes a `Value` so
/// no production request struct needs a test-only `Serialize` derive.
fn body_bytes(body: &Value) -> axum::body::Bytes {
    axum::body::Bytes::from(serde_json::to_vec(body).expect("serialize body"))
}

/// Build the request headers that clear the operator gate.
fn operator_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    headers.insert(
        ADMIN_SCOPE_HEADER,
        REQUIRED_ADMIN_SCOPE.parse().expect("header"),
    );
    headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
    headers
}

#[tokio::test]
async fn handle_create_persists_and_returns_201() {
    let (state, store, audit, _c) = fixture();
    let resp = handle_create(
        State(state),
        operator_headers(),
        body_bytes(&json!({ "slug": "  beta-co  ", "cap_bytes": 42 })),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    // Exactly one NEW-state tenant landed, slug trimmed.
    let rows = store.list_by_state(PilotState::New, 0, 50).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].slug, "beta-co");
    assert_eq!(rows[0].cap_bytes, 42);
    // Audit: attempt then ok.
    let kinds: Vec<_> = audit
        .snapshot()
        .expect("audit")
        .into_iter()
        .map(|r| (r.event_type, r.exit_status))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (EVENT_TYPE_CREATED.to_owned(), "attempt".to_owned()),
            (EVENT_TYPE_CREATED.to_owned(), "ok".to_owned()),
        ]
    );
}

#[tokio::test]
async fn handle_create_rejects_empty_slug() {
    let (state, store, _audit, _c) = fixture();
    let resp = handle_create(
        State(state),
        operator_headers(),
        body_bytes(&json!({ "slug": "   " })),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(store
        .list_by_state(PilotState::New, 0, 50)
        .expect("list")
        .is_empty());
}

#[tokio::test]
async fn handle_create_rejects_without_internal_auth() {
    let (state, _store, _audit, _c) = fixture();
    // No operator secret header → 403 (gate is the PRIMARY boundary).
    let resp = handle_create(
        State(state),
        HeaderMap::new(),
        body_bytes(&json!({ "slug": "x" })),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// M3 regression: an unauthenticated caller supplying a syntactically
/// INVALID body must be rejected by the auth gate (403) BEFORE the body
/// is ever JSON-parsed — never a 400 body-parse error. A 400 here would
/// prove the body was parsed before the gate (the M3 anti-pattern).
#[tokio::test]
async fn handle_create_gates_auth_before_parsing_invalid_body() {
    let (state, _store, _audit, _c) = fixture();
    let resp = handle_create(
        State(state),
        HeaderMap::new(),
        axum::body::Bytes::from_static(b"not-json-at-all"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn handle_create_audit_failure_aborts_before_mutation() {
    let (state, store, audit, _c) = fixture();
    audit.inject_failure("sink down").expect("inject");
    let resp = handle_create(
        State(state),
        operator_headers(),
        body_bytes(&json!({ "slug": "x" })),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    // Fail-CLOSED: nothing persisted.
    assert!(store
        .list_by_state(PilotState::New, 0, 50)
        .expect("list")
        .is_empty());
}

// ── D1 store (hermetic mock, no live D1) ─────────────────────────────────

/// Hermetic mock [`PilotD1`]: canned result sets keyed by an SQL
/// fragment; every (sql, binds) recorded for assertions. Mirrors
/// `customer_d1::MockD1`.
#[derive(Debug, Default)]
struct MockPilotD1 {
    canned: Vec<(&'static str, Vec<D1Row>)>,
    calls: Mutex<Vec<(String, Vec<Value>)>>,
    fail: bool,
}

impl MockPilotD1 {
    fn with(canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
        Self {
            canned,
            calls: Mutex::new(Vec::new()),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<(String, Vec<Value>)> {
        self.calls.lock().expect("lock").clone()
    }
}

impl PilotD1 for MockPilotD1 {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        self.calls
            .lock()
            .expect("lock")
            .push((sql.to_owned(), binds));
        if self.fail {
            return Err("D1 HTTP 500: transport down".to_owned());
        }
        for (fragment, rows) in &self.canned {
            if sql.contains(fragment) {
                return Ok(rows.clone());
            }
        }
        Ok(Vec::new())
    }
}

fn d1_row(pairs: &[(&str, Value)]) -> D1Row {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn pilot_d1_row(id: Uuid, state: &str, signup: i64) -> D1Row {
    d1_row(&[
        ("tenant_id", json!(id.to_string())),
        ("slug", json!("acme")),
        ("tier", json!("free")),
        ("cap_bytes", json!(0_i64)),
        ("pilot_state", json!(state)),
        ("signup_at_ms", json!(signup)),
        ("tier_granted_at_ms", Value::Null),
        ("first_blob_at_ms", Value::Null),
    ])
}

#[test]
fn pilot_row_to_tenant_maps_every_column() {
    let id = Uuid::now_v7();
    let row = d1_row(&[
        ("tenant_id", json!(id.to_string())),
        ("slug", json!("acme-builds")),
        ("tier", json!("pilot")),
        ("cap_bytes", json!(100_i64)),
        ("pilot_state", json!("ACTIVE")),
        ("signup_at_ms", json!(1_000_i64)),
        ("tier_granted_at_ms", json!(5_000_i64)),
        ("first_blob_at_ms", Value::Null),
    ]);
    let t = pilot_row_to_tenant(&row).expect("map");
    assert_eq!(t.tenant_id, id);
    assert_eq!(t.slug, "acme-builds");
    assert_eq!(t.tier, "pilot");
    assert_eq!(t.cap_bytes, 100);
    assert_eq!(t.pilot_state, PilotState::Active);
    assert_eq!(t.signup_at_ms, 1_000);
    assert_eq!(t.tier_granted_at_ms, Some(5_000));
    assert!(t.first_blob_at_ms.is_none());
}

#[test]
fn pilot_row_to_tenant_fails_closed_on_missing_column() {
    let row = d1_row(&[("slug", json!("acme"))]);
    assert!(pilot_row_to_tenant(&row).is_err());
}

#[test]
fn d1_store_list_binds_state_limit_offset_and_maps_rows() {
    let id = Uuid::now_v7();
    let db = Arc::new(MockPilotD1::with(vec![(
        "FROM pilot_tenants",
        vec![pilot_d1_row(id, "NEW", 1_000)],
    )]));
    let store = D1PilotStore::new(db.clone());
    let rows = store.list_by_state(PilotState::New, 7, 25).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tenant_id, id);
    // Binds: state, limit, offset — values only, never the table name.
    let calls = db.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.contains("ORDER BY signup_at_ms ASC"));
    assert_eq!(calls[0].1, vec![json!("NEW"), json!(25_i64), json!(7_i64)]);
}

#[test]
fn d1_store_get_returns_none_for_absent_row() {
    let db = Arc::new(MockPilotD1::with(vec![]));
    let store = D1PilotStore::new(db);
    assert!(store.get(Uuid::now_v7()).expect("get").is_none());
}

#[test]
fn d1_store_create_pre_checks_then_inserts() {
    // Empty canned → get() returns None (free id), so create inserts.
    let db = Arc::new(MockPilotD1::with(vec![]));
    let store = D1PilotStore::new(db.clone());
    let id = Uuid::now_v7();
    let created = store.create(id, "beta", 9, 2_000).expect("create");
    assert_eq!(created.pilot_state, PilotState::New);
    assert_eq!(created.tier, "free");
    let calls = db.calls();
    // 1: existence SELECT, 2: INSERT.
    assert_eq!(calls.len(), 2);
    assert!(calls[0].0.contains("SELECT"));
    assert!(calls[1].0.contains("INSERT INTO pilot_tenants"));
    assert_eq!(
        calls[1].1,
        vec![
            json!(id.to_string()),
            json!("beta"),
            json!(9_i64),
            json!(2_000_i64),
        ]
    );
}

#[test]
fn d1_store_create_rejects_existing_id() {
    let id = Uuid::now_v7();
    let db = Arc::new(MockPilotD1::with(vec![(
        "FROM pilot_tenants",
        vec![pilot_d1_row(id, "NEW", 1)],
    )]));
    let store = D1PilotStore::new(db);
    let err = store.create(id, "x", 0, 1).expect_err("dup");
    assert_eq!(err, "tenant already exists");
}

#[test]
fn d1_store_grant_tier_rejects_already_active_without_update() {
    let id = Uuid::now_v7();
    let db = Arc::new(MockPilotD1::with(vec![(
        "FROM pilot_tenants",
        vec![pilot_d1_row(id, "ACTIVE", 1)],
    )]));
    let store = D1PilotStore::new(db.clone());
    let err = store
        .apply_grant_tier(id, "pilot", 1, 5)
        .expect_err("conflict");
    assert!(err.contains("not in grant-eligible state"));
    // Only the read happened — no UPDATE was issued.
    assert!(db.calls().iter().all(|(sql, _)| !sql.contains("UPDATE")));
}

#[test]
fn d1_store_fails_closed_on_transport_error() {
    let store = D1PilotStore::new(Arc::new(MockPilotD1::failing()));
    assert!(store.list_by_state(PilotState::New, 0, 50).is_err());
    assert!(store.get(Uuid::now_v7()).is_err());
}

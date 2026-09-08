// ── Mock D1 ──────────────────────────────────────────────────────────────

/// Hermetic mock row source: canned result sets keyed by an SQL
/// fragment, every executed (sql, binds) recorded for assertions.
/// Mirrors the billing_d1_http "no live D1 in unit tests" posture.
#[derive(Debug, Default)]
struct MockD1 {
    canned: Vec<(&'static str, Vec<D1Row>)>,
    calls: Mutex<Vec<(String, Vec<Value>)>>,
    state: Mutex<AtomicRows>,
    fail: bool,
    /// When `Some(fragment)`, ONLY the queries whose SQL contains
    /// `fragment` fail (every other query behaves normally). Lets a test
    /// fault a specific statement — e.g. the `customer_audit_events`
    /// insert — to prove the audit-fail-CLOSED / emit-before-mutate ordering.
    fail_on: Option<&'static str>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct AtomicRows {
    audit: usize,
    pats: usize,
    team_members: usize,
}

impl MockD1 {
    fn with(canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
        Self {
            canned,
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(AtomicRows::default()),
            fail: false,
            fail_on: None,
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    /// A mock that faults ONLY on queries whose SQL contains `fragment`.
    fn failing_on(fragment: &'static str, canned: Vec<(&'static str, Vec<D1Row>)>) -> Self {
        Self {
            canned,
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(AtomicRows::default()),
            fail: false,
            fail_on: Some(fragment),
        }
    }

    fn calls(&self) -> Vec<(String, Vec<Value>)> {
        self.calls.lock().unwrap().clone()
    }

    fn atomic_rows(&self) -> AtomicRows {
        *self.state.lock().unwrap()
    }
}

impl CustomerD1 for MockD1 {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        self.calls.lock().unwrap().push((sql.to_owned(), binds));
        if self.fail {
            return Err("D1 HTTP 500: transport down".to_owned());
        }
        if let Some(fragment) = self.fail_on {
            if sql.contains(fragment) {
                return Err(format!("D1 HTTP 500: induced failure on {fragment}"));
            }
        }
        for (fragment, rows) in &self.canned {
            if sql.contains(fragment) {
                return Ok(rows.clone());
            }
        }
        Ok(Vec::new())
    }

    fn create_pat_with_audit(
        &self,
        op: CustomerPatCreateOperation,
    ) -> Result<(), CustomerAtomicError> {
        self.calls.lock().unwrap().push((
            "INSERT INTO customer_audit_events (tenant_id, event_type, actor, target, ts_ms, detail) VALUES (?1, 'pat.created', ?2, ?3, ?4, ?5)".to_owned(),
            vec![json!(op.tenant_id.clone()), json!(op.audit_actor.clone()), json!(op.pat_id.clone()), json!(op.audit_ts_ms), json!(op.audit_detail.clone())],
        ));
        if self.fail {
            return Err(CustomerAtomicError::Transport("D1 HTTP 500: transport down".to_owned()));
        }
        if self.fail_on == Some("customer_audit_events") {
            return Err(CustomerAtomicError::Audit("induced audit failure".to_owned()));
        }
        self.calls.lock().unwrap().push((
            "INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, shown_once_token, shown_once_consumed, created_ms, token_id, name, find_only) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10)".to_owned(),
            vec![json!(op.pat_id), json!(op.tenant_id), json!(op.pat_hash), json!(op.scope), json!(op.expires_ms), json!(op.shown_once_token), json!(op.created_ms), json!(op.token_id), json!(op.name), json!(i32::from(op.find_only))],
        ));
        if self.fail_on == Some("INSERT INTO pat") {
            return Err(CustomerAtomicError::Mutation("induced PAT failure".to_owned()));
        }
        let mut staged = *self.state.lock().unwrap();
        staged.audit += 1;
        staged.pats += 1;
        *self.state.lock().unwrap() = staged;
        Ok(())
    }

    fn invite_team_member_with_audit(
        &self,
        op: CustomerTeamInviteOperation,
    ) -> Result<(), CustomerAtomicError> {
        self.calls.lock().unwrap().push((
            "INSERT INTO customer_audit_events (tenant_id, event_type, actor, target, ts_ms, detail) VALUES (?1, 'team.invited', ?2, ?3, ?4, ?5)".to_owned(),
            vec![json!(op.tenant_id.clone()), json!(op.audit_actor.clone()), json!(op.invitation_id.clone()), json!(op.audit_ts_ms), json!(op.audit_detail.clone())],
        ));
        if self.fail {
            return Err(CustomerAtomicError::Transport("D1 HTTP 500: transport down".to_owned()));
        }
        if self.fail_on == Some("customer_audit_events") {
            return Err(CustomerAtomicError::Audit("induced audit failure".to_owned()));
        }
        self.calls.lock().unwrap().push((
            "INSERT INTO team_member (tenant_id, user_id, email_hash, invitation_token_hash, role, status, invited_by, invited_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, 'invited', ?6, ?7)".to_owned(),
            vec![json!(op.tenant_id), json!(op.invitation_id), json!(op.email_hash), json!(op.invitation_token_hash), json!(op.role), json!(op.invited_by), json!(op.invited_at_ms)],
        ));
        if self.fail_on == Some("INSERT INTO team_member") {
            return Err(CustomerAtomicError::Mutation("induced team-member failure".to_owned()));
        }
        let mut staged = *self.state.lock().unwrap();
        staged.audit += 1;
        staged.team_members += 1;
        *self.state.lock().unwrap() = staged;
        Ok(())
    }
}

#[derive(Debug)]
struct MockPortal {
    url: &'static str,
    fail: bool,
}

impl PortalSessions for MockPortal {
    fn create(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
        _idempotency_key: &str,
    ) -> Result<String, String> {
        if self.fail {
            return Err("stripe down".to_owned());
        }
        assert!(!stripe_customer_id.is_empty());
        assert!(!return_url.is_empty());
        Ok(self.url.to_owned())
    }
}

fn row(pairs: &[(&str, Value)]) -> D1Row {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn tenant_row_fixture() -> D1Row {
    row(&[
        ("tenant_id", json!(TENANT)),
        ("tier", json!("pro")),
        ("clerk_user_id", json!("user_2abc")),
        ("byok_status", json!("active")),
        ("created_at_ms", json!(1_690_000_000_000_i64)),
    ])
}

struct Fixture {
    handler: D1CustomerHandler,
    db: Arc<MockD1>,
    audit: Arc<InMemoryAuditSink>,
    sli: Arc<InMemorySliObserver>,
}

fn fixture_with(db: MockD1, portal: Option<Arc<dyn PortalSessions>>) -> Fixture {
    let db = Arc::new(db);
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let signing = PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap();
    let handler = D1CustomerHandler::new(
        db.clone(),
        portal,
        "https://humangr.com/corelink/en/customer/billing".to_owned(),
        Some((Arc::new(signing), 1)),
        audit.clone(),
        sli.clone(),
        Arc::new(InMemoryFakeWallClock::at_unix_ms(NOW_MS)),
    );
    Fixture {
        handler,
        db,
        audit,
        sli,
    }
}

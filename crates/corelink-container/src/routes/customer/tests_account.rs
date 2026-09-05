use super::*;

use std::sync::Mutex;

use crate::storage::d1_http::D1Row;

/// Records the calls it captured; configurable to fail / report NotFound.
#[derive(Debug, Default)]
struct MockRequester {
    calls: Mutex<Vec<String>>,
    fail: bool,
    not_found: bool,
}

impl AccountDeletionRequester for MockRequester {
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError> {
        self.calls.lock().unwrap().push(tenant_id.to_owned());
        if self.not_found {
            return Err(AccountDeletionError::NotFound);
        }
        if self.fail {
            return Err(AccountDeletionError::Internal("boom".to_owned()));
        }
        Ok(())
    }
}

fn fixture_with_requester(req: Arc<dyn AccountDeletionRequester>) -> CustomerRouteState {
    let (mut state, _) = fixture();
    state.account_deletion = Some(req);
    state
}

#[tokio::test]
async fn account_delete_clerk_session_returns_202() {
    let requester = Arc::new(MockRequester::default());
    let app = router(fixture_with_requester(requester.clone()));
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-tenant-id", "t-acct")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "confirm": true }).to_string()))
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(requester.calls.lock().unwrap().as_slice(), ["t-acct"]);
}

#[tokio::test]
async fn account_delete_non_owner_clerk_session_is_403() {
    // A NON-owner team seat (admin/member/viewer, migration 0074) carries a
    // Clerk session and resolves to the OWNING tenant — but must NOT be able to
    // erase the WHOLE tenant. Gated OWNER-only; the requester is never reached.
    let requester = Arc::new(MockRequester::default());
    let app = router(fixture_with_requester(requester.clone()));
    for role in ["member", "admin", "viewer", ""] {
        let r = Request::builder()
            .uri("/v1/customer/account/delete")
            .method("POST")
            .header("x-corelink-tenant-id", "t-acct")
            .header("x-corelink-token-prefix", "clerk")
            .header("x-corelink-role", role)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(r).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "role {role:?} must be denied account deletion (owner-only)"
        );
    }
    assert!(
        requester.calls.lock().unwrap().is_empty(),
        "a non-owner must never reach the erasure requester"
    );
}

#[tokio::test]
async fn account_delete_pat_caller_is_403() {
    // A cache PAT (non-`clerk` prefix) must NOT trigger account erasure.
    let requester = Arc::new(MockRequester::default());
    let app = router(fixture_with_requester(requester.clone()));
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-tenant-id", "t-acct")
        .header("x-corelink-token-prefix", "clpat_abc")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(
        requester.calls.lock().unwrap().is_empty(),
        "a PAT caller must never reach the requester"
    );
}

#[tokio::test]
async fn account_delete_missing_tenant_is_401() {
    let app = router(fixture_with_requester(Arc::new(MockRequester::default())));
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn account_delete_unwired_fails_closed_503() {
    // account_deletion = None (dev/CI) → fail-CLOSED, never a silent 202.
    let (state, _) = fixture();
    let app = router(state);
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-tenant-id", "t-acct")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn account_delete_internal_error_is_500() {
    let requester = Arc::new(MockRequester {
        fail: true,
        ..MockRequester::default()
    });
    let app = router(fixture_with_requester(requester));
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-tenant-id", "t-acct")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn account_delete_no_account_is_idempotent_202() {
    let requester = Arc::new(MockRequester {
        not_found: true,
        ..MockRequester::default()
    });
    let app = router(fixture_with_requester(requester));
    let r = Request::builder()
        .uri("/v1/customer/account/delete")
        .method("POST")
        .header("x-corelink-tenant-id", "t-acct")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}

#[test]
fn deterministic_dsr_id_is_v5_shaped_and_stable() {
    let id = deterministic_dsr_id("user_2abc");
    // Canonical UUID shape, version nibble = 5, RFC-4122 variant (8/9/a/b).
    assert_eq!(id.len(), 36);
    let parts: Vec<&str> = id.split('-').collect();
    assert_eq!(
        parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12]
    );
    assert_eq!(&parts[2][0..1], "5", "version 5 nibble");
    assert!(
        matches!(&parts[3][0..1], "8" | "9" | "a" | "b"),
        "RFC-4122 variant"
    );
    // Deterministic.
    assert_eq!(id, deterministic_dsr_id("user_2abc"));
    assert_ne!(id, deterministic_dsr_id("user_other"));
}

/// Hermetic D1 mock: canned rows keyed by an SQL fragment; records writes.
#[derive(Debug, Default)]
struct MockReqD1 {
    tenant_rows: Vec<D1Row>,
    calls: Mutex<Vec<(String, Vec<Value>)>>,
}

impl crate::customer_d1::CustomerD1 for MockReqD1 {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        self.calls.lock().unwrap().push((sql.to_owned(), binds));
        if sql.contains("FROM tenant WHERE tenant_id") {
            return Ok(self.tenant_rows.clone());
        }
        Ok(Vec::new())
    }
}

#[derive(Debug, Default)]
struct MockSink {
    last: Mutex<Option<Value>>,
}

impl DsrErasureSink for MockSink {
    fn enqueue(&self, message: &Value) -> Result<(), String> {
        *self.last.lock().unwrap() = Some(message.clone());
        Ok(())
    }
}

fn d1row(pairs: &[(&str, Value)]) -> D1Row {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

// The requester reads the process-global `ERASURE_SALT_KEY`; all three
// behaviors are sequenced in ONE test so a parallel sibling can never observe
// a half-mutated env (the rest of the suite never touches this var).
#[test]
fn requester_dsr_anchor_message_and_salt_failclosed() {
    // Phase A: salt key SET → success; anchor written BEFORE enqueue; the
    // enqueued message mirrors buildErasureQueueMessage's shape.
    std::env::set_var("ERASURE_SALT_KEY", "test-erasure-salt-key-0123456789abcdef");
    let db = Arc::new(MockReqD1 {
        tenant_rows: vec![d1row(&[("clerk_user_id", json!("user_2abc"))])],
        ..MockReqD1::default()
    });
    let sink = Arc::new(MockSink::default());
    let requester = D1AccountDeletionRequester::new(db.clone(), sink.clone());
    requester.request_erasure("t-acct").expect("must succeed");

    let calls = db.calls.lock().unwrap().clone();
    let insert = calls
        .iter()
        .find(|(sql, _)| sql.contains("INSERT OR IGNORE INTO dsr_requested"))
        .expect("dsr_requested INSERT must run");
    let expected_dsr = deterministic_dsr_id("user_2abc");
    assert_eq!(insert.1[0], json!(expected_dsr));
    assert_eq!(insert.1[1], json!("t-acct"));

    let msg = sink
        .last
        .lock()
        .unwrap()
        .clone()
        .expect("a message was enqueued");
    assert_eq!(msg["schema"], DSR_QUEUED_SCHEMA);
    assert_eq!(msg["dsr_id"], json!(expected_dsr));
    assert_eq!(msg["tenant_id"], "t-acct");
    assert_eq!(msg["subject_id"], "t-acct");
    assert_eq!(msg["source"], "customer.account.delete");
    assert_eq!(msg["legal_hold"], json!(false));
    assert!(msg["erasure_salt_hex"]
        .as_str()
        .is_some_and(|s| s.len() == 64));

    // Phase B: no tenant row → NotFound, no enqueue.
    let db2 = Arc::new(MockReqD1::default());
    let sink2 = Arc::new(MockSink::default());
    let r2 = D1AccountDeletionRequester::new(db2, sink2.clone());
    assert!(matches!(
        r2.request_erasure("ghost"),
        Err(AccountDeletionError::NotFound)
    ));
    assert!(
        sink2.last.lock().unwrap().is_none(),
        "no enqueue on NotFound"
    );

    // Phase C: salt key UNSET → fail-CLOSED (Internal), no enqueue.
    std::env::remove_var("ERASURE_SALT_KEY");
    let db3 = Arc::new(MockReqD1 {
        tenant_rows: vec![d1row(&[("clerk_user_id", json!("user_2abc"))])],
        ..MockReqD1::default()
    });
    let sink3 = Arc::new(MockSink::default());
    let r3 = D1AccountDeletionRequester::new(db3, sink3.clone());
    assert!(matches!(
        r3.request_erasure("t-acct"),
        Err(AccountDeletionError::Internal(_))
    ));
    assert!(
        sink3.last.lock().unwrap().is_none(),
        "no enqueue when the salt key is unset (fail-CLOSED)"
    );
}

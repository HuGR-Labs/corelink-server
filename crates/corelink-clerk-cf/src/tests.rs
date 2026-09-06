use super::*;
use std::sync::Mutex;

type TestAuditBuf = Arc<Mutex<Vec<(HealthDoOp, String)>>>;

fn recorder() -> (AuditFn, TestAuditBuf) {
    let buf: TestAuditBuf = Arc::new(Mutex::new(Vec::new()));
    let buf_clone = Arc::clone(&buf);
    let f: AuditFn = Arc::new(move |op, subject| {
        buf_clone
            .lock()
            .map_err(|_| HealthDoError::Backend("audit lock poisoned".to_owned()))?
            .push((op, subject.to_owned()));
        Ok(())
    });
    (f, buf)
}

#[test]
fn parsed_route_happy_path() {
    let r = ParsedRoute::parse("POST", "/record/tnt0123/cid-xyz").expect("happy parse");
    assert_eq!(r.tenant_id, "tnt0123");
    assert_eq!(r.correlation_id, "cid-xyz");
    assert_eq!(r.method, HealthMethod::Post);
    assert_eq!(r.audit_subject(), "tenant:tnt0123:cid-xyz");
}

#[test]
fn parsed_route_rejects_missing_segments() {
    ParsedRoute::parse("GET", "/record/onlyTenant").expect_err("missing cid rejected");
    ParsedRoute::parse("GET", "/record//cid").expect_err("empty tenant rejected");
    ParsedRoute::parse("GET", "/wrongprefix/a/b").expect_err("non /record/ prefix rejected");
}

#[test]
fn parsed_route_rejects_bad_method() {
    ParsedRoute::parse("PATCH", "/record/a/b").expect_err("PATCH rejected");
}

#[test]
fn tenant_assertion_constant_time_path() {
    let r = ParsedRoute::parse("GET", "/record/tnt0123/cid").expect("parse");
    r.assert_tenant("tnt0123").expect("match");
    let err = r.assert_tenant("tntFFFF").expect_err("mismatch rejected");
    assert!(err.to_string().contains("tenant_scope"));
}

#[test]
fn upsert_get_delete_roundtrip() {
    let (audit, buf) = recorder();
    let mut logic = ClerkHealthLogic::new("tnt0123")
        .expect("ctor")
        .with_audit(audit);
    let route = ParsedRoute::parse("POST", "/record/tnt0123/cid1").expect("parse");
    let rec = logic.upsert(&route, "hello", 1_000_000).expect("upsert");
    assert_eq!(rec.note, "hello");
    assert_eq!(rec.created_at_ms, 1_000_000);

    let got = logic
        .get(&ParsedRoute::parse("GET", "/record/tnt0123/cid1").expect("parse"))
        .expect("get");
    assert_eq!(got, rec);

    let removed = logic
        .tombstone(&ParsedRoute::parse("DELETE", "/record/tnt0123/cid1").expect("parse"))
        .expect("delete");
    assert!(removed, "first delete returns true");

    let err = logic
        .get(&ParsedRoute::parse("GET", "/record/tnt0123/cid1").expect("parse"))
        .expect_err("absent after delete");
    assert!(matches!(err, HealthDoError::NotFound));

    let captured = buf.lock().expect("lock");
    // upsert + get + tombstone + get-after-delete = 4 audit events.
    // The advisory audit on GET fires BEFORE the storage lookup —
    // a 404-equivalent outcome does not suppress the trail.
    assert_eq!(captured.len(), 4, "events: {captured:?}");
    assert_eq!(captured[0].0, HealthDoOp::Upsert);
    assert_eq!(captured[1].0, HealthDoOp::Get);
    assert_eq!(captured[2].0, HealthDoOp::Tombstone);
    assert_eq!(captured[3].0, HealthDoOp::Get);
}

#[test]
fn cross_tenant_upsert_rejected_close() {
    let mut logic = ClerkHealthLogic::new("tntAAAA").expect("ctor");
    let route = ParsedRoute::parse("POST", "/record/tntBBBB/cidX").expect("parse");
    let err = logic
        .upsert(&route, "spoof", 1_000)
        .expect_err("cross-tenant rejected");
    assert!(matches!(err, HealthDoError::TenantScope(_)));
}

#[test]
fn audit_fail_closed_blocks_mutation() {
    let deny: AuditFn = Arc::new(|_op, _subj| Err(HealthDoError::AuditDenied("policy".to_owned())));
    let mut logic = ClerkHealthLogic::new("tnt0123")
        .expect("ctor")
        .with_audit(deny);
    let route = ParsedRoute::parse("POST", "/record/tnt0123/cid").expect("parse");
    let err = logic
        .upsert(&route, "note", 1_000)
        .expect_err("audit blocks upsert");
    assert!(matches!(err, HealthDoError::AuditDenied(_)));
    // State stayed empty — fail-CLOSED.
    assert!(logic.state().records().is_empty());
}

#[test]
fn sweep_removes_expired_records() {
    let (audit, buf) = recorder();
    let state = ClerkHealthState::new().with_ttl_ms(1_000);
    let mut logic = ClerkHealthLogic::new("tnt0123")
        .expect("ctor")
        .with_audit(audit)
        .with_state(state);
    let route_a = ParsedRoute::parse("POST", "/record/tnt0123/A").expect("parse");
    let route_b = ParsedRoute::parse("POST", "/record/tnt0123/B").expect("parse");
    logic.upsert(&route_a, "old", 1_000).expect("upsert A");
    logic.upsert(&route_b, "young", 5_000).expect("upsert B");
    // now=5500 → A is older than 5500-1000=4500 → A expires; B remains.
    let removed = logic.sweep(5_500).expect("sweep");
    assert_eq!(removed, 1);
    assert!(!logic.state().records().contains_key("A"));
    assert!(logic.state().records().contains_key("B"));
    let captured = buf.lock().expect("lock");
    // upsert + upsert + sweep(A) = 3 audit events.
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[2].0, HealthDoOp::Sweep);
    assert_eq!(captured[2].1, "tenant:tnt0123:A");
}

#[test]
fn record_size_limits_enforced() {
    HealthRecord::new("", "note", 0).expect_err("empty cid");
    let big_cid = "a".repeat(257);
    HealthRecord::new(big_cid, "note", 0).expect_err("oversize cid");
    let big_note = "x".repeat(1025);
    HealthRecord::new("cid", big_note, 0).expect_err("oversize note");
    HealthRecord::new("c\0id", "note", 0).expect_err("NUL in cid");
    HealthRecord::new("cid", "no\0te", 0).expect_err("NUL in note");
}

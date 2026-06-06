//! Integration tests for the `ClerkHealthDo` Durable Object actor class.
//!
//! These tests exercise the pure-logic surface
//! ([`corelink_clerk_cf::clerk_health_do::ClerkHealthLogic`]) on the
//! native target. The wasm32 actor shell (`ClerkHealthDo`,
//! gated under `#[cfg(target_arch = "wasm32")]`) delegates to the same
//! logic type, so this test suite pins the operational contract
//! (routing, audit emission, tenant-scope enforcement, TTL sweep)
//! WITHOUT requiring the wasm32 toolchain or a CF Workers
//! `workerd` runtime.
//!
//! # What each test pins
//!
//! - `happy_path_get_post_delete_round_trip` —
//!   POST → GET → DELETE → GET. Asserts: response shapes, audit
//!   emission count (= 4: upsert, get, tombstone, get-after-tombstone),
//!   final state cleared. Note: the second GET returns
//!   `HealthDoError::NotFound`, which the wasm32 actor maps to HTTP
//!   404 via `HealthDoError::status()`.
//! - `cross_tenant_post_rejected_close` — POST a tenant-B-shaped URL
//!   into a tenant-A-anchored actor. Asserts: HTTP 403-equivalent
//!   (TenantScope error), state remains empty.
//! - `ttl_alarm_sweep_removes_expired_records` — seed two records,
//!   sweep with a `now` past one of them. Asserts: only the expired
//!   record is removed; audit emits one Sweep event per removal.
//! - `audit_emission_count_equals_mutation_count` — three mutations
//!   (POST, POST, DELETE) produce exactly three audit events.
//! - `audit_fail_closed_blocks_mutation` — an audit hook that returns
//!   `Err` aborts every mutation; state stays empty.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::type_complexity,
    reason = "integration test code: assertion failures via panic are the canonical signal"
)]

use std::sync::{Arc, Mutex};

use corelink_clerk_cf::clerk_health_do::{
    AuditFn, ClerkHealthLogic, ClerkHealthState, HealthDoError, HealthDoOp, ParsedRoute,
};

/// Type alias for the recorder buffer — keeps the `recorder` return
/// signature within clippy's `type_complexity` budget.
type AuditBuf = Arc<Mutex<Vec<(HealthDoOp, String)>>>;

const TENANT_A: &str = "0123456789abcdef";
const TENANT_B: &str = "fedcba9876543210";
const CID_1: &str = "cid-0001-correlation";
const CID_2: &str = "cid-0002-correlation";

/// Build a recording audit hook backed by a `Vec<(op, subject)>`.
fn recorder() -> (AuditFn, AuditBuf) {
    let buf: AuditBuf = Arc::new(Mutex::new(Vec::new()));
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
fn happy_path_get_post_delete_round_trip() {
    let (audit, buf) = recorder();
    let mut logic = ClerkHealthLogic::new(TENANT_A)
        .expect("ctor")
        .with_audit(audit);

    let post =
        ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_1}")).expect("post parse");
    let rec = logic
        .upsert(&post, "hello-probe", 1_000_000)
        .expect("upsert");
    assert_eq!(rec.correlation_id, CID_1);
    assert_eq!(rec.note, "hello-probe");
    assert_eq!(rec.created_at_ms, 1_000_000);

    let get = ParsedRoute::parse("GET", &format!("/record/{TENANT_A}/{CID_1}")).expect("get parse");
    let got = logic.get(&get).expect("get");
    assert_eq!(got, rec);

    let del =
        ParsedRoute::parse("DELETE", &format!("/record/{TENANT_A}/{CID_1}")).expect("del parse");
    let existed = logic.tombstone(&del).expect("delete");
    assert!(existed, "first delete must signal existence");

    // GET after delete returns NotFound (HTTP 404 at the actor layer).
    let err = logic
        .get(&get)
        .expect_err("after-delete get returns 404-equivalent");
    assert!(matches!(err, HealthDoError::NotFound));
    assert_eq!(err.status(), 404);

    // Audit events: upsert, get, tombstone, get-attempt → 4 emissions.
    let captured = buf.lock().expect("lock").clone();
    assert_eq!(captured.len(), 4, "events: {captured:?}");
    assert_eq!(captured[0].0, HealthDoOp::Upsert);
    assert_eq!(captured[1].0, HealthDoOp::Get);
    assert_eq!(captured[2].0, HealthDoOp::Tombstone);
    assert_eq!(captured[3].0, HealthDoOp::Get);
    // All subjects follow the canonical `tenant:<id>:<cid>` shape.
    for (_, subject) in &captured {
        assert!(
            subject.starts_with(&format!("tenant:{TENANT_A}:")),
            "subject must be tenant-scoped: {subject}"
        );
        assert!(subject.ends_with(CID_1), "subject ends with cid: {subject}");
    }
    // Final state is empty after the round-trip.
    assert!(logic.state().records().is_empty(), "state cleared");
}

#[test]
fn cross_tenant_post_rejected_close() {
    // Anchor the actor to TENANT_A; the request URL carries TENANT_B
    // as the path tenant segment. The `assert_tenant` gate rejects the
    // mismatch BEFORE the audit hook fires, so no state mutates and
    // no audit event is recorded.
    let (audit, buf) = recorder();
    let mut logic = ClerkHealthLogic::new(TENANT_A)
        .expect("ctor")
        .with_audit(audit);

    let foreign =
        ParsedRoute::parse("POST", &format!("/record/{TENANT_B}/{CID_1}")).expect("parse");
    let err = logic
        .upsert(&foreign, "spoof", 5_000)
        .expect_err("cross-tenant rejected");
    match &err {
        HealthDoError::TenantScope(msg) => {
            assert!(
                msg.contains(TENANT_B) && msg.contains(TENANT_A),
                "tenant_scope message names both tenants: {msg}"
            );
        }
        other => panic!("expected TenantScope, got {other:?}"),
    }
    // Maps to HTTP 403 at the actor's response layer.
    assert_eq!(err.status(), 403);

    // No state mutation.
    assert!(logic.state().records().is_empty());
    // No audit event (the assert_tenant gate fires BEFORE the audit
    // emission — fail-CLOSED on the *validation* path).
    let captured = buf.lock().expect("lock").clone();
    assert!(
        captured.is_empty(),
        "cross-tenant must NOT emit audit (rejected pre-audit): {captured:?}"
    );
}

#[test]
fn ttl_alarm_sweep_removes_expired_records() {
    // Seed two records with a 1,000 ms TTL.
    let (audit, buf) = recorder();
    let state = ClerkHealthState::new().with_ttl_ms(1_000);
    let mut logic = ClerkHealthLogic::new(TENANT_A)
        .expect("ctor")
        .with_audit(audit)
        .with_state(state);

    let route1 =
        ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_1}")).expect("parse 1");
    let route2 =
        ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_2}")).expect("parse 2");
    logic.upsert(&route1, "old-probe", 1_000).expect("upsert 1");
    logic
        .upsert(&route2, "young-probe", 5_000)
        .expect("upsert 2");
    assert_eq!(logic.state().records().len(), 2);

    // Trigger the alarm at t=5_500ms. TTL=1000 → cutoff=4500. Record 1
    // (created at 1_000) is older than 4500 → expires. Record 2
    // (created at 5_000) survives.
    let removed = logic.sweep(5_500).expect("sweep");
    assert_eq!(removed, 1, "exactly one record swept");
    assert!(
        !logic.state().records().contains_key(CID_1),
        "expired record gone"
    );
    assert!(
        logic.state().records().contains_key(CID_2),
        "young record retained"
    );

    // Audit events: 2 upserts + 1 sweep = 3 events.
    let captured = buf.lock().expect("lock").clone();
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[2].0, HealthDoOp::Sweep);
    assert_eq!(captured[2].1, format!("tenant:{TENANT_A}:{CID_1}"));
}

#[test]
fn audit_emission_count_equals_mutation_count() {
    // Charter: every mutation MUST emit exactly one audit event before
    // the storage mutation runs. Three mutations (POST, POST, DELETE)
    // produce three audit events.
    let (audit, buf) = recorder();
    let mut logic = ClerkHealthLogic::new(TENANT_A)
        .expect("ctor")
        .with_audit(audit);

    let post1 = ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_1}")).expect("p1");
    let post2 = ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_2}")).expect("p2");
    let del1 = ParsedRoute::parse("DELETE", &format!("/record/{TENANT_A}/{CID_1}")).expect("d1");

    logic.upsert(&post1, "a", 1_000).expect("upsert 1");
    logic.upsert(&post2, "b", 2_000).expect("upsert 2");
    logic.tombstone(&del1).expect("tombstone 1");

    let captured = buf.lock().expect("lock").clone();
    let mutations: Vec<&HealthDoOp> = captured
        .iter()
        .map(|(op, _)| op)
        .filter(|op| op.is_mutation())
        .collect();
    assert_eq!(mutations.len(), 3, "3 mutations → 3 audit events");
    assert_eq!(*mutations[0], HealthDoOp::Upsert);
    assert_eq!(*mutations[1], HealthDoOp::Upsert);
    assert_eq!(*mutations[2], HealthDoOp::Tombstone);
}

#[test]
fn audit_fail_closed_blocks_every_mutation() {
    // An audit hook that always denies MUST block every mutation. The
    // actor's storage stays empty.
    let deny: AuditFn = Arc::new(|_op, _subj| Err(HealthDoError::AuditDenied("policy".to_owned())));
    let mut logic = ClerkHealthLogic::new(TENANT_A)
        .expect("ctor")
        .with_audit(deny);

    let post = ParsedRoute::parse("POST", &format!("/record/{TENANT_A}/{CID_1}")).expect("parse");
    let err = logic
        .upsert(&post, "x", 1_000)
        .expect_err("audit blocks upsert");
    assert!(matches!(err, HealthDoError::AuditDenied(_)));
    assert_eq!(err.status(), 403);

    let del = ParsedRoute::parse("DELETE", &format!("/record/{TENANT_A}/{CID_1}")).expect("parse");
    let err = logic.tombstone(&del).expect_err("audit blocks delete");
    assert!(matches!(err, HealthDoError::AuditDenied(_)));

    // State stayed empty across both attempted mutations.
    assert!(logic.state().records().is_empty(), "fail-CLOSED honored");
}

#[test]
fn route_parser_rejects_malformed_paths() {
    // Spot-check the path parser surface — these are the inputs the
    // actor's `fetch()` would receive on the wasm32 hot path.
    assert!(ParsedRoute::parse("POST", "/record/tnt/cid").is_ok());
    assert!(ParsedRoute::parse("GET", "/record/").is_err());
    assert!(ParsedRoute::parse("PATCH", "/record/a/b").is_err());
    assert!(ParsedRoute::parse("POST", "/other/a/b").is_err());
    assert!(ParsedRoute::parse("DELETE", "/record/a/b/c").is_err());
}

//! Integration tests for `corelink-turbo-bridge`.
//!
//! Exercises the full PUT/GET lifecycle, cross-tenant denial, opaque-hash
//! storage semantics, and telemetry endpoints using both `InMemoryTurboHandler`
//! (native handler) and `CasAdapterTurboHandler` (adapter over port traits).
//!
//! Test count target: ≥ 15.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use corelink_turbo_bridge::{
    adapter::{CasAdapterTurboHandler, InMemoryKvStore},
    audit::{InMemoryTurboAuditSink, TurboAuditEventKind},
    events::TurboEventsRequest,
    handler::{InMemoryTurboHandler, TurboArtifactHandler, TurboGetRequest, TurboPutRequest},
    status::TurboStatusRequest,
    TurboBridgeError, MAX_HASH_LEN, VERCEL_API_VERSION,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn inmem_handler() -> (Arc<InMemoryTurboAuditSink>, InMemoryTurboHandler) {
    let audit = Arc::new(InMemoryTurboAuditSink::new());
    let h = InMemoryTurboHandler::new(audit.clone());
    (audit, h)
}

fn adapter_handler() -> (
    Arc<InMemoryTurboAuditSink>,
    Arc<InMemoryKvStore>,
    CasAdapterTurboHandler,
) {
    let audit = Arc::new(InMemoryTurboAuditSink::new());
    let store = Arc::new(InMemoryKvStore::new());
    let h = CasAdapterTurboHandler::new(store.clone(), store.clone(), audit.clone());
    (audit, store, h)
}

// ── VERCEL_API_VERSION constant ───────────────────────────────────────────────

#[test]
fn vercel_api_version_is_v8() {
    assert_eq!(VERCEL_API_VERSION, "v8");
}

// ── InMemoryTurboHandler integration ─────────────────────────────────────────

#[test]
fn inmem_put_get_round_trip() {
    let (_, h) = inmem_handler();
    let payload = b"next.js build cache artifact contents".to_vec();
    h.put(TurboPutRequest::new(
        "a1b2c3d4",
        "team-oss",
        "my-monorepo",
        payload.clone(),
        Some(123),
        "ci_bot",
        "team-oss",
        1_000,
    ))
    .expect("put");
    let resp = h
        .get(TurboGetRequest::new(
            "a1b2c3d4",
            "team-oss",
            "my-monorepo",
            "ci_bot",
            "team-oss",
            2_000,
        ))
        .expect("get");
    assert_eq!(resp.bytes, payload, "round-trip bytes must match");
}

#[test]
fn inmem_put_response_contains_url_with_team_id() {
    let (_, h) = inmem_handler();
    let resp = h
        .put(TurboPutRequest::new(
            "hash1",
            "team_y",
            "app",
            b"x".to_vec(),
            None,
            "p",
            "team_y",
            1,
        ))
        .expect("put");
    assert!(!resp.urls.is_empty(), "urls must not be empty");
    let url = &resp.urls[0];
    assert!(url.contains("team_y"), "URL should contain team_id");
    assert!(url.contains("hash1"), "URL should contain hash");
}

#[test]
fn inmem_cross_tenant_isolated_via_caller_tenant() {
    // NEW invariant: isolation is the `caller_tenant` storage dimension, not a
    // `team_id == caller_tenant` denial. An artifact PUT under
    // caller_tenant="tenantA" is unreachable from caller_tenant="tenantB" with
    // the SAME team_id+hash — a NotFound MISS, not a denial.
    let (_, h) = inmem_handler();
    h.put(TurboPutRequest::new(
        "hX",
        "shared_team",
        "s",
        b"tenantA bytes".to_vec(),
        None,
        "userA",
        "tenantA",
        1,
    ))
    .expect("put under tenantA");
    let err = h
        .get(TurboGetRequest::new(
            "hX",
            "shared_team",
            "s",
            "userB",
            "tenantB",
            2,
        ))
        .expect_err("tenantB isolated from tenantA");
    assert!(matches!(err, TurboBridgeError::NotFound { .. }));
}

#[test]
fn inmem_no_denied_audit_rows_on_normal_path() {
    // Repurposed: with the tautology gone there is no *Denied audit kind on the
    // turbo path. A normal PUT then GET round-trip emits only Attempted/
    // Committed/Served — never PutDenied/GetDenied.
    let (audit, h) = inmem_handler();
    h.put(TurboPutRequest::new(
        "hX",
        "team1",
        "s",
        b"bytes".to_vec(),
        None,
        "p",
        "tenant1",
        1,
    ))
    .expect("put");
    h.get(TurboGetRequest::new("hX", "team1", "s", "p", "tenant1", 2))
        .expect("get");
    let rows = audit.snapshot().expect("snap");
    assert!(rows
        .iter()
        .all(|r| r.kind != TurboAuditEventKind::PutDenied
            && r.kind != TurboAuditEventKind::GetDenied));
}

#[test]
fn inmem_get_missing_artifact_returns_not_found() {
    let (_, h) = inmem_handler();
    let err = h
        .get(TurboGetRequest::new("ghost", "t1", "s", "p", "t1", 1))
        .expect_err("not found");
    assert!(matches!(err, TurboBridgeError::NotFound { .. }));
}

#[test]
fn inmem_hash_too_long_rejected_without_audit() {
    let (audit, h) = inmem_handler();
    let long = "f".repeat(MAX_HASH_LEN + 1);
    let err = h
        .put(TurboPutRequest::new(
            long,
            "t1",
            "s",
            vec![],
            None,
            "p",
            "t1",
            1,
        ))
        .expect_err("too long");
    assert!(matches!(err, TurboBridgeError::HashTooLong { .. }));
    assert!(
        audit.snapshot().expect("snap").is_empty(),
        "no audit events for DoS path"
    );
}

#[test]
fn inmem_opaque_hash_round_trip() {
    // Simulates a real Turbo client sending an xxhash instead of sha256.
    let (_, h) = inmem_handler();
    let opaque_hash = "turbo::xxh3::9f4b2e1a"; // not valid hex sha256
    let data = b"compiled JS bundle".to_vec();
    h.put(TurboPutRequest::new(
        opaque_hash,
        "team1",
        "webapp",
        data.clone(),
        None,
        "gh_actions",
        "team1",
        100,
    ))
    .expect("opaque hash accepted");
    let resp = h
        .get(TurboGetRequest::new(
            opaque_hash,
            "team1",
            "webapp",
            "gh_actions",
            "team1",
            200,
        ))
        .expect("opaque hash retrieved");
    assert_eq!(resp.bytes, data);
}

#[test]
fn inmem_two_teams_isolated_same_hash() {
    let (_, h) = inmem_handler();
    let data_a = b"team_a bundle".to_vec();
    let data_b = b"team_b bundle".to_vec();
    h.put(TurboPutRequest::new(
        "shared",
        "team_a",
        "mono",
        data_a.clone(),
        None,
        "p",
        "team_a",
        1,
    ))
    .expect("put a");
    h.put(TurboPutRequest::new(
        "shared",
        "team_b",
        "mono",
        data_b.clone(),
        None,
        "p",
        "team_b",
        2,
    ))
    .expect("put b");
    let ra = h
        .get(TurboGetRequest::new(
            "shared", "team_a", "mono", "p", "team_a", 3,
        ))
        .expect("get a");
    let rb = h
        .get(TurboGetRequest::new(
            "shared", "team_b", "mono", "p", "team_b", 4,
        ))
        .expect("get b");
    assert_eq!(ra.bytes, data_a, "team_a gets its own data");
    assert_eq!(rb.bytes, data_b, "team_b gets its own data");
}

#[test]
fn inmem_events_accepts_any_body() {
    let (_, h) = inmem_handler();
    for body in [b"{}".as_ref(), b"".as_ref(), b"garbage".as_ref()] {
        h.events(TurboEventsRequest::new(body.to_vec(), "p", 1))
            .expect("events always 200");
    }
}

#[test]
fn inmem_status_always_enabled() {
    let (_, h) = inmem_handler();
    let _ = TurboStatusRequest::new("p", 1); // verify constructor exists
    let s = h.status().expect("status");
    assert_eq!(s.status, "enabled");
}

// ── CasAdapterTurboHandler integration ───────────────────────────────────────

#[test]
fn adapter_put_get_round_trip() {
    let (_, _, h) = adapter_handler();
    let data = b"esbuild output chunk 42".to_vec();
    h.put(TurboPutRequest::new(
        "adapterHash",
        "orgA",
        "infra",
        data.clone(),
        None,
        "ci",
        "orgA",
        1,
    ))
    .expect("adapter put");
    let resp = h
        .get(TurboGetRequest::new(
            "adapterHash",
            "orgA",
            "infra",
            "ci",
            "orgA",
            2,
        ))
        .expect("adapter get");
    assert_eq!(resp.bytes, data);
}

#[test]
fn adapter_cross_tenant_isolated_via_caller_tenant() {
    // NEW invariant on the adapter (port-trait) path: cross-tenant access is an
    // isolation MISS via the `caller_tenant` storage dimension, not a denial.
    let (_, _, h) = adapter_handler();
    h.put(TurboPutRequest::new(
        "h2",
        "shared_team",
        "s",
        b"tenantA bytes".to_vec(),
        None,
        "userA",
        "tenantA",
        1,
    ))
    .expect("put under tenantA");
    let err = h
        .get(TurboGetRequest::new(
            "h2",
            "shared_team",
            "s",
            "userB",
            "tenantB",
            2,
        ))
        .expect_err("tenantB isolated from tenantA");
    assert!(matches!(err, TurboBridgeError::NotFound { .. }));
}

#[test]
fn adapter_audit_ordering_attempted_before_committed() {
    let (audit, _, h) = adapter_handler();
    h.put(TurboPutRequest::new(
        "orderH",
        "tOrg",
        "s",
        b"bytes".to_vec(),
        None,
        "p",
        "tOrg",
        1,
    ))
    .expect("put");
    let rows = audit.snapshot().expect("snap");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].kind,
        TurboAuditEventKind::PutAttempted,
        "attempted must come first"
    );
    assert_eq!(
        rows[1].kind,
        TurboAuditEventKind::PutCommitted,
        "committed must come second"
    );
}

#[test]
fn adapter_status_enabled() {
    let (_, _, h) = adapter_handler();
    assert_eq!(h.status().expect("status").status, "enabled");
}

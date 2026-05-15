//! Integration tests for the multipart in-flight inventory at failover
//! (DEBT-011 P2-003).
//!
//! Mirrors the pattern of `failback_outbox_drain.rs` in
//! `corelink-failover-router`: drill-matrix scenario (seed in-flight → drain
//! → assert one CloudEvent per row + per-tenant grouping + audit
//! fail-CLOSED).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use corelink_r2_multipart::failover::{
    AbortedSessionRow, FailingMultipartAuditSink, InMemoryMultipartAuditSink,
    InMemoryMultipartFailoverInventory, MultipartFailoverError, MultipartFailoverInventory,
    CLOUDEVENT_TYPE_MULTIPART_ABORTED, METRIC_FAILOVER_MULTIPART_ABORTED_TOTAL,
};
use corelink_r2_multipart::types::SessionState;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

fn make_row(upload: &str, tenant: Uuid, bytes: u64, init_offset_secs: u64) -> AbortedSessionRow {
    AbortedSessionRow {
        upload_id: upload.to_string(),
        tenant_id: tenant,
        object_key: format!("tenants/{tenant}/blobs/{upload}"),
        initiated_at: SystemTime::UNIX_EPOCH + Duration::from_secs(init_offset_secs),
        bytes_uploaded: bytes,
    }
}

#[test]
fn metric_and_event_names_are_load_bearing_canonical() {
    // LOAD-BEARING: on-call dashboard panel + audit chain matchers depend on
    // these exact strings.
    assert_eq!(
        METRIC_FAILOVER_MULTIPART_ABORTED_TOTAL,
        "corelink_failover_multipart_aborted_total"
    );
    assert_eq!(
        CLOUDEVENT_TYPE_MULTIPART_ABORTED,
        "corelink.failover.multipart_aborted.v1"
    );
}

#[test]
fn drill_matrix_three_in_flight_uploads_all_emit_cloudevents() {
    // Acceptance test plan: "staging dry-run with 3 synthetic in-flight
    // uploads; verify all three emit `multipart_aborted` and on-call
    // playbook fires."
    let inv = InMemoryMultipartFailoverInventory::new();
    let enterprise_tenant = Uuid::new_v4();
    let team_tenant = Uuid::new_v4();
    inv.seed_in_flight(make_row("u-large", enterprise_tenant, 5_000_000_000, 0));
    inv.seed_in_flight(make_row("u-med", enterprise_tenant, 1_000_000_000, 60));
    inv.seed_in_flight(make_row("u-small", team_tenant, 250_000, 120));
    assert_eq!(inv.in_flight_count(), 3);

    let sink = InMemoryMultipartAuditSink::new();
    let drained_at = SystemTime::UNIX_EPOCH + Duration::from_secs(300);
    let report = inv
        .drain_and_inventory(&sink, drained_at)
        .expect("drain ok");

    // Success metric: 100% of in-flight sessions accounted for in audit.
    assert_eq!(report.session_count(), 3);
    assert_eq!(sink.events().len(), 3);
    // Every event carries the canonical type + drain-time timestamp.
    for ev in sink.events() {
        assert_eq!(ev.event_type, CLOUDEVENT_TYPE_MULTIPART_ABORTED);
        assert_eq!(ev.aborted_at, drained_at);
    }
}

#[test]
fn per_tenant_grouping_drives_on_call_comms_template() {
    // Acceptance §2: on-call comms template is per-tenant ("you have N
    // aborted uploads") — the report MUST surface a `by_tenant` map.
    let inv = InMemoryMultipartFailoverInventory::new();
    let t_ent = Uuid::new_v4();
    let t_team = Uuid::new_v4();
    inv.seed_in_flight(make_row("u1", t_ent, 100, 0));
    inv.seed_in_flight(make_row("u2", t_ent, 200, 0));
    inv.seed_in_flight(make_row("u3", t_ent, 300, 0));
    inv.seed_in_flight(make_row("u4", t_team, 50, 0));

    let sink = InMemoryMultipartAuditSink::new();
    let report = inv
        .drain_and_inventory(&sink, SystemTime::UNIX_EPOCH)
        .expect("drain ok");

    let by = report.by_tenant();
    assert_eq!(by.len(), 2, "two distinct tenants in the inventory");
    assert_eq!(
        by.get(&t_ent).map(Vec::len),
        Some(3),
        "enterprise tenant has 3 in-flight"
    );
    assert_eq!(
        by.get(&t_team).map(Vec::len),
        Some(1),
        "team tenant has 1 in-flight"
    );
    // Total bytes informs the post-failover review.
    assert_eq!(report.total_bytes_aborted(), 100 + 200 + 300 + 50);
}

#[test]
fn audit_emit_failure_propagates_fail_closed() {
    // Audit fail-CLOSED ordering (S-06 P0-2 lesson, reaffirmed): if any
    // per-session emit fails, the inventory call MUST return
    // AuditEmitFailed — never a partial report — so the drain step can
    // SEV-2 + manual-reconcile.
    let inv = InMemoryMultipartFailoverInventory::new();
    inv.seed_in_flight(make_row("u1", Uuid::new_v4(), 1, 0));
    inv.seed_in_flight(make_row("u2", Uuid::new_v4(), 1, 0));
    let sink = FailingMultipartAuditSink::new("audit bus partition");
    let r = inv.drain_and_inventory(&sink, SystemTime::UNIX_EPOCH);
    match r {
        Err(MultipartFailoverError::AuditEmitFailed(m)) => {
            assert!(m.contains("audit bus partition"));
        }
        other => panic!("expected AuditEmitFailed, got {other:?}"),
    }
}

#[test]
fn completed_sessions_are_not_aborted_idempotency() {
    // Re-drain semantics: only `InProgress` sessions are picked up. A second
    // drain after the first must be a no-op (zero events emitted) — proves
    // the orchestrator can safely retry the drain step without emitting
    // duplicate CloudEvents.
    let inv = InMemoryMultipartFailoverInventory::new();
    let t = Uuid::new_v4();
    inv.seed_terminal(make_row("u-done", t, 100, 0), SessionState::Completed);
    inv.seed_terminal(make_row("u-abrt", t, 100, 0), SessionState::Aborted);
    let sink = InMemoryMultipartAuditSink::new();
    let report = inv
        .drain_and_inventory(&sink, SystemTime::UNIX_EPOCH)
        .expect("drain ok");
    assert_eq!(report.session_count(), 0);
    assert!(sink.events().is_empty());
}

#[test]
fn inventory_query_failure_emits_no_audit_events() {
    // Failure mode (e.g. D1 outage during enumeration): the inventory call
    // must NOT emit *any* audit events (no partial state). The drain step
    // SEV-2's and retries.
    let inv = InMemoryMultipartFailoverInventory::new();
    inv.force_query_failure("D1 outage during drain");
    let sink = InMemoryMultipartAuditSink::new();
    let r = inv.drain_and_inventory(&sink, SystemTime::UNIX_EPOCH);
    assert!(matches!(
        r,
        Err(MultipartFailoverError::InventoryQueryFailed(_))
    ));
    assert!(
        sink.events().is_empty(),
        "no audit events emitted when query failed"
    );
}

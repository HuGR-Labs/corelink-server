//! Failback `audit_outbox` drain guard E2E tests
//! (closes DEBT-011 R-PREP-REPL-P1-002).
//!
//! Covers:
//! 1. Failback orchestrator refuses re-engagement when old primary's
//!    `audit_outbox` has un-drained rows (`emitted_at IS NULL`).
//! 2. Refusal emits the canonical
//!    `corelink_failback_blocked_total{reason="audit_outbox_dirty"}` counter
//!    AND a `failover.resolved`-class audit record with the canonical detail
//!    string — both BEFORE the error returns (audit fail-CLOSED preserved).
//! 3. Counter emit failure does NOT change the gate decision (canonical
//!    refusal is preserved; observability is best-effort).
//! 4. Audit emit failure on a dirty outbox returns `FailoverError::Audit` —
//!    never silently allow re-engagement.
//! 5. Outbox query failure is treated as fail-CLOSED (dirty).
//! 6. Mark-all-drained unblocks subsequent failback (the drill matrix scenario
//!    referenced by `active-failover-drill.sh --staging`).
//!
//! These tests EXERCISE the new path that DR-16 §1 / §7 will consume.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests use direct assertions"
)]

use corelink_failover_router::{
    assert_outbox_drained_or_block, FailingAuditOutbox, FailingFailbackBlockedCounter,
    FailingFailoverAuditSink, FailoverAuditEventType, FailoverError, InMemoryAuditOutbox,
    InMemoryFailbackBlockedCounter, InMemoryFailoverAuditSink, Region,
    FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY, METRIC_FAILBACK_BLOCKED_TOTAL,
};

#[test]
fn metric_and_reason_constants_canonical() {
    // LOAD-BEARING for alerting rule label matchers in slo_catalog.md.
    assert_eq!(
        METRIC_FAILBACK_BLOCKED_TOTAL,
        "corelink_failback_blocked_total"
    );
    assert_eq!(
        FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY,
        "audit_outbox_dirty"
    );
}

#[test]
fn clean_outbox_allows_failback_with_no_audit_emit() {
    let outbox = InMemoryAuditOutbox::new();
    let audit = InMemoryFailoverAuditSink::new();
    let counter = InMemoryFailbackBlockedCounter::new();

    let r = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        1_700_000_000_000,
    );
    assert!(r.is_ok(), "clean outbox must allow failback");
    // Nominal path emits neither audit nor counter (success is logged by
    // the caller after the lease flip, per RB §7.2 step 7).
    assert_eq!(audit.records().len(), 0);
    assert_eq!(counter.increments().len(), 0);
}

#[test]
fn dirty_outbox_refuses_and_emits_observability() {
    let outbox = InMemoryAuditOutbox::new();
    let audit = InMemoryFailoverAuditSink::new();
    let counter = InMemoryFailbackBlockedCounter::new();
    outbox.seed_undrained(Region::Wnam, 4);

    let r = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        1_700_000_000_001,
    );
    match r {
        Err(FailoverError::WriteBlockedDuringFailover { region }) => {
            assert_eq!(region, Region::Wnam);
        }
        other => panic!("expected WriteBlockedDuringFailover, got {other:?}"),
    }

    // Canonical audit emit.
    let records = audit.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].event_type, FailoverAuditEventType::FailoverResolved);
    assert!(records[0].detail.contains("audit_outbox_dirty"));
    assert!(records[0].detail.contains("undrained=4"));
    assert_eq!(records[0].primary_region, "wnam");
    assert_eq!(records[0].replica_region, "enam");

    // Canonical counter emit (best-effort but successful here).
    assert_eq!(
        counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
        1
    );
}

#[test]
fn refusal_preserved_when_counter_fails_observability_best_effort() {
    // The gate decision is canonical; observability emit failure must not
    // mask the refusal.
    let outbox = InMemoryAuditOutbox::new();
    let audit = InMemoryFailoverAuditSink::new();
    let counter = FailingFailbackBlockedCounter::new("prometheus pushgateway down");
    outbox.seed_undrained(Region::Wnam, 1);

    let r = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        0,
    );
    assert!(
        matches!(r, Err(FailoverError::WriteBlockedDuringFailover { .. })),
        "counter failure must NOT mask refusal"
    );
    // Audit emit still happened — the forensic chain is canonical.
    assert_eq!(audit.records().len(), 1);
}

#[test]
fn audit_emit_failure_surfaces_audit_error_never_silent_allow() {
    // Strict fail-CLOSED: if audit emit itself fails on a dirty-outbox refusal,
    // surface FailoverError::Audit. NEVER silently allow re-engagement.
    let outbox = InMemoryAuditOutbox::new();
    let audit = FailingFailoverAuditSink::new("audit-chain unreachable");
    let counter = InMemoryFailbackBlockedCounter::new();
    outbox.seed_undrained(Region::Wnam, 1);

    let r = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        0,
    );
    match r {
        Err(FailoverError::Audit(msg)) => {
            assert!(msg.contains("audit-chain unreachable"));
        }
        other => panic!("expected Audit error, got {other:?}"),
    }
}

#[test]
fn outbox_query_failure_is_treated_as_dirty_fail_closed() {
    let outbox = FailingAuditOutbox::new("d1 read timed out");
    let audit = InMemoryFailoverAuditSink::new();
    let counter = InMemoryFailbackBlockedCounter::new();

    let r = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        0,
    );
    match r {
        Err(FailoverError::Internal(msg)) => {
            assert!(msg.contains("audit_outbox query failed"));
            assert!(msg.contains("d1 read timed out"));
        }
        other => panic!("expected Internal error on query fail, got {other:?}"),
    }
    // Forensic audit trail recorded.
    let records = audit.records();
    assert_eq!(records.len(), 1);
    assert!(records[0].detail.contains("outbox_query_failed"));
    // Observability counter incremented.
    assert_eq!(
        counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
        1
    );
}

#[test]
fn drill_matrix_scenario_dirty_then_drain_then_unblocked() {
    // Reproduces the new E2E scenario added to
    // `active-failover-drill.sh --staging` matrix:
    //   1. seed dirty outbox on old primary
    //   2. attempt failback → refused
    //   3. drain outbox
    //   4. retry failback → succeeds
    let outbox = InMemoryAuditOutbox::new();
    let audit = InMemoryFailoverAuditSink::new();
    let counter = InMemoryFailbackBlockedCounter::new();

    // Step 1
    outbox.seed_undrained(Region::Wnam, 5);

    // Step 2 — refused
    let r1 = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        1,
    );
    assert!(matches!(
        r1,
        Err(FailoverError::WriteBlockedDuringFailover { .. })
    ));

    // Step 3
    outbox.mark_all_drained(Region::Wnam);

    // Step 4 — succeeds
    let r2 = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        2,
    );
    assert!(r2.is_ok());

    // Audit records: exactly the one refusal.
    assert_eq!(audit.records().len(), 1);
    // Counter: exactly the one increment from the refusal.
    assert_eq!(
        counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
        1
    );
}

#[test]
fn refusal_per_region_no_cross_contamination() {
    // Dirty outbox on WNAM must not block a WEUR→SAM failback.
    let outbox = InMemoryAuditOutbox::new();
    let audit = InMemoryFailoverAuditSink::new();
    let counter = InMemoryFailbackBlockedCounter::new();
    outbox.seed_undrained(Region::Wnam, 99);

    // WEUR → SAM (sibling pair, WEUR clean).
    let r_ok = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Weur,
        Region::Sam,
        0,
    );
    assert!(r_ok.is_ok(), "clean WEUR must allow failback");

    // WNAM → ENAM (sibling pair, WNAM dirty).
    let r_blocked = assert_outbox_drained_or_block(
        &outbox,
        &audit,
        &counter,
        Region::Wnam,
        Region::Enam,
        0,
    );
    assert!(matches!(
        r_blocked,
        Err(FailoverError::WriteBlockedDuringFailover { .. })
    ));

    // Exactly one refusal recorded.
    assert_eq!(audit.records().len(), 1);
    assert_eq!(
        counter.count_for(FAILBACK_BLOCKED_REASON_AUDIT_OUTBOX_DIRTY),
        1
    );
}

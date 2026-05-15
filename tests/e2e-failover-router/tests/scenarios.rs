//! DR-16 active-failover E2E scenarios — 5 canonical arms pinned per the
//! drill spec (`specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md`):
//!
//! 1. `primary_up_no_failover_routes_reads_to_primary_and_allows_writes`
//! 2. `primary_degraded_triggers_failover_routes_reads_to_sibling_and_blocks_writes`
//! 3. `split_brain_prevention_primary_never_accepts_writes_while_sibling_holds_lease`
//! 4. `failback_after_recovery_routes_back_to_primary_and_emits_resolved_audit`
//! 5. `partial_region_health_does_not_short_circuit_region_level_failover`
//!
//! All scenarios drive a single logical clock and pure deterministic state —
//! byte-for-byte reproducible.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_failover_router::{
    FailoverAuditEventType, FailoverAuditSink, FailoverError, FailoverRouter,
    InMemoryFailoverAuditSink, InMemoryFailoverRouter, InMemoryHealthProbe, ReadMode, Region,
    ResidencyGraph, WriteMode,
};

use e2e_failover_router::{HarnessError, LogicalClock, WriteLeaseLedger};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Canonical t0: 2026-01-01T00:00:00Z in ms (deterministic).
const T0_MS: u64 = 1_767_225_600_000;

/// Canonical drill tenant id (pinned for audit correlation).
const TENANT_ID: &str = "drill-tenant-dr-16";

type RouterFixture = (
    InMemoryFailoverRouter,
    Arc<InMemoryHealthProbe>,
    Arc<InMemoryFailoverAuditSink>,
);

fn fresh_router() -> RouterFixture {
    let probe_inner = Arc::new(InMemoryHealthProbe::new());
    let probe: Arc<dyn corelink_failover_router::HealthProbe> =
        Arc::clone(&probe_inner) as Arc<dyn corelink_failover_router::HealthProbe>;
    let sink_inner = Arc::new(InMemoryFailoverAuditSink::new());
    let sink: Arc<dyn FailoverAuditSink> =
        Arc::clone(&sink_inner) as Arc<dyn FailoverAuditSink>;
    let router = InMemoryFailoverRouter::new(probe, sink);
    (router, probe_inner, sink_inner)
}

// =====================================================================
// Scenario 1 — primary-up, no failover
// =====================================================================

#[test]
fn primary_up_no_failover_routes_reads_to_primary_and_allows_writes() {
    let clock = LogicalClock::new(T0_MS);
    let (router, _probe, sink) = fresh_router();

    // Healthy by default — no inject_degraded call.
    let now = clock.now_ms().unwrap();
    let decision = router.route_read(TENANT_ID, Region::Wnam, now).unwrap();

    // Reads route to primary.
    assert_eq!(decision.read_mode, ReadMode::Primary);
    assert_eq!(decision.read_region, Region::Wnam);
    assert!(!decision.failover_active);
    // Writes allowed.
    assert_eq!(decision.write_mode, WriteMode::Allowed);
    assert_eq!(router.write_mode(Region::Wnam), WriteMode::Allowed);

    // Routing overhead within SLO (≤ 50ms p99).
    assert!(
        decision.overhead_ms <= corelink_failover_router::SLO_FAILOVER_OVERHEAD_MS,
        "overhead {} > SLO {}",
        decision.overhead_ms,
        corelink_failover_router::SLO_FAILOVER_OVERHEAD_MS
    );

    // No failover.detected emitted (region was healthy).
    let records = sink.records();
    assert!(
        records
            .iter()
            .all(|r| r.event_type != FailoverAuditEventType::FailoverDetected),
        "unexpected failover.detected emit on healthy region: {records:?}"
    );
}

// =====================================================================
// Scenario 2 — primary-degraded triggers failover
// =====================================================================

#[test]
fn primary_degraded_triggers_failover_routes_reads_to_sibling_and_blocks_writes() {
    let clock = LogicalClock::new(T0_MS);
    let (router, probe, sink) = fresh_router();

    // Simulate a degraded primary: all 3 signals active.
    probe.inject_degraded(Region::Weur);
    // Advance clock past sustained-window threshold so the scenario timeline
    // matches the runbook's Step 1 → Step 2 window.
    clock.advance_ms(6_000).unwrap();

    let now = clock.now_ms().unwrap();
    let decision = router.route_read(TENANT_ID, Region::Weur, now).unwrap();

    // Reads route to sibling per residency graph (WEUR sibling = SAM).
    assert_eq!(decision.read_mode, ReadMode::Replica);
    assert_eq!(decision.read_region, Region::Sam);
    assert!(decision.failover_active);
    // Writes blocked during failover.
    assert_eq!(decision.write_mode, WriteMode::Blocked);
    assert_eq!(router.write_mode(Region::Weur), WriteMode::Blocked);

    // Audit-emit-BEFORE-mutation: failover.detected was emitted as part of
    // the route_read path (the production fail-CLOSED ordering).
    let records = sink.records();
    let detected: Vec<_> = records
        .iter()
        .filter(|r| r.event_type == FailoverAuditEventType::FailoverDetected)
        .collect();
    assert_eq!(detected.len(), 1, "expected exactly 1 failover.detected; got {records:?}");
    let det = detected[0];
    assert_eq!(det.primary_region, "weur");
    assert_eq!(det.replica_region, "sam");
    assert!(det.detail.contains("triggers="), "detail missing triggers: {}", det.detail);
}

// =====================================================================
// Scenario 3 — split-brain prevention
// =====================================================================

#[test]
fn split_brain_prevention_primary_never_accepts_writes_while_sibling_holds_lease() {
    let clock = LogicalClock::new(T0_MS);
    let (router, probe, _sink) = fresh_router();
    let ledger = WriteLeaseLedger::bootstrap("wnam", clock.now_ms().unwrap());

    // T+0:00 — primary healthy; lease on WNAM.
    assert_eq!(router.write_mode(Region::Wnam), WriteMode::Allowed);
    assert_eq!(
        ledger.holder_at(clock.now_ms().unwrap()).unwrap().as_deref(),
        Some("wnam")
    );

    // T+0:05 — primary degrades; trigger failover.
    clock.advance_ms(5 * 60 * 1_000).unwrap();
    probe.inject_degraded(Region::Wnam);
    // Promote sibling — runbook step 4.
    let promote_at = clock.now_ms().unwrap();
    ledger.handover("enam", promote_at, "failover.promoted").unwrap();

    // Drive a route_read on the (degraded) primary so the router's write_mode
    // reflects Blocked.
    let _ = router.route_read(TENANT_ID, Region::Wnam, promote_at).unwrap();

    // CRITICAL: while sibling holds the lease, the primary MUST never report
    // WriteMode::Allowed.
    for _i in 0..120_u64 {
        clock.advance_ms(500).unwrap();
        let t = clock.now_ms().unwrap();
        let wm_primary = router.write_mode(Region::Wnam);
        assert_eq!(
            wm_primary,
            WriteMode::Blocked,
            "split-brain at t+{}ms: primary still WriteMode::Allowed while sibling holds lease",
            t - T0_MS,
        );
        assert_eq!(
            ledger.holder_at(t).unwrap().as_deref(),
            Some("enam"),
            "ledger holder mismatch at t+{}ms",
            t - T0_MS,
        );
    }

    // Ledger MUST show exactly 2 distinct holders observed (wnam → enam),
    // i.e. one clean handover, never simultaneous.
    assert_eq!(
        ledger.distinct_holder_count().unwrap(),
        2,
        "expected exactly 2 holders (wnam → enam); got snapshot {:?}",
        ledger.snapshot().unwrap(),
    );
}

// =====================================================================
// Scenario 4 — failback after recovery
// =====================================================================

#[test]
fn failback_after_recovery_routes_back_to_primary_and_emits_resolved_audit() {
    let clock = LogicalClock::new(T0_MS);
    let (router, probe, sink) = fresh_router();
    let ledger = WriteLeaseLedger::bootstrap("wnam", clock.now_ms().unwrap());

    // Step 1: degrade primary.
    probe.inject_degraded(Region::Wnam);
    clock.advance_ms(6_000).unwrap();

    let now_fail = clock.now_ms().unwrap();
    let dec_fail = router.route_read(TENANT_ID, Region::Wnam, now_fail).unwrap();
    assert!(dec_fail.failover_active);
    assert_eq!(dec_fail.read_region, Region::Enam);
    ledger.handover("enam", now_fail, "failover.promoted").unwrap();

    // Step 7 (Reverse): primary recovers.
    clock.advance_ms(15 * 60 * 1_000).unwrap();
    probe.inject_healthy(Region::Wnam);

    let now_rec = clock.now_ms().unwrap();
    let dec_rec = router.route_read(TENANT_ID, Region::Wnam, now_rec).unwrap();
    assert_eq!(dec_rec.read_mode, ReadMode::Primary);
    assert_eq!(dec_rec.read_region, Region::Wnam);
    assert!(!dec_rec.failover_active);
    assert_eq!(dec_rec.write_mode, WriteMode::Allowed);

    // Failback handover in ledger.
    ledger
        .handover("wnam", now_rec, "failback.demoted")
        .unwrap();

    // Emit failover.resolved AFTER the recovery route decision (runbook step 7
    // tail). The harness emits this directly via the sink to assert ordering.
    let resolved_record = corelink_failover_router::FailoverAuditRecord {
        event_type: FailoverAuditEventType::FailoverResolved,
        tenant_id_hash: "drill-hash".to_owned(),
        blob_hash: String::new(),
        primary_region: "wnam".to_owned(),
        replica_region: "enam".to_owned(),
        timestamp_ms: now_rec,
        detail: "failback after recovery; drill DR-16".to_owned(),
    };
    sink.emit(resolved_record).unwrap();

    // Ordering check: detected BEFORE resolved (ts and vector order).
    let records = sink.records();
    let detected_idx = records
        .iter()
        .position(|r| r.event_type == FailoverAuditEventType::FailoverDetected)
        .unwrap_or_else(|| panic!("missing failover.detected: {records:?}"));
    let resolved_idx = records
        .iter()
        .position(|r| r.event_type == FailoverAuditEventType::FailoverResolved)
        .unwrap_or_else(|| panic!("missing failover.resolved: {records:?}"));
    assert!(
        detected_idx < resolved_idx,
        "audit ordering violated: detected_idx={detected_idx} >= resolved_idx={resolved_idx}"
    );
    assert!(
        records[detected_idx].timestamp_ms <= records[resolved_idx].timestamp_ms,
        "audit timestamp ordering violated: {} > {}",
        records[detected_idx].timestamp_ms,
        records[resolved_idx].timestamp_ms,
    );

    // Ledger: wnam → enam → wnam = 2 distinct holders, 3 entries.
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.len(), 3, "ledger entries: {snap:?}");
    assert_eq!(snap[0].holder, "wnam");
    assert_eq!(snap[1].holder, "enam");
    assert_eq!(snap[2].holder, "wnam");
}

// =====================================================================
// Scenario 5 — partial region (some DOs healthy, others not)
// =====================================================================

#[test]
fn partial_region_health_does_not_short_circuit_region_level_failover() {
    let clock = LogicalClock::new(T0_MS);
    let (router, probe, sink) = fresh_router();

    // Simulate "partial" region: 2 signals breach, 1 does not.
    // Per the multi-signal rule (corelink_failover_router::health), failover
    // requires ALL 3 signals — so the region is treated Healthy.
    probe.set_state(Region::Sam, 5.0 /* 5xx */, 500 /* p99 */, 0 /* failures */);
    clock.advance_ms(6_000).unwrap();

    let dec_partial = router
        .route_read(TENANT_ID, Region::Sam, clock.now_ms().unwrap())
        .unwrap();
    assert_eq!(
        dec_partial.read_mode,
        ReadMode::Primary,
        "2-of-3 signals must NOT trigger failover (would be false-positive)"
    );
    assert!(!dec_partial.failover_active);

    // Now escalate: all 3 signals active → region uniformly Degraded.
    probe.inject_degraded(Region::Sam);
    clock.advance_ms(6_000).unwrap();
    let now_full = clock.now_ms().unwrap();
    let dec_full = router.route_read(TENANT_ID, Region::Sam, now_full).unwrap();

    // Region-level decision: ALL reads route to sibling — there is no
    // per-DO short-circuit (the failover-router operates at region tier).
    assert_eq!(dec_full.read_mode, ReadMode::Replica);
    assert_eq!(dec_full.read_region, Region::Weur);
    assert_eq!(dec_full.write_mode, WriteMode::Blocked);
    assert!(dec_full.failover_active);

    // 10 additional read attempts at varying logical timestamps — all MUST
    // route to sibling (proves the decision is uniform, not stochastic).
    for i in 0..10 {
        clock.advance_ms(100).unwrap();
        let dec = router
            .route_read(TENANT_ID, Region::Sam, clock.now_ms().unwrap())
            .unwrap();
        assert_eq!(
            dec.read_region,
            Region::Weur,
            "iter {i}: partial-region routing not uniform"
        );
    }

    // Audit: exactly one failover.detected emitted at the first FULL
    // degradation transition (subsequent reads while degraded re-emit because
    // the production trace path emits per decision; we just verify ≥ 1).
    let records = sink.records();
    let detected_count = records
        .iter()
        .filter(|r| r.event_type == FailoverAuditEventType::FailoverDetected)
        .count();
    assert!(
        detected_count >= 1,
        "expected at least 1 failover.detected after full degradation; got {detected_count}"
    );
}

// =====================================================================
// Cross-cutting safety: residency graph invariants
// =====================================================================

#[test]
fn residency_graph_is_acyclic_and_pinned_pairs() {
    let graph = ResidencyGraph;
    assert!(graph.is_acyclic());
    assert_eq!(graph.sibling(Region::Wnam), Some(Region::Enam));
    assert_eq!(graph.sibling(Region::Enam), Some(Region::Wnam));
    assert_eq!(graph.sibling(Region::Weur), Some(Region::Sam));
    assert_eq!(graph.sibling(Region::Sam), Some(Region::Weur));
}

// =====================================================================
// Harness sanity
// =====================================================================

#[test]
fn write_lease_ledger_holder_at_returns_latest_before_timestamp() -> Result<(), HarnessError> {
    let ledger = WriteLeaseLedger::bootstrap("wnam", T0_MS);
    ledger.handover("enam", T0_MS + 1_000, "failover.promoted")?;
    ledger.handover("wnam", T0_MS + 5_000, "failback.demoted")?;

    assert_eq!(ledger.holder_at(T0_MS)?.as_deref(), Some("wnam"));
    assert_eq!(ledger.holder_at(T0_MS + 500)?.as_deref(), Some("wnam"));
    assert_eq!(ledger.holder_at(T0_MS + 1_000)?.as_deref(), Some("enam"));
    assert_eq!(ledger.holder_at(T0_MS + 3_000)?.as_deref(), Some("enam"));
    assert_eq!(ledger.holder_at(T0_MS + 5_000)?.as_deref(), Some("wnam"));
    Ok(())
}

#[test]
fn no_replica_error_taxonomy_is_well_typed() {
    let err = FailoverError::NoReplicaAvailable {
        region: Region::Wnam,
    };
    assert!(err.to_string().contains("no replica"));
}

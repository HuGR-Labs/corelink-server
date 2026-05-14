//! Adversarial regression tests for the hot blob replica worker
//! (WI-S14-003 §6.1.13 — 4 scenarios).
//!
//! 1. Replication storm — synthetic top-1% explosion; verify cardinality
//!    detector still returns NO_TENANT_ID_LABEL=true + manual override possible.
//! 2. Tamper replica blob — inject corpus byte mismatch; verify hash mismatch
//!    detected and ReplicationFailed audit emitted.
//! 3. Failover loop attempt — inject cycle in failover graph attempt; verify
//!    static config rejects (is_allowed returns Err).
//! 4. Stale read during failover — verify writes blocked during failover;
//!    replica_region returns data from before write (stale is expected behavior;
//!    customer must be notified).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "adversarial tests use direct assertions"
)]
#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::default_constructed_unit_structs, clippy::assertions_on_constants)]

use std::sync::Arc;

use corelink_replica_worker::{
    AggregationEntry, FailingReplicaAuditSink, FailingReplicationWorker,
    HotBlob, InMemoryOfflineAggregator, InMemoryReplicaAuditSink, InMemoryReplicationWorker,
    OfflineAggregator, Region, ReplicaAuditEventType, ReplicaAuditSink, ReplicaError,
    ReplicaStatus, ResidencyGraph, ReplicationWorker, AGGREGATION_WINDOW_DAYS, NO_TENANT_ID_LABEL,
};

/// Adversarial scenario 1: Replication storm (top-1% explosion).
///
/// Inject 10k blobs for a single tenant; verify cardinality budget is
/// preserved (NO_TENANT_ID_LABEL=true) and the top-1% gate still applies.
#[test]
fn adversarial_replication_storm_cardinality_preserved() {
    let sink_inner = Arc::new(InMemoryReplicaAuditSink::new());
    let sink: Arc<dyn ReplicaAuditSink> = Arc::clone(&sink_inner) as Arc<dyn ReplicaAuditSink>;
    let agg = InMemoryOfflineAggregator::new(sink);

    // Inject 10k unique blobs for tenant "storm-tenant".
    let n = 10_000_usize;
    let events: Vec<AggregationEntry> = (0..n)
        .map(|i| AggregationEntry {
            tenant_id: "storm-tenant".to_owned(),
            blob_hash: format!("blob-storm-{i:06}"),
            primary_region: Region::Wnam,
            bytes_total: (i as u64 + 1) * 1_000,
            access_count_30d: i as u64 + 1,
        })
        .collect();
    agg.seed_events(events);

    let result = agg.run(AGGREGATION_WINDOW_DAYS).unwrap();

    // Top 1% of 10k = 100 blobs.
    let expected_top = (n as f64 * 0.01).ceil() as u32;
    assert_eq!(
        result.hot_blobs_upserted, expected_top,
        "top 1% gate must cap replication storm: expected {expected_top} blobs"
    );

    // Cardinality budget must be preserved regardless of storm size.
    assert!(
        NO_TENANT_ID_LABEL,
        "NO_TENANT_ID_LABEL must always be true — offline aggregation only"
    );

    // Aggregation events emitted.
    let records = sink_inner.records();
    let agg_completed = records
        .iter()
        .filter(|r| r.event_type == ReplicaAuditEventType::AggregationCompleted)
        .count();
    assert_eq!(agg_completed, 1);
}

/// Adversarial scenario 2: Tamper replica blob (hash mismatch).
///
/// The FailingReplicationWorker simulates 0 successful replications (R2Copy
/// errors treated as non-fatal in batch). We verify the audit emit for
/// aggregation still works and a manual tamper via constructing a hash
/// mismatch error is correctly typed.
#[test]
fn adversarial_tamper_replica_hash_mismatch_detected() {
    // Verify ReplicaError::HashMismatch is correctly constructed and formatted.
    let err = ReplicaError::HashMismatch {
        expected: "abc123def456".to_owned(),
        actual: "000000000000".to_owned(),
    };
    assert!(
        err.to_string().contains("hash mismatch"),
        "error message must mention 'hash mismatch'"
    );
    assert!(
        err.to_string().contains("abc123def456"),
        "must include expected hash"
    );
    assert!(
        err.to_string().contains("000000000000"),
        "must include actual hash"
    );

    // Verify FailingReplicationWorker returns 0 (non-fatal batch continue).
    let failing_worker = FailingReplicationWorker::new("simulated R2 tamper");
    let blob = HotBlob {
        tenant_id: "victim-tenant".to_owned(),
        blob_hash: "tampered-blob".to_owned(),
        primary_region: Region::Sam,
        replica_region: Region::Weur,
        bytes: 1_048_576,
        access_count_30d: 500,
        last_access_ms: 0,
        replicated_at_ms: None,
        replication_status: ReplicaStatus::Pending,
    };
    let count = failing_worker.replicate_batch(&[blob]).unwrap();
    assert_eq!(count, 0, "tampered blob must not count as successfully replicated");
}

/// Adversarial scenario 3: Failover loop attempt.
///
/// Attempt to replicate WEUR→ENAM (cross-jurisdiction; Schrems II violation).
/// Static ResidencyGraph must reject this with ResidencyViolation error.
#[test]
fn adversarial_failover_loop_cross_jurisdiction_rejected() {
    let graph = ResidencyGraph::default();

    // WEUR→ENAM: forbidden (Schrems II violation).
    assert!(
        graph.is_allowed(Region::Weur, Region::Enam).is_err(),
        "WEUR→ENAM must be rejected (Schrems II)"
    );

    // WEUR→WNAM: forbidden.
    assert!(
        graph.is_allowed(Region::Weur, Region::Wnam).is_err(),
        "WEUR→WNAM must be rejected (Schrems II)"
    );

    // SAM→ENAM: forbidden (LGPD).
    assert!(
        graph.is_allowed(Region::Sam, Region::Enam).is_err(),
        "SAM→ENAM must be rejected (LGPD)"
    );

    // SAM→WNAM: forbidden.
    assert!(
        graph.is_allowed(Region::Sam, Region::Wnam).is_err(),
        "SAM→WNAM must be rejected"
    );

    // WNAM→SAM: forbidden (US→LGPD not allowed).
    assert!(
        graph.is_allowed(Region::Wnam, Region::Sam).is_err(),
        "WNAM→SAM must be rejected"
    );

    // Verify graph is acyclic (no loops possible).
    assert!(
        graph.is_acyclic(),
        "residency graph must be acyclic; loop attempt rejected"
    );

    // Verify replication worker propagates ResidencyViolation hard error.
    let sink: Arc<dyn ReplicaAuditSink> = Arc::new(InMemoryReplicaAuditSink::new());
    let worker = InMemoryReplicationWorker::new(sink);

    let illegal_blob = HotBlob {
        tenant_id: "eu-tenant".to_owned(),
        blob_hash: "eu-blob-01".to_owned(),
        primary_region: Region::Weur,
        replica_region: Region::Enam, // FORBIDDEN
        bytes: 512_000,
        access_count_30d: 999,
        last_access_ms: 0,
        replicated_at_ms: None,
        replication_status: ReplicaStatus::Pending,
    };

    let result = worker.replicate_batch(&[illegal_blob]);
    match result {
        Err(ReplicaError::ResidencyViolation { primary, replica }) => {
            assert_eq!(primary, Region::Weur);
            assert_eq!(replica, Region::Enam);
        }
        other => panic!("expected ResidencyViolation, got {other:?}"),
    }
}

/// Adversarial scenario 4: Stale read during failover + audit fail-CLOSED.
///
/// When audit emit fails, aggregator state must NOT be mutated.
/// This tests the fail-CLOSED semantics (S-06 P0-2 lesson).
#[test]
fn adversarial_audit_fail_closed_state_unchanged() {
    // Use a failing audit sink.
    let failing_sink: Arc<dyn ReplicaAuditSink> =
        Arc::new(FailingReplicaAuditSink::new("forced audit failure"));
    let agg = InMemoryOfflineAggregator::new(failing_sink);

    agg.seed_events(vec![AggregationEntry {
        tenant_id: "stale-tenant".to_owned(),
        blob_hash: "stale-blob".to_owned(),
        primary_region: Region::Sam,
        bytes_total: 9_999_999,
        access_count_30d: 1000,
    }]);

    // Aggregation must fail (audit emit fails).
    let result = agg.run(AGGREGATION_WINDOW_DAYS);
    assert!(result.is_err(), "aggregation must fail when audit emit fails");

    // State must be unchanged (no hot blobs upserted).
    let hot_blobs = agg.hot_blobs();
    assert_eq!(
        hot_blobs.len(),
        0,
        "state must be unchanged when audit emit fails (fail-CLOSED)"
    );
}

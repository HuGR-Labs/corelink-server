//! Integration tests for the Wave-18 Neon analytics shadow sync.
//!
//! These tests exercise the end-to-end pipeline:
//!
//!   1. `InMemoryR2AuditSink` emits a chain of audit events.
//!   2. `ArchiveProducer` buffers + flushes per the canonical R2 NDJSON
//!      archive (Wave 15).
//!   3. `InMemoryNeonShadowSink` mirrors each flushed chunk to the
//!      Neon analytics shadow.
//!   4. Analytics aggregates (`aggregate_event_count`,
//!      `aggregate_timeline`) are validated against the expected
//!      ground truth.
//!
//! The three discriminating tests required by the WI deliverable are
//! pinned at the bottom of this file:
//!
//! - `roundtrip_hundred_events_through_archive_plus_shadow_sync_matches_aggregate`
//! - `lag_detection_emits_sev2_when_observed_lag_breaches_60min`
//! - `tenant_isolation_cross_tenant_query_fails_closed`
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::{
    archive_chunk_key, ArchiveProducer, ArchiveSink, AuditEvent, AuditEventKind, ChainHash,
    FlushPolicy, InMemoryArchiveSink, InMemoryAuditChainAuditSink, InMemoryNeonShadowSink,
    InMemoryR2AuditSink, InMemoryShadowSyncAuditSink, NeonShadowError, NeonShadowSink,
    PersistedAuditLine, ShadowEventRow, SHADOW_LAG_SEV2_THRESHOLD_MS,
};
use serde_json::json;
use uuid::Uuid;

const BASE_MS: u64 = 1_700_000_000_000;

fn fresh_audit_sink() -> InMemoryR2AuditSink<InMemoryAuditChainAuditSink> {
    let audit = Arc::new(InMemoryAuditChainAuditSink::new());
    InMemoryR2AuditSink::new(audit)
}

/// Emit `n` chain events for `tenant` with deterministic per-index
/// `event_kind` distribution: index % 3 == 0 → CasPut, 1 → CasGet,
/// 2 → AcLookup. Returns the in-order persisted lines.
fn emit_n_events(
    audit_sink: &InMemoryR2AuditSink<InMemoryAuditChainAuditSink>,
    tenant: Uuid,
    n: u64,
) -> Vec<PersistedAuditLine> {
    let kinds = [
        AuditEventKind::CasPut,
        AuditEventKind::CasGet,
        AuditEventKind::AcLookup,
    ];
    let e0 = AuditEvent::genesis(
        kinds[0],
        "corelink/region/iad",
        Uuid::now_v7(),
        BASE_MS,
        tenant,
        Region::Iad,
        json!({"i": 0}),
    );
    audit_sink.emit(e0, "req-0", BASE_MS).unwrap();
    for i in 1..n {
        let (seq, prev) = audit_sink.next_link_inputs(tenant);
        let kind = kinds[(i as usize) % 3];
        let e = AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            BASE_MS + i,
            tenant,
            Region::Iad,
            seq,
            prev,
            json!({"i": i}),
        );
        audit_sink.emit(e, "req-x", BASE_MS + i).unwrap();
    }
    audit_sink.snapshot_for_tenant(tenant)
}

#[test]
fn roundtrip_hundred_events_through_archive_plus_shadow_sync_matches_aggregate() {
    let tenant = Uuid::now_v7();
    let region = Region::Iad;

    // 1. Emit 100 events through the canonical audit-chain sink.
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 100);
    assert_eq!(lines.len(), 100);

    // 2. Flush through the archive producer in chunks of 25 events
    //    each (yields 4 chunks).
    let r2_sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
    let policy = FlushPolicy::new(u64::MAX, 25, u64::MAX);
    let producer = ArchiveProducer::new_for_tenant(
        tenant,
        r2_sink.clone() as Arc<dyn ArchiveSink>,
        policy,
        ChainHash::genesis(),
        0,
    );

    // 3. Build the Neon shadow sink + drive sync_chunk on every flushed
    //    receipt. We feed the same buffered slice into the shadow.
    let audit_emit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let shadow = InMemoryNeonShadowSink::new(tenant, region, audit_emit.clone());

    let mut buffered: Vec<PersistedAuditLine> = Vec::new();
    let mut chunk_count = 0usize;
    for (i, line) in lines.iter().enumerate() {
        buffered.push(line.clone());
        let now_ms = BASE_MS + (i as u64) + 100;
        if let Some(receipt) = producer.observe(line.clone(), now_ms).unwrap() {
            // Build shadow rows from the just-flushed buffered slice.
            // Wave-21 (B-P1-05): `from_persisted_line` now returns a
            // `Result`. The producer guarantees well-formed NDJSON so
            // the unwrap is structurally safe on the happy path.
            let shadow_rows: Vec<ShadowEventRow> = buffered
                .iter()
                .map(|l| {
                    ShadowEventRow::from_persisted_line(l, region)
                        .expect("archive_producer emits well-formed NDJSON")
                })
                .collect();
            let sync_receipt = shadow.sync_chunk(&receipt, &shadow_rows, now_ms).unwrap();
            assert_eq!(sync_receipt.rows_persisted, 25);
            assert_eq!(sync_receipt.region, region);
            buffered.clear();
            chunk_count += 1;
        }
    }
    assert_eq!(chunk_count, 4, "expected 4 chunks of 25 events");
    assert_eq!(shadow.row_count(), 100);

    // 4. Aggregate event count: expected distribution from emit_n_events
    //    is index % 3 ∈ {0,1,2} mapped to {CasPut, CasGet, AcLookup}.
    //    For i ∈ 0..100: count(0%3)=34, count(1%3)=33, count(2%3)=33.
    let buckets = shadow
        .aggregate_event_count(0, u64::MAX, None)
        .expect("aggregate ok");
    let total: u64 = buckets.iter().map(|b| b.count).sum();
    assert_eq!(total, 100);

    let put_kind = AuditEventKind::CasPut.event_type();
    let get_kind = AuditEventKind::CasGet.event_type();
    let lookup_kind = AuditEventKind::AcLookup.event_type();
    let count_of = |ty: &str| -> u64 {
        buckets
            .iter()
            .find(|b| b.event_type == ty)
            .map(|b| b.count)
            .unwrap_or(0)
    };
    assert_eq!(count_of(put_kind), 34, "CasPut count");
    assert_eq!(count_of(get_kind), 33, "CasGet count");
    assert_eq!(count_of(lookup_kind), 33, "AcLookup count");

    // 5. Aggregate timeline: bucket size = 20ms over a 100ms window.
    //    Every event lives at BASE_MS+i for i ∈ 0..100, so the window
    //    [BASE_MS, BASE_MS+100) covers all 100 events; 5 buckets × 20.
    let timeline = shadow
        .aggregate_timeline(BASE_MS, BASE_MS + 100, 20)
        .expect("timeline ok");
    assert_eq!(timeline.len(), 5);
    for (i, b) in timeline.iter().enumerate() {
        assert_eq!(b.bucket_start_ms, BASE_MS + (i as u64) * 20);
        assert_eq!(b.count, 20);
    }

    // 6. Audit emits: 4 chunks → 4 SHADOW_SYNCED audit rows.
    let emits = audit_emit.snapshot().unwrap();
    assert_eq!(emits.len(), 4);
    for emit in &emits {
        assert_eq!(emit.event_type, "corelink.audit.neon_shadow_synced.v1");
        assert_eq!(emit.tenant_id, tenant);
        assert_eq!(emit.region, region);
        assert_eq!(emit.sev, "info");
    }

    // 7. Sanity: every R2 chunk key parses through `archive_chunk_key`.
    let r2_snap = r2_sink.snapshot();
    for (key, _body) in &r2_snap {
        assert!(key.starts_with("audit/"));
        assert!(key.ends_with(".ndjson"));
    }
    assert_eq!(r2_snap.len(), 4);
    // First chunk key for seq=0:
    assert_eq!(r2_snap[0].0, archive_chunk_key(BASE_MS, 0));
}

#[test]
fn lag_detection_emits_sev2_when_observed_lag_breaches_60min() {
    let tenant = Uuid::now_v7();
    let region = Region::Iad;
    let audit_emit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let shadow = InMemoryNeonShadowSink::new(tenant, region, audit_emit.clone());

    // Build one event at BASE_MS + simulate sync at BASE_MS + 60 min.
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 1);

    let r2_sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
    let policy = FlushPolicy::new(u64::MAX, 1, u64::MAX);
    let producer = ArchiveProducer::new_for_tenant(
        tenant,
        r2_sink.clone() as Arc<dyn ArchiveSink>,
        policy,
        ChainHash::genesis(),
        0,
    );

    let receipt = producer
        .observe(lines[0].clone(), BASE_MS)
        .unwrap()
        .expect("1-event chunk flushes immediately");

    // Simulate a 60-min delay between R2 archive emit and Neon sync.
    let now_ms = BASE_MS + SHADOW_LAG_SEV2_THRESHOLD_MS;
    let shadow_rows: Vec<ShadowEventRow> = lines
        .iter()
        .map(|l| {
            ShadowEventRow::from_persisted_line(l, region)
                .expect("archive_producer emits well-formed NDJSON")
        })
        .collect();
    let sync_receipt = shadow.sync_chunk(&receipt, &shadow_rows, now_ms).unwrap();
    assert!(sync_receipt.breaches_sev2_threshold());

    // The audit emit must be SEV-2 (analytics lag), NOT a chain break.
    let emits = audit_emit.snapshot().unwrap();
    assert_eq!(emits.len(), 1);
    assert_eq!(emits[0].event_type, "corelink.audit.neon_shadow_synced.v1");
    assert_eq!(emits[0].sev, "sev-2");
    assert!(emits[0].observed_lag_ms >= SHADOW_LAG_SEV2_THRESHOLD_MS);
}

#[test]
fn tenant_isolation_cross_tenant_query_fails_closed() {
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let region = Region::Iad;
    let audit_emit = Arc::new(InMemoryShadowSyncAuditSink::new());

    // Persist a row for tenant_a.
    let shadow_a = InMemoryNeonShadowSink::new(tenant_a, region, audit_emit.clone());
    let row_a = ShadowEventRow::new(
        tenant_a,
        0,
        BASE_MS,
        "x".to_string(),
        ChainHash::genesis(),
        ChainHash([1; 32]),
        region,
        "{}".to_string(),
    );
    let receipt_a = corelink_audit_chain::ArchiveReceipt {
        r2_key: "k".into(),
        tenant_id: tenant_a,
        first_event_time_ms: BASE_MS,
        last_event_time_ms: BASE_MS,
        first_sequence_number: 0,
        last_sequence_number: 0,
        prev_hash_anchor: ChainHash::genesis(),
        chain_head_after: ChainHash([1; 32]),
        bytes_written: 10,
        events_written: 1,
    };
    shadow_a
        .sync_chunk(&receipt_a, &[row_a], BASE_MS + 10)
        .unwrap();

    // 1. A tenant_b shadow sink instance can NEVER ingest a tenant_a row.
    let shadow_b = InMemoryNeonShadowSink::new(tenant_b, region, audit_emit.clone());
    let cross_row = ShadowEventRow::new(
        tenant_a, // <-- cross-tenant attempt
        0,
        BASE_MS,
        "x".to_string(),
        ChainHash::genesis(),
        ChainHash([1; 32]),
        region,
        "{}".to_string(),
    );
    let receipt_cross = corelink_audit_chain::ArchiveReceipt {
        tenant_id: tenant_a,
        ..receipt_a.clone()
    };
    let err = shadow_b
        .sync_chunk(&receipt_cross, &[cross_row], BASE_MS + 20)
        .expect_err("must reject cross-tenant write");
    assert!(matches!(
        err,
        NeonShadowError::TenantIsolationViolation { .. }
    ));

    // 2. A tenant_b aggregate query MUST NOT see tenant_a's row.
    //    (The InMemory sink scopes by self.tenant_id; the SQL-layer
    //    equivalent is RLS keyed on `app.current_tenant`.)
    let buckets = shadow_b
        .aggregate_event_count(0, u64::MAX, None)
        .expect("query ok");
    assert!(
        buckets.is_empty(),
        "tenant_b must see 0 rows; got {:?}",
        buckets
    );

    // 3. tenant_a's own query still sees its 1 row.
    let buckets_a = shadow_a
        .aggregate_event_count(0, u64::MAX, None)
        .expect("query ok");
    assert_eq!(buckets_a.len(), 1);
    assert_eq!(buckets_a[0].count, 1);

    // 4. A SHADOW_SYNC_FAILED audit row was emitted for the rejected
    //    cross-tenant attempt.
    let emits = audit_emit.snapshot().unwrap();
    let failures: Vec<_> = emits
        .iter()
        .filter(|e| e.event_type == "corelink.audit.neon_shadow_sync_failed.v1")
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].sev, "sev-2");
}

use super::*;
use crate::audit::InMemoryAuditChainAuditSink;
use crate::event::{AuditEvent, AuditEventKind};
use crate::sink::InMemoryR2AuditSink;
use corelink_analytics::Region;
use serde_json::json;

fn fresh_producer(
    tenant: Uuid,
    policy: FlushPolicy,
) -> (ArchiveProducer, Arc<InMemoryArchiveSink>) {
    let sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
    let producer = ArchiveProducer::new_for_tenant(
        tenant,
        sink.clone() as Arc<dyn ArchiveSink>,
        policy,
        ChainHash::genesis(),
        0,
    );
    (producer, sink)
}

fn fresh_audit_sink() -> InMemoryR2AuditSink<InMemoryAuditChainAuditSink> {
    let audit = Arc::new(InMemoryAuditChainAuditSink::new());
    InMemoryR2AuditSink::new(audit)
}

fn emit_n_events(
    audit_sink: &InMemoryR2AuditSink<InMemoryAuditChainAuditSink>,
    tenant: Uuid,
    n: u64,
) -> Vec<PersistedAuditLine> {
    let base_ms = 1_700_000_000_000u64;
    // Genesis event.
    let e0 = AuditEvent::genesis(
        AuditEventKind::Tenant,
        "corelink/region/iad",
        Uuid::now_v7(),
        base_ms,
        tenant,
        Region::Iad,
        json!({"i": 0}),
    );
    audit_sink.emit(e0, "req-0", base_ms).unwrap();
    for i in 1..n {
        let (seq, prev) = audit_sink.next_link_inputs(tenant);
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            base_ms + i,
            tenant,
            Region::Iad,
            seq,
            prev,
            json!({"i": i}),
        );
        audit_sink.emit(e, "req-x", base_ms + i).unwrap();
    }
    audit_sink.snapshot_for_tenant(tenant)
}

#[test]
fn fresh_producer_is_empty() {
    let tenant = Uuid::now_v7();
    let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
    assert_eq!(p.buffered_event_count().unwrap(), 0);
    assert_eq!(p.buffered_bytes().unwrap(), 0);
    assert_eq!(p.receipts_emitted().unwrap(), 0);
    assert_eq!(p.tenant_id(), tenant);
    assert!(sink.is_empty());
    assert_eq!(p.pending_head_after().unwrap(), ChainHash::genesis());
    assert_eq!(p.next_expected_sequence().unwrap(), 0);
}

#[test]
fn observe_buffers_genesis_without_flushing_under_policy() {
    let tenant = Uuid::now_v7();
    let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 1);
    let receipt = p.observe(lines[0].clone(), 1_700_000_000_000).unwrap();
    assert!(receipt.is_none(), "should not flush under default policy");
    assert_eq!(p.buffered_event_count().unwrap(), 1);
    assert!(sink.is_empty());
    assert_eq!(p.next_expected_sequence().unwrap(), 1);
    // chain head advanced.
    assert_ne!(p.pending_head_after().unwrap(), ChainHash::genesis());
}

#[test]
fn observe_with_wrong_tenant_returns_violation() {
    let producer_tenant = Uuid::now_v7();
    let other_tenant = Uuid::now_v7();
    let (p, sink) = fresh_producer(producer_tenant, FlushPolicy::default());
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, other_tenant, 1);
    let err = p.observe(lines[0].clone(), 1000).unwrap_err();
    assert!(matches!(
        err,
        ArchiveProducerError::TenantPrefixViolation { .. }
    ));
    assert_eq!(p.buffered_event_count().unwrap(), 0);
    assert!(sink.is_empty());
}

#[test]
fn observe_with_wrong_sequence_returns_violation() {
    let tenant = Uuid::now_v7();
    let (p, _sink) = fresh_producer(tenant, FlushPolicy::default());
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 3);
    // Skip line 0 (seq=0); observe line 1 (seq=1) first — should violate.
    let err = p.observe(lines[1].clone(), 1000).unwrap_err();
    assert!(matches!(
        err,
        ArchiveProducerError::SequenceOrderingViolation {
            expected: 0,
            observed: 1
        }
    ));
}

#[test]
fn flush_trips_on_max_events_per_chunk() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX, // disable time trigger
        max_events_per_chunk: 3,
        max_bytes_per_chunk: u64::MAX, // disable bytes trigger
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 3);
    let r0 = p.observe(lines[0].clone(), 1_000).unwrap();
    let r1 = p.observe(lines[1].clone(), 1_001).unwrap();
    let r2 = p.observe(lines[2].clone(), 1_002).unwrap();
    assert!(r0.is_none());
    assert!(r1.is_none());
    let receipt = r2.expect("3rd observe must flush");
    assert_eq!(receipt.tenant_id, tenant);
    assert_eq!(receipt.first_sequence_number, 0);
    assert_eq!(receipt.last_sequence_number, 2);
    assert_eq!(receipt.events_written, 3);
    // canonical r2 key shape: audit/<YYYY>/<MM>/<DD>/00000000.ndjson
    assert!(
        receipt.r2_key.starts_with("audit/"),
        "key={}",
        receipt.r2_key
    );
    assert!(
        receipt.r2_key.ends_with("/00000000.ndjson"),
        "key={}",
        receipt.r2_key
    );
    // chain head after final event = lines[2].link_hash.
    assert_eq!(receipt.chain_head_after, lines[2].link_hash);
    // prev_hash anchor for genesis = ChainHash::genesis() (zero).
    assert_eq!(receipt.prev_hash_anchor, ChainHash::genesis());
    // buffer drained.
    assert_eq!(p.buffered_event_count().unwrap(), 0);
    assert_eq!(p.receipts_emitted().unwrap(), 1);
    // sink received exactly one chunk.
    assert_eq!(sink.len(), 1);
}

#[test]
fn flush_trips_on_max_bytes_per_chunk() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: u64::MAX,
        max_bytes_per_chunk: 1, // any non-zero byte trips after first event
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 1);
    let receipt = p
        .observe(lines[0].clone(), 1_000)
        .unwrap()
        .expect("byte threshold trips immediately");
    assert_eq!(receipt.events_written, 1);
    assert_eq!(sink.len(), 1);
}

#[test]
fn flush_trips_on_wall_clock_age() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: 100,
        max_events_per_chunk: u64::MAX,
        max_bytes_per_chunk: u64::MAX,
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 2);
    // First observe at t=1000.
    let r0 = p.observe(lines[0].clone(), 1_000).unwrap();
    assert!(r0.is_none());
    // Second observe at t=1100 → age = 100ms, trips flush.
    let r1 = p.observe(lines[1].clone(), 1_100).unwrap();
    assert!(r1.is_some());
    assert_eq!(sink.len(), 1);
}

#[test]
fn force_flush_is_idempotent_on_empty_buffer() {
    let tenant = Uuid::now_v7();
    let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
    let r = p.force_flush(1_000).unwrap();
    assert!(r.is_none());
    assert!(sink.is_empty());
}

#[test]
fn force_flush_drains_partial_chunk() {
    let tenant = Uuid::now_v7();
    let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 2);
    p.observe(lines[0].clone(), 1_000).unwrap();
    p.observe(lines[1].clone(), 1_001).unwrap();
    assert_eq!(p.buffered_event_count().unwrap(), 2);
    let r = p
        .force_flush(2_000)
        .unwrap()
        .expect("non-empty buffer drains");
    assert_eq!(r.events_written, 2);
    assert_eq!(sink.len(), 1);
    assert_eq!(p.buffered_event_count().unwrap(), 0);
}

#[test]
fn chunk_ndjson_body_has_newline_separators_no_trailing() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: 3,
        max_bytes_per_chunk: u64::MAX,
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 3);
    for (i, line) in lines.iter().enumerate() {
        p.observe(line.clone(), 1_000 + i as u64).unwrap();
    }
    let snap = sink.snapshot();
    let body = &snap[0].1;
    // Expect exactly 2 newlines (3 lines, '\n' separator, no trailing).
    let nl_count = body.iter().filter(|b| **b == b'\n').count();
    assert_eq!(nl_count, 2);
    // Body must NOT end in newline.
    assert_ne!(*body.last().unwrap(), b'\n');
}

#[test]
fn chain_head_continuity_persists_across_two_chunks() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: 2,
        max_bytes_per_chunk: u64::MAX,
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 4);
    let mut receipts = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(r) = p.observe(line.clone(), 1_000 + i as u64).unwrap() {
            receipts.push(r);
        }
    }
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].first_sequence_number, 0);
    assert_eq!(receipts[0].last_sequence_number, 1);
    assert_eq!(receipts[1].first_sequence_number, 2);
    assert_eq!(receipts[1].last_sequence_number, 3);
    // The chain-head pointer between chunks MUST be continuous:
    // chunk N's chain_head_after == chunk N+1's prev_hash_anchor.
    assert_eq!(receipts[0].chain_head_after, receipts[1].prev_hash_anchor);
    // sink got exactly two chunks.
    assert_eq!(sink.len(), 2);
}

#[test]
fn chunk_keys_are_lexicographically_sortable_by_sequence() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: 1, // 1 chunk per event
        max_bytes_per_chunk: u64::MAX,
    };
    let (p, sink) = fresh_producer(tenant, policy);
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 5);
    for (i, line) in lines.iter().enumerate() {
        p.observe(line.clone(), 1_000 + i as u64).unwrap();
    }
    let mut keys: Vec<String> = sink.snapshot().into_iter().map(|(k, _)| k).collect();
    let original = keys.clone();
    keys.sort();
    assert_eq!(keys, original, "R2 key sort = chronological order");
}

#[test]
fn sink_failure_propagates_fail_closed() {
    let tenant = Uuid::now_v7();
    let sink = Arc::new(FailingArchiveSink::new());
    let producer = ArchiveProducer::new_for_tenant(
        tenant,
        sink as Arc<dyn ArchiveSink>,
        FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 1,
            max_bytes_per_chunk: u64::MAX,
        },
        ChainHash::genesis(),
        0,
    );
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 1);
    let err = producer.observe(lines[0].clone(), 1_000).unwrap_err();
    assert!(matches!(err, ArchiveProducerError::SinkBackend(_)));
}

#[test]
fn archive_chunk_key_shape_matches_spec() {
    // 2026-05-15T12:00:00Z = 1747310400000 ms; key shape:
    // audit/2026/05/15/<8-digit>.ndjson
    let ts = 1_747_310_400_000u64;
    let key = archive_chunk_key(ts, 42);
    // The exact date depends on the canonical_date_yyyy_mm_dd
    // algorithm; we assert the shape rather than the exact date.
    assert!(
        key.starts_with("audit/"),
        "expected `audit/` prefix, got {}",
        key
    );
    assert!(
        key.ends_with("/00000042.ndjson"),
        "expected 8-digit seq + .ndjson suffix, got {}",
        key
    );
    // Path segments: audit / YYYY / MM / DD / <seq>.ndjson = 5.
    let segments: Vec<&str> = key.split('/').collect();
    assert_eq!(
        segments.len(),
        5,
        "expected 5 path segments, got {} in {}",
        segments.len(),
        key
    );
    assert_eq!(segments[0], "audit");
    assert_eq!(segments[1].len(), 4, "YYYY len = 4");
    assert_eq!(segments[2].len(), 2, "MM len = 2");
    assert_eq!(segments[3].len(), 2, "DD len = 2");
}

#[test]
fn chain_hashes_eq_ct_returns_true_for_equal_hashes() {
    let a = ChainHash([0xAB; 32]);
    let b = ChainHash([0xAB; 32]);
    assert!(chain_hashes_eq_ct(&a, &b));
}

#[test]
fn chain_hashes_eq_ct_returns_false_for_different_hashes() {
    let a = ChainHash([0xAB; 32]);
    let b = ChainHash([0xCD; 32]);
    assert!(!chain_hashes_eq_ct(&a, &b));
}

#[test]
fn cloned_producer_shares_buffer_state() {
    let tenant = Uuid::now_v7();
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: 10,
        max_bytes_per_chunk: u64::MAX,
    };
    let (p, _sink) = fresh_producer(tenant, policy);
    let p2 = p.clone();
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 2);
    p.observe(lines[0].clone(), 1_000).unwrap();
    p2.observe(lines[1].clone(), 1_001).unwrap();
    assert_eq!(p.buffered_event_count().unwrap(), 2);
    assert_eq!(p2.buffered_event_count().unwrap(), 2);
}

#[test]
fn resumed_producer_starts_from_checkpoint() {
    let tenant = Uuid::now_v7();
    // Drive a fresh chain to seq 3 so we have a non-genesis
    // checkpoint to resume from.
    let audit_sink = fresh_audit_sink();
    let lines = emit_n_events(&audit_sink, tenant, 4);
    // Resume a producer at seq=3 with the chain head AFTER seq=2.
    let checkpoint_head = lines[2].link_hash;
    let sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
    let policy = FlushPolicy {
        flush_after_ms: u64::MAX,
        max_events_per_chunk: 1,
        max_bytes_per_chunk: u64::MAX,
    };
    let producer = ArchiveProducer::new_for_tenant(
        tenant,
        sink.clone() as Arc<dyn ArchiveSink>,
        policy,
        checkpoint_head,
        3,
    );
    // Observe seq=3 — must succeed.
    let r = producer.observe(lines[3].clone(), 1_000).unwrap();
    let receipt = r.expect("policy 1-event chunk: flush trips");
    assert_eq!(receipt.first_sequence_number, 3);
    assert_eq!(receipt.last_sequence_number, 3);
    assert_eq!(receipt.chain_head_after, lines[3].link_hash);
}

#[test]
fn flush_policy_should_flush_combinations() {
    let p = FlushPolicy {
        flush_after_ms: 1000,
        max_events_per_chunk: 10,
        max_bytes_per_chunk: 100,
    };
    // Trip via events.
    assert!(p.should_flush(10, 0, 0));
    // Trip via bytes.
    assert!(p.should_flush(0, 100, 0));
    // Trip via age.
    assert!(p.should_flush(0, 0, 1000));
    // No trip.
    assert!(!p.should_flush(9, 99, 999));
}

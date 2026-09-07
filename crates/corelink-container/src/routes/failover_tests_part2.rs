// ── B.3 bounded audit sink ───────────────────────────────────────────────

#[test]
fn ring_buffer_sink_keeps_newest_cap_records() {
    use corelink_failover_router::{
        FailoverAuditEventType as ReplicaAuditEventType, FailoverAuditRecord as ReplicaAuditRecord,
        FailoverAuditSink as ReplicaAuditSink,
    };
    let mk = |i: u32| ReplicaAuditRecord {
        event_type: ReplicaAuditEventType::ReplicationStarted,
        tenant_id_hash: format!("t{i}"),
        blob_hash: String::new(),
        primary_region: "enam".to_owned(),
        replica_region: "wnam".to_owned(),
        timestamp_ms: 1_000 + u64::from(i),
        detail: String::new(),
    };
    // The production wiring's cap (same constant `with_primary` uses).
    let state = FailoverLayerState::with_primary(Some(Region::Enam));
    let sink = state.audit_sink();
    for i in 0..(FAILOVER_AUDIT_SINK_CAP as u32 + 10) {
        sink.emit(mk(i)).unwrap();
    }
    let records = sink.records();
    assert_eq!(records.len(), FAILOVER_AUDIT_SINK_CAP, "cap enforced");
    assert_eq!(
        records.last().unwrap().tenant_id_hash,
        format!("t{}", FAILOVER_AUDIT_SINK_CAP as u32 + 9),
        "newest preserved"
    );
    assert_eq!(
        records.first().unwrap().tenant_id_hash,
        "t10",
        "oldest dropped"
    );
}

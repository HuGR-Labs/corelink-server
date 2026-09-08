use super::*;

fn dummy_receipt(tenant: Uuid, first: u64, last: u64) -> ArchiveReceipt {
    ArchiveReceipt {
        r2_key: format!("audit/2026/05/15/{first:08}.ndjson"),
        tenant_id: tenant,
        first_event_time_ms: 1_000,
        last_event_time_ms: 2_000,
        first_sequence_number: first,
        last_sequence_number: last,
        prev_hash_anchor: ChainHash::genesis(),
        chain_head_after: ChainHash([0xAB; 32]),
        bytes_written: 100,
        events_written: last - first + 1,
    }
}

fn dummy_row(tenant: Uuid, region: Region, seq: u64, time_ms: u64, ty: &str) -> ShadowEventRow {
    ShadowEventRow {
        tenant_id: tenant,
        seq,
        event_time_ms: time_ms,
        event_type: ty.to_string(),
        prev_hash: ChainHash::genesis(),
        link_hash: ChainHash([(seq as u8); 32]),
        region,
        payload_json: format!("{{\"i\":{seq}}}"),
    }
}

#[test]
fn lag_constants_pin_to_canonical_slo() {
    // Wave-18 spec: nominal 5 min, SEV-2 60 min.
    assert_eq!(SHADOW_LAG_NOMINAL_MAX_MS, 5 * 60 * 1_000);
    assert_eq!(SHADOW_LAG_SEV2_THRESHOLD_MS, 60 * 60 * 1_000);
    assert!(SHADOW_LAG_SEV2_THRESHOLD_MS > SHADOW_LAG_NOMINAL_MAX_MS);
}

#[test]
fn audit_event_type_constants_match_spec() {
    assert_eq!(
        EVENT_TYPE_SHADOW_SYNCED,
        "corelink.audit.neon_shadow_synced.v1"
    );
    assert_eq!(
        EVENT_TYPE_SHADOW_SYNC_FAILED,
        "corelink.audit.neon_shadow_sync_failed.v1"
    );
}

#[test]
fn shadow_sync_receipt_lag_classifiers() {
    let tenant = Uuid::now_v7();
    let r = ShadowSyncReceipt {
        tenant_id: tenant,
        first_seq: 0,
        last_seq: 5,
        rows_persisted: 6,
        observed_lag_ms: 60_000, // 1 min
        region: Region::Iad,
    };
    assert!(r.within_nominal_lag());
    assert!(!r.breaches_sev2_threshold());

    let r_breach = ShadowSyncReceipt {
        observed_lag_ms: SHADOW_LAG_SEV2_THRESHOLD_MS,
        ..r.clone()
    };
    assert!(!r_breach.within_nominal_lag());
    assert!(r_breach.breaches_sev2_threshold());
}

#[test]
fn sync_chunk_persists_rows_and_emits_success_audit() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
    let rows = vec![
        dummy_row(
            tenant,
            Region::Iad,
            0,
            1_000,
            "dev.hugr.corelink.cas.put.v1",
        ),
        dummy_row(
            tenant,
            Region::Iad,
            1,
            2_000,
            "dev.hugr.corelink.cas.get.v1",
        ),
    ];
    let receipt = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 1), &rows, 3_000)
        .expect("sync ok");
    assert_eq!(receipt.rows_persisted, 2);
    assert_eq!(receipt.tenant_id, tenant);
    assert_eq!(sink.row_count(), 2);
    // Lag = now_ms - first_event_time_ms = 3000 - 1000 = 2000ms
    assert_eq!(receipt.observed_lag_ms, 2_000);
    assert!(receipt.within_nominal_lag());

    let audit_snap = audit.snapshot().expect("snap");
    assert_eq!(audit_snap.len(), 1);
    assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNCED);
    assert_eq!(audit_snap[0].sev, "info");
}

#[test]
fn sync_chunk_rejects_cross_tenant_fail_closed() {
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant_a, Region::Iad, audit.clone());
    let rows = vec![dummy_row(tenant_b, Region::Iad, 0, 1_000, "x")];
    let err = sink
        .sync_chunk(&dummy_receipt(tenant_b, 0, 0), &rows, 2_000)
        .expect_err("must reject cross-tenant");
    assert!(matches!(
        err,
        NeonShadowError::TenantIsolationViolation { .. }
    ));
    // No rows persisted.
    assert_eq!(sink.row_count(), 0);
    // Failure audit emitted.
    let audit_snap = audit.snapshot().expect("snap");
    assert_eq!(audit_snap.len(), 1);
    assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
    assert_eq!(audit_snap[0].sev, "sev-2");
}

#[test]
fn sync_chunk_rejects_cross_region_fail_closed() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
    let rows = vec![dummy_row(tenant, Region::Fra, 0, 1_000, "x")];
    let err = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
        .expect_err("must reject cross-region");
    assert!(matches!(err, NeonShadowError::ResidencyViolation { .. }));
}

#[test]
fn sync_chunk_injected_failure_emits_sev2() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
    sink.inject_failure(Some("neon connection lost".into()))
        .unwrap();
    let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
    let err = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
        .expect_err("injected failure must propagate");
    assert!(matches!(err, NeonShadowError::Backend(_)));
    assert_eq!(sink.row_count(), 0);
    let audit_snap = audit.snapshot().unwrap();
    assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
}

#[test]
fn aggregate_event_count_basic_window() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let rows = vec![
        dummy_row(tenant, Region::Iad, 0, 1_000, "cas.put"),
        dummy_row(tenant, Region::Iad, 1, 2_000, "cas.put"),
        dummy_row(tenant, Region::Iad, 2, 3_000, "cas.get"),
    ];
    sink.sync_chunk(&dummy_receipt(tenant, 0, 2), &rows, 4_000)
        .unwrap();
    let buckets = sink.aggregate_event_count(0, 10_000, None).unwrap();
    let put = buckets.iter().find(|b| b.event_type == "cas.put").unwrap();
    let get = buckets.iter().find(|b| b.event_type == "cas.get").unwrap();
    assert_eq!(put.count, 2);
    assert_eq!(get.count, 1);
}

#[test]
fn aggregate_timeline_buckets_correctly() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let rows = vec![
        dummy_row(tenant, Region::Iad, 0, 1_000, "x"),
        dummy_row(tenant, Region::Iad, 1, 1_500, "x"),
        dummy_row(tenant, Region::Iad, 2, 5_000, "x"),
    ];
    sink.sync_chunk(&dummy_receipt(tenant, 0, 2), &rows, 6_000)
        .unwrap();
    // 2s granularity → bucket [0..2000) has 2 rows, [4000..6000) has 1.
    let timeline = sink.aggregate_timeline(0, 6_000, 2_000).unwrap();
    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].bucket_start_ms, 0);
    assert_eq!(timeline[0].count, 2);
    assert_eq!(timeline[1].bucket_start_ms, 4_000);
    assert_eq!(timeline[1].count, 1);
}

#[test]
fn aggregate_zero_granularity_returns_internal_error() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let err = sink
        .aggregate_timeline(0, 1, 0)
        .expect_err("zero granularity");
    assert!(matches!(err, NeonShadowError::Internal(_)));
}

#[test]
fn shadow_event_row_from_persisted_line_extracts_fields() {
    let tenant = Uuid::now_v7();
    let prev_hash_hex = "00".repeat(32);
    let ndjson = format!(
            "{{\"type\":\"dev.hugr.corelink.cas.put.v1\",\"time_ms\":1700,\"prev_hash\":\"{prev_hash_hex}\"}}"
        );
    let line = PersistedAuditLine {
        r2_key: "k".into(),
        ndjson,
        tenant_id: tenant,
        sequence_number: 7,
        link_hash: ChainHash([0x11; 32]),
    };
    let row =
        ShadowEventRow::from_persisted_line(&line, Region::Iad).expect("well-formed NDJSON parses");
    assert_eq!(row.tenant_id, tenant);
    assert_eq!(row.seq, 7);
    assert_eq!(row.event_time_ms, 1_700);
    assert_eq!(row.event_type, "dev.hugr.corelink.cas.put.v1");
    assert_eq!(row.region, Region::Iad);
    assert_eq!(row.link_hash, ChainHash([0x11; 32]));
}

#[test]
fn from_persisted_line_rejects_malformed_input() {
    // Wave-21 (B-P1-05 closure) — every malformed NDJSON shape
    // surfaces a typed `ParseError`, never silently admits a row
    // with `event_time_ms = 0` / `event_type = ""`.
    let tenant = Uuid::now_v7();
    let mk = |ndjson: &str| PersistedAuditLine {
        r2_key: "k".into(),
        ndjson: ndjson.to_string(),
        tenant_id: tenant,
        sequence_number: 0,
        link_hash: ChainHash::genesis(),
    };

    // 1. Garbage (not valid JSON at all).
    let garbage = ShadowEventRow::from_persisted_line(&mk("this is not json"), Region::Iad)
        .expect_err("non-JSON must reject");
    assert!(matches!(garbage, ParseError::InvalidJson(_)));

    // 2. JSON missing `time_ms`.
    let no_time = ShadowEventRow::from_persisted_line(
            &mk("{\"type\":\"x\",\"prev_hash\":\"00000000000000000000000000000000000000000000000000000000000000ab\"}"),
            Region::Iad,
        )
        .expect_err("missing time_ms must reject");
    assert!(matches!(no_time, ParseError::MissingField("time_ms")));

    // 3. JSON missing `type`.
    let no_type = ShadowEventRow::from_persisted_line(
            &mk("{\"time_ms\":1,\"prev_hash\":\"00000000000000000000000000000000000000000000000000000000000000ab\"}"),
            Region::Iad,
        )
        .expect_err("missing type must reject");
    assert!(matches!(no_type, ParseError::MissingField("type")));

    // 4. JSON missing `prev_hash`.
    let no_prev =
        ShadowEventRow::from_persisted_line(&mk("{\"time_ms\":1,\"type\":\"x\"}"), Region::Iad)
            .expect_err("missing prev_hash must reject");
    assert!(matches!(no_prev, ParseError::MissingField("prev_hash")));

    // 5. Invalid hex in prev_hash.
    let bad_hex = ShadowEventRow::from_persisted_line(
        &mk("{\"time_ms\":1,\"type\":\"x\",\"prev_hash\":\"NOT_HEX\"}"),
        Region::Iad,
    )
    .expect_err("non-hex prev_hash must reject");
    assert!(matches!(bad_hex, ParseError::InvalidPrevHash(_)));
}

/// Audit sink that fails every emit — exercises the wave-21
/// `AuditEmitFailed` lift for the in-memory shadow sink.
#[derive(Debug, Default)]
struct AlwaysFailAuditSink;

impl ShadowSyncAuditSink for AlwaysFailAuditSink {
    fn emit(&self, _row: ShadowSyncAuditRow) -> Result<(), &'static str> {
        Err("synthetic audit-emit failure")
    }
}

#[test]
fn in_memory_sink_propagates_audit_emit_failure_on_success_path() {
    // Wave-21 (B-P1-02 closure) — happy-path audit emit failure no
    // longer silently swallowed; surfaces `AuditEmitFailed`.
    let tenant = Uuid::now_v7();
    let audit: Arc<dyn ShadowSyncAuditSink> = Arc::new(AlwaysFailAuditSink);
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
    let err = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
        .expect_err("audit emit failure must propagate");
    assert!(matches!(err, NeonShadowError::AuditEmitFailed(_)));
}

#[test]
fn aggregate_event_count_filter_narrows_to_one_type() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let rows = vec![
        dummy_row(tenant, Region::Iad, 0, 100, "a"),
        dummy_row(tenant, Region::Iad, 1, 200, "b"),
    ];
    sink.sync_chunk(&dummy_receipt(tenant, 0, 1), &rows, 300)
        .unwrap();
    let only_a = sink.aggregate_event_count(0, 1_000, Some("a")).unwrap();
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].event_type, "a");
    assert_eq!(only_a[0].count, 1);
}

#[test]
fn sync_chunk_emits_sev2_when_lag_breaches_threshold() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
    let first_event_time = 1_000u64;
    // now_ms = first_event_time + 60 min (3_600_000 ms)
    let now_ms = first_event_time + SHADOW_LAG_SEV2_THRESHOLD_MS;
    let rows = vec![dummy_row(tenant, Region::Iad, 0, first_event_time, "x")];
    let receipt = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, now_ms)
        .unwrap();
    assert!(receipt.breaches_sev2_threshold());
    let audit_snap = audit.snapshot().unwrap();
    assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNCED);
    assert_eq!(audit_snap[0].sev, "sev-2");
}

#[test]
fn sync_chunk_is_idempotent_on_duplicate_seq() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let rows = vec![dummy_row(tenant, Region::Iad, 0, 100, "x")];
    sink.sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 200)
        .unwrap();
    sink.sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 250)
        .unwrap();
    // PRIMARY KEY (tenant_id, seq) dedup: still 1 row.
    assert_eq!(sink.row_count(), 1);
}

#[test]
fn empty_rows_slice_rejected_internal() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
    let err = sink
        .sync_chunk(&dummy_receipt(tenant, 0, 0), &[], 100)
        .expect_err("empty rows");
    assert!(matches!(err, NeonShadowError::Internal(_)));
}

#[test]
fn audit_sink_trait_is_object_safe() {
    let sinks: Vec<Arc<dyn ShadowSyncAuditSink>> =
        vec![Arc::new(InMemoryShadowSyncAuditSink::new())];
    assert_eq!(sinks.len(), 1);
}

#[test]
fn neon_shadow_sink_trait_is_object_safe() {
    let tenant = Uuid::now_v7();
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let sinks: Vec<Arc<dyn NeonShadowSink>> = vec![Arc::new(InMemoryNeonShadowSink::new(
        tenant,
        Region::Iad,
        audit,
    ))];
    assert_eq!(sinks.len(), 1);
}

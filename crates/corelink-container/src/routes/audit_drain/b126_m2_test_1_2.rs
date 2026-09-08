#[test]
fn audit_drain_ok_is_true_when_no_partition_failed() {
    assert!(audit_drain_ok(0));
}

#[test]
fn audit_drain_ok_is_false_when_any_partition_failed() {
    assert!(!audit_drain_ok(1));
    assert!(!audit_drain_ok(7));
}

#[test]
fn audit_drain_response_body_ok_field_reflects_partitions_failed() {
    // All-zero → ok:true.
    let j = build_drain_response_body(&DrainOutcome {
        ok: true,
        partitions_drained: 0,
        rows_sealed: 0,
        partitions_drifted: 0,
        partitions_failed: 0,
        partitions_leased: 0,
        heads_resigned: 0,
        incomplete: false,
    });
    assert_eq!(j["ok"], serde_json::Value::Bool(true));
    assert_eq!(j["incomplete"], serde_json::Value::Bool(false));
    // partitions_failed > 0 → ok:false (the fix).
    let j = build_drain_response_body(&DrainOutcome {
        ok: false,
        partitions_drained: 1,
        rows_sealed: 5,
        partitions_drifted: 0,
        partitions_failed: 1,
        partitions_leased: 0,
        heads_resigned: 0,
        incomplete: false,
    });
    assert_eq!(j["ok"], serde_json::Value::Bool(false));
    assert_eq!(
        j["partitions_failed"],
        serde_json::Value::Number(1u64.into())
    );
    // incomplete stays independent of ok (a budget-bound sweep with
    // zero failures is a successful sweep that needs another call).
    let j = build_drain_response_body(&DrainOutcome {
        ok: true,
        partitions_drained: 0,
        rows_sealed: 200,
        partitions_drifted: 0,
        partitions_failed: 0,
        partitions_leased: 0,
        heads_resigned: 0,
        incomplete: true,
    });
    assert_eq!(j["ok"], serde_json::Value::Bool(true));
    assert_eq!(j["incomplete"], serde_json::Value::Bool(true));
    // And incomplete combined with a real failure.
    let j = build_drain_response_body(&DrainOutcome {
        ok: false,
        partitions_drained: 0,
        rows_sealed: 200,
        partitions_drifted: 0,
        partitions_failed: 3,
        partitions_leased: 0,
        heads_resigned: 0,
        incomplete: true,
    });
    assert_eq!(j["ok"], serde_json::Value::Bool(false));
    assert_eq!(j["incomplete"], serde_json::Value::Bool(true));
}

#[allow(dead_code)]
const B126_M2_TEST_1_2_REANCHOR: () = ();

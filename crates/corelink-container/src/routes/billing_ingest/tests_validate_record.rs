//! The acceptance boundary of `validate_record`, tested from both sides at
//! once: the canonical record passes through with its typed fields intact, and
//! a field is refused the first byte past its declared bound.
//!
//! The wire integers are intentionally tested at the exact signed-storage cap
//! and one above it. A cap tested only from above cannot tell an off-by-one
//! from a correct limit.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::{hex64, record_json, tenant_a};
use super::*;

#[test]
fn validate_record_accepts_canonical() {
    let wire: UsageRecordWire =
        serde_json::from_value(record_json(&tenant_a(), &hex64(0x20))).unwrap();
    let rec = validate_record(wire).unwrap();
    assert_eq!(rec.event_kind, UsageEventKind::RunnerSlotSeconds);
    assert_eq!(rec.region, "iad");
    assert_eq!(rec.qty, 7200);
}

#[test]
fn signed_storage_boundary_is_exact_for_qty_and_time_ms() {
    let at_cap: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x25));
        r["qty"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE);
        r["time_ms"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE);
        r
    })
    .unwrap();
    assert!(validate_record(at_cap).is_ok());

    let qty_above: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x26));
        r["qty"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
        r
    })
    .unwrap();
    assert_eq!(
        validate_record(qty_above),
        Err(RecordError::QtyOutOfStorageRange)
    );

    let time_above: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x27));
        r["time_ms"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
        r
    })
    .unwrap();
    assert_eq!(
        validate_record(time_above),
        Err(RecordError::TimeMsOutOfStorageRange)
    );
}

#[test]
fn source_over_cap_is_rejected() {
    let too_long: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x23));
        r["source"] = serde_json::json!("x".repeat(MAX_SOURCE_LEN + 1));
        r
    })
    .unwrap();
    assert_eq!(validate_record(too_long), Err(RecordError::SourceTooLong));

    // A source exactly at the cap is still accepted.
    let at_cap: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x24));
        r["source"] = serde_json::json!("x".repeat(MAX_SOURCE_LEN));
        r
    })
    .unwrap();
    assert!(validate_record(at_cap).is_ok());
}

/// This deliberately has no wildcard: adding a [`RecordError`] variant makes
/// this test stop compiling until its rejection fixture is added below.
fn record_error_variant_name(error: &RecordError) -> &'static str {
    match error {
        RecordError::BadTenantId => "bad tenant ID",
        RecordError::BadBillingPeriod => "bad billing period",
        RecordError::QtyOutOfStorageRange => "quantity outside storage range",
        RecordError::BadRegion => "bad region",
        RecordError::BadIdemKey => "bad idempotency key",
        RecordError::EmptySource => "empty source",
        RecordError::SourceTooLong => "oversized source",
        RecordError::TimeMsOutOfStorageRange => "timestamp outside storage range",
    }
}

#[test]
fn every_record_error_variant_has_a_rejection_fixture() {
    let cases: Vec<(UsageRecordWire, RecordError, &str)> = vec![
        (
            serde_json::from_value(record_json("not-a-uuid", &hex64(0x30))).unwrap(),
            RecordError::BadTenantId,
            "bad_tenant_id",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x31));
                record["billing_period"] = serde_json::json!("2026-13");
                record
            })
            .unwrap(),
            RecordError::BadBillingPeriod,
            "bad_billing_period",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x35));
                record["qty"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
                record
            })
            .unwrap(),
            RecordError::QtyOutOfStorageRange,
            "qty_out_of_storage_range",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x32));
                record["region"] = serde_json::json!("i2d");
                record
            })
            .unwrap(),
            RecordError::BadRegion,
            "bad_region",
        ),
        (
            serde_json::from_value(record_json(&tenant_a(), "not-64-hex")).unwrap(),
            RecordError::BadIdemKey,
            "bad_idem_key",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x33));
                record["source"] = serde_json::json!("");
                record
            })
            .unwrap(),
            RecordError::EmptySource,
            "empty_source",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x34));
                record["source"] = serde_json::json!("x".repeat(MAX_SOURCE_LEN + 1));
                record
            })
            .unwrap(),
            RecordError::SourceTooLong,
            "source_too_long",
        ),
        (
            serde_json::from_value({
                let mut record = record_json(&tenant_a(), &hex64(0x36));
                record["time_ms"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
                record
            })
            .unwrap(),
            RecordError::TimeMsOutOfStorageRange,
            "time_ms_out_of_storage_range",
        ),
    ];

    for (wire, expected, expected_code) in cases {
        let label = record_error_variant_name(&expected);
        assert_eq!(
            expected.code(),
            expected_code,
            "{label} has a stable reason code"
        );
        assert_eq!(validate_record(wire), Err(expected), "{label} is rejected");
    }
}

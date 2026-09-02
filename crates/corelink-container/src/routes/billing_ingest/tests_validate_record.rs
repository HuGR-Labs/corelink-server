//! The acceptance boundary of `validate_record`, tested from both sides at
//! once: the canonical record passes through with its typed fields intact, and
//! a field is refused the first byte past its declared bound.
//!
//! `source` carries the bound because it is the one free-text field on the
//! record; the enum, uuid, period and hex fields are bounded by their own
//! parsers. The exactly-at-cap case is here on purpose — a cap tested only
//! from above cannot tell an off-by-one from a correct limit.
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

//! The `event_payload_hash` that binds the billable fields of a staged row is
//! a deterministic 64-char BLAKE3 hex.
//!
//! The staging table's `CHECK(length(event_payload_hash) = 64)` and the tamper
//! check it exists for both rest on this: the digest binds `qty`, period,
//! region and the rest into one image, so a stored `qty` edited after the fact
//! no longer reproduces it. A digest that is not reproducible from the same
//! record would make that check fire on honest rows and prove nothing about
//! dishonest ones.
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
fn payload_hash_is_64_hex_and_deterministic() {
    let wire: UsageRecordWire =
        serde_json::from_value(record_json(&tenant_a(), &hex64(0x22))).unwrap();
    let rec = validate_record(wire).unwrap();
    let h1 = D1UsageStagingStore::payload_hash(&rec);
    let h2 = D1UsageStagingStore::payload_hash(&rec);
    assert_eq!(h1.len(), 64, "BLAKE3-256 hex must be 64 chars");
    assert!(h1.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        h1, h2,
        "the same record must reproduce the same payload hash"
    );
}

//! The property: the batch never emits a statement D1 would REFUSE, and never
//! one that means NOTHING.
//!
//! Distinct from the shape property in `tests_batch_shape`: those tests ask
//! what the rows say, these ask whether the statement carrying them is legal
//! at the boundaries. D1 caps a single string value at 2,000,000 bytes, so at
//! the `FIND_MISSING_BLOB_CAP` of 4096 digests the rows MUST be split at
//! `AUDIT_BATCH_ROWS_PER_STATEMENT` — with every chunk fitting and every
//! requested digest still getting its own row. At the other end, zero events
//! must produce no round trip at all rather than a degenerate
//! `json_each('[]')`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::stub_sink;
use super::*;

#[test]
fn batch_chunking_respects_d1s_value_size_cap_at_the_find_missing_cap() {
    // D1 caps a single string/BLOB/row at 2,000,000 bytes. At the
    // FIND_MISSING_BLOB_CAP of 4096 digests the batch must therefore be
    // split — and every chunk must fit with room to spare.
    const D1_MAX_VALUE_BYTES: usize = 2_000_000;
    let sink = stub_sink();
    // Deliberately long tenant/principal: the worst realistic element.
    let tenant = "d863fafb-0000-4000-8000-000000000000";
    let principal = "pat_0123456789abcdef0123456789abcdef@d863fafb-0000-4000-8000-000000000000";
    let rows: Vec<AuditRow> = (0..4096)
        .map(|i| {
            sink.build_row(
                "read.attempted",
                tenant,
                Some(&format!("{i:064x}")),
                principal,
                1_700_000_000_123,
            )
            .expect("row builds")
        })
        .collect();

    let chunks: Vec<&[AuditRow]> = rows.chunks(AUDIT_BATCH_ROWS_PER_STATEMENT).collect();
    assert_eq!(chunks.len(), 16, "4096 / 256 statements, not 4096");
    let mut total = 0usize;
    for chunk in &chunks {
        let (_, params) = D1AuditOutboxSink::build_batch_insert(chunk).expect("chunk builds");
        let encoded = params[0].as_str().expect("string");
        assert!(
            encoded.len() < D1_MAX_VALUE_BYTES,
            "chunk param is {} bytes, D1 caps a value at {D1_MAX_VALUE_BYTES}",
            encoded.len()
        );
        total += serde_json::from_str::<Value>(encoded)
            .expect("valid JSON")
            .as_array()
            .expect("array")
            .len();
    }
    assert_eq!(total, 4096, "every requested digest still gets its own row");
}

#[test]
fn empty_batch_issues_no_statement_at_all() {
    // `append_batch_async` dispatches precisely this list. Zero events must
    // therefore produce no `json_each('[]')` statement or D1 round trip.
    let statements = D1AuditOutboxSink::build_batch_statements(&[]).expect("batch builds");
    assert!(
        statements.is_empty(),
        "an empty batch emits zero statements"
    );
}

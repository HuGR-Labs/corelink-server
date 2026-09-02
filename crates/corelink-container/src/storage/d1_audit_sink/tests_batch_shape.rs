//! The property: batching changes HOW MANY round trips carry the rows, never
//! WHAT an auditor reads.
//!
//! The `findMissingBlobs` seam can turn one request into 4096 `ReadAttempted`
//! events, so `append_batch_async` writes them with a JSON1
//! `INSERT OR IGNORE … SELECT … FROM json_each(?1)` instead of N serial
//! statements. Both write shapes derive their row from the SAME `build_row`,
//! and these tests hold them to that: the batch element and the single-row
//! positional params agree field for field, the batch rides in exactly ONE
//! bound parameter (D1 caps a statement at 100), an empty digest still becomes
//! a SQL NULL, and a digest repeated inside one request still derives the same
//! `id` and `request_id` so `INSERT OR IGNORE` dedupes it — the batch does not
//! pre-dedupe, the DB does, exactly as with a serial re-emit.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::{stub_sink, DIGEST_A};
use super::*;

#[test]
fn batch_element_and_single_row_params_agree_field_for_field() {
    let sink = stub_sink();
    let (sql_one, params) = sink
        .build_insert(
            "read.attempted",
            "t-1",
            Some(DIGEST_A),
            "p@t-1",
            1_700_000_000_123,
        )
        .expect("single-row insert builds");
    let row = sink
        .build_row(
            "read.attempted",
            "t-1",
            Some(DIGEST_A),
            "p@t-1",
            1_700_000_000_123,
        )
        .expect("row builds");
    let element = row.to_json_element();

    // Positional params of the single-row INSERT, in column order.
    assert_eq!(element["id"], params[0], "id (PK / idempotency key)");
    assert_eq!(element["tenant"], params[1], "tenant_id");
    assert_eq!(element["digest"], params[2], "digest");
    assert_eq!(element["request_id"], params[3], "request_id (UNIQUE key)");
    assert_eq!(element["event_type"], params[4], "event_type (taxonomy)");
    assert_eq!(element["payload"], params[5], "payload_json envelope");
    assert_eq!(element["at"], params[6], "enqueued_at");
    assert_eq!(params.len(), 7, "single-row shape unchanged");
    assert!(sql_one.contains("INSERT OR IGNORE"), "{sql_one}");
}

#[test]
fn batch_insert_binds_exactly_one_param_carrying_every_row() {
    let sink = stub_sink();
    let rows: Vec<AuditRow> = (0..40)
        .map(|i| {
            sink.build_row(
                "read.attempted",
                "t-1",
                Some(&format!("{i:064x}")),
                "p@t-1",
                7,
            )
            .expect("row builds")
        })
        .collect();
    let (sql, params) = D1AuditOutboxSink::build_batch_insert(&rows).expect("batch builds");

    // ONE param — this is the whole point: D1 caps a statement at 100
    // bound params, so 40 rows x 7 params could not have been bound
    // positionally.
    assert_eq!(params.len(), 1, "the batch rides in a single parameter");
    let encoded = params[0].as_str().expect("param is the JSON array string");
    let parsed: Value = serde_json::from_str(encoded).expect("valid JSON");
    let arr = parsed.as_array().expect("JSON array");
    assert_eq!(arr.len(), 40, "N events -> N rows, no summarisation");

    // Same table, same columns, same idempotency, same residency guard.
    assert!(sql.contains("INSERT OR IGNORE INTO audit_outbox"), "{sql}");
    assert!(sql.contains("FROM json_each(?1)"), "{sql}");
    assert!(
        sql.contains("COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = json_extract(value,'$.tenant')), 'wnam')"),
        "the migration-0023 residency trigger MUST be satisfied per row: {sql}"
    );
    // Every `$.…` path the SQL reads must exist on every element.
    for element in arr {
        for key in [
            "id",
            "tenant",
            "digest",
            "request_id",
            "event_type",
            "payload",
            "at",
        ] {
            assert!(
                element.get(key).is_some(),
                "element missing $.{key}: {element}"
            );
        }
    }
}

#[test]
fn batch_preserves_null_digest_for_events_without_a_hash() {
    let sink = stub_sink();
    let row = sink
        .build_row("read.attempted", "t-1", Some(""), "p@t-1", 7)
        .expect("row builds");
    let element = row.to_json_element();
    assert_eq!(element["digest"], Value::Null, "empty digest -> SQL NULL");
    // …and the id/request_id use the same `-` sentinel the single-row
    // path uses, so the two shapes dedupe against each other.
    assert!(
        element["id"].as_str().expect("id").contains(":-:"),
        "{element}"
    );
}

#[test]
fn duplicate_events_in_one_batch_collide_on_the_idempotency_keys() {
    // A REAPI client may repeat a digest inside one findMissingBlobs
    // request. Both occurrences must derive the SAME `id` (PK) and the
    // SAME `request_id` so `INSERT OR IGNORE` dedupes them, exactly as
    // it dedupes a serial re-emit today.
    let sink = stub_sink();
    let mk = || {
        sink.build_row("read.attempted", "t-1", Some(DIGEST_A), "p@t-1", 42)
            .expect("row builds")
    };
    let (a, b) = (mk(), mk());
    assert_eq!(a, b, "identical events derive identical rows");

    let (_, params) = D1AuditOutboxSink::build_batch_insert(&[a, b]).expect("batch builds");
    let parsed: Value =
        serde_json::from_str(params[0].as_str().expect("string")).expect("valid JSON");
    let arr = parsed.as_array().expect("array");
    assert_eq!(arr.len(), 2, "the batch does NOT pre-dedupe; the DB does");
    assert_eq!(arr[0]["id"], arr[1]["id"], "same PK -> INSERT OR IGNORE");
    assert_eq!(
        arr[0]["request_id"], arr[1]["request_id"],
        "same UNIQUE(request_id, event_type) key"
    );
}

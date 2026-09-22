//! One invalid RECORD is counted in `rejected` and skipped: it never
//! blocks its batch-mates and never leaves the runner re-POSTing forever.
//!
//! The old all-or-nothing behaviour let a single poison record 400 its whole
//! batch, and a non-2xx makes the runner retain and re-push the same buffer
//! indefinitely — so a per-record defect turned into an ingest flood that also
//! stranded the GOOD records travelling with it. The tally is the contract:
//! `total` counts only rows actually staged.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use tower::ServiceExt;

use super::tests_support::{
    body_json, hex64, ingest_request, record_json, state_with, tenant_a, FakeStore, TEST_AUTH_KEY,
};
use super::*;

#[tokio::test]
async fn bad_tenant_id_is_skipped_not_fatal() {
    // A rejected-only batch returns 422 with a record-level outcome.
    // NOT a 400 — the per-record-skip contract. The reason-code mapping is
    // pinned exhaustively in `every_record_error_variant_has_a_rejection_fixture`.
    assert_eq!(RecordError::BadTenantId.code(), "bad_tenant_id");
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json("not-a-uuid", &hex64(0x12))]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
    assert!(store.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bad_billing_period_is_skipped_not_fatal() {
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let mut rec = record_json(&tenant_a(), &hex64(0x13));
    rec["billing_period"] = serde_json::json!("2026-13");
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([rec]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
    assert!(store.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bad_idem_key_is_skipped_not_fatal() {
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json(&tenant_a(), "too-short")]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
    assert!(store.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_record_is_skipped_good_record_staged() {
    // A batch with one GOOD then one BAD record must stage the GOOD one and
    // SKIP the bad one (202, `rejected:1`) — NOT 400 the whole batch. The old
    // all-or-nothing behaviour let one poison record block its batch-mates
    // AND flood the ingest (the runner retains + re-POSTs a non-2xx forever).
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let mut bad = record_json(&tenant_a(), &hex64(0x15));
    bad["billing_period"] = serde_json::json!("nope");
    let body = serde_json::json!([record_json(&tenant_a(), &hex64(0x16)), bad]);
    let resp = app
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!(ir.accepted, 1, "the good record is staged");
    assert_eq!(ir.rejected, 1, "the bad record is counted, not fatal");
    assert_eq!(ir.total, 1, "total counts only successfully-staged rows");
    assert_eq!(
        store.seen.lock().unwrap().len(),
        1,
        "exactly the good record is staged"
    );
}

#[tokio::test]
async fn all_records_invalid_are_422() {
    // A rejected-only batch is caller-correctable and carries each record's
    // stable result. Batch-level faults (empty / oversized / unparseable) stay 400.
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let mut b1 = record_json(&tenant_a(), &hex64(0x18));
    b1["region"] = serde_json::json!("nope4"); // > 3 chars → BadRegion
    let mut b2 = record_json(&tenant_a(), &hex64(0x19));
    b2["billing_period"] = serde_json::json!("2026-13");
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([b1, b2]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!(
        (ir.accepted, ir.deduped, ir.rejected, ir.total),
        (0, 0, 2, 0)
    );
    assert!(store.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn out_of_storage_range_records_skip_at_first_middle_and_last() {
    let store = Arc::new(FakeStore::new());
    let existing_key = hex64(0x60);
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));

    // Seed the identity used by the invalid last member. Validation must
    // still reject that member before deduplication; an existing coordinate
    // cannot turn an out-of-domain payload into a successful replay.
    let seeded = app
        .clone()
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json(&tenant_a(), &existing_key)]),
        ))
        .await
        .unwrap();
    assert_eq!(seeded.status(), StatusCode::ACCEPTED);

    let mut first = record_json(&tenant_a(), &hex64(0x61));
    first["qty"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
    let mut middle = record_json(&tenant_a(), &hex64(0x62));
    middle["time_ms"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
    let mut last_existing = record_json(&tenant_a(), &existing_key);
    last_existing["qty"] = serde_json::json!(MAX_PERSISTED_NONNEGATIVE + 1);
    let body = serde_json::json!([
        first,
        record_json(&tenant_a(), &hex64(0x63)),
        middle,
        record_json(&tenant_a(), &hex64(0x64)),
        last_existing,
    ]);

    let resp = app
        .clone()
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), body.clone()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let first_attempt: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!(
        (
            first_attempt.accepted,
            first_attempt.deduped,
            first_attempt.rejected,
            first_attempt.total
        ),
        (2, 0, 3, 2)
    );
    assert_eq!(
        first_attempt.outcomes[0].reason.as_deref(),
        Some("qty_out_of_storage_range")
    );
    assert_eq!(
        first_attempt.outcomes[2].reason.as_deref(),
        Some("time_ms_out_of_storage_range")
    );
    assert_eq!(
        first_attempt.outcomes[4].reason.as_deref(),
        Some("qty_out_of_storage_range")
    );
    assert_eq!(
        store.seen.lock().unwrap().len(),
        3,
        "only valid siblings stage"
    );

    // Replaying the same mixed batch deterministically re-rejects the poison
    // records and dedups the valid siblings. It never becomes a retryable 503.
    let resp = app
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let retry: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
    assert_eq!(
        (retry.accepted, retry.deduped, retry.rejected, retry.total),
        (0, 2, 3, 2)
    );
    assert_eq!(store.seen.lock().unwrap().len(), 3);
}

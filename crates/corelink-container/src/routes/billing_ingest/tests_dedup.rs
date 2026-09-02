//! A `(tenant_id, idem_key)` coordinate stages EXACTLY ONCE — whether the
//! repeat arrives in a later request, twice inside the SAME batch, or spelled
//! in the other hex case.
//!
//! This is the double-billing guard. The coordinate is the D1 PRIMARY KEY of
//! `usage_event_staging`, so anything that reaches the store under a second
//! spelling of the same key becomes a second row the aggregator rolls up.
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
async fn fresh_batch_all_accepted() {
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(store));
    let body = serde_json::json!([
        record_json(&tenant_a(), &hex64(0x01)),
        record_json(&tenant_a(), &hex64(0x02)),
    ]);
    let resp = app
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let v = body_json(resp).await;
    assert_eq!(v["accepted"], serde_json::json!(2));
    assert_eq!(v["deduped"], serde_json::json!(0));
    assert_eq!(v["total"], serde_json::json!(2));
}

#[tokio::test]
async fn repush_same_idem_key_is_deduped() {
    // Push a record, then re-push the SAME (tenant, idem_key): the second
    // push must be a no-op (deduped), never a double-count. The store is
    // shared across both requests (clone shares the Arc).
    let store: Arc<dyn UsageStagingStore> = Arc::new(FakeStore::new());
    let idem = hex64(0x07);

    let app1 = router(state_with(Arc::clone(&store)));
    let resp1 = app1
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json(&tenant_a(), &idem)]),
        ))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(resp1).await["accepted"], serde_json::json!(1));

    let app2 = router(state_with(Arc::clone(&store)));
    let resp2 = app2
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json(&tenant_a(), &idem)]),
        ))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::ACCEPTED);
    let v = body_json(resp2).await;
    assert_eq!(
        v["accepted"],
        serde_json::json!(0),
        "re-push must not insert again"
    );
    assert_eq!(v["deduped"], serde_json::json!(1), "re-push must dedup");
}

#[tokio::test]
async fn intra_batch_duplicate_idem_key_deduped() {
    // The SAME idem_key twice WITHIN one batch: first inserts, second
    // dedups — the tally is accepted=1, deduped=1.
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(store));
    let idem = hex64(0x09);
    let body = serde_json::json!([
        record_json(&tenant_a(), &idem),
        record_json(&tenant_a(), &idem),
    ]);
    let resp = app
        .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let v = body_json(resp).await;
    assert_eq!(v["accepted"], serde_json::json!(1));
    assert_eq!(v["deduped"], serde_json::json!(1));
}

#[test]
fn idem_key_is_canonicalized_lowercase() {
    // The SAME BLAKE3 key spelled upper- vs lower-case must canonicalize to
    // ONE coordinate, else the `(tenant_id, idem_key)` dedup is bypassable.
    let lower = hex64(0xab); // "abab…" (32×"ab")
    let upper = lower.to_ascii_uppercase(); // "ABAB…"
    assert_ne!(lower, upper, "fixture must actually differ in case");

    let wire_lower: UsageRecordWire =
        serde_json::from_value(record_json(&tenant_a(), &lower)).unwrap();
    let wire_upper: UsageRecordWire =
        serde_json::from_value(record_json(&tenant_a(), &upper)).unwrap();

    let rec_lower = validate_record(wire_lower).unwrap();
    let rec_upper = validate_record(wire_upper).unwrap();

    // Both collapse to the same canonical (lowercase) idem_key → one row.
    assert_eq!(rec_lower.idem_key, lower);
    assert_eq!(rec_upper.idem_key, lower);
    assert_eq!(rec_lower.idem_key, rec_upper.idem_key);
}

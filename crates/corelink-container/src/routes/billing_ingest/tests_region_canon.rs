//! `region` is canonicalized by ASCII case-folding ONLY, and anything that is
//! not three ASCII letters fails closed as `RecordError::BadRegion`.
//!
//! Both halves are load-bearing. Case-folding is a regression guard: a box
//! misconfigured with `BILLING_REGION="IAD"` emits the same colo as `"iad"`,
//! and rejecting it would have made the runner retain-and-retry that batch
//! forever. Fail-closed on everything else is what keeps the folding from
//! widening into "lowercase it and hope" — a region is a colo code, not free
//! text, and it lands in a `usage_event_staging` column the aggregator groups
//! by.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use tower::ServiceExt;

use super::tests_support::{
    hex64, ingest_request, record_json, state_with, tenant_a, FakeStore, TEST_AUTH_KEY,
};
use super::*;

#[tokio::test]
async fn uppercase_region_is_canonicalized_and_accepted() {
    // Regression for the live `bad_region` flood: a box misconfigured with
    // BILLING_REGION="IAD" (uppercase) emits the SAME colo as "iad". It must
    // be canonicalized + ACCEPTED (202), staged as "iad" — never a 400 that
    // makes the runner retain-and-retry the batch forever.
    let store = Arc::new(FakeStore::new());
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let mut rec = record_json(&tenant_a(), &hex64(0x14));
    rec["region"] = serde_json::json!("IAD");
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([rec]),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(
        store.seen.lock().unwrap().len(),
        1,
        "the upper-cased colo must be staged, not dropped"
    );
}

#[test]
fn region_uppercase_canonicalizes_to_lowercase() {
    let wire: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x14));
        r["region"] = serde_json::json!("IaD");
        r
    })
    .unwrap();
    assert_eq!(validate_record(wire).unwrap().region, "iad");
}

#[test]
fn region_with_non_letters_is_bad_region_even_after_lowercasing() {
    for bad in ["i2d", "us", "iada", "i-d"] {
        let wire: UsageRecordWire = serde_json::from_value({
            let mut r = record_json(&tenant_a(), &hex64(0x14));
            r["region"] = serde_json::json!(bad);
            r
        })
        .unwrap();
        assert_eq!(
            validate_record(wire),
            Err(RecordError::BadRegion),
            "region {bad:?} must fail-closed"
        );
    }
}

#[test]
fn validate_record_reasons_pinned() {
    let bad_region: UsageRecordWire = serde_json::from_value({
        let mut r = record_json(&tenant_a(), &hex64(0x21));
        r["region"] = serde_json::json!("us");
        r
    })
    .unwrap();
    assert_eq!(validate_record(bad_region), Err(RecordError::BadRegion));
}

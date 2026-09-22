//! The HTTP contract reports every input in order, while a persistence fault
//! remains deliberately bodyless so callers cannot mistake partial work for an
//! acknowledged batch.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{self, Request};
use tower::ServiceExt;

use super::tests_support::{
    body_json, hex64, ingest_request, record_json, state_with, tenant_a, TEST_AUTH_KEY,
};
use super::*;

#[derive(Debug)]
struct ScriptedStore {
    results: Mutex<VecDeque<Result<StageOutcome, String>>>,
    calls: AtomicUsize,
}

impl ScriptedStore {
    fn new(results: impl IntoIterator<Item = Result<StageOutcome, String>>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl UsageStagingStore for ScriptedStore {
    async fn stage(&self, _: &StagedUsageRecord) -> Result<StageOutcome, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("fixture has one outcome per staged record")
    }
}

#[tokio::test]
async fn conflict_returns_409_with_ordered_per_input_outcomes() {
    let app = router(state_with(Arc::new(ScriptedStore::new([
        Ok(StageOutcome::Inserted),
        Ok(StageOutcome::Conflict(StageConflictReason::PayloadMismatch)),
        Ok(StageOutcome::Deduped),
    ]))));
    let a = hex64(0x31);
    let b = hex64(0x32);
    let c = hex64(0x33);
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([
                record_json(&tenant_a(), &a),
                record_json(&tenant_a(), &b),
                record_json(&tenant_a(), &c),
            ]),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body = body_json(resp).await;
    assert_eq!(body["accepted"], 1);
    assert_eq!(body["deduped"], 1);
    assert_eq!(
        body["outcomes"],
        serde_json::json!([
            {"index": 0, "idem_key": a, "outcome": "accepted"},
            {"index": 1, "idem_key": b, "outcome": "conflict", "reason": "payload_mismatch"},
            {"index": 2, "idem_key": c, "outcome": "deduped"},
        ])
    );
}

#[tokio::test]
async fn rejected_only_batch_returns_422_and_keeps_canonical_idem_key() {
    let idem = hex64(0x41).to_ascii_uppercase();
    let app = router(state_with(Arc::new(ScriptedStore::new([]))));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([record_json("not-a-uuid", &idem)]),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["outcomes"],
        serde_json::json!([
            {"index": 0, "idem_key": idem.to_ascii_lowercase(), "outcome": "rejected", "reason": "bad_tenant_id"},
        ])
    );
}

#[tokio::test]
async fn persistence_uncertainty_is_503_without_partial_acknowledgement() {
    let store = Arc::new(ScriptedStore::new([
        Ok(StageOutcome::Inserted),
        Err("d1 unavailable".to_owned()),
    ]));
    let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
    let resp = app
        .oneshot(ingest_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!([
                record_json(&tenant_a(), &hex64(0x51)),
                record_json(&tenant_a(), &hex64(0x52)),
            ]),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(resp).await, serde_json::Value::Null);
    assert_eq!(store.calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn auth_gate_precedes_body_parse() {
    let app = router(state_with(Arc::new(ScriptedStore::new([]))));
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/internal/v1/billing/usage")
        .header("content-type", "application/json")
        .body(Body::from("{definitely-not-json"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(resp).await["error"], "unauthorized");
}

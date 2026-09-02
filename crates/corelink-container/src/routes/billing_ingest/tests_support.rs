//! Shared fixtures for the `billing_ingest` test modules.
//!
//! The in-memory staging store and the request/JSON helpers live here rather
//! than inside one of the property files, because six of the eight need them
//! and importing from a sibling would make that sibling look load-bearing for
//! the others when it is only a neighbour.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::collections::HashSet;
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{self, Request};

use super::*;

pub(super) const TEST_AUTH_KEY: &str = "billing-ingest-test-key-32-chars!!!!";

/// In-memory staging store: dedups by `(tenant_id, idem_key)` exactly
/// like the D1 PRIMARY KEY. `backend_err` forces a 503 path.
#[derive(Debug, Default)]
pub(super) struct FakeStore {
    pub(super) seen: Mutex<HashSet<(Uuid, String)>>,
    backend_err: Option<String>,
}

impl FakeStore {
    pub(super) fn new() -> Self {
        Self::default()
    }
    pub(super) fn failing(err: &str) -> Self {
        Self {
            seen: Mutex::new(HashSet::new()),
            backend_err: Some(err.to_owned()),
        }
    }
}

#[async_trait]
impl UsageStagingStore for FakeStore {
    async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String> {
        if let Some(e) = &self.backend_err {
            return Err(e.clone());
        }
        let mut g = self.seen.lock().unwrap();
        let key = (record.tenant_id, record.idem_key.clone());
        if g.insert(key) {
            Ok(StageOutcome::Inserted)
        } else {
            Ok(StageOutcome::Deduped)
        }
    }
}

pub(super) fn state_with(store: Arc<dyn UsageStagingStore>) -> BillingIngestRouteState {
    BillingIngestRouteState::new(Arc::from(TEST_AUTH_KEY), store)
}

pub(super) fn record_json(tenant: &str, idem: &str) -> serde_json::Value {
    serde_json::json!({
        "tenant_id": tenant,
        "event_kind": "runner_slot_seconds",
        "qty": 7200u64,
        "billing_period": "2026-06",
        "region": "iad",
        "source": "corelink/runner/iad",
        "time_ms": 1_718_000_000_000u64,
        "idem_key": idem,
    })
}

pub(super) fn tenant_a() -> String {
    "11111111-1111-4111-8111-111111111111".to_owned()
}

pub(super) fn hex64(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

pub(super) fn ingest_request(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(http::Method::POST)
        .uri("/internal/v1/billing/usage")
        .header("content-type", "application/json");
    if let Some(a) = auth {
        b = b.header("x-corelink-internal-auth", a);
    }
    b.body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

pub(super) async fn body_json(resp: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 16_384)
        .await
        .unwrap();
    if bytes.is_empty() {
        return serde_json::Value::Null;
    }
    serde_json::from_slice(&bytes).unwrap()
}

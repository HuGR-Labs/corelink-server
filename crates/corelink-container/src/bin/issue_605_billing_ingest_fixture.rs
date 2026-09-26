//! Credentialless loopback fixture for the runners #605 integration pack.
//!
//! This process mounts the production Axum billing-ingest router with a
//! deterministic in-memory store. The separate `/__fixture/*` controls only
//! inject one persistence fault; they are never part of the production router.
//! The store survives runner-process restarts for the lifetime of this fixture.

use std::{
    collections::HashMap,
    io::Write as _,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use axum::{http::StatusCode, routing::post, Json, Router};
use corelink_server::routes::billing_ingest::{
    router as billing_ingest_router, BillingIngestRouteState, StageConflictReason, StageOutcome,
    StagedUsageRecord, UsageStagingStore,
};

const TEST_AUTH_KEY: &str = "issue-605-integration-test-key-not-a-provider-secret";

#[derive(Debug, Default)]
struct FixtureStore {
    records: Mutex<HashMap<(String, String), StagedUsageRecord>>,
    fail_next: AtomicBool,
}

#[async_trait]
impl UsageStagingStore for FixtureStore {
    async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String> {
        if self.fail_next.swap(false, Ordering::AcqRel) {
            return Err("issue-605 injected persistence failure".to_owned());
        }
        let key = (record.tenant_id.to_string(), record.idem_key.clone());
        let mut records = self
            .records
            .lock()
            .map_err(|_| "issue-605 fixture store lock poisoned".to_owned())?;
        match records.get(&key) {
            None => {
                records.insert(key, record.clone());
                Ok(StageOutcome::Inserted)
            }
            Some(existing) if existing == record => Ok(StageOutcome::Deduped),
            Some(_) => Ok(StageOutcome::Conflict(StageConflictReason::PayloadMismatch)),
        }
    }
}

async fn fail_next_persist(
    axum::extract::State(store): axum::extract::State<Arc<FixtureStore>>,
) -> StatusCode {
    store.fail_next.store(true, Ordering::Release);
    StatusCode::NO_CONTENT
}

async fn record_count(
    axum::extract::State(store): axum::extract::State<Arc<FixtureStore>>,
) -> Result<Json<usize>, StatusCode> {
    store
        .records
        .lock()
        .map(|records| Json(records.len()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(FixtureStore::default());
    let state = BillingIngestRouteState::new(Arc::from(TEST_AUTH_KEY), store.clone());
    let fixture_controls = Router::new()
        .route("/__fixture/fail-next-persist", post(fail_next_persist))
        .route("/__fixture/record-count", axum::routing::get(record_count))
        .with_state(store);
    let app = billing_ingest_router(state).merge(fixture_controls);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address: SocketAddr = listener.local_addr()?;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    writeln!(stdout, "http://{address}")?;
    stdout.flush()?;
    axum::serve(listener, app).await?;
    Ok(())
}

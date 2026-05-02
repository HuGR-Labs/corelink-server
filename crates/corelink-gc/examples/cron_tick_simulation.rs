//! Simulate one cron tick fired against the in-memory scheduler. Use
//! this as the canonical "happy path" smoke for the scheduler →
//! worker chain.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "example: prints + ergonomic unwraps for clarity"
)]

use std::sync::Arc;

use corelink_gc::{
    GcRegion, GcScheduler, InMemoryDegradeProbe, InMemoryGcAuditSink, InMemoryGcMetrics,
    InMemoryGcRunStore, InMemoryGcScheduler, InMemoryGcWorker, ScheduleConfig,
};
use uuid::Uuid;

fn main() {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let degrade = Arc::new(InMemoryDegradeProbe::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let worker = InMemoryGcWorker::new(
        Arc::clone(&runs),
        Arc::clone(&degrade),
        Arc::clone(&audit),
        Arc::clone(&metrics),
    );
    let cfg = ScheduleConfig::with_defaults(GcRegion::Sam).unwrap();
    let scheduler = InMemoryGcScheduler::new(
        cfg,
        worker,
        Arc::clone(&runs),
        Arc::clone(&degrade),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        0xa11ce,
    );

    let tenants: Vec<Uuid> = (0u32..3).map(|i| Uuid::from_u128(u128::from(i) + 1)).collect();
    let outcome = scheduler.cron_tick(1_700_000_000_000, &tenants).unwrap();
    println!(
        "cron tick result: region={:?} admitted={} jittered_start_ms={} paused={}",
        outcome.region, outcome.admitted_tenant_count, outcome.jittered_start_ms, outcome.paused,
    );
    println!("audit records: {}", audit.snapshot().len());
}

//! Drive the degrade-mode `gc-pause` flow: probe flips ON, scheduler
//! refuses new admissions, aborted run is captured in the audit sink.

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
    GcEventType, GcRegion, GcScheduler, InMemoryDegradeProbe, InMemoryGcAuditSink,
    InMemoryGcMetrics, InMemoryGcRunStore, InMemoryGcScheduler, InMemoryGcWorker, ScheduleConfig,
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
    let cfg = ScheduleConfig::with_defaults(GcRegion::Iad).unwrap();
    let scheduler = InMemoryGcScheduler::new(
        cfg,
        worker,
        Arc::clone(&runs),
        Arc::clone(&degrade),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        0,
    );

    // Operator enables gc-pause via the (forward) admin plane.
    degrade.pause("pat_op_dev", 1_700_000_000_000, "incident_demo");

    let tenants: Vec<Uuid> = vec![Uuid::from_u128(1)];
    let outcome = scheduler.cron_tick(1_700_000_000_000, &tenants).unwrap();
    println!(
        "paused={} admitted_tenant_count={}",
        outcome.paused, outcome.admitted_tenant_count
    );
    println!(
        "run_started events: {}",
        audit.snapshot_of(GcEventType::RunStarted).len()
    );
    // Property: admitted_tenant_count == 0 when paused.
    assert!(outcome.paused);
    assert_eq!(outcome.admitted_tenant_count, 0);
}

//! Demonstrate the idempotent crash-resume contract:
//!
//! 1. Run starts, transitions to Mark, then "crashes" (terminal
//!    `Crashed`).
//! 2. The crashed row is preserved (forensic trail) but the partial
//!    UNIQUE on `WHERE status='running'` releases.
//! 3. A new run for the same `(tenant, region)` is accepted and
//!    completes successfully.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "example: prints + ergonomic unwraps for clarity"
)]

use corelink_gc::{
    CheckpointDeltas, FailureContext, GcPhase, GcRegion, GcRunStore, GcStatus, InMemoryGcRunStore,
    RunId,
};
use uuid::Uuid;

fn main() {
    let runs = InMemoryGcRunStore::new();
    let tenant = Uuid::from_u128(0xc0ffee);
    let region = GcRegion::Sam;
    let rid_first = RunId(Uuid::from_u128(1));
    let rid_resume = RunId(Uuid::from_u128(2));

    // First run — start, transition to Mark, partial work, crash.
    runs.insert_pending(rid_first, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid_first, tenant, 200).unwrap();
    runs.transition_phase(rid_first, tenant, GcPhase::Mark, 300)
        .unwrap();
    runs.checkpoint(
        rid_first,
        tenant,
        400,
        CheckpointDeltas {
            blobs_marked_delta: 1234,
            bytes_reclaimed_delta: 5_678_900,
            ..Default::default()
        },
    )
    .unwrap();
    runs.finalize(
        rid_first,
        tenant,
        GcStatus::Crashed,
        500,
        Some(FailureContext {
            failed_phase: GcPhase::Mark,
            failed_reason: "worker_panic".into(),
        }),
    )
    .unwrap();
    let crashed = runs.lookup(rid_first, tenant).unwrap().unwrap();
    println!(
        "crashed row preserved: status={:?} blobs_marked={} bytes_reclaimed={}",
        crashed.status, crashed.blobs_marked_count, crashed.bytes_reclaimed
    );

    // New run takes the per-(tenant, region) lock.
    runs.insert_pending(rid_resume, tenant, region, 600, "cron".into())
        .unwrap();
    runs.acquire_running(rid_resume, tenant, 700).unwrap();
    let row = runs
        .current_running(tenant, region)
        .unwrap()
        .expect("running row");
    println!("resumed row run_id={}", row.run_id);
    println!("snapshot total rows: {}", runs.snapshot().len());
}

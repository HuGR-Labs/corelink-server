//! Example: Manual abort of an active rollout (WI-S13-005).
//!
//! Demonstrates admin aborting an in-progress rollout via dual-approval
//! gate (WI-S13-002). The abort emits audit `corelink.admin.rollout.abort`
//! and clears the active session so a new rollout can start.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "examples use println! for demonstration"
)]

use std::sync::Arc;

use corelink_rollout_controller::{
    budget::InMemoryBudgetTracker,
    controller::{fresh_actor, signed_artifact, InMemoryRolloutController},
    types::RolloutStatus,
    InMemoryRolloutAuditSink, RolloutController,
};

fn main() {
    let audit = Arc::new(InMemoryRolloutAuditSink::new());
    let budget = Arc::new(InMemoryBudgetTracker::new());
    let ctrl = InMemoryRolloutController::new(Arc::clone(&audit), Arc::clone(&budget));

    let actor = fresh_actor(0);
    let artifact = signed_artifact();

    // Start rollout
    let handle = ctrl.start(&artifact, &actor).expect("start must succeed");
    println!("Rollout started: handle={}", handle.handle_id);

    // Admin decides to abort (dual-approval verified by WI-S13-002 in
    // production; here we call abort directly for illustration)
    let aborted = ctrl.abort(&handle, &actor).expect("abort must succeed");
    assert_eq!(aborted.status, RolloutStatus::ManuallyAborted);
    println!("Rollout aborted: status={:?}", aborted.status);

    // After abort, a new rollout can start (active session cleared)
    let handle2 = ctrl.start(&artifact, &actor).expect("second start must succeed");
    println!("New rollout started after abort: handle={}", handle2.handle_id);

    let records = audit.records().expect("audit records");
    println!("Audit events:");
    for r in &records {
        println!("  - {:?}", r.event_type.as_cloud_event_type());
    }
}

//! Example: Start a progressive rollout (WI-S13-005).
//!
//! Demonstrates the happy-path: Cosign-signed artifact → rollout
//! starts at Stage1Pct → probe advances through all 4 stages.

#![allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "examples use println! for demonstration"
)]

use std::sync::Arc;

use corelink_rollout_controller::{
    budget::InMemoryBudgetTracker,
    controller::{fresh_actor, passing_metrics, signed_artifact, InMemoryRolloutController},
    types::{NextAction, RolloutStage},
    InMemoryRolloutAuditSink, RolloutController,
};

fn main() {
    let audit = Arc::new(InMemoryRolloutAuditSink::new());
    let budget = Arc::new(InMemoryBudgetTracker::new());
    let ctrl = InMemoryRolloutController::new(Arc::clone(&audit), Arc::clone(&budget));

    let actor = fresh_actor(0);
    let artifact = signed_artifact();

    let handle = ctrl.start(&artifact, &actor).expect("start must succeed");
    println!(
        "Rollout started: handle={} stage={:?}",
        handle.handle_id, handle.current_stage
    );

    // Simulate probing through all 4 stages with passing metrics
    let stages = [
        RolloutStage::Stage1Pct,
        RolloutStage::Stage10Pct,
        RolloutStage::Stage50Pct,
        RolloutStage::Stage100Pct,
    ];

    for stage in stages {
        let metrics = passing_metrics(stage);
        match ctrl
            .probe_and_advance(&handle, metrics)
            .expect("probe must succeed")
            .next_action
        {
            NextAction::Advance(next) => {
                println!("Advanced from {:?} → {:?}", stage, next);
            }
            NextAction::Complete => {
                println!("Rollout completed at {:?}", stage);
                break;
            }
            NextAction::Hold(reason) => {
                println!("Holding at {:?}: {}", stage, reason);
            }
            NextAction::AutoRollback(trigger) => {
                println!("Auto-rollback triggered at {:?}: {:?}", stage, trigger);
                break;
            }
            _ => {
                println!("Unexpected action at {:?}", stage);
                break;
            }
        }
    }

    let records = audit.records().expect("audit records");
    println!("Audit events emitted: {}", records.len());
    for r in &records {
        println!("  - {:?}", r.event_type.as_cloud_event_type());
    }
}

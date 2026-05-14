//! Example: Monthly rollback budget exceeded handling (WI-S13-005).
//!
//! Demonstrates the budget cap safeguard: after 30% of the monthly
//! error budget is consumed by auto-rollbacks, further rollout starts
//! return `BudgetExceeded` (freeze + SEV-2 alert path).
//!
//! Manual override requires Architect + Security lead approval + ADR waiver.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples use println!/eprintln! for demonstration"
)]

use std::sync::Arc;

use corelink_rollout_controller::{
    budget::{BudgetRecord, InMemoryBudgetTracker},
    controller::{fresh_actor, signed_artifact, InMemoryRolloutController},
    error::RolloutError,
    BudgetTracker, InMemoryRolloutAuditSink, RolloutController,
};
use uuid::Uuid;

fn main() {
    let audit = Arc::new(InMemoryRolloutAuditSink::new());
    let budget = Arc::new(InMemoryBudgetTracker::new());
    let ctrl = InMemoryRolloutController::new(Arc::clone(&audit), Arc::clone(&budget));

    let now_ms = 1_000_000u64;
    ctrl.set_now_ms(now_ms).unwrap();
    budget.set_now_ms(now_ms).unwrap();

    // Simulate 3 previous auto-rollbacks, each consuming 1100 bps
    // 3 × 1100 = 3300 bps > 3000 bps (30% cap = 3000/10000 bps)
    for i in 0..3 {
        let bps = 1100u32;
        budget
            .record_rollback(BudgetRecord {
                handle_id: Uuid::now_v7(),
                rollback_started_ms: now_ms - 10_000,
                rollback_completed_ms: now_ms,
                error_count_consumed: u64::from(bps),
                monthly_error_budget_target: 10_000,
                budget_consumed_bps: bps,
            })
            .unwrap();
        println!("Recorded rollback #{}: {}bps", i + 1, bps);
    }

    let consumed = budget.consumed_ratio().unwrap();
    println!(
        "Budget consumed ratio: {consumed:.4} (cap threshold: 1.0 = 30%)"
    );
    assert!(consumed > 1.0, "budget should be exceeded");

    // Attempt a 4th rollout start → BudgetExceeded
    let actor = fresh_actor(0);
    match ctrl.start(&signed_artifact(), &actor) {
        Err(RolloutError::BudgetExceeded { consumed: c }) => {
            println!(
                "BudgetExceeded: consumed={c:.4} — deploys frozen. \
                 Requires Architect + Security lead override + ADR waiver."
            );
        }
        Ok(_) => {
            eprintln!("ERROR: start should not succeed when budget exceeded");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("ERROR: unexpected error: {e:?}");
            std::process::exit(1);
        }
    }

    println!("To unfreeze: wait for rolling 30d window to expire OR obtain manual override.");
}

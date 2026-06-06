//! Example: PAT signing key rotation lifecycle (24h overlap; S-03).
//!
//! Demonstrates generate → promote → retire with PAT-ROLL-FORWARD-001
//! auto-rollback triggered by injected 5% downstream error rate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "example code: panics + println are intentional for demonstration"
)]

use std::sync::Arc;

use corelink_ops::rotation::worker::{
    InMemoryRotationStateMachine, ProbeContext, RollbackConfig, RollbackDriver, RollbackOutcome,
    RotationMetrics, RotationOrchestrator, RotationOutcome,
};
use corelink_rotation_adapters::{AssetClass, KeyHandle, KeyState, PatSigningRotationAdapter};

fn main() {
    let adapter = Arc::new(PatSigningRotationAdapter::new("us-east".to_string()));
    let sm = Arc::new(InMemoryRotationStateMachine::new());
    let metrics = Arc::new(RotationMetrics::new());
    let orch = RotationOrchestrator::new(
        Arc::clone(&adapter),
        Arc::clone(&sm),
        Arc::clone(&metrics),
        "us-east".to_string(),
    );

    let t0 = 1_000_000_u64;

    // First rotation: generate + promote (key1 Active).
    let gen1 = orch.generate(t0).expect("generate 1 failed");
    let key1_pending = match gen1 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    let prom1 = orch
        .promote(&key1_pending, t0 + 1)
        .expect("promote 1 failed");
    let key1_active = match prom1 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    println!(
        "[PAT] key1 promoted: key_id={} state={}",
        key1_active.key_id, key1_active.state
    );
    println!(
        "[PAT] Overlap window: {}s (24h)",
        AssetClass::PatSigning.overlap_seconds()
    );

    // Second rotation: key1 → Overlap; key2 → Active.
    let t1 = t0 + AssetClass::PatSigning.overlap_seconds() * 1_000 + 100;
    let gen2 = orch.generate(t1).expect("generate 2 failed");
    let key2_pending = match gen2 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    let prom2 = orch
        .promote(&key2_pending, t1 + 1)
        .expect("promote 2 failed");
    let key2_active = match prom2 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    println!(
        "[PAT] key2 promoted: key_id={} state={}",
        key2_active.key_id, key2_active.state
    );

    // Synthesize key1 Overlap handle.
    let key1_overlap = KeyHandle {
        state: KeyState::Overlap,
        overlap_until_ms: Some(t1 + 1 + AssetClass::PatSigning.overlap_seconds() * 1_000),
        ..key1_active
    };

    // Inject 5% downstream error rate (PAT verify failures simulation).
    adapter.set_error_rate(0.05);
    println!("[PAT] Injected 5% downstream error rate (simulating PAT verify failures)");

    // Run rollback driver with 5-probe sustain window.
    let rb_cfg = RollbackConfig {
        error_threshold: 0.01,
        sustain_window_probes: 5,
    };
    let mut driver = RollbackDriver::new(rb_cfg);

    for probe in 1..=5_u32 {
        let result = driver.probe(ProbeContext {
            adapter: adapter.as_ref(),
            new_key: &key2_active,
            previous_key: &key1_overlap,
            state_machine: sm.as_ref(),
            metrics: metrics.as_ref(),
            region: "us-east",
            now_ms: t1 + u64::from(probe) * 60_000,
        });
        match result {
            Ok(RollbackOutcome::Accumulating { consecutive }) => {
                println!(
                    "[PAT] probe {probe}: Accumulating ({consecutive} consecutive above-threshold)"
                );
            }
            Err(e) => {
                println!("[PAT] probe {probe}: Rollback triggered — {e}");
                break;
            }
            Ok(other) => println!("[PAT] probe {probe}: {other:?}"),
        }
    }

    println!(
        "[PAT] rolled_back counter: {}",
        metrics.total(AssetClass::PatSigning, "rolled_back")
    );
    println!("[PAT] State machine records: {}", sm.len());
}

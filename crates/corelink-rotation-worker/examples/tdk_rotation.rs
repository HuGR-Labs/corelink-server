//! Example: TDK rotation lifecycle (7d overlap; S-01 envelope re-wrap).
//!
//! Demonstrates the full generate → promote → rekey_downstream →
//! retire → destroy lifecycle for TDK keys.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "example code: panics + println are intentional for demonstration"
)]

use std::sync::Arc;

use corelink_rotation_adapters::{AssetClass, TdkRotationAdapter};
use corelink_rotation_worker::{
    InMemoryRotationStateMachine, RotationMetrics, RotationOrchestrator, RotationOutcome,
};

fn main() {
    let adapter = Arc::new(TdkRotationAdapter::new("us-east".to_string()));
    let sm = Arc::new(InMemoryRotationStateMachine::new());
    let metrics = Arc::new(RotationMetrics::new());
    let orch = RotationOrchestrator::new(
        Arc::clone(&adapter),
        Arc::clone(&sm),
        Arc::clone(&metrics),
        "us-east".to_string(),
    );

    let t0 = 1_000_000_u64;

    // Step 1: Generate new TDK key.
    let gen = orch.generate(t0).expect("generate failed");
    let key_pending = match gen {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected generate outcome: {other:?}"),
    };
    println!("[TDK] Generated key_id={} state={}", key_pending.key_id, key_pending.state);

    // Step 2: Promote (previous Active → Overlap; new Pending → Active).
    let prom = orch.promote(&key_pending, t0 + 1).expect("promote failed");
    let key_active = match prom {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected promote outcome: {other:?}"),
    };
    println!("[TDK] Promoted key_id={} state={}", key_active.key_id, key_active.state);
    println!(
        "[TDK] Overlap window: {}s ({} days; canonical per key_management.md §3.2.1)",
        AssetClass::Tdk.overlap_seconds(),
        AssetClass::Tdk.overlap_seconds() / 86_400
    );

    // Step 3: Re-key downstream envelopes (reports progress).
    orch.rekey_downstream(&key_active, t0 + 2).expect("rekey_downstream failed");
    println!("[TDK] Re-key downstream progress: {:.1}%", metrics.rekey_progress(AssetClass::Tdk) * 100.0);

    println!("[TDK] Second rotation to create an Overlap key...");
    let t1 = t0 + AssetClass::Tdk.overlap_seconds() * 1_000 + 100;
    let gen2 = orch.generate(t1).expect("generate 2 failed");
    let key2_pending = match gen2 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected generate 2 outcome: {other:?}"),
    };
    let prom2 = orch.promote(&key2_pending, t1 + 1).expect("promote 2 failed");
    let key2_active = match prom2 {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected promote 2 outcome: {other:?}"),
    };

    // Synthesize overlap handle for key1.
    let key1_overlap = corelink_rotation_adapters::KeyHandle {
        state: corelink_rotation_adapters::KeyState::Overlap,
        overlap_until_ms: key2_active.promoted_at_ms.map(|ms| ms + AssetClass::Tdk.overlap_seconds() * 1_000),
        ..key_active.clone()
    };

    // Step 4: Retire the old key after overlap window.
    let t2 = t1 + AssetClass::Tdk.overlap_seconds() * 1_000 + 100;
    let retired = orch.retire(&key1_overlap, t2).expect("retire failed");
    let retired_key = match retired {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected retire outcome: {other:?}"),
    };
    println!("[TDK] Retired key_id={} state={}", retired_key.key_id, retired_key.state);

    // Step 5: Destroy after 90d grace.
    let t3 = t2 + 90 * 24 * 3_600 * 1_000;
    let destroyed = orch.destroy(&retired_key, t3).expect("destroy failed");
    let destroyed_key = match destroyed {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("unexpected destroy outcome: {other:?}"),
    };
    println!("[TDK] Destroyed key_id={} state={}", destroyed_key.key_id, destroyed_key.state);
    println!("[TDK] Rotation total ok: {}", metrics.total(AssetClass::Tdk, "ok"));
    println!("[TDK] State machine records: {}", sm.len());

    let _ = key2_active; // used in overlap setup above
}

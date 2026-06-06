//! Example: BYOK customer CMK rotation stub (7d overlap; S-14 forward).
//!
//! Demonstrates the customer-trigger rotation flow: generate → promote
//! → 7d overlap (customer validates new key access) → retire.
//! Early retire path: customer revokes CMK mid-overlap (INV-BYOK-CRYPTO-
//! SOVEREIGNTY preserved — intentional).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "example code: panics + println are intentional for demonstration"
)]

use std::sync::Arc;

use corelink_ops::rotation::worker::{
    is_valid_read_state, is_valid_write_state, InMemoryRotationStateMachine, RotationMetrics,
    RotationOrchestrator, RotationOutcome,
};
use corelink_rotation_adapters::{AssetClass, ByokRotationAdapter, KeyHandle, KeyState};

fn main() {
    let tenant_id = "tenant-enterprise-001";
    println!("[BYOK] Customer-trigger rotation for tenant={tenant_id}");

    let adapter = Arc::new(ByokRotationAdapter::new(tenant_id.to_string()));
    let sm = Arc::new(InMemoryRotationStateMachine::new());
    let metrics = Arc::new(RotationMetrics::new());
    let orch = RotationOrchestrator::new(
        Arc::clone(&adapter),
        Arc::clone(&sm),
        Arc::clone(&metrics),
        "us-east".to_string(),
    );

    let t0 = 1_000_000_u64;

    // Customer triggers POST /v1/customer/byok/rotate → generate.
    let gen = orch.generate(t0).expect("generate failed");
    let key_pending = match gen {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    println!(
        "[BYOK] Generated key_id={} state={}",
        key_pending.key_id, key_pending.state
    );

    // First rotation: key1 Active.
    let prom = orch.promote(&key_pending, t0 + 1).expect("promote failed");
    let key1_active = match prom {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    println!(
        "[BYOK] Promoted key1_id={} state={}",
        key1_active.key_id, key1_active.state
    );
    println!(
        "[BYOK] Overlap window: {}s ({} days; customer notification window)",
        AssetClass::Byok.overlap_seconds(),
        AssetClass::Byok.overlap_seconds() / 86_400
    );

    // Second rotation (after 7d): key1 → Overlap; key2 → Active.
    let t1 = t0 + AssetClass::Byok.overlap_seconds() * 1_000 + 100;
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

    // Synthesize key1 Overlap handle.
    let key1_overlap = KeyHandle {
        state: KeyState::Overlap,
        overlap_until_ms: Some(t1 + 1 + AssetClass::Byok.overlap_seconds() * 1_000),
        ..key1_active
    };

    // During 7d overlap: both keys accepted for reads.
    assert!(is_valid_read_state(key1_overlap.state));
    assert!(is_valid_read_state(key2_active.state));
    assert!(!is_valid_write_state(key1_overlap.state)); // old: reads only
    assert!(is_valid_write_state(key2_active.state)); // new: reads + writes
    println!("[BYOK] 7d overlap: key1 (reads only) + key2 (reads + writes) — INV-KEY-OVERLAP ✓");

    // Scenario: customer revokes old CMK mid-overlap → early retire.
    // INV-BYOK-CRYPTO-SOVEREIGNTY: intentional customer-triggered inaccessibility.
    let t_revoke = t1 + 3 * 24 * 3_600 * 1_000; // 3d into 7d overlap
    let retired = orch
        .retire(&key1_overlap, t_revoke)
        .expect("early retire failed");
    let retired_key = match retired {
        RotationOutcome::Ok { handle, .. } => handle,
        other => panic!("{other:?}"),
    };
    println!(
        "[BYOK] Customer revoke mid-overlap — early retire key1_id={} state={} (INV-BYOK-CRYPTO-SOVEREIGNTY preserved)",
        retired_key.key_id, retired_key.state
    );

    println!("[BYOK] State machine records: {}", sm.len());
    println!(
        "[BYOK] Overlap observations: {:?}",
        metrics.overlap_observations(AssetClass::Byok)
    );
    let _ = key2_active; // active key continues serving writes
}

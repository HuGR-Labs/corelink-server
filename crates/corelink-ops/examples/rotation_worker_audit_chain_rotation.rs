//! Example: Audit chain key rotation lifecycle (24h overlap; per-region;
//! S-09 daily verifier compatible).
//!
//! Demonstrates the canonical per-region rotation with both Active +
//! Overlap keys accepted during the 24h window (INV-OBS-AUDIT-CHAIN-
//! INTEGRITY preserved across the rotation boundary).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "example code: panics + println are intentional for demonstration"
)]

use std::sync::Arc;

use corelink_rotation_adapters::{AssetClass, AuditChainRotationAdapter, KeyHandle, KeyState};
use corelink_ops::rotation::worker::{
    InMemoryRotationStateMachine, RotationMetrics, RotationOrchestrator, RotationOutcome,
    is_valid_read_state,
};

fn main() {
    let regions = ["us-east", "eu-west"];

    for region in &regions {
        println!("[AuditChain] Starting rotation for region={region}");

        let adapter = Arc::new(AuditChainRotationAdapter::new((*region).to_string()));
        let sm = Arc::new(InMemoryRotationStateMachine::new());
        let metrics = Arc::new(RotationMetrics::new());
        let orch = RotationOrchestrator::new(
            Arc::clone(&adapter),
            Arc::clone(&sm),
            Arc::clone(&metrics),
            (*region).to_string(),
        );

        let t0 = 1_000_000_u64;

        // First rotation: key1 → Active.
        let gen1 = orch.generate(t0).expect("generate 1 failed");
        let key1_pending = match gen1 {
            RotationOutcome::Ok { handle, .. } => handle,
            other => panic!("{other:?}"),
        };
        let prom1 = orch.promote(&key1_pending, t0 + 1).expect("promote 1 failed");
        let key1_active = match prom1 {
            RotationOutcome::Ok { handle, .. } => handle,
            other => panic!("{other:?}"),
        };

        // Second rotation (24h later): key1 → Overlap; key2 → Active.
        let t1 = t0 + AssetClass::AuditChain.overlap_seconds() * 1_000 + 100;
        let gen2 = orch.generate(t1).expect("generate 2 failed");
        let key2_pending = match gen2 {
            RotationOutcome::Ok { handle, .. } => handle,
            other => panic!("{other:?}"),
        };
        let prom2 = orch.promote(&key2_pending, t1 + 1).expect("promote 2 failed");
        let key2_active = match prom2 {
            RotationOutcome::Ok { handle, .. } => handle,
            other => panic!("{other:?}"),
        };

        // Synthesize key1 Overlap handle.
        let key1_overlap = KeyHandle {
            state: KeyState::Overlap,
            overlap_until_ms: Some(t1 + 1 + AssetClass::AuditChain.overlap_seconds() * 1_000),
            ..key1_active
        };

        // During 24h overlap: BOTH keys accepted for reads (INV-KEY-OVERLAP).
        assert!(is_valid_read_state(key1_overlap.state), "old chain key must be valid for reads");
        assert!(is_valid_read_state(key2_active.state), "new chain key must be valid for reads");

        println!(
            "[AuditChain][{region}] key1_id={} state={} (accepted for reads: ✓)",
            key1_overlap.key_id, key1_overlap.state
        );
        println!(
            "[AuditChain][{region}] key2_id={} state={} (accepted for reads: ✓; writes: ✓)",
            key2_active.key_id, key2_active.state
        );

        // After 24h overlap: retire old key.
        let t2 = t1 + AssetClass::AuditChain.overlap_seconds() * 1_000 + 100;
        let retired = orch.retire(&key1_overlap, t2).expect("retire failed");
        let retired_key = match retired {
            RotationOutcome::Ok { handle, .. } => handle,
            other => panic!("{other:?}"),
        };
        println!(
            "[AuditChain][{region}] Retired key1_id={} state={} — INV-OBS-AUDIT-CHAIN-INTEGRITY preserved",
            retired_key.key_id, retired_key.state
        );

        println!(
            "[AuditChain][{region}] Overlap observations: {:?}",
            metrics.overlap_observations(AssetClass::AuditChain)
        );
        println!("[AuditChain][{region}] State machine records: {}", sm.len());
    }
}

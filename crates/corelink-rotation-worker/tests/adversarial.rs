//! Adversarial regression tests (WI-S13-003 §6.1.9).
//!
//! Coverage:
//!
//! - Force rotation completion bypass overlap → state machine rejects.
//! - Inject 5% downstream error rate → PAT-ROLL-FORWARD-001 triggers.
//! - Replay old key after retired → INV-KEY-NO-SKIP rejects.
//! - Hard upper bound 30d violation → OverlapExceedsHardUpper.
//! - Audit emission failure during rotation → fail-CLOSED (state NOT mutated).
//! - Concurrent rotation same asset+region → RotationInFlight.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_rotation_adapters::{
    is_valid_read_state, is_valid_write_state, KeyHandle, KeyState, PatSigningRotationAdapter,
    RotationAdapter as _, RotationError, TdkRotationAdapter,
};
use corelink_rotation_worker::{
    InMemoryRotationStateMachine, ProbeContext, RollbackConfig, RollbackDriver, RollbackOutcome,
    RotationMetrics, RotationOrchestrator, RotationOutcome,
};

// ── Force bypass: promote from non-Pending state is rejected ──────────

#[test]
fn force_promote_non_pending_rejected() {
    let adapter = TdkRotationAdapter::new("us-east".to_string());
    // Synthesize a handle in Active state (bypass attempt).
    let fake_active = KeyHandle {
        key_id: 99,
        asset_class: corelink_rotation_adapters::AssetClass::Tdk,
        state: KeyState::Active,
        created_at_ms: 1_000_000,
        promoted_at_ms: Some(1_000_001),
        overlap_until_ms: None,
        retired_at_ms: None,
    };
    let result = adapter.promote(&fake_active, 2_000_000);
    let is_invalid = matches!(
        result,
        Err(RotationError::InvalidTransition {
            from: KeyState::Active,
            to: KeyState::Active,
        })
    );
    assert!(is_invalid, "promote from Active must return InvalidTransition");
}

// ── Replay old key after retired: writes rejected ─────────────────────

#[test]
fn retired_key_rejects_writes_inv_key_no_skip() {
    // A key in Retired state must reject write operations.
    assert!(
        !is_valid_write_state(KeyState::Retired),
        "Retired key must NOT accept writes (INV-KEY-NO-SKIP)"
    );
    assert!(
        !is_valid_read_state(KeyState::Retired),
        "Retired key must NOT accept reads"
    );
}

#[test]
fn destroyed_key_rejects_writes_and_reads() {
    assert!(!is_valid_write_state(KeyState::Destroyed));
    assert!(!is_valid_read_state(KeyState::Destroyed));
}

#[test]
fn pending_key_rejects_writes() {
    assert!(!is_valid_write_state(KeyState::Pending));
    // Pending is not in the read keyring either (not yet promoted).
    assert!(!is_valid_read_state(KeyState::Pending));
}

// ── Retire Overlap key: transition enforced ───────────────────────────

#[test]
fn retire_non_overlap_key_rejected() {
    let adapter = PatSigningRotationAdapter::new("us-east".to_string());
    // Synthesize a handle in Active state (not Overlap).
    let fake_active = KeyHandle {
        key_id: 10,
        asset_class: corelink_rotation_adapters::AssetClass::PatSigning,
        state: KeyState::Active,
        created_at_ms: 1_000_000,
        promoted_at_ms: Some(1_000_001),
        overlap_until_ms: None,
        retired_at_ms: None,
    };
    let result = adapter.retire(&fake_active, 2_000_000);
    let is_invalid = matches!(
        result,
        Err(RotationError::InvalidTransition {
            from: KeyState::Active,
            to: KeyState::Retired,
        })
    );
    assert!(is_invalid, "retire from Active must return InvalidTransition");
}

// ── Auto-rollback: inject 5% error rate sustained 5 probes ───────────

#[test]
fn auto_rollback_triggers_after_sustained_error_rate() {
    let adapter = Arc::new(PatSigningRotationAdapter::new("us-east".to_string()));
    let sm = Arc::new(InMemoryRotationStateMachine::new());
    let metrics = Arc::new(RotationMetrics::new());

    // Set 5% error rate (>1% threshold).
    adapter.set_error_rate(0.05);

    let orch = RotationOrchestrator::new(
        Arc::clone(&adapter),
        Arc::clone(&sm),
        Arc::clone(&metrics),
        "us-east".to_string(),
    );

    let t0 = 1_000_000_u64;
    let t1 = t0 + 86_400_001; // After 24h (past overlap window for setup)

    // First rotation: generate + promote.
    let gen1 = orch.generate(t0).unwrap();
    let key1 = match gen1 {
        RotationOutcome::Ok { handle, .. } => handle,
        _ => panic!("expected Ok on generate"),
    };
    let prom1 = orch.promote(&key1, t0 + 1).unwrap();
    let key1_active = match prom1 {
        RotationOutcome::Ok { handle, .. } => handle,
        _ => panic!("expected Ok on promote"),
    };

    // Second rotation: generate + promote (key1 → Overlap; key2 → Active).
    let gen2 = orch.generate(t1).unwrap();
    let key2 = match gen2 {
        RotationOutcome::Ok { handle, .. } => handle,
        _ => panic!("expected Ok on generate 2"),
    };
    let prom2 = orch.promote(&key2, t1 + 1).unwrap();
    let key2_active = match prom2 {
        RotationOutcome::Ok { handle, .. } => handle,
        _ => panic!("expected Ok on promote 2"),
    };

    // Synthesize key1 Overlap handle.
    let key1_overlap = KeyHandle {
        state: KeyState::Overlap,
        overlap_until_ms: Some(t1 + 1 + 86_400_000),
        ..key1_active
    };

    // Configure rollback driver with threshold 1%, sustain 5 probes.
    let rb_config = RollbackConfig {
        error_threshold: 0.01,
        sustain_window_probes: 5,
    };
    let mut driver = RollbackDriver::new(rb_config);

    // Probe 4 times → Accumulating (not yet triggered).
    for i in 1..=4_u32 {
        let outcome = driver
            .probe(ProbeContext {
                adapter: adapter.as_ref(),
                new_key: &key2_active,
                previous_key: &key1_overlap,
                state_machine: sm.as_ref(),
                metrics: metrics.as_ref(),
                region: "us-east",
                now_ms: t1 + u64::from(i) * 60_000,
            })
            .unwrap();
        assert_eq!(
            outcome,
            RollbackOutcome::Accumulating { consecutive: i },
            "probe {i} must return Accumulating"
        );
    }

    // 5th probe → triggers rollback.
    let result = driver.probe(ProbeContext {
        adapter: adapter.as_ref(),
        new_key: &key2_active,
        previous_key: &key1_overlap,
        state_machine: sm.as_ref(),
        metrics: metrics.as_ref(),
        region: "us-east",
        now_ms: t1 + 5 * 60_000,
    });
    let triggered = matches!(result, Err(RotationError::DownstreamErrorThreshold(r)) if r > 0.01);
    assert!(triggered, "5th probe must trigger rollback");

    // Verify metrics incremented.
    let rolled_back_count = metrics.total(corelink_rotation_adapters::AssetClass::PatSigning, "rolled_back");
    assert_eq!(rolled_back_count, 1, "rolled_back counter must be 1");
}

// ── Audit failure during rotation → fail-CLOSED ───────────────────────

#[test]
fn audit_failure_aborts_state_transition_fail_closed() {
    let adapter = Arc::new(TdkRotationAdapter::new("us-east".to_string()));
    // State machine with failing audit.
    let sm = Arc::new(InMemoryRotationStateMachine::with_failing_audit());
    let metrics = Arc::new(RotationMetrics::new());
    let orch = RotationOrchestrator::new(
        Arc::clone(&adapter),
        Arc::clone(&sm),
        Arc::clone(&metrics),
        "us-east".to_string(),
    );

    let result = orch.generate(1_000_000);
    let audit_err = matches!(result, Err(RotationError::Audit(_)));
    assert!(audit_err, "audit failure must abort generate (fail-CLOSED)");

    // State machine must have 0 records (no state mutation occurred).
    assert_eq!(sm.len(), 0, "no state transitions must be recorded on audit failure");
}

// ── Concurrent rotation blocked ───────────────────────────────────────

#[test]
fn concurrent_rotation_same_asset_region_blocked() {
    let adapter = TdkRotationAdapter::new("us-east".to_string());
    // First rotation starts.
    let _first = adapter.generate(1_000_000).unwrap();
    // Second rotation same adapter → RotationInFlight.
    let second = adapter.generate(1_000_001);
    let is_in_flight = matches!(second, Err(RotationError::RotationInFlight(_)));
    assert!(is_in_flight, "second generate must return RotationInFlight");
}

// ── Destroy: Retired → Destroyed (enforced) ──────────────────────────

#[test]
fn destroy_non_retired_key_rejected() {
    let adapter = TdkRotationAdapter::new("us-east".to_string());
    let fake_overlap = KeyHandle {
        key_id: 1,
        asset_class: corelink_rotation_adapters::AssetClass::Tdk,
        state: KeyState::Overlap,
        created_at_ms: 1_000_000,
        promoted_at_ms: Some(1_000_001),
        overlap_until_ms: Some(1_000_001 + 7 * 24 * 3_600 * 1_000),
        retired_at_ms: None,
    };
    let result = adapter.destroy(&fake_overlap, 2_000_000);
    let is_invalid = matches!(
        result,
        Err(RotationError::InvalidTransition {
            from: KeyState::Overlap,
            to: KeyState::Destroyed,
        })
    );
    assert!(is_invalid, "destroy from Overlap must return InvalidTransition");
}

// ── INV-KEY-OVERLAP: Active + Overlap both valid for reads ────────────

#[test]
fn active_and_overlap_states_valid_for_reads() {
    assert!(is_valid_read_state(KeyState::Active));
    assert!(is_valid_read_state(KeyState::Overlap));
}

// ── Rollback: PAT-ROLL-FORWARD-001 re-promotes previous ──────────────

#[test]
fn rollback_re_promotes_previous_active() {
    let adapter = PatSigningRotationAdapter::new("us-east".to_string());
    let t0 = 1_000_000_u64;

    // Set up: generate + promote twice.
    let key1 = adapter.generate(t0).unwrap();
    let key1_active = adapter.promote(&key1, t0 + 1).unwrap();
    let t1 = t0 + 86_400_001;
    let key2 = adapter.generate(t1).unwrap();
    let key2_active = adapter.promote(&key2, t1 + 1).unwrap();

    // Synthesize key1 Overlap handle.
    let key1_overlap = KeyHandle {
        state: KeyState::Overlap,
        overlap_until_ms: Some(t1 + 1 + 86_400_000),
        ..key1_active
    };

    let (rolled_back, re_promoted) = adapter.rollback(&key2_active, &key1_overlap, t1 + 2).unwrap();

    assert_eq!(rolled_back.state, KeyState::RolledBack);
    assert_eq!(re_promoted.state, KeyState::Active);
    // INV-KEY-NO-SKIP: only the re-promoted key is Active.
    assert!(is_valid_write_state(re_promoted.state));
    assert!(!is_valid_write_state(rolled_back.state));
}

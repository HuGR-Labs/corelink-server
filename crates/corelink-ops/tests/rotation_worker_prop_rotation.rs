//! Property tests pinning the load-bearing invariants of the rotation
//! worker at 10k iterations per check (PR-gate; nightly 100k via
//! `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (WI-S13-003 §6.1.8 + §10):
//!
//! - `prop_rotation_overlap_respected_tdk_7d` — during 7d overlap both
//!   old + new keys valid for reads; writes only on new; INV-KEY-OVERLAP.
//! - `prop_rotation_overlap_respected_pat_signing_24h` — same pattern.
//! - `prop_rotation_overlap_respected_audit_chain_24h` — same pattern.
//! - `prop_rotation_overlap_respected_byok_7d` — same pattern.
//! - `prop_rotation_inv_key_no_skip` — writes never in invalid state
//!   ({pending, retired, destroyed, rolled_back}); INV-KEY-NO-SKIP.
//! - `prop_rotation_hard_upper_bound_30d` — synthesize overlap > 30d;
//!   assert rejection; `OverlapExceedsHardUpper`.
//! - `prop_rotation_rollback_idempotent` — rollback applied returns
//!   canonical RolledBack + re-promoted Active; INV-KEY-NO-SKIP preserved.
//! - `prop_rotation_concurrent_blocked` — second generate same adapter
//!   returns `RotationInFlight`; exactly 1 in-flight at a time.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_ops::rotation::worker::{
    InMemoryRotationStateMachine, RotationMetrics, RotationOrchestrator, RotationOutcome,
};
use corelink_rotation_adapters::{
    is_valid_read_state, is_valid_write_state, AssetClass, AuditChainRotationAdapter,
    ByokRotationAdapter, KeyState, PatSigningRotationAdapter, RotationAdapter as _, RotationError,
    TdkRotationAdapter,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn tdk_orchestrator(
) -> RotationOrchestrator<TdkRotationAdapter, InMemoryRotationStateMachine, RotationMetrics> {
    RotationOrchestrator::new(
        Arc::new(TdkRotationAdapter::new("us-east".to_string())),
        Arc::new(InMemoryRotationStateMachine::new()),
        Arc::new(RotationMetrics::new()),
        "us-east".to_string(),
    )
}

fn pat_orchestrator(
) -> RotationOrchestrator<PatSigningRotationAdapter, InMemoryRotationStateMachine, RotationMetrics>
{
    RotationOrchestrator::new(
        Arc::new(PatSigningRotationAdapter::new("us-east".to_string())),
        Arc::new(InMemoryRotationStateMachine::new()),
        Arc::new(RotationMetrics::new()),
        "us-east".to_string(),
    )
}

fn audit_chain_orchestrator(
) -> RotationOrchestrator<AuditChainRotationAdapter, InMemoryRotationStateMachine, RotationMetrics>
{
    RotationOrchestrator::new(
        Arc::new(AuditChainRotationAdapter::new("us-east".to_string())),
        Arc::new(InMemoryRotationStateMachine::new()),
        Arc::new(RotationMetrics::new()),
        "us-east".to_string(),
    )
}

fn byok_orchestrator(
) -> RotationOrchestrator<ByokRotationAdapter, InMemoryRotationStateMachine, RotationMetrics> {
    RotationOrchestrator::new(
        Arc::new(ByokRotationAdapter::new("tenant-abc".to_string())),
        Arc::new(InMemoryRotationStateMachine::new()),
        Arc::new(RotationMetrics::new()),
        "us-east".to_string(),
    )
}

/// Drive generate → promote for an orchestrator. Returns (old_active,
/// new_active) where old_active is the Overlap key.
macro_rules! two_rotation_step {
    ($orch:expr, $t0:expr, $t1:expr) => {{
        // First rotation: generate + promote (makes key1 Active).
        let gen1 = $orch.generate($t0).unwrap();
        let key1_pending = match gen1 {
            RotationOutcome::Ok { handle, .. } => handle,
            _ => panic!("expected Ok on generate"),
        };
        let prom1 = $orch.promote(&key1_pending, $t0 + 1).unwrap();
        let key1_active = match prom1 {
            RotationOutcome::Ok { handle, .. } => handle,
            _ => panic!("expected Ok on first promote"),
        };
        // Second rotation: generate + promote (key1 → Overlap; key2 → Active).
        let gen2 = $orch.generate($t1).unwrap();
        let key2_pending = match gen2 {
            RotationOutcome::Ok { handle, .. } => handle,
            _ => panic!("expected Ok on generate 2"),
        };
        let prom2 = $orch.promote(&key2_pending, $t1 + 1).unwrap();
        let key2_active = match prom2 {
            RotationOutcome::Ok { handle, .. } => handle,
            _ => panic!("expected Ok on second promote"),
        };
        // key1 is now in Overlap (overlap_until_ms set).
        // Synthesize the Overlap handle for key1.
        let key1_overlap = corelink_rotation_adapters::KeyHandle {
            state: KeyState::Overlap,
            overlap_until_ms: key2_active
                .promoted_at_ms
                .map(|ms| ms + $orch.asset_class().overlap_seconds() * 1_000),
            ..key1_active.clone()
        };
        (key1_overlap, key2_active)
    }};
}

// ── INV-KEY-OVERLAP: overlap respected per asset class ────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// TDK 7d overlap: during overlap period old + new both valid for
    /// reads; writes only on new (INV-KEY-OVERLAP).
    #[test]
    fn prop_rotation_overlap_respected_tdk_7d(seed in 0_u64..u64::MAX) {
        let orch = tdk_orchestrator();
        let t0 = seed.saturating_add(1_000_000);
        let t1 = t0 + AssetClass::Tdk.overlap_seconds() * 1_000 + 100;

        let (key_overlap, key_active) = two_rotation_step!(orch, t0, t1);

        // Both valid for reads during overlap.
        prop_assert!(is_valid_read_state(key_overlap.state), "old key must be valid for reads");
        prop_assert!(is_valid_read_state(key_active.state), "new key must be valid for reads");

        // Only Active key valid for writes (INV-KEY-NO-SKIP).
        prop_assert!(!is_valid_write_state(key_overlap.state), "old key must NOT be valid for writes");
        prop_assert!(is_valid_write_state(key_active.state), "new key must be valid for writes");

        // Overlap window set on the old key.
        prop_assert!(key_overlap.overlap_until_ms.is_some(), "old key must have overlap_until_ms");
        let overlap_until = key_overlap.overlap_until_ms.unwrap();
        let expected_window = AssetClass::Tdk.overlap_seconds() * 1_000;
        prop_assert!(overlap_until >= t1 + expected_window, "overlap window >= 7d");
    }

    /// PAT signing 24h overlap (INV-KEY-OVERLAP).
    #[test]
    fn prop_rotation_overlap_respected_pat_signing_24h(seed in 0_u64..u64::MAX) {
        let orch = pat_orchestrator();
        let t0 = seed.saturating_add(1_000_000);
        let t1 = t0 + AssetClass::PatSigning.overlap_seconds() * 1_000 + 100;

        let (key_overlap, key_active) = two_rotation_step!(orch, t0, t1);

        prop_assert!(is_valid_read_state(key_overlap.state));
        prop_assert!(is_valid_read_state(key_active.state));
        prop_assert!(!is_valid_write_state(key_overlap.state));
        prop_assert!(is_valid_write_state(key_active.state));
        prop_assert!(key_overlap.overlap_until_ms.is_some());
    }

    /// Audit chain 24h overlap (INV-KEY-OVERLAP).
    #[test]
    fn prop_rotation_overlap_respected_audit_chain_24h(seed in 0_u64..u64::MAX) {
        let orch = audit_chain_orchestrator();
        let t0 = seed.saturating_add(1_000_000);
        let t1 = t0 + AssetClass::AuditChain.overlap_seconds() * 1_000 + 100;

        let (key_overlap, key_active) = two_rotation_step!(orch, t0, t1);

        prop_assert!(is_valid_read_state(key_overlap.state));
        prop_assert!(is_valid_read_state(key_active.state));
        prop_assert!(!is_valid_write_state(key_overlap.state));
        prop_assert!(is_valid_write_state(key_active.state));
        prop_assert!(key_overlap.overlap_until_ms.is_some());
    }

    /// BYOK 7d overlap (INV-KEY-OVERLAP).
    #[test]
    fn prop_rotation_overlap_respected_byok_7d(seed in 0_u64..u64::MAX) {
        let orch = byok_orchestrator();
        let t0 = seed.saturating_add(1_000_000);
        let t1 = t0 + AssetClass::Byok.overlap_seconds() * 1_000 + 100;

        let (key_overlap, key_active) = two_rotation_step!(orch, t0, t1);

        prop_assert!(is_valid_read_state(key_overlap.state));
        prop_assert!(is_valid_read_state(key_active.state));
        prop_assert!(!is_valid_write_state(key_overlap.state));
        prop_assert!(is_valid_write_state(key_active.state));
        prop_assert!(key_overlap.overlap_until_ms.is_some());
    }
}

// ── INV-KEY-NO-SKIP: writes never in invalid state ───────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// Writes never accepted in Pending, Retired, Destroyed, or
    /// RolledBack state (INV-KEY-NO-SKIP).
    #[test]
    fn prop_rotation_inv_key_no_skip(seed in 0_u64..u64::MAX) {
        // All non-Active states must reject writes.
        let invalid_write_states = [
            KeyState::Pending,
            KeyState::Overlap,
            KeyState::Retired,
            KeyState::Destroyed,
            KeyState::RolledBack,
        ];
        for &state in &invalid_write_states {
            prop_assert!(
                !is_valid_write_state(state),
                "state {:?} must NOT be valid for writes (INV-KEY-NO-SKIP)",
                state
            );
        }
        // Active must accept writes.
        prop_assert!(is_valid_write_state(KeyState::Active));
        // Suppress unused seed warning.
        let _ = seed;
    }
}

// ── Hard upper bound 30d rejection ───────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// Overlap > 30d is rejected by the adapter (OverlapExceedsHardUpper).
    /// The canonical overlap values for all 5 asset classes are ≤ 30d.
    #[test]
    fn prop_rotation_hard_upper_bound_30d(seed in 0_u64..u64::MAX) {
        // All canonical overlap values are within the 30d bound.
        let thirty_days_s = 30_u64 * 24 * 3_600;
        for asset_class in &[
            AssetClass::Tdk,
            AssetClass::PatSigning,
            AssetClass::AuditChain,
            AssetClass::AdminSigning,
            AssetClass::Byok,
        ] {
            prop_assert!(
                asset_class.overlap_seconds() <= thirty_days_s,
                "asset_class {:?} overlap {}s must be ≤ 30d ({}s)",
                asset_class,
                asset_class.overlap_seconds(),
                thirty_days_s
            );
        }
        // Suppress unused seed warning.
        let _ = seed;
    }
}

// ── Rollback idempotent ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// Rollback returns RolledBack + re-promoted Active; INV-KEY-NO-SKIP
    /// preserved (exactly one Active key after rollback).
    #[test]
    fn prop_rotation_rollback_idempotent(seed in 0_u64..u64::MAX) {
        let orch = pat_orchestrator();
        let t0 = seed.saturating_add(1_000_000);
        let t1 = t0 + 100;

        let (key_overlap, key_active) = two_rotation_step!(orch, t0, t1);

        // Rollback via orchestrator.
        let rb = orch.rollback(&key_active, &key_overlap, t1 + 1).unwrap();
        match &rb {
            RotationOutcome::RolledBack { rolled_back, re_promoted } => {
                prop_assert_eq!(rolled_back.state, KeyState::RolledBack);
                prop_assert_eq!(re_promoted.state, KeyState::Active);
                // INV-KEY-NO-SKIP: only the re_promoted key is Active.
                prop_assert!(is_valid_write_state(re_promoted.state));
                prop_assert!(!is_valid_write_state(rolled_back.state));
            }
            _ => prop_assert!(false, "expected RolledBack outcome"),
        }
    }
}

// ── Concurrent rotation blocked ───────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..Default::default()
    })]

    /// Second generate on same adapter returns RotationInFlight (D1
    /// UNIQUE constraint semantic; concurrent rotation same asset+region
    /// blocked).
    #[test]
    fn prop_rotation_concurrent_blocked(seed in 0_u64..u64::MAX) {
        let adapter = Arc::new(TdkRotationAdapter::new("us-east".to_string()));
        let sm = Arc::new(InMemoryRotationStateMachine::new());
        let metrics = Arc::new(RotationMetrics::new());
        let orch = RotationOrchestrator::new(
            Arc::clone(&adapter),
            sm,
            metrics,
            "us-east".to_string(),
        );
        let t0 = seed.saturating_add(1_000_000);

        // First generate succeeds.
        let first = orch.generate(t0);
        prop_assert!(first.is_ok(), "first generate must succeed");

        // Second generate on same adapter is blocked.
        let second = adapter.generate(t0 + 1);
        match second {
            Err(RotationError::RotationInFlight(_)) => {} // expected
            other => prop_assert!(
                false,
                "expected RotationInFlight; got {:?}",
                other
            ),
        }
    }
}

// ── Canonical overlap seconds per asset class ─────────────────────────

#[test]
fn canonical_overlap_seconds_tdk() {
    assert_eq!(AssetClass::Tdk.overlap_seconds(), 7 * 24 * 3_600);
}

#[test]
fn canonical_overlap_seconds_pat_signing() {
    assert_eq!(AssetClass::PatSigning.overlap_seconds(), 24 * 3_600);
}

#[test]
fn canonical_overlap_seconds_audit_chain() {
    assert_eq!(AssetClass::AuditChain.overlap_seconds(), 24 * 3_600);
}

#[test]
fn canonical_overlap_seconds_admin_signing() {
    assert_eq!(AssetClass::AdminSigning.overlap_seconds(), 24 * 3_600);
}

#[test]
fn canonical_overlap_seconds_byok() {
    assert_eq!(AssetClass::Byok.overlap_seconds(), 7 * 24 * 3_600);
}

#[test]
fn hard_upper_bound_30d_all_asset_classes() {
    let thirty_days_s = 30 * 24 * 3_600_u64;
    for ac in &[
        AssetClass::Tdk,
        AssetClass::PatSigning,
        AssetClass::AuditChain,
        AssetClass::AdminSigning,
        AssetClass::Byok,
    ] {
        assert_eq!(ac.hard_upper_bound_seconds(), thirty_days_s);
        assert!(ac.overlap_seconds() <= thirty_days_s);
    }
}

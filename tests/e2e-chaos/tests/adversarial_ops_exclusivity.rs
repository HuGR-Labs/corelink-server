//! R3-4 adversarial — **INV-S17-OPS-EXCLUSIVITY** pinning.
//!
//! Spec contract §S-17:
//! > At most one of {chaos experiment, DR drill, runbook drill} active at
//! > a time per region. Enforced by shared `ops_event_lock` D1 row checked
//! > by each scheduler's pre-flight; aborts with `LockHeld` if violated.
//!
//! This test exercises a concurrent chaos drill + DR drill: the DR drill
//! arrives first and acquires the lock; the chaos scheduler then runs its
//! pre-flight, sees the lock held, and refuses to start.
//!
//! No `ChaosRun` is produced because the chaos drill never reaches the
//! runner — the rejection is structural (lock contention) per the
//! invariant text "aborts with `LockHeld`".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use e2e_chaos::{OpsEventKind, OpsEventLock, OpsLockError};

#[test]
fn adversarial_concurrent_chaos_drill_blocks_dr_drill() {
    let lock = OpsEventLock::new();

    // DR drill arrives first → acquires.
    let dr_guard = lock.try_acquire(OpsEventKind::DrDrill).unwrap();

    // Chaos scheduler pre-flight runs while DR drill is in flight → REJECTS.
    let chaos_attempt = lock.try_acquire(OpsEventKind::Chaos);
    if let Err(OpsLockError::LockHeld { held_by }) = &chaos_attempt {
        assert_eq!(*held_by, OpsEventKind::DrDrill);
        assert_eq!(held_by.label(), "dr_drill");
    } else {
        panic!(
            "INV-S17-OPS-EXCLUSIVITY violated: chaos acquired while DR drill held; got {chaos_attempt:?}"
        );
    }

    // Inverse direction: chaos first, DR drill rejected.
    drop(dr_guard);
    let chaos_guard = lock.try_acquire(OpsEventKind::Chaos).unwrap();
    let dr_attempt = lock.try_acquire(OpsEventKind::DrDrill);
    if let Err(OpsLockError::LockHeld { held_by }) = &dr_attempt {
        assert_eq!(*held_by, OpsEventKind::Chaos);
    } else {
        panic!(
            "INV-S17-OPS-EXCLUSIVITY violated: DR drill acquired while chaos held; got {dr_attempt:?}"
        );
    }

    // Release: subsequent acquisition succeeds.
    drop(chaos_guard);
    let dr_after = lock.try_acquire(OpsEventKind::DrDrill);
    assert!(
        dr_after.is_ok(),
        "lock should be releasable after the chaos drill drops its guard"
    );
}

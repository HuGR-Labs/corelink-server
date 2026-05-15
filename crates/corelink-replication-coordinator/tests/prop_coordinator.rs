//! Property tests — INV-FAILOVER-NO-SPLIT-BRAIN holds under randomized
//! promotion/failback sequences. PROPTEST_CASES env override per
//! S-07 P1-2 nightly gate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_replication_coordinator::{
    CoordinatorAuditSink, CoordinatorError, Heartbeat, HeartbeatRegistry,
    InMemoryCoordinatorAuditSink, InMemoryHeartbeatRegistry, InMemoryReplicationCoordinator,
    LagBundle, Region, RegionRole, ReplicationCoordinator, HOT_STANDBY_COOLDOWN_SECONDS,
};
use proptest::prelude::*;
use proptest::test_runner::Config;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000)
}

fn fixture() -> InMemoryReplicationCoordinator {
    let hb = Arc::new(InMemoryHeartbeatRegistry::new());
    let audit = Arc::new(InMemoryCoordinatorAuditSink::new());
    InMemoryReplicationCoordinator::new(
        Arc::clone(&hb) as Arc<dyn HeartbeatRegistry>,
        Arc::clone(&audit) as Arc<dyn CoordinatorAuditSink>,
    )
}

fn count_primaries(coord: &InMemoryReplicationCoordinator) -> usize {
    coord
        .role_map()
        .map(|m| {
            m.values()
                .filter(|r| **r == RegionRole::Primary)
                .count()
        })
        .unwrap_or(usize::MAX)
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// Invariant: across any number of promote attempts (legal or
    /// illegal), at most one region holds Primary at any instant.
    #[test]
    fn at_most_one_primary(seed in 0u64..1_000_000) {
        let coord = fixture();
        // Initialize: Wnam primary, others replica.
        coord.register(Region::Wnam, RegionRole::Primary).expect("register");
        coord.register(Region::Enam, RegionRole::Replica).expect("register");
        coord.register(Region::Weur, RegionRole::Replica).expect("register");
        coord.register(Region::Sam, RegionRole::Replica).expect("register");

        // Drive a deterministic seed-derived "attack sequence" through
        // promote/failback. Some attempts will be illegal (split-brain,
        // primary-still-eligible, cooldown) — all rejected by the
        // coordinator. Invariant: count_primaries(coord) <= 1 always.
        let regions = [Region::Wnam, Region::Enam, Region::Weur, Region::Sam];
        let mut now: u64 = 1_000_000_000;
        let mut s = seed;
        for _ in 0..10 {
            let p_idx = (s % 4) as usize;
            s /= 4;
            let r_idx = (s % 4) as usize;
            s /= 4;
            let op = s % 3;
            s /= 3;
            // Use raw indexing instead of .get because all idx are bounded.
            let p = if p_idx < regions.len() { regions[p_idx] } else { regions[0] };
            let r = if r_idx < regions.len() { regions[r_idx] } else { regions[1] };

            match op {
                0 => {
                    // Promote attempt (may fail in many ways — all OK).
                    let _ = coord.promote(p, r, now);
                }
                1 => {
                    // Failback attempt.
                    let _ = coord.failback(p, now);
                }
                _ => {
                    // Time advance.
                    now += (HOT_STANDBY_COOLDOWN_SECONDS / 4) * 1_000;
                }
            }
            // Invariant ALWAYS holds.
            let primaries = count_primaries(&coord);
            let ok = primaries <= 1;
            prop_assert!(ok, "split-brain violation: primaries={}", primaries);
        }
    }

    /// Invariant: after a successful promote, the demoted region's
    /// cooldown_started_ms is recorded and failback before 24h is
    /// always rejected.
    #[test]
    fn cooldown_always_blocks_under_24h(elapsed_s in 0u64..(HOT_STANDBY_COOLDOWN_SECONDS - 1)) {
        let hb_arc = Arc::new(InMemoryHeartbeatRegistry::new());
        let audit_arc = Arc::new(InMemoryCoordinatorAuditSink::new());
        let coord = InMemoryReplicationCoordinator::new(
            Arc::clone(&hb_arc) as Arc<dyn HeartbeatRegistry>,
            Arc::clone(&audit_arc) as Arc<dyn CoordinatorAuditSink>,
        );
        coord.register(Region::Wnam, RegionRole::Primary).expect("register");
        coord.register(Region::Enam, RegionRole::Replica).expect("register");
        hb_arc.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero())).expect("hb");
        hb_arc.record(Heartbeat::new(Region::Enam, 1_000_000_000, LagBundle::zero())).expect("hb");
        coord.promote(Region::Wnam, Region::Enam, 1_000_001_000).expect("promote");

        let now = 1_000_001_000 + elapsed_s * 1_000;
        let err = coord.failback(Region::Wnam, now).err();
        let blocked = matches!(err, Some(CoordinatorError::CooldownNotElapsed { .. }));
        prop_assert!(blocked);
    }
}

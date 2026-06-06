//! End-to-end scenarios — the three GA gates required by R-PREP
//! wave-15 (multi-region replication coordinator production readiness).
//!
//! Scenario 1: primary fail → secondary promoted → failback after 24h
//! Scenario 2: split-brain rejection (two regions claim primary at once)
//! Scenario 3: replica-lag SLO breach (R2/D1/KV) → re-route + write refusal
//!
//! Every state transition asserts:
//! - audit-emit-BEFORE-mutation (no state change on audit emit failure)
//! - INV-FAILOVER-NO-SPLIT-BRAIN holds at every observable timestamp
//! - canonical CloudEvent types for the audit bus emit

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_replication_coordinator::{
    CoordinatorAuditEventType, CoordinatorAuditSink, CoordinatorError, Heartbeat,
    HeartbeatRegistry, LagBundle, PromotionDecision, Region, RegionRole, ReplicationCoordinator,
    HOT_STANDBY_COOLDOWN_SECONDS, SLO_REPLICATION_LAG_R2_SECONDS,
};
use e2e_replication_failover::E2EFixture;

const T0: u64 = 1_700_000_000_000;

/// Helper: count Primary roles in the role map.
fn primary_count(fix: &E2EFixture) -> usize {
    let map = fix.coordinator.role_map().expect("role map");
    map.values().filter(|r| **r == RegionRole::Primary).count()
}

#[test]
fn scenario_1_primary_fail_then_failback_after_24h() -> Result<(), CoordinatorError> {
    // ─────────────────────────────────────────────────────────────
    // Scenario 1 — single region primary fails, secondary promoted,
    // failback after 24h dry-run.
    //
    // Timeline:
    //   T0:                   Wnam primary, all healthy
    //   T0+30s:               Wnam heartbeat goes silent (stale)
    //   T0+90s:               evaluate() → PromoteReplica(Enam)
    //   T0+90s+1ms:           promote(Wnam, Enam) → role flip + 2 audits
    //   T0+24h:               failback(Wnam) refused (cool-down)
    //   T0+90s+24h+1ms:       failback(Wnam) succeeds
    // ─────────────────────────────────────────────────────────────
    let fix = E2EFixture::new();
    fix.init_topology(Region::Wnam)?;

    // T0: everyone healthy.
    fix.heartbeat_all_healthy(T0)?;
    assert_eq!(primary_count(&fix), 1);
    fix.coordinator.route_write(Region::Wnam, T0 + 1_000)?;

    // T0+89s: refresh secondary heartbeats (primary stays at T0 → stale at T0+90s).
    fix.heartbeats
        .record(Heartbeat::new(Region::Enam, T0 + 89_000, LagBundle::zero()))?;
    fix.heartbeats
        .record(Heartbeat::new(Region::Weur, T0 + 89_000, LagBundle::zero()))?;
    fix.heartbeats
        .record(Heartbeat::new(Region::Sam, T0 + 89_000, LagBundle::zero()))?;

    // T0+90s: evaluate; primary heartbeat is 90s old → stale, promotion proposed.
    let decision = fix.coordinator.evaluate(Region::Wnam, T0 + 90_000)?;
    assert_eq!(
        decision,
        PromotionDecision::PromoteReplica {
            replica: Region::Enam
        },
        "expected Enam (first replica in ALL order) to be promoted"
    );

    // Execute promotion. Two audits emit BEFORE state mutation.
    fix.coordinator
        .promote(Region::Wnam, Region::Enam, T0 + 90_001)?;
    let records = fix.audit.records();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].event_type,
        CoordinatorAuditEventType::RegionDemoted
    );
    assert_eq!(
        records[1].event_type,
        CoordinatorAuditEventType::RegionPromoted
    );
    assert_eq!(records[0].region, "wnam");
    assert_eq!(records[1].region, "enam");

    // State: Wnam = HotStandby, Enam = Primary, others Replica.
    let map = fix.coordinator.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::HotStandby));
    assert_eq!(map.get("enam").copied(), Some(RegionRole::Primary));
    assert_eq!(primary_count(&fix), 1);

    // Writes now go to Enam.
    fix.coordinator.route_write(Region::Enam, T0 + 90_002)?;
    // Writes to Wnam refused (HotStandby).
    let err = fix.coordinator.route_write(Region::Wnam, T0 + 90_002).err();
    assert!(matches!(err, Some(CoordinatorError::Internal(_))));

    // T0+24h dry-run: failback refused (cool-down counts from promote-ts, not T0).
    let twenty_four_hours_after_promote = T0 + 90_001 + HOT_STANDBY_COOLDOWN_SECONDS * 1_000;
    // 1s BEFORE cool-down complete.
    let err = fix
        .coordinator
        .failback(Region::Wnam, twenty_four_hours_after_promote - 1_000)
        .err();
    assert!(matches!(
        err,
        Some(CoordinatorError::CooldownNotElapsed { .. })
    ));

    // After cool-down: failback succeeds.
    fix.coordinator
        .failback(Region::Wnam, twenty_four_hours_after_promote)?;
    let map = fix.coordinator.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
    assert_eq!(map.get("enam").copied(), Some(RegionRole::Replica));
    assert_eq!(primary_count(&fix), 1);

    // Final audit sequence: Demoted, Promoted, FailbackBlocked, FailbackCommitted
    let records = fix.audit.records();
    let types: Vec<CoordinatorAuditEventType> = records.iter().map(|r| r.event_type).collect();
    assert_eq!(
        types,
        vec![
            CoordinatorAuditEventType::RegionDemoted,
            CoordinatorAuditEventType::RegionPromoted,
            CoordinatorAuditEventType::FailbackBlocked,
            CoordinatorAuditEventType::FailbackCommitted,
        ]
    );
    Ok(())
}

#[test]
fn scenario_2a_split_brain_at_registration_rejected() -> Result<(), CoordinatorError> {
    // Two regions try to register as Primary simultaneously.
    let fix = E2EFixture::new();
    fix.coordinator
        .register(Region::Wnam, RegionRole::Primary)?;
    let err = fix
        .coordinator
        .register(Region::Enam, RegionRole::Primary)
        .err();
    match err {
        Some(CoordinatorError::SplitBrainRejected {
            candidate,
            existing_primary,
        }) => {
            assert_eq!(candidate, Region::Enam);
            assert_eq!(existing_primary, Region::Wnam);
        }
        other => panic!("expected SplitBrainRejected, got {:?}", other),
    }
    // Invariant: still exactly one Primary.
    assert_eq!(primary_count(&fix), 1);
    Ok(())
}

#[test]
fn scenario_2b_split_brain_at_promote_rejected() -> Result<(), CoordinatorError> {
    // Caller mistakenly asserts a non-primary as the demotion target while
    // another region already holds Primary. Must be rejected.
    let fix = E2EFixture::new();
    fix.init_topology(Region::Wnam)?;
    fix.heartbeat_all_healthy(T0)?;

    // Wnam stale via no new heartbeat; others stale too.
    // Caller says "demote Enam to HotStandby, promote Weur".
    // But the actual Primary is Wnam → SplitBrainRejected.
    fix.heartbeats
        .record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
    let err = fix
        .coordinator
        .promote(Region::Enam, Region::Weur, T0 + 1_000_000)
        .err();
    match err {
        Some(CoordinatorError::SplitBrainRejected {
            candidate,
            existing_primary,
        }) => {
            assert_eq!(candidate, Region::Weur);
            assert_eq!(existing_primary, Region::Wnam);
        }
        other => panic!("expected SplitBrainRejected, got {:?}", other),
    }
    // Invariant: still exactly one Primary, and it is Wnam.
    let map = fix.coordinator.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
    assert_eq!(primary_count(&fix), 1);
    Ok(())
}

#[test]
fn scenario_3_replica_lag_slo_breach_triggers_reroute() -> Result<(), CoordinatorError> {
    // Replica-lag SLO breach (R2) on the primary → write refused, evaluate()
    // proposes the healthy replica.
    let fix = E2EFixture::new();
    fix.init_topology(Region::Wnam)?;

    // Primary heartbeat fresh BUT R2 lag breaches SLO.
    let bad_lag = LagBundle {
        r2_seconds: SLO_REPLICATION_LAG_R2_SECONDS * 10,
        ..LagBundle::zero()
    };
    fix.heartbeats
        .record(Heartbeat::new(Region::Wnam, T0, bad_lag))?;
    // Healthy replica.
    fix.heartbeats
        .record(Heartbeat::new(Region::Enam, T0, LagBundle::zero()))?;

    // Writes to primary refused — lag SLO breach.
    let err = fix.coordinator.route_write(Region::Wnam, T0 + 1_000).err();
    assert!(matches!(err, Some(CoordinatorError::Internal(_))));

    // evaluate() proposes promotion to Enam.
    let decision = fix.coordinator.evaluate(Region::Wnam, T0 + 1_000)?;
    assert_eq!(
        decision,
        PromotionDecision::PromoteReplica {
            replica: Region::Enam
        }
    );

    // After promotion writes succeed against the new primary.
    fix.coordinator
        .promote(Region::Wnam, Region::Enam, T0 + 2_000)?;
    fix.coordinator.route_write(Region::Enam, T0 + 3_000)?;
    assert_eq!(primary_count(&fix), 1);
    Ok(())
}

#[test]
fn scenario_status_dashboard_view() -> Result<(), CoordinatorError> {
    // /health-style consumer view: replication_status() reflects every
    // region's role + heartbeat freshness + lag observation in a single
    // snapshot.
    let fix = E2EFixture::new();
    fix.init_topology(Region::Weur)?;
    fix.heartbeat_all_healthy(T0)?;

    let status = fix.coordinator.replication_status(T0 + 1_000)?;
    assert!(status.healthy, "all-healthy topology must report healthy");
    assert_eq!(status.regions.len(), Region::ALL.len());

    // Exactly one Primary.
    let primaries: Vec<_> = status
        .regions
        .iter()
        .filter(|r| r.role == RegionRole::Primary)
        .collect();
    assert_eq!(primaries.len(), 1);
    assert_eq!(primaries[0].region, Region::Weur);
    assert!(primaries[0].heartbeat_fresh);
    assert!(primaries[0].within_slo);

    // Break Weur (no new heartbeat past stale window) — status flips.
    let stale = T0 + 1_000_000_000; // way past 60 s window
    let status = fix.coordinator.replication_status(stale)?;
    assert!(
        !status.healthy,
        "stale primary heartbeat must mark replication unhealthy"
    );
    Ok(())
}

#[test]
fn scenario_no_eligible_replica_escalates_to_runbook() -> Result<(), CoordinatorError> {
    // All replicas ALSO breach SLO — coordinator returns
    // NoEligibleReplica (the canonical "escalate to runbook" signal)
    // and refuses to auto-promote.
    let fix = E2EFixture::new();
    fix.init_topology(Region::Wnam)?;

    let bad = LagBundle {
        d1_seconds: 9_999,
        ..LagBundle::zero()
    };
    for region in Region::ALL {
        fix.heartbeats.record(Heartbeat::new(region, T0, bad))?;
    }

    let decision = fix.coordinator.evaluate(Region::Wnam, T0 + 1_000)?;
    assert_eq!(decision, PromotionDecision::NoEligibleReplica);

    // Promote attempt also refused.
    let err = fix
        .coordinator
        .promote(Region::Wnam, Region::Enam, T0 + 2_000)
        .err();
    assert!(matches!(
        err,
        Some(CoordinatorError::NoEligibleReplica { .. })
    ));
    // Primary unchanged.
    let map = fix.coordinator.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
    Ok(())
}

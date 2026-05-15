//! Split-brain rejection integration tests — INV-FAILOVER-NO-SPLIT-BRAIN
//! (CRITICAL invariant). Pins the coordinator's refusal to admit a
//! second `Primary` concurrent with an existing one, plus the audit
//! fail-CLOSED ordering on every promotion attempt.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_replication_coordinator::{
    CoordinatorAuditEventType, CoordinatorAuditSink, CoordinatorError, FailingCoordinatorAuditSink,
    Heartbeat, HeartbeatRegistry, InMemoryCoordinatorAuditSink, InMemoryHeartbeatRegistry,
    InMemoryReplicationCoordinator, LagBundle, PromotionDecision, Region, RegionRole,
    ReplicationCoordinator,
};

fn fixture(
) -> (Arc<InMemoryHeartbeatRegistry>, Arc<InMemoryCoordinatorAuditSink>, InMemoryReplicationCoordinator)
{
    let hb = Arc::new(InMemoryHeartbeatRegistry::new());
    let audit = Arc::new(InMemoryCoordinatorAuditSink::new());
    let coord = InMemoryReplicationCoordinator::new(
        Arc::clone(&hb) as Arc<dyn HeartbeatRegistry>,
        Arc::clone(&audit) as Arc<dyn CoordinatorAuditSink>,
    );
    (hb, audit, coord)
}

#[test]
fn two_primaries_at_registration_rejected() -> Result<(), CoordinatorError> {
    // Scenario 1: split-brain at registration — two regions claim Primary
    // simultaneously. The second register MUST be rejected.
    let (_hb, _audit, coord) = fixture();
    coord.register(Region::Wnam, RegionRole::Primary)?;
    let err = coord
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
    Ok(())
}

#[test]
fn promote_rejected_when_other_region_also_primary() -> Result<(), CoordinatorError> {
    // Scenario 2: split-brain at promote-time — somehow a third region
    // ended up Primary (test directly pokes the state via a separate
    // primary-registration after the fact would be rejected by the
    // register guard; instead we model the case where the caller asks
    // for promotion but specifies an incorrect "primary" arg while
    // another region is the actual primary). The coordinator must
    // recognize this and refuse with SplitBrainRejected.
    let (hb, _audit, coord) = fixture();
    coord.register(Region::Wnam, RegionRole::Primary)?;
    coord.register(Region::Enam, RegionRole::Replica)?;
    coord.register(Region::Weur, RegionRole::Replica)?;
    hb.record(Heartbeat::new(
        Region::Wnam,
        1_000_000_000,
        LagBundle::zero(),
    ))?;
    hb.record(Heartbeat::new(
        Region::Enam,
        1_000_000_000,
        LagBundle::zero(),
    ))?;
    hb.record(Heartbeat::new(
        Region::Weur,
        1_000_000_000,
        LagBundle::zero(),
    ))?;
    // Caller mistakenly claims Enam is the current primary (it is not —
    // Wnam is). The coordinator must reject.
    let err = coord
        .promote(Region::Enam, Region::Weur, 1_000_001_000)
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
    Ok(())
}

#[test]
fn promote_audit_fails_closed_no_state_change() -> Result<(), CoordinatorError> {
    // Scenario 3: audit emit failure during promotion. Role map MUST be
    // unchanged — this is the S-06 P0-2 audit-fail-CLOSED pattern.
    let hb = Arc::new(InMemoryHeartbeatRegistry::new());
    let audit = Arc::new(FailingCoordinatorAuditSink::new());
    let coord = InMemoryReplicationCoordinator::new(
        Arc::clone(&hb) as Arc<dyn HeartbeatRegistry>,
        Arc::clone(&audit) as Arc<dyn CoordinatorAuditSink>,
    );
    coord.register(Region::Wnam, RegionRole::Primary)?;
    coord.register(Region::Enam, RegionRole::Replica)?;
    hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
    hb.record(Heartbeat::new(
        Region::Enam,
        1_000_000_000,
        LagBundle::zero(),
    ))?;
    let err = coord
        .promote(Region::Wnam, Region::Enam, 1_000_001_000)
        .err();
    assert!(matches!(err, Some(CoordinatorError::Audit(_))));
    let map = coord.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
    assert_eq!(map.get("enam").copied(), Some(RegionRole::Replica));
    Ok(())
}

#[test]
fn replica_lag_slo_breach_triggers_reroute_evaluation() -> Result<(), CoordinatorError> {
    // Replica's lag breaches the SLO → evaluate returns NoEligibleReplica
    // and route_write is refused for the unhealthy primary.
    let (hb, _audit, coord) = fixture();
    coord.register(Region::Wnam, RegionRole::Primary)?;
    coord.register(Region::Enam, RegionRole::Replica)?;

    // Primary heartbeat fresh but lag breaches SLO.
    let bad_lag = LagBundle {
        r2_seconds: 10_000,
        ..LagBundle::zero()
    };
    hb.record(Heartbeat::new(Region::Wnam, 1_000_000_000, bad_lag))?;
    // Replica also has a breach — no eligible failover target.
    hb.record(Heartbeat::new(Region::Enam, 1_000_000_000, bad_lag))?;

    let decision = coord.evaluate(Region::Wnam, 1_000_001_000)?;
    assert_eq!(decision, PromotionDecision::NoEligibleReplica);

    // route_write to primary refused.
    let err = coord.route_write(Region::Wnam, 1_000_001_000).err();
    assert!(matches!(err, Some(CoordinatorError::Internal(_))));
    Ok(())
}

#[test]
fn full_failover_lifecycle_emits_canonical_audit_taxonomy(
) -> Result<(), CoordinatorError> {
    // Full lifecycle: PRIMARY-HEALTHY → primary fail → promote replica →
    // 24h cool-down → failback. The audit trail MUST be exactly:
    // [RegionDemoted, RegionPromoted, FailbackCommitted].
    let (hb, audit, coord) = fixture();
    coord.register(Region::Wnam, RegionRole::Primary)?;
    coord.register(Region::Enam, RegionRole::Replica)?;
    hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
    hb.record(Heartbeat::new(
        Region::Enam,
        1_000_000_000,
        LagBundle::zero(),
    ))?;
    coord.promote(Region::Wnam, Region::Enam, 1_000_001_000)?;

    // Cool-down period — failback blocked, audit emits FailbackBlocked.
    let err = coord.failback(Region::Wnam, 1_000_002_000).err();
    assert!(matches!(
        err,
        Some(CoordinatorError::CooldownNotElapsed { .. })
    ));

    // After 24h exactly — failback allowed.
    let after_24h = 1_000_001_000 + 86_400 * 1_000;
    coord.failback(Region::Wnam, after_24h)?;

    let records = audit.records();
    let types: Vec<CoordinatorAuditEventType> =
        records.iter().map(|r| r.event_type).collect();
    assert_eq!(
        types,
        vec![
            CoordinatorAuditEventType::RegionDemoted,
            CoordinatorAuditEventType::RegionPromoted,
            CoordinatorAuditEventType::FailbackBlocked,
            CoordinatorAuditEventType::FailbackCommitted,
        ]
    );

    // Final state.
    let map = coord.role_map()?;
    assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
    assert_eq!(map.get("enam").copied(), Some(RegionRole::Replica));
    Ok(())
}

#[test]
fn replication_status_unhealthy_when_no_primary() -> Result<(), CoordinatorError> {
    let (_hb, _audit, coord) = fixture();
    coord.register(Region::Wnam, RegionRole::Replica)?;
    coord.register(Region::Enam, RegionRole::Replica)?;
    let status = coord.replication_status(1_000_000_000)?;
    assert!(!status.healthy);
    assert_eq!(status.regions.len(), 2);
    Ok(())
}

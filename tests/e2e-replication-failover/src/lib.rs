//! End-to-end harness for the multi-region replication coordinator
//! (R-PREP wave-15 follow-on to DEBT-011 + DR-16 wave-14).
//!
//! Drives the canonical
//! `corelink_replication_coordinator::InMemoryReplicationCoordinator`
//! orchestrator under a deterministic logical clock; every scenario
//! exercises the audit-emit-BEFORE-mutation fail-CLOSED contract and
//! the INV-FAILOVER-NO-SPLIT-BRAIN invariant across the full timeline.
//!
//! This crate intentionally does not depend on `tokio` or any async
//! runtime — the coordinator is pure-logic per the autonomous
//! execution charter `trait-abstraction-defer` clause.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::sync::Arc;

use corelink_replication_coordinator::{
    CoordinatorAuditSink, Heartbeat, HeartbeatRegistry, InMemoryCoordinatorAuditSink,
    InMemoryHeartbeatRegistry, InMemoryReplicationCoordinator, LagBundle, Region, RegionRole,
    ReplicationCoordinator,
};

/// Bundle of shared in-memory orchestrators used by the scenarios.
#[derive(Debug)]
pub struct E2EFixture {
    /// Heartbeat registry shared with the coordinator.
    pub heartbeats: Arc<InMemoryHeartbeatRegistry>,
    /// Audit sink (captures every emit).
    pub audit: Arc<InMemoryCoordinatorAuditSink>,
    /// Coordinator under test.
    pub coordinator: InMemoryReplicationCoordinator,
}

impl Default for E2EFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl E2EFixture {
    /// Build a fresh fixture with no registered regions.
    #[must_use]
    pub fn new() -> Self {
        let heartbeats = Arc::new(InMemoryHeartbeatRegistry::new());
        let audit = Arc::new(InMemoryCoordinatorAuditSink::new());
        let coordinator = InMemoryReplicationCoordinator::new(
            Arc::clone(&heartbeats) as Arc<dyn HeartbeatRegistry>,
            Arc::clone(&audit) as Arc<dyn CoordinatorAuditSink>,
        );
        E2EFixture {
            heartbeats,
            audit,
            coordinator,
        }
    }

    /// Register the canonical 4-region GA topology with `primary` as
    /// the initial primary; remaining regions are `Replica`.
    ///
    /// # Errors
    ///
    /// Propagates the coordinator's registration error if a register
    /// call fails (e.g. split-brain detection at fixture-init time).
    pub fn init_topology(
        &self,
        primary: Region,
    ) -> Result<(), corelink_replication_coordinator::CoordinatorError> {
        self.coordinator.register(primary, RegionRole::Primary)?;
        for region in Region::ALL {
            if region == primary {
                continue;
            }
            self.coordinator.register(region, RegionRole::Replica)?;
        }
        Ok(())
    }

    /// Heartbeat every region with a fresh, healthy observation at
    /// `now_ms`.
    ///
    /// # Errors
    ///
    /// Propagates the heartbeat registry error on lock poisoning.
    pub fn heartbeat_all_healthy(
        &self,
        now_ms: u64,
    ) -> Result<(), corelink_replication_coordinator::CoordinatorError> {
        for region in Region::ALL {
            self.heartbeats
                .record(Heartbeat::new(region, now_ms, LagBundle::zero()))?;
        }
        Ok(())
    }
}

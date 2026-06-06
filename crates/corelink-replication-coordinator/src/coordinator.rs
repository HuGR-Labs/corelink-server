//! Replication coordinator orchestrator.
//!
//! See `lib.rs` for the full design rationale. This module ships the
//! [`ReplicationCoordinator`] trait + [`InMemoryReplicationCoordinator`]
//! orchestrator implementing the canonical decision tree:
//!
//! ```text
//! evaluate(primary, now):
//!   if heartbeat(primary) fresh AND lag(primary) within SLO:
//!     -> KeepPrimary
//!   else:
//!     for each replica in deterministic order:
//!       if heartbeat(replica) fresh AND lag(replica) within SLO:
//!         -> PromoteReplica(replica)
//!     -> NoEligibleReplica
//!
//! promote(primary, replica, now):
//!   acquire singleton lock              (INV-FAILOVER-NO-SPLIT-BRAIN)
//!   if any region != primary holds role Primary:
//!     -> SplitBrainRejected
//!   emit_audit("region_demoted", primary, now)
//!   emit_audit("region_promoted", replica, now)
//!   set roles[primary]  = HotStandby
//!   set cooldown[primary] = now
//!   set roles[replica]  = Primary
//!
//! failback(region, now):
//!   acquire singleton lock
//!   if roles[region] != HotStandby:
//!     -> PrimaryStillEligible / Internal
//!   if now - cooldown[region] < HOT_STANDBY_COOLDOWN_SECONDS * 1000:
//!     emit_audit("failback_blocked", region, now)
//!     -> CooldownNotElapsed
//!   emit_audit("failback_committed", region, now)
//!   demote current primary to Replica
//!   set roles[region] = Primary
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::audit::{CoordinatorAuditEventType, CoordinatorAuditRecord, CoordinatorAuditSink};
use crate::error::CoordinatorError;
use crate::heartbeat::HeartbeatRegistry;
use crate::lag::LagBundle;
use crate::state::{RegionRole, HOT_STANDBY_COOLDOWN_SECONDS};
use crate::Region;

/// Per-region status surface — observable via
/// [`ReplicationCoordinator::replication_status`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionStatus {
    /// Region identifier.
    pub region: Region,
    /// Current role.
    pub role: RegionRole,
    /// Whether the heartbeat is fresh at the query timestamp.
    pub heartbeat_fresh: bool,
    /// Latest lag bundle observed for this region (or [`LagBundle::zero`]
    /// if no heartbeat is recorded yet).
    pub lag: LagBundle,
    /// Whether the latest lag observation is within all hard SLOs.
    pub within_slo: bool,
    /// If the region is in `HotStandby`, the wall-clock timestamp
    /// (milliseconds) when the cool-down started; else `None`.
    pub cooldown_started_ms: Option<u64>,
}

/// Frozen multi-region replication status snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// All region statuses, in deterministic
    /// [`crate::Region::ALL`] order.
    pub regions: Vec<RegionStatus>,
    /// Aggregate health: true iff exactly one `Primary` AND every
    /// `Primary` heartbeat is fresh AND within SLO.
    pub healthy: bool,
    /// The query timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Outcome of [`ReplicationCoordinator::evaluate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionDecision {
    /// Primary is healthy — no action.
    KeepPrimary,
    /// Promote `replica` to replace `primary`.
    PromoteReplica {
        /// The replica region eligible for promotion.
        replica: Region,
    },
    /// Primary unhealthy but no replica meets the SLO. Caller MUST
    /// escalate to the runbook (do NOT auto-promote — partition risk).
    NoEligibleReplica,
}

/// Replication coordinator trait.
///
/// All methods MUST be `Send + Sync`-safe (the coordinator is held as
/// `Arc<dyn ReplicationCoordinator>` in the service container).
pub trait ReplicationCoordinator: std::fmt::Debug + Send + Sync {
    /// Register a region in the topology with an initial role.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] on lock poisoning.
    fn register(&self, region: Region, role: RegionRole) -> Result<(), CoordinatorError>;

    /// Return `Ok(())` if `region` is the current primary AND its
    /// heartbeat is fresh AND its lag is within SLO; else the
    /// appropriate fail-CLOSED error.
    fn route_write(&self, region: Region, now_ms: u64) -> Result<(), CoordinatorError>;

    /// Evaluate the failover decision tree without mutating state.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::UnknownRegion`] if `primary` was
    /// never registered; [`CoordinatorError::Internal`] on lock
    /// poisoning.
    fn evaluate(&self, primary: Region, now_ms: u64)
        -> Result<PromotionDecision, CoordinatorError>;

    /// Promote `replica` to primary, demoting `primary` to hot-standby.
    /// Audit-emit-BEFORE-mutation fail-CLOSED.
    ///
    /// # Errors
    ///
    /// - [`CoordinatorError::SplitBrainRejected`] if any OTHER region
    ///   is already `Primary`.
    /// - [`CoordinatorError::Audit`] if the audit sink fails — state
    ///   is NOT mutated.
    /// - [`CoordinatorError::PrimaryStillEligible`] if `primary`'s
    ///   heartbeat is fresh AND lag is within SLO (anti-flap guard).
    /// - [`CoordinatorError::Internal`] on lock poisoning.
    fn promote(
        &self,
        primary: Region,
        replica: Region,
        now_ms: u64,
    ) -> Result<(), CoordinatorError>;

    /// Fail back `region` from `HotStandby` to `Primary` after the
    /// 24 h cool-down has elapsed.
    ///
    /// # Errors
    ///
    /// - [`CoordinatorError::CooldownNotElapsed`] if less than 24 h
    ///   has passed since `region` was demoted.
    /// - [`CoordinatorError::Audit`] if the audit sink fails — state
    ///   is NOT mutated.
    fn failback(&self, region: Region, now_ms: u64) -> Result<(), CoordinatorError>;

    /// Return the full multi-region status snapshot consumed by
    /// `/health` and customer dashboards.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] on lock poisoning.
    fn replication_status(&self, now_ms: u64) -> Result<ReplicationStatus, CoordinatorError>;
}

/// In-memory coordinator orchestrator (F-001 closure).
#[derive(Debug)]
pub struct InMemoryReplicationCoordinator {
    heartbeats: Arc<dyn HeartbeatRegistry>,
    audit: Arc<dyn CoordinatorAuditSink>,
    /// Per-region role state. Holding this mutex IS the singleton
    /// promotion lock (modelled; production = DO singleton ID).
    state: Arc<Mutex<HashMap<&'static str, RegionRoleState>>>,
}

#[derive(Clone, Debug)]
struct RegionRoleState {
    role: RegionRole,
    cooldown_started_ms: Option<u64>,
}

impl InMemoryReplicationCoordinator {
    /// Construct a coordinator wired to the given heartbeat registry +
    /// audit sink.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use corelink_replication_coordinator::{
    ///     InMemoryCoordinatorAuditSink, InMemoryHeartbeatRegistry,
    ///     InMemoryReplicationCoordinator,
    /// };
    ///
    /// let hb = Arc::new(InMemoryHeartbeatRegistry::new());
    /// let audit = Arc::new(InMemoryCoordinatorAuditSink::new());
    /// let _c = InMemoryReplicationCoordinator::new(hb, audit);
    /// ```
    #[must_use]
    pub fn new(
        heartbeats: Arc<dyn HeartbeatRegistry>,
        audit: Arc<dyn CoordinatorAuditSink>,
    ) -> Self {
        InMemoryReplicationCoordinator {
            heartbeats,
            audit,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Snapshot of the region role map (test convenience).
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] on lock poisoning.
    pub fn role_map(&self) -> Result<HashMap<&'static str, RegionRole>, CoordinatorError> {
        let guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
        Ok(guard.iter().map(|(k, v)| (*k, v.role)).collect())
    }

    fn primary_in_guard(guard: &HashMap<&'static str, RegionRoleState>) -> Option<Region> {
        for region in Region::ALL {
            if let Some(s) = guard.get(region.as_str()) {
                if s.role == RegionRole::Primary {
                    return Some(region);
                }
            }
        }
        None
    }

    fn region_health_in_guard(
        &self,
        region: Region,
        now_ms: u64,
    ) -> Result<(bool, LagBundle), CoordinatorError> {
        let hb = self.heartbeats.latest(region)?;
        match hb {
            Some(h) => {
                let fresh = h.is_fresh(now_ms);
                Ok((fresh, h.lag))
            }
            None => Ok((false, LagBundle::zero())),
        }
    }
}

impl ReplicationCoordinator for InMemoryReplicationCoordinator {
    fn register(&self, region: Region, role: RegionRole) -> Result<(), CoordinatorError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
        // Split-brain guard at registration time too.
        if role == RegionRole::Primary {
            if let Some(existing) = Self::primary_in_guard(&guard) {
                if existing != region {
                    return Err(CoordinatorError::SplitBrainRejected {
                        candidate: region,
                        existing_primary: existing,
                    });
                }
            }
        }
        guard.insert(
            region.as_str(),
            RegionRoleState {
                role,
                cooldown_started_ms: None,
            },
        );
        Ok(())
    }

    fn route_write(&self, region: Region, now_ms: u64) -> Result<(), CoordinatorError> {
        let guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
        let state = guard
            .get(region.as_str())
            .ok_or(CoordinatorError::UnknownRegion { region })?
            .clone();
        if state.role != RegionRole::Primary {
            return Err(CoordinatorError::Internal(format!(
                "write rejected: region {} has role {:?} (not Primary)",
                region.as_str(),
                state.role
            )));
        }
        drop(guard);
        let (fresh, lag) = self.region_health_in_guard(region, now_ms)?;
        if !fresh {
            return Err(CoordinatorError::Internal(format!(
                "write rejected: primary {} heartbeat stale at {}",
                region.as_str(),
                now_ms
            )));
        }
        if !lag.within_slo() {
            return Err(CoordinatorError::Internal(format!(
                "write rejected: primary {} lag breach r2={} d1={} kv={}",
                region.as_str(),
                lag.r2_seconds,
                lag.d1_seconds,
                lag.kv_seconds
            )));
        }
        Ok(())
    }

    fn evaluate(
        &self,
        primary: Region,
        now_ms: u64,
    ) -> Result<PromotionDecision, CoordinatorError> {
        // First check registration.
        {
            let guard = self
                .state
                .lock()
                .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
            if !guard.contains_key(primary.as_str()) {
                return Err(CoordinatorError::UnknownRegion { region: primary });
            }
        }
        let (primary_fresh, primary_lag) = self.region_health_in_guard(primary, now_ms)?;
        if primary_fresh && primary_lag.within_slo() {
            return Ok(PromotionDecision::KeepPrimary);
        }
        // Primary unhealthy. Scan replicas in deterministic ALL order.
        for replica in Region::ALL {
            if replica == primary {
                continue;
            }
            // Replica must be registered.
            let registered = {
                let guard = self
                    .state
                    .lock()
                    .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
                guard.contains_key(replica.as_str())
            };
            if !registered {
                continue;
            }
            let (replica_fresh, replica_lag) = self.region_health_in_guard(replica, now_ms)?;
            if replica_fresh && replica_lag.within_slo() {
                return Ok(PromotionDecision::PromoteReplica { replica });
            }
        }
        Ok(PromotionDecision::NoEligibleReplica)
    }

    fn promote(
        &self,
        primary: Region,
        replica: Region,
        now_ms: u64,
    ) -> Result<(), CoordinatorError> {
        // SINGLETON LOCK — held across the entire promotion sequence.
        // INV-FAILOVER-NO-SPLIT-BRAIN: any concurrent caller blocks here.
        let mut guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;

        // Refuse self-promote (no-op or programming error — fail-CLOSED).
        if primary == replica {
            return Err(CoordinatorError::Internal(format!(
                "promote rejected: primary and replica are the same region ({})",
                primary.as_str()
            )));
        }

        // Both regions must be registered.
        if !guard.contains_key(primary.as_str()) {
            return Err(CoordinatorError::UnknownRegion { region: primary });
        }
        if !guard.contains_key(replica.as_str()) {
            return Err(CoordinatorError::UnknownRegion { region: replica });
        }

        // INV-FAILOVER-NO-SPLIT-BRAIN: at most one Primary at any instant.
        // The CURRENT primary must equal `primary` (the caller-specified
        // demotion target). Any other region holding `Primary` simultaneously
        // is a split-brain violation.
        let current_primary = Self::primary_in_guard(&guard);
        match current_primary {
            Some(r) if r == primary => {}
            Some(r) => {
                return Err(CoordinatorError::SplitBrainRejected {
                    candidate: replica,
                    existing_primary: r,
                })
            }
            None => {
                // No primary at all — allow promotion (recovery from a
                // partition where no region holds the role).
            }
        }

        // Anti-flap: if primary is STILL eligible, refuse promotion.
        let (primary_fresh, primary_lag) = self.region_health_in_guard(primary, now_ms)?;
        if primary_fresh && primary_lag.within_slo() {
            return Err(CoordinatorError::PrimaryStillEligible { primary });
        }

        // Replica must be eligible (fresh heartbeat + within SLO).
        let (replica_fresh, replica_lag) = self.region_health_in_guard(replica, now_ms)?;
        if !replica_fresh || !replica_lag.within_slo() {
            return Err(CoordinatorError::NoEligibleReplica { primary });
        }

        // Audit-emit-BEFORE-mutation (fail-CLOSED).
        // 1) demoted
        self.audit
            .emit(CoordinatorAuditRecord {
                event_type: CoordinatorAuditEventType::RegionDemoted,
                region: primary.as_str().to_owned(),
                previous_primary: primary.as_str().to_owned(),
                timestamp_ms: now_ms,
                detail: format!(
                    "demoted_to=hot_standby reason=heartbeat_or_lag_breach \
                     r2_lag={} d1_lag={} kv_lag={} heartbeat_fresh={}",
                    primary_lag.r2_seconds,
                    primary_lag.d1_seconds,
                    primary_lag.kv_seconds,
                    primary_fresh
                ),
            })
            .map_err(CoordinatorError::Audit)?;

        // 2) promoted
        self.audit
            .emit(CoordinatorAuditRecord {
                event_type: CoordinatorAuditEventType::RegionPromoted,
                region: replica.as_str().to_owned(),
                previous_primary: primary.as_str().to_owned(),
                timestamp_ms: now_ms,
                detail: format!(
                    "promoted_from=replica r2_lag={} d1_lag={} kv_lag={}",
                    replica_lag.r2_seconds, replica_lag.d1_seconds, replica_lag.kv_seconds
                ),
            })
            .map_err(CoordinatorError::Audit)?;

        // Audits OK — now safe to mutate.
        guard.insert(
            primary.as_str(),
            RegionRoleState {
                role: RegionRole::HotStandby,
                cooldown_started_ms: Some(now_ms),
            },
        );
        guard.insert(
            replica.as_str(),
            RegionRoleState {
                role: RegionRole::Primary,
                cooldown_started_ms: None,
            },
        );
        Ok(())
    }

    fn failback(&self, region: Region, now_ms: u64) -> Result<(), CoordinatorError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
        let state = guard
            .get(region.as_str())
            .ok_or(CoordinatorError::UnknownRegion { region })?
            .clone();

        if state.role != RegionRole::HotStandby {
            return Err(CoordinatorError::Internal(format!(
                "failback rejected: region {} role={:?} (not HotStandby)",
                region.as_str(),
                state.role
            )));
        }

        let cooldown_started = state.cooldown_started_ms.ok_or_else(|| {
            CoordinatorError::Internal(format!(
                "failback rejected: region {} has HotStandby role but no cooldown_started_ms",
                region.as_str()
            ))
        })?;

        let elapsed_ms = now_ms.saturating_sub(cooldown_started);
        let elapsed_seconds = elapsed_ms / 1_000;
        let required_seconds = HOT_STANDBY_COOLDOWN_SECONDS;

        if elapsed_seconds < required_seconds {
            let remaining = required_seconds - elapsed_seconds;
            // Audit emit a `failback_blocked` for forensic trail (best-effort:
            // even if THIS emit fails, the failback is still blocked — but we
            // surface the audit error to the caller for full fail-CLOSED).
            self.audit
                .emit(CoordinatorAuditRecord {
                    event_type: CoordinatorAuditEventType::FailbackBlocked,
                    region: region.as_str().to_owned(),
                    previous_primary: region.as_str().to_owned(),
                    timestamp_ms: now_ms,
                    detail: format!(
                        "cooldown_elapsed_s={} cooldown_remaining_s={}",
                        elapsed_seconds, remaining
                    ),
                })
                .map_err(CoordinatorError::Audit)?;
            return Err(CoordinatorError::CooldownNotElapsed {
                region,
                elapsed_seconds,
                remaining_seconds: remaining,
            });
        }

        // Eligible failback — demote current primary to Replica + audit.
        let current_primary = Self::primary_in_guard(&guard);

        // Audit-emit-BEFORE-mutation
        self.audit
            .emit(CoordinatorAuditRecord {
                event_type: CoordinatorAuditEventType::FailbackCommitted,
                region: region.as_str().to_owned(),
                previous_primary: current_primary
                    .map(|r| r.as_str().to_owned())
                    .unwrap_or_default(),
                timestamp_ms: now_ms,
                detail: format!(
                    "cooldown_elapsed_s={} (>= {})",
                    elapsed_seconds, required_seconds
                ),
            })
            .map_err(CoordinatorError::Audit)?;

        // Demote current primary (if any) to Replica.
        if let Some(prev) = current_primary {
            guard.insert(
                prev.as_str(),
                RegionRoleState {
                    role: RegionRole::Replica,
                    cooldown_started_ms: None,
                },
            );
        }
        // Promote `region` back to Primary.
        guard.insert(
            region.as_str(),
            RegionRoleState {
                role: RegionRole::Primary,
                cooldown_started_ms: None,
            },
        );
        Ok(())
    }

    fn replication_status(&self, now_ms: u64) -> Result<ReplicationStatus, CoordinatorError> {
        let guard = self
            .state
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("state lock poisoned: {e}")))?;
        let mut regions = Vec::with_capacity(Region::ALL.len());
        let mut primary_count = 0_usize;
        let mut any_unhealthy_primary = false;
        for region in Region::ALL {
            let Some(state) = guard.get(region.as_str()) else {
                continue;
            };
            let hb = self.heartbeats.latest(region)?;
            let (fresh, lag) = match hb {
                Some(h) => (h.is_fresh(now_ms), h.lag),
                None => (false, LagBundle::zero()),
            };
            let within = lag.within_slo();
            if state.role == RegionRole::Primary {
                primary_count += 1;
                if !fresh || !within {
                    any_unhealthy_primary = true;
                }
            }
            regions.push(RegionStatus {
                region,
                role: state.role,
                heartbeat_fresh: fresh,
                lag,
                within_slo: within,
                cooldown_started_ms: state.cooldown_started_ms,
            });
        }
        let healthy = primary_count == 1 && !any_unhealthy_primary;
        Ok(ReplicationStatus {
            regions,
            healthy,
            timestamp_ms: now_ms,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingCoordinatorAuditSink, InMemoryCoordinatorAuditSink};
    use crate::heartbeat::{Heartbeat, InMemoryHeartbeatRegistry};

    fn fixture() -> (
        Arc<InMemoryHeartbeatRegistry>,
        Arc<InMemoryCoordinatorAuditSink>,
        InMemoryReplicationCoordinator,
    ) {
        let hb = Arc::new(InMemoryHeartbeatRegistry::new());
        let audit = Arc::new(InMemoryCoordinatorAuditSink::new());
        let coord = InMemoryReplicationCoordinator::new(
            Arc::clone(&hb) as Arc<dyn HeartbeatRegistry>,
            Arc::clone(&audit) as Arc<dyn CoordinatorAuditSink>,
        );
        (hb, audit, coord)
    }

    #[test]
    fn register_and_route_write_happy_path() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        hb.record(Heartbeat::new(Region::Wnam, 1_000_000, LagBundle::zero()))?;
        coord.route_write(Region::Wnam, 1_001_000)?;
        Ok(())
    }

    #[test]
    fn route_write_to_replica_rejected() -> Result<(), CoordinatorError> {
        let (_hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Replica)?;
        let err = coord.route_write(Region::Wnam, 0).err();
        assert!(matches!(err, Some(CoordinatorError::Internal(_))));
        Ok(())
    }

    #[test]
    fn register_split_brain_rejected() -> Result<(), CoordinatorError> {
        let (_hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        let err = coord.register(Region::Enam, RegionRole::Primary).err();
        assert!(matches!(
            err,
            Some(CoordinatorError::SplitBrainRejected { .. })
        ));
        Ok(())
    }

    #[test]
    fn evaluate_keep_primary_when_healthy() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        hb.record(Heartbeat::new(Region::Wnam, 1_000_000, LagBundle::zero()))?;
        let decision = coord.evaluate(Region::Wnam, 1_001_000)?;
        assert_eq!(decision, PromotionDecision::KeepPrimary);
        Ok(())
    }

    #[test]
    fn evaluate_promotes_when_primary_stale_and_replica_healthy() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        // Wnam stale (no heartbeat / old).
        hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
        hb.record(Heartbeat::new(
            Region::Enam,
            1_000_000_000,
            LagBundle::zero(),
        ))?;
        let decision = coord.evaluate(Region::Wnam, 1_000_001_000)?;
        // Note: Enam heartbeat is fresh at now=1_000_001_000 (only 1s old),
        // Wnam is stale.
        assert_eq!(
            decision,
            PromotionDecision::PromoteReplica {
                replica: Region::Enam
            }
        );
        Ok(())
    }

    #[test]
    fn evaluate_no_eligible_replica_when_all_breach() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        // Both stale.
        hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
        hb.record(Heartbeat::new(Region::Enam, 0, LagBundle::zero()))?;
        let decision = coord.evaluate(Region::Wnam, 999_999_999)?;
        assert_eq!(decision, PromotionDecision::NoEligibleReplica);
        Ok(())
    }

    #[test]
    fn promote_emits_two_audits_before_mutation() -> Result<(), CoordinatorError> {
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
        let records = audit.records();
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].event_type,
            CoordinatorAuditEventType::RegionDemoted
        );
        assert_eq!(
            records[1].event_type,
            CoordinatorAuditEventType::RegionPromoted
        );
        // Verify role flip happened.
        let map = coord.role_map()?;
        assert_eq!(map.get("wnam").copied(), Some(RegionRole::HotStandby));
        assert_eq!(map.get("enam").copied(), Some(RegionRole::Primary));
        Ok(())
    }

    #[test]
    fn promote_fail_closed_when_audit_fails() -> Result<(), CoordinatorError> {
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
        // State UNCHANGED.
        let map = coord.role_map()?;
        assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
        assert_eq!(map.get("enam").copied(), Some(RegionRole::Replica));
        Ok(())
    }

    #[test]
    fn promote_anti_flap_when_primary_still_eligible() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        // Both healthy.
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
        let err = coord
            .promote(Region::Wnam, Region::Enam, 1_000_001_000)
            .err();
        assert!(matches!(
            err,
            Some(CoordinatorError::PrimaryStillEligible { .. })
        ));
        Ok(())
    }

    #[test]
    fn failback_cooldown_blocks_until_24h() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
        hb.record(Heartbeat::new(
            Region::Enam,
            1_000_000_000,
            LagBundle::zero(),
        ))?;
        coord.promote(Region::Wnam, Region::Enam, 1_000_001_000)?;
        // Just after promotion — failback blocked.
        let err = coord.failback(Region::Wnam, 1_000_002_000).err();
        assert!(matches!(
            err,
            Some(CoordinatorError::CooldownNotElapsed { .. })
        ));
        // After 24h - 1s — still blocked.
        let almost = 1_000_001_000 + (HOT_STANDBY_COOLDOWN_SECONDS - 1) * 1_000;
        let err = coord.failback(Region::Wnam, almost).err();
        assert!(matches!(
            err,
            Some(CoordinatorError::CooldownNotElapsed { .. })
        ));
        // After 24h exactly — allowed. Need heartbeat for primary candidate to
        // be observable in status, but failback itself doesn't gate on heartbeat.
        let after_24h = 1_000_001_000 + HOT_STANDBY_COOLDOWN_SECONDS * 1_000;
        coord.failback(Region::Wnam, after_24h)?;
        let map = coord.role_map()?;
        assert_eq!(map.get("wnam").copied(), Some(RegionRole::Primary));
        assert_eq!(map.get("enam").copied(), Some(RegionRole::Replica));
        Ok(())
    }

    #[test]
    fn replication_status_reflects_health() -> Result<(), CoordinatorError> {
        let (hb, _audit, coord) = fixture();
        coord.register(Region::Wnam, RegionRole::Primary)?;
        coord.register(Region::Enam, RegionRole::Replica)?;
        hb.record(Heartbeat::new(
            Region::Wnam,
            1_000_000_000,
            LagBundle::zero(),
        ))?;
        let status = coord.replication_status(1_000_001_000)?;
        assert!(status.healthy);
        assert_eq!(status.regions.len(), 2);
        // Now break the primary.
        hb.record(Heartbeat::new(Region::Wnam, 0, LagBundle::zero()))?;
        let status = coord.replication_status(1_000_001_000)?;
        assert!(!status.healthy);
        Ok(())
    }
}

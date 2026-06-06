//! Coordinator error taxonomy.
//!
//! `#[non_exhaustive]` per the charter; adding a variant does NOT break
//! downstream `match` statements (they MUST use `_ =>` arm).

use thiserror::Error;

use crate::Region;

/// Replication coordinator error taxonomy.
///
/// Every variant carries enough forensic context to drive runbook +
/// audit emit. All errors are **fail-CLOSED**: callers never silently
/// allow a write or a promotion through.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoordinatorError {
    /// Region not registered in the coordinator state map.
    #[error("region {region:?} not registered in coordinator state")]
    UnknownRegion {
        /// The region the caller referenced.
        region: Region,
    },

    /// No eligible replica found for promotion (every replica also
    /// breaches the SLO). Caller MUST escalate to the runbook —
    /// auto-promoting an unhealthy replica risks partition-split-brain.
    #[error("no eligible replica for promotion from {primary:?} (all replicas breach SLO)")]
    NoEligibleReplica {
        /// The primary that needed a replacement.
        primary: Region,
    },

    /// Concurrent promotion rejected — another region is already `Primary`.
    /// INV-FAILOVER-NO-SPLIT-BRAIN: the coordinator NEVER admits a
    /// second `Primary` concurrent with an existing one.
    #[error(
        "split-brain rejected: cannot promote {candidate:?}; \
         {existing_primary:?} is already Primary"
    )]
    SplitBrainRejected {
        /// The region the caller tried to promote.
        candidate: Region,
        /// The region currently holding the primary role.
        existing_primary: Region,
    },

    /// Primary is still eligible (heartbeat fresh + lag within SLO);
    /// promotion is refused to prevent flap-promotion.
    #[error("primary {primary:?} is still eligible; promotion refused")]
    PrimaryStillEligible {
        /// The region that the caller tried to demote.
        primary: Region,
    },

    /// Failback refused — the 24 h hot-standby cool-down has not elapsed.
    #[error(
        "failback refused for {region:?}: cool-down elapsed_seconds={elapsed_seconds} \
         remaining_seconds={remaining_seconds}"
    )]
    CooldownNotElapsed {
        /// The region the caller tried to fail back to.
        region: Region,
        /// How long the region has been in hot-standby, in seconds.
        elapsed_seconds: u64,
        /// How much longer the region must wait before failback, in seconds.
        remaining_seconds: u64,
    },

    /// Audit sink emit failed. State was NOT mutated (fail-CLOSED).
    #[error("coordinator audit emit failed: {0}")]
    Audit(String),

    /// Catch-all forensic surface — internal invariant violations.
    /// Production callers should never observe this; if they do, page
    /// SEV-2 and inspect the trace.
    #[error("coordinator internal error: {0}")]
    Internal(String),
}

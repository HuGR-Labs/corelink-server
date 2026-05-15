//! Region role state machine for the replication coordinator.
//!
//! Per `specs/03_architecture/resilience_patterns.md §Failback`, every region
//! is in exactly one of three roles at any instant:
//!
//! - [`RegionRole::Primary`]   — accepts writes; replicates to siblings.
//! - [`RegionRole::HotStandby`] — recently demoted primary; reads OK, writes
//!   blocked, MUST wait [`HOT_STANDBY_COOLDOWN_SECONDS`] (= 24 h) before it
//!   may be re-promoted via [`crate::ReplicationCoordinator::failback`].
//! - [`RegionRole::Replica`]   — passive replica receiving cross-region
//!   replication writes.
//!
//! INV-FAILOVER-NO-SPLIT-BRAIN: at any instant, **at most one** region is
//! `Primary` per tenant scope. The coordinator enforces this by holding a
//! singleton lock during any role mutation (modelled as per-instance
//! `Mutex` in [`crate::InMemoryReplicationCoordinator`]).

use serde::{Deserialize, Serialize};

/// Canonical hot-standby cool-down after a primary→hot-standby demotion.
///
/// Source: `specs/03_architecture/resilience_patterns.md §Failback` —
/// the demoted primary stays in `HotStandby` for 24 h before
/// [`crate::ReplicationCoordinator::failback`] may re-promote it. The
/// cool-down protects against flap-promotion when the previously-failing
/// region recovers transiently.
pub const HOT_STANDBY_COOLDOWN_SECONDS: u64 = 24 * 60 * 60;

/// Role of a region in the coordinator topology.
///
/// `#[non_exhaustive]` — adding a new role (e.g. `MaintenanceFreeze`)
/// requires an ADR + audit; this is enforced by the `#[non_exhaustive]`
/// attribute on the `match` exhaustiveness checks downstream.
///
/// # Examples
///
/// ```
/// use corelink_replication_coordinator::RegionRole;
///
/// assert_eq!(RegionRole::Primary.as_str(), "primary");
/// assert_eq!(RegionRole::HotStandby.as_str(), "hot_standby");
/// assert_eq!(RegionRole::Replica.as_str(), "replica");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RegionRole {
    /// Region accepts writes and replicates to siblings.
    Primary,
    /// Region was recently primary; reads allowed; writes blocked; cool-down active.
    HotStandby,
    /// Passive replica receiving cross-region replication.
    Replica,
}

impl RegionRole {
    /// Lowercase canonical string for metrics labels + CloudEvent payloads.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RegionRole::Primary => "primary",
            RegionRole::HotStandby => "hot_standby",
            RegionRole::Replica => "replica",
        }
    }

    /// Whether this role accepts writes.
    #[must_use]
    pub fn accepts_writes(self) -> bool {
        matches!(self, RegionRole::Primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_is_24h() {
        assert_eq!(HOT_STANDBY_COOLDOWN_SECONDS, 86_400);
    }

    #[test]
    fn only_primary_accepts_writes() {
        assert!(RegionRole::Primary.accepts_writes());
        assert!(!RegionRole::HotStandby.accepts_writes());
        assert!(!RegionRole::Replica.accepts_writes());
    }

    #[test]
    fn as_str_canonical() {
        assert_eq!(RegionRole::Primary.as_str(), "primary");
        assert_eq!(RegionRole::HotStandby.as_str(), "hot_standby");
        assert_eq!(RegionRole::Replica.as_str(), "replica");
    }
}

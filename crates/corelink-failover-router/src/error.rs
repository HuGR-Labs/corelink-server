//! `FailoverError` — `#[non_exhaustive]` error taxonomy for the failover router.

use crate::Region;
use thiserror::Error;

/// Error taxonomy for the failover router.
///
/// All variants are `#[non_exhaustive]` at the enum level — callers must
/// use a wildcard match arm per CoreLink codex §9.3.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::{FailoverError, Region};
///
/// let e = FailoverError::RegionDegraded {
///     region: Region::Weur,
///     routed_to: Some(Region::Sam),
/// };
/// assert!(e.to_string().contains("degraded"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FailoverError {
    /// Primary region is degraded; reads may be routed to sibling.
    #[error("region {region:?} degraded; routed_to={routed_to:?}")]
    RegionDegraded {
        /// Degraded primary region.
        region: Region,
        /// Sibling region reads were routed to (or `None` if no replica available).
        routed_to: Option<Region>,
    },

    /// Write attempted during failover read-only mode.
    ///
    /// Caller must return HTTP 503 + notify customer.
    #[error("write blocked during failover: region {region:?} is in read-only mode")]
    WriteBlockedDuringFailover {
        /// The region in failover read-only mode.
        region: Region,
    },

    /// No replica available for failover (sibling region also degraded).
    #[error("no replica available for region {region:?}")]
    NoReplicaAvailable {
        /// Region with no available sibling.
        region: Region,
    },

    /// Audit emit failed — fail-CLOSED: no state mutation.
    #[error("audit emit failed: {0}")]
    Audit(String),

    /// Internal error (lock poisoning, etc.).
    #[error("internal failover error: {0}")]
    Internal(String),
}

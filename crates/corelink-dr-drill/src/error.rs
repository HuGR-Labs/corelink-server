//! `DrillError` — `#[non_exhaustive]` error taxonomy for the DR drill crate.

use crate::Region;
use thiserror::Error;

/// Error taxonomy for the DR drill scheduler + simulator.
///
/// All variants are `#[non_exhaustive]` at the enum level — callers must
/// use a wildcard match arm per CoreLink codex §9.3.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::{DrillError, Region};
///
/// let e = DrillError::ProdEnvForbidden;
/// assert!(e.to_string().contains("production"));
///
/// let e = DrillError::NoSiblingAvailable { region: Region::Weur };
/// assert!(e.to_string().contains("sibling"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DrillError {
    /// DR drill attempted in a `Production` environment. Hard-fail at the
    /// canonical staging-only enforcement boundary (WI §1, Lote 10.17 P0).
    #[error("DR drill forbidden in production environment; staging-only at GA")]
    ProdEnvForbidden,

    /// Region identifier not recognized.
    #[error("region {region:?} is unknown to the residency graph")]
    RegionUnknown {
        /// The unknown region (string form; debug-only).
        region: String,
    },

    /// No sibling region available for failover (sibling also degraded).
    #[error("no sibling region available for {region:?}")]
    NoSiblingAvailable {
        /// Region with no available sibling.
        region: Region,
    },

    /// SLO violation: RTO or RPO exceeded the canonical ceiling.
    #[error("SLO violation: {slo} measured {measured_seconds}s exceeds ceiling {ceiling_seconds}s")]
    SloViolation {
        /// Which SLO was violated (`"rto"` or `"rpo"`).
        slo: &'static str,
        /// Measured value in seconds.
        measured_seconds: u64,
        /// Ceiling in seconds.
        ceiling_seconds: u64,
    },

    /// Internal error (lock poisoning, state machine inconsistency).
    #[error("internal DR drill error: {0}")]
    Internal(String),
}

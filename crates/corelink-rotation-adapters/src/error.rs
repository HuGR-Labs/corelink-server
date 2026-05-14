//! [`RotationError`] `#[non_exhaustive]` taxonomy for all rotation
//! failure modes (WI-S13-003 §1 RotationError).

use crate::types::{AssetClass, KeyState};

/// Canonical rotation error taxonomy.
///
/// `#[non_exhaustive]` per CoreLink codex (S-06 P0-2 lift): callers
/// MUST use a wildcard arm so future variants are forward-compatible.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RotationError {
    /// Downstream error rate during rotation exceeds 1% sustained 5
    /// minutes. PAT-ROLL-FORWARD-001 auto-rollback triggers.
    #[error(
        "downstream error rate {0:.4} exceeds 1% threshold; \
         PAT-ROLL-FORWARD-001 auto-rollback triggered"
    )]
    DownstreamErrorThreshold(f64),

    /// Key state transition is invalid per the canonical state machine
    /// (INV-KEY-NO-SKIP enforcement).
    #[error("key state transition invalid: {from:?} → {to:?}")]
    InvalidTransition {
        /// State before the attempted transition.
        from: KeyState,
        /// State that was attempted.
        to: KeyState,
    },

    /// Overlap window exceeds the hard upper bound of 30 days per
    /// `key_management.md §3.2.1`. An ADR + Security lead sign-off is
    /// required to override.
    #[error(
        "overlap window {seconds}s exceeds hard upper bound 30d \
         (ADR + Security lead sign-off required to override)"
    )]
    OverlapExceedsHardUpper {
        /// The requested overlap in seconds.
        seconds: u64,
    },

    /// A rotation is already in-flight for this (asset_class, region)
    /// pair. The D1 UNIQUE index on `(asset_class, region)` WHERE
    /// state='pending' prevents concurrent rotations.
    #[error("rotation in-flight (asset_class={0:?}); previous still in progress")]
    RotationInFlight(AssetClass),

    /// KMS / Cloudflare Workers Secrets error.
    #[error("KMS error: {0}")]
    Kms(String),

    /// D1 storage error.
    #[error("D1 storage error: {0}")]
    Storage(String),

    /// Audit outbox emit failed (INV-KEY-AUDIT; fail-CLOSED: state
    /// transition is aborted on audit failure).
    #[error("audit emit failed: {0}")]
    Audit(String),
}

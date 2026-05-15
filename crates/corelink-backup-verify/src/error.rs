//! `BackupVerifyError` taxonomy.

use thiserror::Error;

use crate::tier::BackupTier;

/// Error taxonomy for the daily backup-verification harness.
///
/// All variants are reachable via `#[non_exhaustive]`; callers MUST use a
/// wildcard match arm (CoreLink codex §9.3).
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackupVerifyError {
    /// No snapshot was found for the requested tier — backups never ran
    /// or the catalog is empty. Treated as fail-CLOSED (alert).
    #[error("no snapshot present for tier {tier:?}")]
    NoSnapshot {
        /// Tier missing.
        tier: BackupTier,
    },

    /// Sample-restore destination was non-ephemeral. The verifier MUST
    /// refuse any restore target other than `Namespace::Ephemeral`
    /// (`INV-BACKUP-RESTORE-EPHEMERAL`).
    #[error("sample-restore target must be ephemeral; got {namespace}")]
    NonEphemeralRestoreTarget {
        /// Debug form of the rejected namespace.
        namespace: String,
    },

    /// Caller asked for more integrity samples than the per-tenant cap.
    #[error(
        "integrity sample request {requested} per tenant exceeds cap \
         {cap} (INV-BACKUP-INTEGRITY-SAMPLE-CAP)"
    )]
    SampleCapExceeded {
        /// Requested count.
        requested: usize,
        /// Cap from [`crate::MAX_INTEGRITY_SAMPLES_PER_TENANT`].
        cap: usize,
    },

    /// Internal state-machine inconsistency (lock poisoning, etc.).
    #[error("internal backup-verify error: {0}")]
    Internal(String),
}

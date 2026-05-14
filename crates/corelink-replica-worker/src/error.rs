//! `ReplicaError` — `#[non_exhaustive]` error taxonomy for the replica worker.

use crate::region::Region;
use thiserror::Error;

/// Error taxonomy for the replica worker and offline aggregator.
///
/// All variants are `#[non_exhaustive]` at the enum level — callers must
/// use a wildcard match arm (`_ => ...`) per CoreLink codex §9.3.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::ReplicaError;
/// use corelink_replica_worker::Region;
///
/// let e = ReplicaError::HashMismatch {
///     expected: "abc123".to_owned(),
///     actual: "def456".to_owned(),
/// };
/// assert!(e.to_string().contains("hash mismatch"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReplicaError {
    /// Offline aggregation failed (DataFusion-style batch query error).
    #[error("offline aggregation failed: {0}")]
    Aggregation(String),

    /// R2 cross-region copy failed (network / R2 API error).
    #[error("R2 copy failed: {0}")]
    R2Copy(String),

    /// Hash mismatch post-replica — INV-CAS-INTEGRITY violation.
    ///
    /// Primary R2 ETag does not match replica R2 ETag after copy.
    /// Triggers retry (5 max); persistent mismatch = SEV-2 alert + audit.
    #[error("hash mismatch post-replica: expected={expected} actual={actual}")]
    HashMismatch {
        /// Expected hash (primary region ETag / BLAKE3).
        expected: String,
        /// Actual hash observed at replica region.
        actual: String,
    },

    /// Residency violation — INV-REGION-NO-CROSS-LEAK / Schrems II.
    ///
    /// Attempt to replicate to a non-allowed sibling region (e.g., WEUR → ENAM).
    #[error(
        "residency violation: primary_region={primary:?} replica_region={replica:?} \
         not in allowed sibling set (Schrems II / LGPD)"
    )]
    ResidencyViolation {
        /// Tenant's primary region.
        primary: Region,
        /// Attempted replica region (forbidden).
        replica: Region,
    },

    /// D1 storage error (read/write `hot_blobs` table).
    #[error("D1 storage error: {0}")]
    Storage(String),

    /// Audit emit failed — fail-CLOSED: state NOT mutated.
    #[error("audit emit failed: {0}")]
    Audit(String),

    /// All retry attempts exhausted for a blob replication.
    ///
    /// Triggers SEV-2 alert + audit emit `corelink.region.replication.failed`.
    #[error("retry exhausted after {attempts} attempts for blob {blob_hash}")]
    RetryExhausted {
        /// Number of attempts made.
        attempts: u32,
        /// Blob hash that failed.
        blob_hash: String,
    },
}

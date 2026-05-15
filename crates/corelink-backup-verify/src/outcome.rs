//! Verification outcome records.

use serde::{Deserialize, Serialize};

use crate::tier::BackupTier;

/// Top-level outcome status, used as the Prometheus metric label
/// `corelink_backup_verification_status{result=...}`.
///
/// `#[non_exhaustive]` — future variants (e.g. `Throttled`) may be added.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VerificationStatus {
    /// Freshness within RPO + every integrity sample matched + every
    /// restored object's content matched.
    Ok,
    /// Latest snapshot exceeds the per-tier RPO budget.
    Stale,
    /// At least one sampled object's backup hash did not match its live
    /// hash, OR the snapshot's manifest hash mismatched.
    Corrupt,
    /// Sample-restore byte comparison failed for ≥ 1 object.
    RestoreFailed,
}

impl VerificationStatus {
    /// Stable identifier for metric / log emission (matches Prom labels
    /// in `scripts/backup-daily-verify.sh`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            VerificationStatus::Ok => "ok",
            VerificationStatus::Stale => "stale",
            VerificationStatus::Corrupt => "corrupt",
            VerificationStatus::RestoreFailed => "restore_failed",
        }
    }

    /// Returns `true` iff the cycle passed (`Ok`).
    #[must_use]
    pub const fn is_ok(self) -> bool {
        matches!(self, VerificationStatus::Ok)
    }
}

/// Detailed verdict from `verify_integrity`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityVerdict {
    /// Number of samples drawn this cycle.
    pub sampled: usize,
    /// Number of samples that matched their backup hash.
    pub matched: usize,
    /// Number of samples whose backup hash mismatched (corruption).
    pub mismatched: usize,
    /// Number of samples not found in the snapshot manifest at all.
    pub missing: usize,
}

impl IntegrityVerdict {
    /// Returns `true` iff every sample matched and none was missing.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.mismatched == 0 && self.missing == 0
    }
}

/// Detailed verdict from `sample_restore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreVerdict {
    /// Number of objects restored this cycle (capped at
    /// [`crate::MAX_SAMPLE_RESTORE_OBJECTS`]).
    pub restored: usize,
    /// Number of objects whose restored bytes matched the live bytes.
    pub byte_matched: usize,
    /// Number of objects whose restored bytes did NOT match the live bytes.
    pub byte_mismatched: usize,
}

impl RestoreVerdict {
    /// Returns `true` iff every restored object's bytes matched.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.byte_mismatched == 0
    }
}

/// Full per-tier outcome record emitted by one daily verification cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationOutcome {
    /// Which tier this outcome covers.
    pub tier: BackupTier,
    /// Overall status (Prometheus label).
    pub status: VerificationStatus,
    /// Age in seconds of the most recent snapshot at the moment of check.
    pub snapshot_age_seconds: u64,
    /// Integrity verdict (always populated; empty `sampled=0` if no live
    /// catalog was available).
    pub integrity: IntegrityVerdict,
    /// Restore verdict (always populated; empty `restored=0` if no
    /// integrity-clean snapshot was available).
    pub restore: RestoreVerdict,
}

impl VerificationOutcome {
    /// Returns `true` iff the cycle passed (`status == Ok`).
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.status.is_ok()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn status_metric_labels_stable() {
        assert_eq!(VerificationStatus::Ok.as_str(), "ok");
        assert_eq!(VerificationStatus::Stale.as_str(), "stale");
        assert_eq!(VerificationStatus::Corrupt.as_str(), "corrupt");
        assert_eq!(VerificationStatus::RestoreFailed.as_str(), "restore_failed");
    }

    #[test]
    fn integrity_clean_iff_no_mismatch_or_missing() {
        let v = IntegrityVerdict {
            sampled: 10,
            matched: 10,
            mismatched: 0,
            missing: 0,
        };
        assert!(v.is_clean());

        let v = IntegrityVerdict {
            sampled: 10,
            matched: 9,
            mismatched: 1,
            missing: 0,
        };
        assert!(!v.is_clean());

        let v = IntegrityVerdict {
            sampled: 10,
            matched: 9,
            mismatched: 0,
            missing: 1,
        };
        assert!(!v.is_clean());
    }

    #[test]
    fn restore_clean_iff_no_byte_mismatch() {
        let v = RestoreVerdict {
            restored: 5,
            byte_matched: 5,
            byte_mismatched: 0,
        };
        assert!(v.is_clean());

        let v = RestoreVerdict {
            restored: 5,
            byte_matched: 4,
            byte_mismatched: 1,
        };
        assert!(!v.is_clean());
    }
}

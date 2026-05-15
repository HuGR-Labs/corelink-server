//! `BackupTier` + per-tier RPO budgets.

use serde::{Deserialize, Serialize};

/// The three backup tiers CoreLink ships at GA.
///
/// `#[non_exhaustive]` — additional tiers (Cold-Glacier, etc.) may be
/// added post-GA without breaking match arms.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BackupTier {
    /// R2 cold-tier object storage (CAS blobs + manifests).
    R2,
    /// D1 relational backups (auth / billing / audit metadata).
    D1,
    /// KV-namespace backups (feature flags, hot caches, light state).
    Kv,
}

impl BackupTier {
    /// Stable identifier for metric / log emission.
    ///
    /// Matches the Prometheus label values used by the daily script.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            BackupTier::R2 => "r2",
            BackupTier::D1 => "d1",
            BackupTier::Kv => "kv",
        }
    }
}

/// Per-tier RPO budget in seconds.
///
/// Snapshots older than this are `Stale` (see [`crate::VerificationStatus::Stale`]).
///
/// | Tier | RPO budget | Rationale                                        |
/// |------|-----------|---------------------------------------------------|
/// | R2   | 86_400 s  | Cold blobs; one-day budget per backup-daily.sh    |
/// | D1   | 21_600 s  | Auth/billing rows critical; 6h budget             |
/// | Kv   | 43_200 s  | Hot caches; 12h budget                            |
#[must_use]
pub const fn rpo_seconds(tier: BackupTier) -> u64 {
    match tier {
        BackupTier::R2 => 86_400,
        BackupTier::D1 => 21_600,
        BackupTier::Kv => 43_200,
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
    fn rpo_budgets_match_canonical_table() {
        assert_eq!(rpo_seconds(BackupTier::R2), 86_400);
        assert_eq!(rpo_seconds(BackupTier::D1), 21_600);
        assert_eq!(rpo_seconds(BackupTier::Kv), 43_200);
    }

    #[test]
    fn tier_stable_metric_labels() {
        assert_eq!(BackupTier::R2.as_str(), "r2");
        assert_eq!(BackupTier::D1.as_str(), "d1");
        assert_eq!(BackupTier::Kv.as_str(), "kv");
    }
}

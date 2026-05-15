//! Backup snapshot + blob sample records.

use serde::{Deserialize, Serialize};

use crate::tier::BackupTier;

/// Canonical hash length: BLAKE3-256 → 32 bytes hex-encoded → 64 chars.
pub const BLAKE3_HEX_LEN: usize = 64;

/// Namespace classification used by [`crate::BackupVerifier::sample_restore`].
///
/// `#[non_exhaustive]` — production-only / staging-only namespace variants
/// may be added post-GA without breaking match arms. `Ephemeral` is the
/// only variant a sample restore is allowed to write into (see
/// `INV-BACKUP-RESTORE-EPHEMERAL`).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Namespace {
    /// Long-lived production namespace. Restore MUST refuse this target.
    Production,
    /// Long-lived staging namespace. Restore MUST refuse this target.
    Staging,
    /// Ephemeral test namespace; the daily verifier tears it down after
    /// the cycle. Only legal sample-restore destination.
    Ephemeral,
}

/// A single backup snapshot record returned by the catalog listing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackupSnapshot {
    /// Which storage tier this snapshot covers.
    pub tier: BackupTier,
    /// UTC unix-seconds timestamp the snapshot was written.
    pub taken_at_unix_s: u64,
    /// Stable opaque identifier (date prefix + tier suffix recommended).
    pub snapshot_id: String,
    /// BLAKE3-256 hex digest of the manifest (`64` hex chars).
    pub manifest_hash_hex: String,
}

/// A single object sample used by integrity / restore verification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobSample {
    /// Tenant the blob belongs to (used for per-tenant sample caps).
    pub tenant_id: String,
    /// Stable content-addressed identifier.
    pub blob_id: String,
    /// BLAKE3-256 hex digest of the live blob (`64` hex chars).
    pub live_hash_hex: String,
    /// Size in bytes (informational; not used for verification).
    pub size_bytes: u64,
}

impl BackupSnapshot {
    /// Convenience constructor used by the in-memory fake and unit tests.
    #[must_use]
    pub fn new(
        tier: BackupTier,
        taken_at_unix_s: u64,
        snapshot_id: impl Into<String>,
        manifest_hash_hex: impl Into<String>,
    ) -> Self {
        Self {
            tier,
            taken_at_unix_s,
            snapshot_id: snapshot_id.into(),
            manifest_hash_hex: manifest_hash_hex.into(),
        }
    }
}

impl BlobSample {
    /// Convenience constructor.
    #[must_use]
    pub fn new(
        tenant_id: impl Into<String>,
        blob_id: impl Into<String>,
        live_hash_hex: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            blob_id: blob_id.into(),
            live_hash_hex: live_hash_hex.into(),
            size_bytes,
        }
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
    fn snapshot_clone_eq() {
        let s = BackupSnapshot::new(BackupTier::R2, 1_700_000_000, "snap-1", "a".repeat(64));
        let s2 = s.clone();
        assert_eq!(s, s2);
        assert_eq!(s.tier, BackupTier::R2);
        assert_eq!(s.taken_at_unix_s, 1_700_000_000);
        assert_eq!(s.manifest_hash_hex.len(), BLAKE3_HEX_LEN);
    }

    #[test]
    fn namespace_ephemeral_is_only_restore_target() {
        // Documentation guard — the trait contract is what enforces this at
        // call time; here we just assert the variant exists and is distinct.
        assert_ne!(Namespace::Ephemeral, Namespace::Production);
        assert_ne!(Namespace::Ephemeral, Namespace::Staging);
    }
}

//! `BackupVerifier` trait + in-memory deterministic fake.

use std::collections::HashMap;
use std::sync::Mutex;

use super::error::BackupVerifyError;
use super::outcome::{
    IntegrityVerdict, RestoreVerdict, VerificationOutcome, VerificationStatus,
};
use super::snapshot::{BackupSnapshot, BlobSample, Namespace};
use super::tier::{rpo_seconds, BackupTier};
use super::{MAX_INTEGRITY_SAMPLES_PER_TENANT, MAX_SAMPLE_RESTORE_OBJECTS};

/// Trait every concrete backup-verification handler implements.
///
/// The CF Worker handler (`corelink-backup-verify-handler`, deferred) will
/// implement this against real R2 / D1 / KV bindings + wrangler API. The
/// in-memory fake [`InMemoryBackupVerifier`] is used by tests + by the
/// `--dry-run` mode of `scripts/backup-daily-verify.sh`.
pub trait BackupVerifier {
    /// Verify the freshness of the most-recent snapshot for `tier`
    /// against the per-tier RPO budget at the supplied wall-clock
    /// `now_unix_s`.
    ///
    /// # Errors
    ///
    /// Returns [`BackupVerifyError::NoSnapshot`] if no snapshot exists.
    fn verify_freshness(
        &self,
        tier: BackupTier,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError>;

    /// Verify backup integrity by sampling up to
    /// [`MAX_INTEGRITY_SAMPLES_PER_TENANT`] objects per tenant from the
    /// live catalog and cross-validating against the latest snapshot.
    ///
    /// `samples_per_tenant` MUST be `≤ MAX_INTEGRITY_SAMPLES_PER_TENANT`.
    ///
    /// # Errors
    ///
    /// - [`BackupVerifyError::NoSnapshot`] if no snapshot exists.
    /// - [`BackupVerifyError::SampleCapExceeded`] if the requested per-tenant
    ///   count exceeds [`MAX_INTEGRITY_SAMPLES_PER_TENANT`].
    fn verify_integrity(
        &self,
        tier: BackupTier,
        samples_per_tenant: usize,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError>;

    /// Sample-restore up to [`MAX_SAMPLE_RESTORE_OBJECTS`] random objects
    /// into the supplied namespace and confirm content bytes match.
    /// `target` MUST be [`Namespace::Ephemeral`].
    ///
    /// # Errors
    ///
    /// - [`BackupVerifyError::NoSnapshot`] if no snapshot exists.
    /// - [`BackupVerifyError::NonEphemeralRestoreTarget`] if `target` is
    ///   not [`Namespace::Ephemeral`] (`INV-BACKUP-RESTORE-EPHEMERAL`).
    fn sample_restore(
        &self,
        tier: BackupTier,
        target: Namespace,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError>;
}

/// Internal state for one tier in the in-memory fake.
#[derive(Debug, Default)]
struct TierState {
    /// Latest snapshot for this tier (None if backup never ran).
    snapshot: Option<BackupSnapshot>,
    /// Backup manifest: `(tenant_id, blob_id) -> hash_hex_in_snapshot`.
    /// A missing key = the snapshot does not cover that object.
    /// A different hash than the live one = corruption.
    backup_hashes: HashMap<(String, String), String>,
    /// Live catalog: every object the live system currently exposes for
    /// this tier. Same key shape as `backup_hashes`.
    live_blobs: Vec<BlobSample>,
    /// Restored bytes: simulated bytes returned by the restore step,
    /// keyed by `(tenant_id, blob_id)`. If `None` for a key, restore is
    /// considered byte-matched against live (happy path). If `Some(b)`,
    /// the byte sequence is compared against `live_blobs[i].live_hash_hex`
    /// — equal hash → match; different → mismatch.
    restored_hashes: HashMap<(String, String), String>,
}

/// Deterministic in-memory backup verifier.
///
/// Intended for unit tests, property tests, and the dry-run path of the
/// daily script. NOT a production handler. Sample selection is fully
/// deterministic (sorted catalog order) so test outcomes never depend on
/// allocator order or HashMap iteration.
#[derive(Debug, Default)]
pub struct InMemoryBackupVerifier {
    tiers: Mutex<HashMap<BackupTier, TierState>>,
}

impl InMemoryBackupVerifier {
    /// Construct an empty verifier with no snapshots and no live blobs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the most-recent snapshot for `tier`.
    ///
    /// # Errors
    ///
    /// Returns [`BackupVerifyError::Internal`] if the internal lock is
    /// poisoned.
    pub fn record_snapshot(
        &self,
        snapshot: BackupSnapshot,
    ) -> Result<(), BackupVerifyError> {
        let mut guard = self.lock_state()?;
        let tier = snapshot.tier;
        let entry = guard.entry(tier).or_default();
        entry.snapshot = Some(snapshot);
        Ok(())
    }

    /// Register a `(tenant, blob) -> hash` entry in the snapshot manifest.
    ///
    /// # Errors
    ///
    /// Returns [`BackupVerifyError::Internal`] if the internal lock is poisoned.
    pub fn record_backup_hash(
        &self,
        tier: BackupTier,
        tenant_id: impl Into<String>,
        blob_id: impl Into<String>,
        hash_hex: impl Into<String>,
    ) -> Result<(), BackupVerifyError> {
        let mut guard = self.lock_state()?;
        let entry = guard.entry(tier).or_default();
        entry
            .backup_hashes
            .insert((tenant_id.into(), blob_id.into()), hash_hex.into());
        Ok(())
    }

    /// Register a live-catalog blob the verifier may sample from.
    ///
    /// # Errors
    ///
    /// Returns [`BackupVerifyError::Internal`] if the internal lock is poisoned.
    pub fn record_live_blob(
        &self,
        tier: BackupTier,
        sample: BlobSample,
    ) -> Result<(), BackupVerifyError> {
        let mut guard = self.lock_state()?;
        let entry = guard.entry(tier).or_default();
        entry.live_blobs.push(sample);
        Ok(())
    }

    /// Override the restored-bytes hash for a single key, simulating a
    /// restore-time byte mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`BackupVerifyError::Internal`] if the internal lock is poisoned.
    pub fn inject_restore_mismatch(
        &self,
        tier: BackupTier,
        tenant_id: impl Into<String>,
        blob_id: impl Into<String>,
        wrong_hash_hex: impl Into<String>,
    ) -> Result<(), BackupVerifyError> {
        let mut guard = self.lock_state()?;
        let entry = guard.entry(tier).or_default();
        entry
            .restored_hashes
            .insert((tenant_id.into(), blob_id.into()), wrong_hash_hex.into());
        Ok(())
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<BackupTier, TierState>>, BackupVerifyError>
    {
        self.tiers
            .lock()
            .map_err(|_| BackupVerifyError::Internal("tier state lock poisoned".into()))
    }
}

impl BackupVerifier for InMemoryBackupVerifier {
    fn verify_freshness(
        &self,
        tier: BackupTier,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError> {
        let guard = self.lock_state()?;
        let state = guard
            .get(&tier)
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let snap = state
            .snapshot
            .as_ref()
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let age = now_unix_s.saturating_sub(snap.taken_at_unix_s);
        let status = if age > rpo_seconds(tier) {
            VerificationStatus::Stale
        } else {
            VerificationStatus::Ok
        };
        Ok(VerificationOutcome {
            tier,
            status,
            snapshot_age_seconds: age,
            integrity: IntegrityVerdict {
                sampled: 0,
                matched: 0,
                mismatched: 0,
                missing: 0,
            },
            restore: RestoreVerdict {
                restored: 0,
                byte_matched: 0,
                byte_mismatched: 0,
            },
        })
    }

    fn verify_integrity(
        &self,
        tier: BackupTier,
        samples_per_tenant: usize,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError> {
        if samples_per_tenant > MAX_INTEGRITY_SAMPLES_PER_TENANT {
            return Err(BackupVerifyError::SampleCapExceeded {
                requested: samples_per_tenant,
                cap: MAX_INTEGRITY_SAMPLES_PER_TENANT,
            });
        }
        let guard = self.lock_state()?;
        let state = guard
            .get(&tier)
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let snap = state
            .snapshot
            .as_ref()
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let age = now_unix_s.saturating_sub(snap.taken_at_unix_s);

        // Deterministic: sort blobs by (tenant, blob_id); take first
        // `samples_per_tenant` per tenant.
        let mut by_tenant: HashMap<&str, Vec<&BlobSample>> = HashMap::new();
        for b in &state.live_blobs {
            by_tenant.entry(b.tenant_id.as_str()).or_default().push(b);
        }
        let mut tenants: Vec<&str> = by_tenant.keys().copied().collect();
        tenants.sort_unstable();

        let mut sampled = 0usize;
        let mut matched = 0usize;
        let mut mismatched = 0usize;
        let mut missing = 0usize;

        for t in tenants {
            // `by_tenant` is guaranteed to contain `t` because we built
            // `tenants` from its keys; fall back to an empty slice if
            // somehow absent (clippy::indexing_slicing-clean).
            let blobs = by_tenant.get(t).map(Vec::as_slice).unwrap_or(&[]);
            let mut blobs: Vec<&BlobSample> = blobs.to_vec();
            blobs.sort_unstable_by(|a, b| a.blob_id.cmp(&b.blob_id));
            for b in blobs.into_iter().take(samples_per_tenant) {
                sampled += 1;
                let key = (b.tenant_id.clone(), b.blob_id.clone());
                match state.backup_hashes.get(&key) {
                    Some(h) if h == &b.live_hash_hex => matched += 1,
                    Some(_) => mismatched += 1,
                    None => missing += 1,
                }
            }
        }

        let status = if age > rpo_seconds(tier) {
            VerificationStatus::Stale
        } else if mismatched > 0 || missing > 0 {
            VerificationStatus::Corrupt
        } else {
            VerificationStatus::Ok
        };

        Ok(VerificationOutcome {
            tier,
            status,
            snapshot_age_seconds: age,
            integrity: IntegrityVerdict {
                sampled,
                matched,
                mismatched,
                missing,
            },
            restore: RestoreVerdict {
                restored: 0,
                byte_matched: 0,
                byte_mismatched: 0,
            },
        })
    }

    fn sample_restore(
        &self,
        tier: BackupTier,
        target: Namespace,
        now_unix_s: u64,
    ) -> Result<VerificationOutcome, BackupVerifyError> {
        if target != Namespace::Ephemeral {
            return Err(BackupVerifyError::NonEphemeralRestoreTarget {
                namespace: format!("{target:?}"),
            });
        }
        let guard = self.lock_state()?;
        let state = guard
            .get(&tier)
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let snap = state
            .snapshot
            .as_ref()
            .ok_or(BackupVerifyError::NoSnapshot { tier })?;
        let age = now_unix_s.saturating_sub(snap.taken_at_unix_s);

        let mut blobs: Vec<&BlobSample> = state.live_blobs.iter().collect();
        blobs.sort_unstable_by(|a, b| {
            a.tenant_id
                .cmp(&b.tenant_id)
                .then_with(|| a.blob_id.cmp(&b.blob_id))
        });

        let mut restored = 0usize;
        let mut byte_matched = 0usize;
        let mut byte_mismatched = 0usize;
        for b in blobs.into_iter().take(MAX_SAMPLE_RESTORE_OBJECTS) {
            restored += 1;
            let key = (b.tenant_id.clone(), b.blob_id.clone());
            let restored_hash = state
                .restored_hashes
                .get(&key)
                .cloned()
                .unwrap_or_else(|| b.live_hash_hex.clone());
            if restored_hash == b.live_hash_hex {
                byte_matched += 1;
            } else {
                byte_mismatched += 1;
            }
        }

        let status = if age > rpo_seconds(tier) {
            VerificationStatus::Stale
        } else if byte_mismatched > 0 {
            VerificationStatus::RestoreFailed
        } else {
            VerificationStatus::Ok
        };

        Ok(VerificationOutcome {
            tier,
            status,
            snapshot_age_seconds: age,
            integrity: IntegrityVerdict {
                sampled: 0,
                matched: 0,
                mismatched: 0,
                missing: 0,
            },
            restore: RestoreVerdict {
                restored,
                byte_matched,
                byte_mismatched,
            },
        })
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

    fn fresh_snapshot(tier: BackupTier, now: u64) -> BackupSnapshot {
        BackupSnapshot::new(tier, now - 60, format!("snap-{}", tier.as_str()), "f".repeat(64))
    }

    #[test]
    fn freshness_ok_when_within_rpo() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        let out = v.verify_freshness(BackupTier::R2, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Ok);
        assert!(out.snapshot_age_seconds <= rpo_seconds(BackupTier::R2));
    }

    #[test]
    fn freshness_stale_when_beyond_rpo() {
        let v = InMemoryBackupVerifier::new();
        let now: u64 = 2_000_000_000;
        let snap = BackupSnapshot::new(
            BackupTier::D1,
            now - rpo_seconds(BackupTier::D1) - 1,
            "snap-stale",
            "0".repeat(64),
        );
        v.record_snapshot(snap).expect("ok");
        let out = v.verify_freshness(BackupTier::D1, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Stale);
    }

    #[test]
    fn freshness_no_snapshot_errors() {
        let v = InMemoryBackupVerifier::new();
        let err = v.verify_freshness(BackupTier::Kv, 100).unwrap_err();
        assert_eq!(err, BackupVerifyError::NoSnapshot { tier: BackupTier::Kv });
    }

    #[test]
    fn integrity_clean_when_all_hashes_match() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        for i in 0..3 {
            let h = format!("{:0>64}", i);
            v.record_live_blob(
                BackupTier::R2,
                BlobSample::new("t1", format!("b{i}"), h.clone(), 1024),
            )
            .expect("ok");
            v.record_backup_hash(BackupTier::R2, "t1", format!("b{i}"), h).expect("ok");
        }
        let out = v.verify_integrity(BackupTier::R2, 100, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Ok);
        assert!(out.integrity.is_clean());
        assert_eq!(out.integrity.sampled, 3);
    }

    #[test]
    fn integrity_corrupt_when_hash_mismatch() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        v.record_live_blob(
            BackupTier::R2,
            BlobSample::new("t1", "b1", "a".repeat(64), 1024),
        )
        .expect("ok");
        v.record_backup_hash(BackupTier::R2, "t1", "b1", "b".repeat(64))
            .expect("ok");
        let out = v.verify_integrity(BackupTier::R2, 1, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Corrupt);
        assert_eq!(out.integrity.mismatched, 1);
    }

    #[test]
    fn integrity_sample_cap_exceeded_rejects() {
        let v = InMemoryBackupVerifier::new();
        let err = v
            .verify_integrity(BackupTier::R2, MAX_INTEGRITY_SAMPLES_PER_TENANT + 1, 0)
            .unwrap_err();
        assert!(matches!(err, BackupVerifyError::SampleCapExceeded { .. }));
    }

    #[test]
    fn integrity_per_tenant_cap_enforced() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        // 10 blobs per tenant, 2 tenants
        for tenant in ["t1", "t2"] {
            for i in 0..10 {
                let h = format!("{:0>64}", i);
                v.record_live_blob(
                    BackupTier::R2,
                    BlobSample::new(tenant, format!("b{i}"), h.clone(), 1024),
                )
                .expect("ok");
                v.record_backup_hash(BackupTier::R2, tenant, format!("b{i}"), h)
                    .expect("ok");
            }
        }
        let out = v.verify_integrity(BackupTier::R2, 3, now).expect("ok");
        // 3 per tenant × 2 tenants = 6
        assert_eq!(out.integrity.sampled, 6);
        assert!(out.integrity.is_clean());
    }

    #[test]
    fn sample_restore_clean_returns_ok() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        for i in 0..7 {
            let h = format!("{:0>64}", i);
            v.record_live_blob(
                BackupTier::R2,
                BlobSample::new("t1", format!("b{i}"), h.clone(), 1024),
            )
            .expect("ok");
        }
        let out = v.sample_restore(BackupTier::R2, Namespace::Ephemeral, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Ok);
        assert_eq!(out.restore.restored, MAX_SAMPLE_RESTORE_OBJECTS);
        assert!(out.restore.is_clean());
    }

    #[test]
    fn sample_restore_byte_mismatch_returns_restore_failed() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        v.record_live_blob(
            BackupTier::R2,
            BlobSample::new("t1", "b1", "a".repeat(64), 1024),
        )
        .expect("ok");
        v.inject_restore_mismatch(BackupTier::R2, "t1", "b1", "z".repeat(64))
            .expect("ok");
        let out = v.sample_restore(BackupTier::R2, Namespace::Ephemeral, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::RestoreFailed);
        assert_eq!(out.restore.byte_mismatched, 1);
    }

    #[test]
    fn sample_restore_refuses_non_ephemeral_target() {
        let v = InMemoryBackupVerifier::new();
        let now = 2_000_000_000;
        v.record_snapshot(fresh_snapshot(BackupTier::R2, now)).expect("ok");
        let err = v
            .sample_restore(BackupTier::R2, Namespace::Production, now)
            .unwrap_err();
        assert!(matches!(
            err,
            BackupVerifyError::NonEphemeralRestoreTarget { .. }
        ));
        let err = v
            .sample_restore(BackupTier::R2, Namespace::Staging, now)
            .unwrap_err();
        assert!(matches!(
            err,
            BackupVerifyError::NonEphemeralRestoreTarget { .. }
        ));
    }

    #[test]
    fn integrity_stale_wins_over_corrupt_in_status() {
        let v = InMemoryBackupVerifier::new();
        let now: u64 = 2_000_000_000;
        // Stale snapshot.
        let snap = BackupSnapshot::new(
            BackupTier::R2,
            now - rpo_seconds(BackupTier::R2) - 1,
            "stale",
            "0".repeat(64),
        );
        v.record_snapshot(snap).expect("ok");
        v.record_live_blob(
            BackupTier::R2,
            BlobSample::new("t1", "b1", "a".repeat(64), 1024),
        )
        .expect("ok");
        v.record_backup_hash(BackupTier::R2, "t1", "b1", "b".repeat(64))
            .expect("ok");
        let out = v.verify_integrity(BackupTier::R2, 1, now).expect("ok");
        assert_eq!(out.status, VerificationStatus::Stale);
    }
}

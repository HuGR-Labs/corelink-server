//! Property tests for `corelink-backup-verify`.
//!
//! ## Test ladder (100 cases default; bump via PROPTEST_CASES env)
//!
//! 1. `prop_freshness_status_matches_rpo_boundary` — for every generated
//!    `(tier, snapshot_age, now)`, `verify_freshness` returns `Stale` iff
//!    `age > rpo_seconds(tier)` and `Ok` otherwise.
//! 2. `prop_integrity_clean_iff_all_hashes_match` — for any set of
//!    `(tenant, blob, hash)` triples where backup hash == live hash for
//!    every blob, `verify_integrity` returns `Ok` and `is_clean()`. If
//!    we mutate any single backup hash, the result is `Corrupt`.
//! 3. `prop_integrity_per_tenant_cap_honored` — `verify_integrity` never
//!    samples more than `samples_per_tenant` per tenant, regardless of
//!    catalog size.
//! 4. `prop_sample_restore_refuses_non_ephemeral` — for every non-`Ephemeral`
//!    namespace, `sample_restore` returns `NonEphemeralRestoreTarget`.
//! 5. `prop_sample_cap_rejects_when_above_cap` — requesting more samples
//!    than `MAX_INTEGRITY_SAMPLES_PER_TENANT` always errors.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use proptest::prelude::*;

use corelink_ops::dr::backup_verify::{
    rpo_seconds, BackupSnapshot, BackupTier, BackupVerifier, BackupVerifyError, BlobSample,
    InMemoryBackupVerifier, Namespace, VerificationStatus, MAX_INTEGRITY_SAMPLES_PER_TENANT,
};

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
}

fn arb_tier() -> impl Strategy<Value = BackupTier> {
    prop_oneof![
        Just(BackupTier::R2),
        Just(BackupTier::D1),
        Just(BackupTier::Kv),
    ]
}

fn arb_namespace_non_ephemeral() -> impl Strategy<Value = Namespace> {
    prop_oneof![Just(Namespace::Production), Just(Namespace::Staging)]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_freshness_status_matches_rpo_boundary(
        tier in arb_tier(),
        // Pick `now` large enough that subtraction never overflows.
        now in 200_000u64..10_000_000u64,
        age_seconds in 0u64..200_000u64,
    ) {
        let v = InMemoryBackupVerifier::new();
        let taken_at = now.saturating_sub(age_seconds);
        let snap = BackupSnapshot::new(tier, taken_at, "snap", "f".repeat(64));
        v.record_snapshot(snap).unwrap();
        let out = v.verify_freshness(tier, now).unwrap();
        let expected = if age_seconds > rpo_seconds(tier) {
            VerificationStatus::Stale
        } else {
            VerificationStatus::Ok
        };
        prop_assert_eq!(out.status, expected);
        prop_assert_eq!(out.snapshot_age_seconds, age_seconds);
    }

    #[test]
    fn prop_integrity_clean_iff_all_hashes_match(
        tier in arb_tier(),
        // 1..=8 blobs; small bounded vec keeps cases fast.
        blob_count in 1usize..=8,
        // single byte we mutate to introduce a mismatch; 0 means no mutation
        mutate_index in 0usize..=8,
    ) {
        let v = InMemoryBackupVerifier::new();
        let now: u64 = 2_000_000_000;
        let snap = BackupSnapshot::new(tier, now - 60, "snap", "f".repeat(64));
        v.record_snapshot(snap).unwrap();
        for i in 0..blob_count {
            let h = format!("{:0>64}", i);
            v.record_live_blob(
                tier,
                BlobSample::new("t1", format!("b{i}"), h.clone(), 1024),
            )
            .unwrap();
            // Mutate exactly one backup hash if mutate_index is in range.
            let backup_hash = if mutate_index < blob_count && mutate_index == i {
                format!("{:0>64}", i + 999)
            } else {
                h
            };
            v.record_backup_hash(tier, "t1", format!("b{i}"), backup_hash).unwrap();
        }
        let out = v.verify_integrity(tier, 100, now).unwrap();
        if mutate_index < blob_count {
            prop_assert_eq!(out.status, VerificationStatus::Corrupt);
            prop_assert!(!out.integrity.is_clean());
        } else {
            prop_assert_eq!(out.status, VerificationStatus::Ok);
            prop_assert!(out.integrity.is_clean());
        }
        prop_assert_eq!(out.integrity.sampled, blob_count);
    }

    #[test]
    fn prop_integrity_per_tenant_cap_honored(
        tier in arb_tier(),
        blobs_per_tenant in 1usize..=20,
        tenants in 1usize..=4,
        samples in 1usize..=5,
    ) {
        let v = InMemoryBackupVerifier::new();
        let now: u64 = 2_000_000_000;
        let snap = BackupSnapshot::new(tier, now - 60, "snap", "f".repeat(64));
        v.record_snapshot(snap).unwrap();
        for t_idx in 0..tenants {
            let tenant = format!("t{t_idx}");
            for i in 0..blobs_per_tenant {
                let h = format!("{:0>64}", i);
                v.record_live_blob(
                    tier,
                    BlobSample::new(&tenant, format!("b{i}"), h.clone(), 1024),
                )
                .unwrap();
                v.record_backup_hash(tier, &tenant, format!("b{i}"), h).unwrap();
            }
        }
        let out = v.verify_integrity(tier, samples, now).unwrap();
        let expected = tenants * samples.min(blobs_per_tenant);
        prop_assert_eq!(out.integrity.sampled, expected);
    }

    #[test]
    fn prop_sample_restore_refuses_non_ephemeral(
        tier in arb_tier(),
        ns in arb_namespace_non_ephemeral(),
    ) {
        let v = InMemoryBackupVerifier::new();
        let now: u64 = 2_000_000_000;
        let snap = BackupSnapshot::new(tier, now - 60, "snap", "f".repeat(64));
        v.record_snapshot(snap).unwrap();
        let err = v.sample_restore(tier, ns, now).unwrap_err();
        let is_ne = matches!(err, BackupVerifyError::NonEphemeralRestoreTarget { .. });
        prop_assert!(is_ne);
    }

    #[test]
    fn prop_sample_cap_rejects_when_above_cap(
        tier in arb_tier(),
        extra in 1usize..=200,
    ) {
        let v = InMemoryBackupVerifier::new();
        let requested = MAX_INTEGRITY_SAMPLES_PER_TENANT + extra;
        let err = v.verify_integrity(tier, requested, 0).unwrap_err();
        let is_cap = matches!(err, BackupVerifyError::SampleCapExceeded { .. });
        prop_assert!(is_cap);
    }
}

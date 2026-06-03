//! Cross-crate SLI-binding test — closes DR-16 wave-14 instrumentation
//! gap for `SLO-BACKUP-VERIFICATION` (slo_catalog.md §4.22).
//!
//! # Why this test
//!
//! `scripts/backup-daily-verify.sh` emits Prometheus rows under the
//! canonical name `corelink_backup_verification_status{tier,result}`.
//! That metric name is also encoded as `Sli::BackupVerification`'s
//! `prometheus_metric_base()` in `corelink-slo`. If either side renames
//! without updating the other, the dashboard panel + multi-burn-rate
//! alert evaluator + verifier script all break silently.
//!
//! This test pins the alignment at `cargo test` time so the failure
//! mode is a compile-time / test-time signal, not a silent metric drop.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_ops::dr::backup_verify::snapshot::BackupSnapshot;
use corelink_ops::dr::backup_verify::{
    outcome::VerificationStatus, verifier::BackupVerifier, BackupTier, InMemoryBackupVerifier,
    METRIC_BACKUP_VERIFICATION_STATUS,
};
use corelink_slo::Sli;

#[test]
fn backup_verification_metric_aligned_with_sli_taxonomy() {
    assert_eq!(
        METRIC_BACKUP_VERIFICATION_STATUS,
        Sli::BackupVerification.prometheus_metric_base()
    );
    assert_eq!(Sli::BackupVerification.slug(), "SLO-BACKUP-VERIFICATION");
}

#[test]
fn happy_path_verify_freshness_yields_ok_status() {
    // Happy-path: a fresh snapshot on each tier yields `VerificationStatus::Ok`,
    // which is the `result="ok"` label that drives the SLI numerator
    // (`# daily verification cycles passing all tiers / # attempted` per
    // `slo_catalog.md §4.22`). The SLI emit point is the daily cron in
    // `scripts/backup-daily-verify.sh` — here we assert the in-memory
    // verifier path that the cron's `--dry-run` mode exercises produces
    // the same canonical-status discriminator.
    let v = InMemoryBackupVerifier::new();
    let now: u64 = 2_000_000_000;
    for tier in [BackupTier::R2, BackupTier::D1, BackupTier::Kv] {
        let snap = BackupSnapshot::new(
            tier,
            now - 60,
            format!("snap-{}", tier.as_str()),
            "f".repeat(64),
        );
        v.record_snapshot(snap).expect("record snapshot");
        let out = v.verify_freshness(tier, now).expect("verify freshness");
        assert_eq!(out.status, VerificationStatus::Ok);
        // Canonical metric label discriminator emitted by the daily cron.
        assert_eq!(out.status.as_str(), "ok");
    }
    // The taxonomy binding remains stable across the happy path.
    assert_eq!(
        METRIC_BACKUP_VERIFICATION_STATUS,
        Sli::BackupVerification.prometheus_metric_base()
    );
}

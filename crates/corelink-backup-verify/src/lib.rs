//! `corelink-backup-verify` — Continuous (daily) backup-verification harness
//! (R-prep, complements GAP-15 cold-restore drill).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous-execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic skeleton**
//! of the daily verification harness that the `scripts/backup-daily-verify.sh`
//! driver and the future CF Worker handler will satisfy.
//!
//! The cold-restore drill (`scripts/cold-restore-drill.sh`) executes only
//! on its schedule (quarterly cycle 1; ad-hoc on demand). Between drill
//! cycles, silent backup corruption (storage rot, GPG-recipient drift,
//! checksum mismatch, key rotation breakage) can accumulate undetected.
//! This crate models the **continuous** verification cycle that catches
//! those failure modes within one day:
//!
//! 1. [`verify_freshness`](BackupVerifier::verify_freshness) — every
//!    backup tier carries an RPO budget (R2: 24h, D1: 6h, KV: 12h).
//!    A snapshot older than `now - rpo_seconds(tier)` is `Stale`.
//!
//! 2. [`verify_integrity`](BackupVerifier::verify_integrity) — sample up
//!    to [`MAX_INTEGRITY_SAMPLES_PER_TENANT`] random object hashes per
//!    tenant from the live catalog and cross-validate them against the
//!    most recent backup snapshot. Any mismatch is `Corrupt`.
//!
//! 3. [`sample_restore`](BackupVerifier::sample_restore) — restore up to
//!    [`MAX_SAMPLE_RESTORE_OBJECTS`] random objects into an ephemeral
//!    dev namespace and confirm content bytes match.  Any byte mismatch
//!    is `RestoreFailed`.
//!
//! # Crate contents
//!
//! - [`tier`] module — [`BackupTier`] (`R2` / `D1` / `KV`) +
//!   [`rpo_seconds`](tier::rpo_seconds) per-tier RPO budgets.
//! - [`snapshot`] module — [`BackupSnapshot`] + [`BlobSample`].
//! - [`outcome`] module — [`VerificationOutcome`] +
//!   [`VerificationStatus`] + [`IntegrityVerdict`] + [`RestoreVerdict`].
//! - [`verifier`] module — [`BackupVerifier`] trait +
//!   [`InMemoryBackupVerifier`] fake (used by every test + by the script
//!   harness in `--dry-run` mode).
//! - [`error`] module — [`BackupVerifyError`] taxonomy.
//!
//! # Invariants enforced
//!
//! - **INV-BACKUP-FRESH** — `verify_freshness` rejects any snapshot whose
//!   `age_seconds > rpo_seconds(tier)`.
//! - **INV-BACKUP-INTEGRITY-SAMPLE-CAP** — `verify_integrity` never
//!   samples more than [`MAX_INTEGRITY_SAMPLES_PER_TENANT`] per tenant
//!   per cycle (avoids accidental O(catalog) hot loops).
//! - **INV-BACKUP-RESTORE-EPHEMERAL** — `sample_restore` always tags the
//!   restored namespace `ephemeral = true`; the trait contract forbids
//!   restoring into a production-named namespace (real handler enforces
//!   via naming guard; in-memory fake enforces via `Namespace::Ephemeral`).
//!
//! # Real handler deferral
//!
//! The real `BackupVerifier` against R2 + D1 + KV + wrangler binding is
//! deferred to `corelink-backup-verify-handler` (future crate). This
//! crate provides only the contract + in-memory fake, exercised by the
//! daily script's `--dry-run` lane.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod error;
pub mod outcome;
pub mod snapshot;
pub mod tier;
pub mod verifier;

pub use error::BackupVerifyError;
pub use outcome::{IntegrityVerdict, RestoreVerdict, VerificationOutcome, VerificationStatus};
pub use snapshot::{BackupSnapshot, BlobSample, Namespace};
pub use tier::{rpo_seconds, BackupTier};
pub use verifier::{BackupVerifier, InMemoryBackupVerifier};

/// Maximum number of integrity-check samples drawn per tenant per cycle.
///
/// Cap from charter `INV-BACKUP-INTEGRITY-SAMPLE-CAP`. The real handler
/// MUST honor this ceiling regardless of tenant catalog size.
pub const MAX_INTEGRITY_SAMPLES_PER_TENANT: usize = 100;

/// Maximum number of objects sample-restored per cycle into the ephemeral
/// dev namespace.
///
/// Intentionally smaller than [`MAX_INTEGRITY_SAMPLES_PER_TENANT`] because
/// real restore is O(byte transfer) and the daily cycle has a 30-minute
/// wall-clock SLA.
pub const MAX_SAMPLE_RESTORE_OBJECTS: usize = 5;

/// Canonical Prometheus metric name emitted by each daily verification
/// cycle, satisfying `SLO-BACKUP-VERIFICATION` (`slo_catalog.md §4.22`).
///
/// LOAD-BEARING:
///   - `scripts/backup-daily-verify.sh` emits gauge rows under this name.
///   - `corelink-slo::Sli::BackupVerification::prometheus_metric_base()`
///     returns this exact string (cross-crate alignment tested in
///     `tests/sli_binding.rs`).
///
/// Labels: `{tier, result}` (canonical four-value `result` discriminator
/// per [`crate::outcome::VerificationStatus::as_str`]). Renaming requires
/// touching both this constant and the SLI taxonomy in `corelink-slo`.
pub const METRIC_BACKUP_VERIFICATION_STATUS: &str = "corelink_backup_verification_status";

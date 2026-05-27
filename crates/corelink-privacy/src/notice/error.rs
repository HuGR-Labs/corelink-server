//! Canonical error taxonomy for `corelink-privacy-notice-emit`.
//!
//! `NoticeEmitterError` is the top-level error; sub-errors are nested
//! per the Lote 10.9-quinquies typed-error discipline (mirroring
//! `DsrError`, `ErasureWorkerError`).

use thiserror::Error;

/// Error returned by the audit sink [`super::audit::NoticeAuditSink::emit`].
/// Failure here MUST cause the caller to abort deploy (fail-CLOSED per AC-008).
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NoticeAuditSinkError {
    /// Audit infrastructure unavailable (R2 audit-`<region>` write failure).
    #[error("notice audit infrastructure unavailable: {reason}")]
    Infrastructure {
        /// Reason mnemonic for runbook + SEV-2 alert routing.
        reason: alloc::string::String,
    },
    /// Internal (e.g. Mutex poison in tests).
    #[error("notice audit internal error: {reason}")]
    Internal {
        /// Internal error reason.
        reason: alloc::string::String,
    },
}

/// Error returned by the store [`super::store::NoticeStateStore`].
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NoticeStoreError {
    /// No published notice version found (pre-GA bootstrap path).
    #[error("no published notice version found")]
    NotFound,
    /// Internal store failure.
    #[error("notice store internal error: {reason}")]
    Internal {
        /// Internal error reason.
        reason: alloc::string::String,
    },
}

/// Top-level error taxonomy for [`super::emitter::NoticeEmitter`].
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NoticeEmitterError {
    /// Audit emit failure — propagates fail-CLOSED (deploy aborted per AC-008).
    #[error("notice audit emit failed (deploy aborted): {0}")]
    Audit(#[from] NoticeAuditSinkError),
    /// Store lookup / mutation failure.
    #[error("notice store error: {0}")]
    Store(#[from] NoticeStoreError),
    /// Semver validation failure (CI hook pre-condition; emitter guards this
    /// at the trait surface).
    #[error("notice semver validation failed: {reason}")]
    SemverInvalid {
        /// Describes the violation.
        reason: alloc::string::String,
    },
    /// 3-locales sync violation: not all 3 canonical locales present in the
    /// publication request (CI hook enforces; emitter re-checks at runtime).
    #[error("3-locales sync violation: {reason}")]
    LocaleSyncViolation {
        /// Describes the missing locales.
        reason: alloc::string::String,
    },
    /// Native speaker review missing for a locale (metadata.yaml checkbox).
    #[error("native speaker review missing for locale: {locale}")]
    NativeSpeakerReviewMissing {
        /// The locale missing native speaker review.
        locale: alloc::string::String,
    },
    /// No bump detected (new version ≤ current published version — blocked
    /// by CI hook and re-checked at emitter runtime).
    #[error("no semver bump detected: new version {new} is not greater than current {current}")]
    NoBumpDetected {
        /// Current published version string.
        current: alloc::string::String,
        /// Candidate new version string.
        new: alloc::string::String,
    },
}

extern crate alloc;

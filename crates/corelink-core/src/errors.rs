//! Workspace-cross-cutting error types.
//!
//! `CoreError` is the chokepoint error that every audit-fail-CLOSED
//! path surfaces (see `corelink-audit::ports::AuditEmitter::emit`).
//! It is intentionally NARROW — per-context errors (e.g. CAS multipart
//! errors, BYOK envelope errors, tier-selection errors) stay in their
//! owning context crate. This enum captures only the small set of
//! failures that span ≥3 unrelated crates today: storage IO at the
//! audit-emit chokepoint, mutex poisoning at the chokepoint, and
//! configuration errors that surface from every context's bootstrap.

mod digest_parse;

pub use digest_parse::DigestParseError;

use thiserror::Error;

/// Workspace-cross-cutting error.
///
/// Map a context-specific error into this surface at the chokepoint
/// boundary (the audit emit path, the bootstrap config-load path,
/// etc.). NOT a catch-all — most errors should stay in their owning
/// context crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Backend storage (D1 batch / R2 / KV) refused or failed to
    /// persist a row that the audit-fail-CLOSED contract requires.
    /// MUST surface as 503-class to the customer; never silently
    /// success.
    #[error("audit / storage backend failure: {0}")]
    Storage(String),

    /// An internal `Mutex` was poisoned (a panic occurred while
    /// another thread held the lock). Production emitters should not
    /// reach this; surfaced for completeness so test fakes can fail
    /// loudly without an `unwrap`.
    #[error("internal mutex poisoned (test fake or hard fault)")]
    MutexPoisoned,

    /// Configuration is structurally invalid (missing required env
    /// var, malformed region token, etc.). Surfaces only from
    /// bootstrap paths; never from hot CAS / AC reads.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Failed to parse a [`crate::types::Digest`] from hex /
    /// length-validated bytes. Bridges to [`DigestParseError`] so
    /// callers that catch `CoreError` get a uniform surface.
    #[error("digest parse: {0}")]
    DigestParse(#[from] DigestParseError),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn storage_error_message_contains_detail() {
        let e = CoreError::Storage("d1 batch rolled back".into());
        let s = format!("{e}");
        assert!(s.contains("d1 batch rolled back"));
        assert!(s.contains("audit / storage"));
    }

    #[test]
    fn digest_parse_bridges_via_from() {
        let inner = DigestParseError::InvalidLength(17);
        let outer: CoreError = inner.into();
        assert!(matches!(outer, CoreError::DigestParse(_)));
    }
}

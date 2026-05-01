//! Error type surface for the `corelink-audit` crate.

use thiserror::Error;

/// All recoverable error variants produced by [`crate`] APIs.
///
/// The crate's design avoids panics by construction (clippy `panic = "deny"`,
/// `unwrap_used = "deny"`, `expect_used = "deny"`); every fallible API returns
/// a typed error so call sites can decide between propagating, logging, and
/// metric-tagging.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditError {
    /// JCS canonicalization or `serde_json`-side serialization failed.
    /// Should be unreachable for the canonical [`crate::AuthEvent`] envelope
    /// because every field type is JSON-compatible by construction; surfaced
    /// for forward-compat with custom data payloads added under the
    /// `#[non_exhaustive]` enum.
    #[error("audit JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    /// JCS canonicalization specifically reported an error (numeric range,
    /// non-finite float, etc.). Distinct from `Serialization` so chain-side
    /// metrics can isolate canonicalization-specific drift.
    #[error("audit JCS canonicalization failed: {0}")]
    Canonicalization(String),

    /// Caller attempted to construct a [`crate::PrincipalIdHash`] /
    /// [`crate::PatIdHash`] / [`crate::EmailHash`] from an empty input.
    /// Empty inputs hash to a well-known constant (`e3b0...` for SHA-256)
    /// and would collapse the pseudonymous space in audit queries; we
    /// reject at the constructor.
    #[error("audit hash newtype rejects empty input: {kind}")]
    EmptyHashInput {
        /// Which hash newtype rejected the input (`principal_id` /
        /// `pat_id` / `email`).
        kind: &'static str,
    },
}

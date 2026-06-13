//! [`TransparencyLogError`] — error taxonomy for the Rekor submission seam.

use thiserror::Error;

/// Transparency-log submission error taxonomy.
///
/// `#[non_exhaustive]` per the CoreLink codex: new variants can be added
/// without breaking downstream `match` arms.
///
/// Note the split between *construction* errors (deterministic, a bug in the
/// caller's entry — surfaced as `Err`) and *transport* errors
/// ([`TransparencyLogError::Transport`], a network/Rekor-availability fault —
/// folded into [`crate::WitnessOutcome::Degraded`] by [`crate::witness_or_degrade`]
/// per the ADR-0066 fail-OPEN rule).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TransparencyLogError {
    /// The proposed Rekor entry could not be canonicalized to JSON.
    #[error("rekor entry serialization error: {0}")]
    Serialization(String),

    /// The Rekor API returned a response that did not parse into a log entry
    /// (missing `logIndex`, absent inclusion proof, malformed body).
    #[error("rekor response parse error: {0}")]
    ResponseParse(String),

    /// The Rekor API rejected the proposed entry (e.g. HTTP 4xx — malformed
    /// signature / unsupported algorithm). This is a deterministic rejection,
    /// NOT a transient availability fault, so it is surfaced as `Err` rather
    /// than degraded.
    #[error("rekor rejected entry: {0}")]
    Rejected(String),

    /// A transient transport / availability fault talking to the public Rekor
    /// instance (DNS, TLS, timeout, 5xx). Per ADR-0066 the witness fails OPEN:
    /// [`crate::witness_or_degrade`] converts this into
    /// [`crate::WitnessOutcome::Degraded`] so it never reaches a caller's
    /// write path.
    #[error("rekor transport error: {0}")]
    Transport(String),
}

impl TransparencyLogError {
    /// Whether this error is a transient transport/availability fault that the
    /// fail-OPEN policy should degrade rather than propagate.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Transport(_))
    }
}

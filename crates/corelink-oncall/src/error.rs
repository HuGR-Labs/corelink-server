//! `corelink-oncall` canonical error taxonomy.

use thiserror::Error;

/// Canonical PagerDuty Schedule / Events API transport error. Hoisted
/// to a typed error so the ledger can map PagerDuty transport
/// failures to the canonical [`OncallError::PagerDuty`] arm without
/// stringifying the cause.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OncallPagerDutyError {
    /// PagerDuty HTTPS transport failure (Schedule API PUT or Events
    /// API v2 POST rejected / network timeout / 5xx). Production
    /// wiring sets this when the API call surface returns a non-2xx
    /// status.
    #[error("PagerDuty transport failure: {0}")]
    Transport(String),
    /// The targeted assignment / schedule entity was not present in
    /// PagerDuty (e.g. handoff target backup engineer not on roster).
    #[error("PagerDuty entity not found: {0}")]
    NotFound(String),
}

/// Canonical `corelink-oncall` error taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs (e.g. APAC
/// region forward / Twilio fallback / HR-policy integration).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OncallError {
    /// Rotation / shift input failed validation (e.g. shift duration
    /// exceeds 7-day cap, tier mismatch, overlap with prior shift).
    #[error("invalid rotation: {0}")]
    InvalidRotation(String),
    /// Audit-of-audit emit failed; the ledger path aborts per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern).
    /// Caller MUST NOT retry the ledger mutation without first
    /// resolving the audit failure.
    #[error("oncall audit emit failed: {0}")]
    Audit(String),
    /// PagerDuty Schedule / Events API transport failure. Production
    /// wiring sets this when the HTTPS call is rejected / times out /
    /// returns 5xx. The handoff is fail-CLOSED per WI §6.1.2 (the
    /// ledger refuses to mark the handoff as executed until PagerDuty
    /// confirms the mutation).
    #[error("PagerDuty client failure: {0}")]
    PagerDuty(#[from] OncallPagerDutyError),
    /// Internal invariant violation surfaced via `Mutex` poisoning or
    /// state corruption. Treat as a non-recoverable fault: the caller
    /// MUST tear down the ledger instance + reconstruct from the
    /// durable D1 mirror.
    #[error("internal oncall ledger fault: {0}")]
    Internal(String),
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
    fn pagerduty_error_to_oncall_error() {
        let pd = OncallPagerDutyError::Transport("502 bad gateway".to_string());
        let oc: OncallError = pd.into();
        assert!(matches!(oc, OncallError::PagerDuty(_)));
    }

    #[test]
    fn error_display_canonical() {
        let e = OncallError::InvalidRotation("dur > 7d".to_string());
        assert_eq!(e.to_string(), "invalid rotation: dur > 7d");
    }
}

//! Canonical [`EdgeError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07 +
//! S-08 lesson — the variant set grows additively across S-08 follow-on
//! WIs (suggest_block humane review queue, reconcile DO, sustained-abuse
//! detector) without breaking downstream callers.

use thiserror::Error;

use crate::edge::audit::EdgeAuditSinkError;
use crate::edge::cidr::CidrParseError;
use crate::edge::metrics::EdgeMetricsObserverError;

/// Canonical errors surfaced by [`crate::edge::policy::EdgePolicy`] and
/// [`crate::edge::blocklist::CidrBlocklist`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EdgeError {
    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The orchestrator rolls
    /// back the per-request blocklist mutation on this error per WI
    /// §6.1.8 fail-closed envelope.
    #[error("edge audit sink error: {0}")]
    Audit(#[from] EdgeAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s08.002.5 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("edge metrics observer error: {0}")]
    Metrics(#[from] EdgeMetricsObserverError),

    /// CIDR parse error — admin supplied a malformed CIDR literal at
    /// the `add_block` boundary. Mapped to 400 by handler.
    #[error("edge CIDR parse error: {0}")]
    CidrParse(String),

    /// CIDR overlap rejected — admin tried to add a network that
    /// overlaps an existing entry (one contains the other). Per WI
    /// §6.1.7 admin intent is ambiguous; admin MUST remove the
    /// broader entry first OR refuse the narrower add. Mapped to 409.
    #[error("edge CIDR overlap rejected: {existing}")]
    CidrOverlap {
        /// Canonical text of the existing entry that overlaps.
        existing: String,
    },

    /// Capacity ceiling reached — the in-memory blocklist is at its
    /// configured `max_size` ceiling. Per WI §6.1.6 the canonical CF
    /// List size limit is 10000; the in-memory mirror enforces the
    /// same hard ceiling. Mapped to 503 by handler.
    #[error("edge blocklist capacity reached: {current}/{max}")]
    CapacityReached {
        /// Current blocklist size.
        current: usize,
        /// Configured maximum.
        max: usize,
    },

    /// Backend transport failure (DO storage / D1 / fake mutex
    /// poisoning).
    #[error("edge backend error: {0}")]
    Backend(String),
}

impl From<CidrParseError> for EdgeError {
    fn from(value: CidrParseError) -> Self {
        Self::CidrParse(format!("{value}"))
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
    fn cidr_parse_carries_message() {
        let e: EdgeError = CidrParseError::MissingPrefix.into();
        let s = format!("{e}");
        assert!(s.contains("CIDR"));
    }

    #[test]
    fn overlap_carries_existing() {
        let e = EdgeError::CidrOverlap {
            existing: "192.0.2.0/24".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("192.0.2.0/24"));
    }

    #[test]
    fn capacity_carries_numbers() {
        let e = EdgeError::CapacityReached {
            current: 10000,
            max: 10000,
        };
        let s = format!("{e}");
        assert!(s.contains("10000"));
    }

    #[test]
    fn backend_carries_message() {
        let e = EdgeError::Backend("D1 unavailable".to_string());
        let s = format!("{e}");
        assert!(s.contains("D1 unavailable"));
    }
}

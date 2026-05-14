//! `corelink-synthetic-pager` canonical error taxonomy.

use thiserror::Error;

/// Canonical error taxonomy. The `#[non_exhaustive]` marker reserves
/// additive growth for follow-on WIs (e.g. APAC roll-forward,
/// dedicated maintenance-window arm).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntheticDrillError {
    /// Drill id failed shape validation (`SP-<UPPER ALNUM/HYPHEN>+`).
    #[error("invalid synthetic drill id: {0}")]
    InvalidDrillId(String),
    /// Engineer slug was empty on ack-record path.
    #[error("empty engineer slug")]
    EmptyEngineer,
    /// Emit timestamp is in the future relative to `now_ms` (clock
    /// skew guard).
    #[error("emit timestamp in future: emit={emit_ts_ms}, now={now_ms}")]
    EmitTimestampInFuture {
        /// Caller-supplied emit timestamp.
        emit_ts_ms: i64,
        /// Caller-supplied `now`.
        now_ms: i64,
    },
    /// Ack timestamp predates the emit timestamp (would yield negative
    /// MTTA).
    #[error("ack before emit: emit={emit_ts_ms}, ack={ack_ts_ms}")]
    AckBeforeEmit {
        /// Emit timestamp.
        emit_ts_ms: i64,
        /// Ack timestamp.
        ack_ts_ms: i64,
    },
    /// Recorder (D1 / in-memory) transport failure. Drill is fail-
    /// CLOSED: the dashboard MUST treat unrecorded drills as RED
    /// (missing-evidence) per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    #[error("synthetic drill recorder failure: {0}")]
    Recorder(String),
    /// Internal invariant violation (e.g. premature classification,
    /// state corruption). Treat as non-recoverable.
    #[error("internal synthetic drill fault: {0}")]
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
    fn display_canonical() {
        let e = SyntheticDrillError::InvalidDrillId("XX".to_string());
        assert_eq!(e.to_string(), "invalid synthetic drill id: XX");
    }

    #[test]
    fn emit_future_display() {
        let e = SyntheticDrillError::EmitTimestampInFuture {
            emit_ts_ms: 2_000,
            now_ms: 1_000,
        };
        assert_eq!(
            e.to_string(),
            "emit timestamp in future: emit=2000, now=1000"
        );
    }
}

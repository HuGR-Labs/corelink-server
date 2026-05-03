//! Canonical error taxonomy for the `corelink-audit-chain` emit + verify
//! surface.
//!
//! All variants `#[non_exhaustive]` so additive growth lands without
//! breaking downstream `match` sites (Lote 10.6bis discipline).

use thiserror::Error;

/// Audit-meta-sink failure surface (lifted into [`AuditChainError::Audit`]).
///
/// The audit-of-audit envelope (`corelink.audit_chain.{event_appended,
/// chain_verified_ok, chain_break_detected, sink_failure}`) emits BEFORE
/// state mutation per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` discipline.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditChainAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("audit-chain audit sink store error: {0}")]
    Store(String),
}

/// R2 sink failure surface (lifted into [`AuditChainError::Sink`]).
///
/// Production wiring fail-CLOSED per Lote 10.6bis lesson + WI §6.1.9 —
/// audit data integrity > availability; missing audit event = compliance
/// gap (4% global revenue regulatory fine risk per GDPR Art. 83).
/// Caller MUST abort the originating transaction on this error.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum R2AuditSinkError {
    /// Backend transport failure (R2 PutObject rejected / Object Lock
    /// retention enforcement / network partition). Production wiring
    /// records this as `corelink_audit_emit_failures_total{reason}`
    /// SEV-1 alert per WI §6.1.10 — fail-CLOSED canonical.
    #[error("audit-chain R2 sink backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::sink`] +
/// [`crate::chain`] + [`crate::verifier`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditChainError {
    /// JCS canonicalization (RFC 8785) failed for the event body
    /// (a non-finite float / non-string map key / etc.). Fail-CLOSED
    /// per audit data integrity priority.
    #[error("audit-chain JCS canonicalization failed: {0}")]
    Canonicalization(String),

    /// Audit-of-audit emit failure aborts the chain emit (fail-closed
    /// envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("audit-chain meta-audit emit failure: {0}")]
    Audit(#[from] AuditChainAuditSinkError),

    /// Underlying R2 sink transport failure (production CF R2 PutObject
    /// rejected / Object Lock retention / network). Fail-CLOSED canonical
    /// per Lote 10.6bis pattern; caller MUST abort the originating
    /// transaction.
    #[error("audit-chain R2 sink failure (fail-CLOSED; transaction abort): {0}")]
    Sink(#[from] R2AuditSinkError),

    /// Hash-chain integrity break detected by the verifier — the
    /// recomputed link hash at sequence `at_sequence` did not match the
    /// claimed link hash on the event. SEV-0 source: fires the
    /// `corelink_audit_chain_break_detected_total` counter per WI
    /// §6.1.10 + RB-AUDIT-CHAIN-001 runbook trigger.
    #[error(
        "audit-chain hash chain break at sequence {at_sequence} (tenant={tenant_id}); first divergence between claimed prev_hash and recomputed prev_hash"
    )]
    ChainBreak {
        /// Sequence number of the FIRST event whose recomputed
        /// `prev_hash` did not match the claimed `prev_hash`.
        at_sequence: u64,
        /// Tenant id of the broken chain (per-tenant chain isolation).
        tenant_id: String,
    },

    /// The verifier was given a slice of events whose sequence numbers
    /// are NOT strictly monotonic increasing by 1 starting from
    /// `expected_start`. Defensive guard against caller misuse; the
    /// production wiring streams events ordered by sequence so this
    /// surfaces only on adversarial / corrupted inputs.
    #[error(
        "audit-chain sequence ordering violation: expected start={expected_start}, observed={observed} at index={index}"
    )]
    SequenceOrderingViolation {
        /// Expected sequence number at the violating slice index.
        expected_start: u64,
        /// Sequence number actually observed at the violating slice index.
        observed: u64,
        /// Slice index where the violation was detected.
        index: usize,
    },

    /// Tenant-isolation violation: the verifier was handed a slice of
    /// events whose `tenant_id` is not uniform (per WI §6.1.4 each chain
    /// is per-tenant; cross-tenant correlation is via `trace_id` only).
    /// Defensive guard against caller misuse.
    #[error(
        "audit-chain tenant isolation violation: chain tenant={chain_tenant}, event tenant={event_tenant} at index={index}"
    )]
    TenantIsolationViolation {
        /// Tenant of the chain head (the first event in the slice).
        chain_tenant: String,
        /// Tenant of the violating event.
        event_tenant: String,
        /// Slice index where the violation was detected.
        index: usize,
    },

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// emit). Production wiring maps this to fail-CLOSED per WI §6.1.6 —
    /// the chain must abort rather than risk corrupt state.
    #[error("audit-chain internal state invariant violated: {0}")]
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
    fn canonicalization_displays_diagnostic() {
        let e = AuditChainError::Canonicalization("non-finite float".into());
        let s = format!("{e}");
        assert!(s.contains("JCS canonicalization"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = AuditChainAuditSinkError::Store("induced".to_string());
        let e: AuditChainError = inner.into();
        assert!(matches!(e, AuditChainError::Audit(_)));
    }

    #[test]
    fn from_sink_error_lifts_cleanly() {
        let inner = R2AuditSinkError::Backend("induced".to_string());
        let e: AuditChainError = inner.into();
        assert!(matches!(e, AuditChainError::Sink(_)));
    }

    #[test]
    fn chain_break_displays_diagnostic_with_seq() {
        let e = AuditChainError::ChainBreak {
            at_sequence: 42,
            tenant_id: "00000000-0000-0000-0000-00000000000a".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("42"));
        assert!(s.contains("hash chain break"));
    }

    #[test]
    fn sequence_ordering_violation_displays_diagnostic() {
        let e = AuditChainError::SequenceOrderingViolation {
            expected_start: 5,
            observed: 7,
            index: 2,
        };
        let s = format!("{e}");
        assert!(s.contains("sequence ordering"));
        assert!(s.contains("expected start=5"));
        assert!(s.contains("observed=7"));
    }

    #[test]
    fn tenant_isolation_violation_displays_diagnostic() {
        let e = AuditChainError::TenantIsolationViolation {
            chain_tenant: "tenant-a".to_string(),
            event_tenant: "tenant-b".to_string(),
            index: 1,
        };
        let s = format!("{e}");
        assert!(s.contains("tenant isolation"));
        assert!(s.contains("tenant-a"));
        assert!(s.contains("tenant-b"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = AuditChainError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}

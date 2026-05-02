//! Canonical [`QuotaCasError`] taxonomy surfaced by every fallible API
//! in the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07
//! lesson — the variant set grows additively across S-08 follow-on WIs
//! (production CF DO singleton + real D1 atomic batch in WI-S08-006)
//! without breaking downstream callers.

use thiserror::Error;
use uuid::Uuid;

use crate::audit::QuotaCasAuditSinkError;
use crate::metrics::QuotaCasMetricsObserverError;
use crate::state::AtomicCasStateError;

/// Canonical errors surfaced by [`crate::cas::AtomicQuotaChecker`] +
/// [`crate::state::AtomicCasState`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaCasError {
    /// `tenant_storage_state` row missing for the tenant + region.
    /// Either the row was never seeded (post-deploy backfill pending)
    /// OR a programmer wiring error misrouted the tenant. Mapped to 5xx
    /// by handler.
    #[error(
        "tenant_storage_state row missing for tenant={tenant_id} region={region}"
    )]
    TenantStorageStateMissing {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Region scope (canonical 5-region literal).
        region: &'static str,
    },

    /// CAS race detected: the tenant's `cas_version` advanced between
    /// the orchestrator's read and the orchestrator's write. The
    /// orchestrator retries up to `max_cas_attempts`; this arm is
    /// surfaced when retries are exhausted (pathological contention).
    #[error(
        "quota CAS race detected after {attempts} attempts (tenant={tenant_id})"
    )]
    CasRaceExhausted {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Number of attempts made before exhaustion.
        attempts: u32,
    },

    /// AtomicCasState backend failure (DO singleton / fake / D1 mirror).
    #[error("atomic CAS state error: {0}")]
    State(#[from] AtomicCasStateError),

    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The orchestrator rolls
    /// back the per-request CAS write on this error per WI §6.1.9
    /// fail-closed envelope.
    #[error("quota CAS audit sink error: {0}")]
    Audit(#[from] QuotaCasAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s08.003.10 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("quota CAS metrics observer error: {0}")]
    Metrics(#[from] QuotaCasMetricsObserverError),

    /// `request_bytes` overflowed `u64::MAX` when added to current
    /// `bytes_used` — mathematically impossible for any tenant under
    /// real workload, but surfaced explicitly for defensive
    /// programming.
    #[error("request_bytes overflow: bytes_used={bytes_used} + request_bytes={request_bytes}")]
    RequestBytesOverflow {
        /// Current used (snapshot).
        bytes_used: u64,
        /// Requested addition.
        request_bytes: u64,
    },

    /// Backend transport failure.
    #[error("quota CAS backend error: {0}")]
    Backend(String),
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
    fn tenant_storage_state_missing_format_carries_tenant_and_region() {
        let e = QuotaCasError::TenantStorageStateMissing {
            tenant_id: Uuid::nil(),
            region: "sam",
        };
        let s = format!("{e}");
        assert!(s.contains("tenant=00000000-0000-0000-0000-000000000000"));
        assert!(s.contains("region=sam"));
    }

    #[test]
    fn cas_race_exhausted_format_carries_attempts_and_tenant() {
        let id = Uuid::from_u128(0xdead_beef);
        let e = QuotaCasError::CasRaceExhausted {
            tenant_id: id,
            attempts: 3,
        };
        let s = format!("{e}");
        assert!(s.contains("3 attempts"));
        assert!(s.contains(&format!("{id}")));
    }

    #[test]
    fn request_bytes_overflow_format_carries_both_args() {
        let e = QuotaCasError::RequestBytesOverflow {
            bytes_used: u64::MAX,
            request_bytes: 1,
        };
        let s = format!("{e}");
        assert!(s.contains("bytes_used="));
        assert!(s.contains("request_bytes=1"));
    }
}

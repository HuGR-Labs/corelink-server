//! Canonical [`EvictionError`] taxonomy surfaced by every fallible API
//! in the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07
//! lesson — the variant set grows additively across S-07 follow-on WIs
//! without breaking downstream callers.

use thiserror::Error;
use uuid::Uuid;

use crate::audit::EvictionAuditSinkError;
use crate::metrics::EvictionMetricsObserverError;
use crate::storage_state::StorageStateError;

/// Canonical errors surfaced by [`crate::phase::EvictionPhase`] +
/// [`crate::trigger::should_fire_quota_trigger`] + the supporting
/// store traits.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EvictionError {
    /// CRITICAL — pre-evict reachable check found `active_refcount > 0`
    /// for the candidate blob (some `ac_meta.blob_refs` row created
    /// BEFORE `evict_started_at_ms` references it). Eviction REFUSED
    /// per WI §6.1 R-S07-3 INV-GC-001 inheritance. The handler treats
    /// this as a successful skip (NOT a 5xx); the metric
    /// `corelink.evict.skipped_reachable_total` is bumped instead.
    /// Surfaced as a typed error so the phase orchestrator can emit
    /// the `corelink.evict.skipped_reachable` audit event and continue
    /// with the next candidate.
    #[error("eviction refused: blob {digest_hex8} still reachable (active_refcount={active_refcount})")]
    BlobReachable {
        /// First-8-hex prefix of the candidate digest (privacy-friendly
        /// forensic trail; matches the GC sweep audit pattern).
        digest_hex8: String,
        /// Active refcount observed (always >= 1 when this fires).
        active_refcount: u32,
    },

    /// Caller invoked the eviction phase against a tenant whose
    /// `tenant_storage_state` row does not exist for the requested
    /// region. Either the row was never seeded (post-deploy backfill
    /// pending) OR a programmer wiring error misrouted the tenant.
    /// Mapped to 5xx by handler.
    #[error(
        "tenant_storage_state row missing for tenant={tenant_id} region={region}"
    )]
    TenantStorageStateMissing {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Region scope (canonical 5-region literal).
        region: &'static str,
    },

    /// `tenant_storage_state` backend (D1 / fake) error.
    #[error("tenant_storage_state store error: {0}")]
    StorageState(#[from] StorageStateError),

    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The phase orchestrator
    /// rolls back the per-candidate soft-delete on this error per
    /// WI §6.1.7 fail-closed envelope.
    #[error("eviction audit sink error: {0}")]
    Audit(#[from] EvictionAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s07.001.6 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("eviction metrics observer error: {0}")]
    Metrics(#[from] EvictionMetricsObserverError),

    /// Phase budget exceeded — eviction worker took longer than the
    /// configured ceiling. The phase orchestrator returns this when
    /// the per-candidate probe trips the deadline. Maps to 5xx with
    /// retry-after at the handler boundary.
    #[error(
        "eviction phase budget exceeded: duration_ms={duration_ms} > budget_ms={budget_ms}"
    )]
    PhaseBudgetExceeded {
        /// Duration observed at the moment the budget was checked.
        duration_ms: u64,
        /// Budget ceiling.
        budget_ms: u64,
    },

    /// Backend transport failure (D1 throttle / parse / cross-tenant
    /// injection caught at storage seam).
    #[error("eviction backend error: {0}")]
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
    fn reachable_error_carries_digest_prefix() {
        let e = EvictionError::BlobReachable {
            digest_hex8: "deadbeef".to_string(),
            active_refcount: 3,
        };
        let s = format!("{e}");
        assert!(s.contains("deadbeef"));
        assert!(s.contains("active_refcount=3"));
    }

    #[test]
    fn phase_budget_error_includes_both_durations() {
        let e = EvictionError::PhaseBudgetExceeded {
            duration_ms: 700,
            budget_ms: 500,
        };
        let s = format!("{e}");
        assert!(s.contains("duration_ms=700"));
        assert!(s.contains("budget_ms=500"));
    }
}

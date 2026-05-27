//! Canonical [`QuotaError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07
//! lesson — the variant set grows additively across S-07 follow-on WIs
//! without breaking downstream callers.

use thiserror::Error;
use uuid::Uuid;

use super::audit::QuotaAuditSinkError;
use super::metrics::QuotaMetricsObserverError;
use super::reservation::ReservationTrackerError;

/// Canonical errors surfaced by [`super::check::QuotaCheck`] +
/// [`super::reservation::ReservationTracker`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaError {
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

    /// `tenant_storage_state` backend (D1 / fake) error surfaced via
    /// `corelink-eviction::storage_state::StorageStateError`.
    #[error("tenant_storage_state store error: {0}")]
    StorageState(#[from] corelink_eviction::StorageStateError),

    /// Reservation tracker backend failure (DO singleton / fake / D1
    /// mirror).
    #[error("reservation tracker error: {0}")]
    Reservation(#[from] ReservationTrackerError),

    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The decision engine
    /// rolls back the per-request reservation insert on this error per
    /// WI §6.1 fail-closed envelope.
    #[error("quota audit sink error: {0}")]
    Audit(#[from] QuotaAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s07.003.10 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("quota metrics observer error: {0}")]
    Metrics(#[from] QuotaMetricsObserverError),

    /// Reservation lookup miss — either TTL expired before commit OR
    /// programmer wiring error.
    #[error("reservation {reservation_id} not found (TTL expired or never created)")]
    ReservationNotFound {
        /// Reservation id that was not found.
        reservation_id: Uuid,
    },

    /// Backend transport failure.
    #[error("quota backend error: {0}")]
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
        let e = QuotaError::TenantStorageStateMissing {
            tenant_id: Uuid::nil(),
            region: "sam",
        };
        let s = format!("{e}");
        assert!(s.contains("tenant=00000000-0000-0000-0000-000000000000"));
        assert!(s.contains("region=sam"));
    }

    #[test]
    fn reservation_not_found_format_carries_id() {
        let id = Uuid::from_u128(0xdead_beef);
        let e = QuotaError::ReservationNotFound { reservation_id: id };
        let s = format!("{e}");
        assert!(s.contains(&format!("{id}")));
    }
}

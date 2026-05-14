//! Error taxonomy for residency enforcement.
//!
//! All errors are `#[non_exhaustive]` per project convention.

use crate::Region;
use thiserror::Error;

/// Residency violation — returned when a request or write targets a region
/// that does not match `tenant.primary_region`.
///
/// Mapped to HTTP **451 `legal_residency_violation`** per PAT-ROUTING-PINNED-001
/// fail-CLOSED canonical (`resilience_patterns.md §3.4`).
///
/// The `remediation_url` field contains the correct custom domain
/// (`<tenant_id>.<expected>.corelink.dev`) for client retry.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ResidencyViolation {
    /// Request was routed to the wrong region.
    #[error(
        "Request region '{requested}' does not match tenant.primary_region '{expected}'. \
         Use <tenant_id>.{expected}.corelink.dev — Schrems II + LGPD Art. 33 §1º + GDPR Art. 44"
    )]
    RequestRegionMismatch {
        /// Tenant whose region was violated.
        tenant_id: String,
        /// Region the request arrived at.
        requested: Region,
        /// Canonical region for this tenant.
        expected: Region,
    },

    /// Backend write targeted the wrong region.
    #[error(
        "Write to backend '{backend}' in region '{target_region}' violates \
         tenant.primary_region '{expected}' — INV-DATA-RESIDENCY CRITICAL"
    )]
    WriteRegionMismatch {
        /// Tenant whose region was violated.
        tenant_id: String,
        /// Backend kind being written.
        backend: crate::BackendKind,
        /// Region the write targeted.
        target_region: Region,
        /// Canonical region for this tenant.
        expected: Region,
    },

    /// Audit emit failed; response must be 503 fail-CLOSED (not 451).
    ///
    /// Per AC-007: audit fail-CLOSED is distinct from the residency 451;
    /// when the audit infrastructure is unavailable the operation is held
    /// (503 + retry-after=60s) so that regulatory evidence is never lost.
    #[error("Audit emit failed — fail-CLOSED: operation suspended until audit available")]
    AuditEmitFailure {
        /// Underlying error message from the audit sink.
        reason: String,
    },

    /// Tenant not found in the store; cannot validate region.
    #[error("Tenant '{tenant_id}' not found — cannot enforce residency")]
    TenantNotFound {
        /// The tenant that was not found.
        tenant_id: String,
    },

    /// Region migration request is too soon (cooldown 30d per ADR-S11-011).
    #[error(
        "Region migration cooldown not elapsed: {days_remaining} days remaining \
         (privacy_model.md §7.2 + ADR-S11-011)"
    )]
    MigrationCooldownNotElapsed {
        /// Days remaining before migration is allowed.
        days_remaining: u64,
    },

    /// Region migration request is pending review.
    #[error("Region migration request '{ticket_id}' is pending Privacy Officer + Compliance review")]
    MigrationPendingReview {
        /// The migration ticket ID.
        ticket_id: String,
    },
}

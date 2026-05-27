//! Evidence stream taxonomy.
//!
//! Each CoreLink internal evidence source maps to exactly one Drata
//! evidence stream, which in turn maps to exactly one REST endpoint.

use serde::{Deserialize, Serialize};

/// Six-variant evidence routing enum. Additive growth requires bumping
/// the schema version on `corelink.compliance.drata_evidence_sent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EvidenceStream {
    /// `audit_outbox` D1 table → Drata audit-logs evidence stream.
    AuditLogs,
    /// `tenants` + RBAC change events → access-reviews stream.
    AccessReviews,
    /// PAT issuance + revocation → credential management stream.
    CredentialManagement,
    /// GitHub PR + merge events → change management stream.
    ChangeManagement,
    /// PagerDuty incident timeline → incident response stream.
    IncidentResponse,
    /// Pentest findings status → vulnerability management stream.
    VulnerabilityManagement,
}

impl EvidenceStream {
    /// Canonical Drata endpoint path. See lib.rs §"Drata API surface
    /// assumptions" — placeholder paths pending Drata public docs;
    /// swap-points are isolated here.
    #[must_use]
    pub const fn endpoint_path(self) -> &'static str {
        match self {
            Self::AuditLogs => "/v1/evidence/audit-logs",
            Self::AccessReviews => "/v1/evidence/access-reviews",
            Self::CredentialManagement => "/v1/evidence/credential-management",
            Self::ChangeManagement => "/v1/evidence/change-management",
            Self::IncidentResponse => "/v1/evidence/incident-response",
            Self::VulnerabilityManagement => "/v1/evidence/vulnerability-management",
        }
    }

    /// Stable canonical string used in JSON serialisation, audit
    /// envelopes, D1 column values, and SOC 2 TSC mapping.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::AuditLogs => "audit_logs",
            Self::AccessReviews => "access_reviews",
            Self::CredentialManagement => "credential_management",
            Self::ChangeManagement => "change_management",
            Self::IncidentResponse => "incident_response",
            Self::VulnerabilityManagement => "vulnerability_management",
        }
    }

    /// All variants — used by the runner to iterate every source on
    /// each daily tick.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::AuditLogs,
            Self::AccessReviews,
            Self::CredentialManagement,
            Self::ChangeManagement,
            Self::IncidentResponse,
            Self::VulnerabilityManagement,
        ]
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
    fn endpoint_paths_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in EvidenceStream::all() {
            assert!(seen.insert(s.endpoint_path()), "duplicate path");
        }
    }

    #[test]
    fn canonical_names_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in EvidenceStream::all() {
            assert!(seen.insert(s.canonical_name()), "duplicate name");
        }
    }

    #[test]
    fn endpoints_under_v1_prefix() {
        for s in EvidenceStream::all() {
            assert!(s.endpoint_path().starts_with("/v1/evidence/"));
        }
    }
}

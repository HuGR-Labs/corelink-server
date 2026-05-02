//! Rate-limit bucket key dimensions and the composite key surface.
//!
//! Per WI-S08-001 §6.1, S-08 ships **3 canonical key dimensions** so a
//! hot endpoint at one tenant cannot starve the rest of the platform
//! while still preserving INV-AVAIL-ISOLATION across tenants:
//!
//! - `PerTenant` — the canonical aggregate token bucket per tenant
//!   (camada 1; the FM-401 thundering-herd primary mitigation).
//! - `PerIp` — per-IP edge bucket (camada 2; per WI-S08-002 hand-off
//!   for adversarial IP floods).
//! - `PerTenantPerEndpoint` — per `(tenant, endpoint)` bucket for
//!   isolating a hot endpoint (e.g. `BatchUpdateBlobs`) from the rest
//!   of the tenant's traffic. Keeps INV-AVAIL-ISOLATION strict at the
//!   per-route level.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (per-PAT for
//! WI-S08-003 PAT misuse detection) can extend the dimension set
//! additively without breaking downstream sinks.

use uuid::Uuid;

/// Canonical 3-dimension list of bucket key dimensions.
///
/// The dimension drives both the PK column in the durable D1 mirror and
/// the per-instance HashMap key in the in-memory orchestrator. The
/// canonical mnemonic strings match the SQL `key_dimension` CHECK
/// constraint byte-for-byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum KeyDimension {
    /// `per_tenant` — global aggregate token bucket per tenant
    /// (camada 1; FM-401 thundering-herd primary mitigation).
    PerTenant,
    /// `per_ip` — per-IP edge bucket (camada 2).
    PerIp,
    /// `per_tenant_per_endpoint` — per `(tenant, endpoint)` bucket for
    /// hot endpoint isolation.
    PerTenantPerEndpoint,
}

impl KeyDimension {
    /// Canonical lower-snake-case mnemonic (matches the SQL CHECK
    /// literal byte-for-byte).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerTenant => "per_tenant",
            Self::PerIp => "per_ip",
            Self::PerTenantPerEndpoint => "per_tenant_per_endpoint",
        }
    }
}

impl core::fmt::Display for KeyDimension {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Pinned canonical 3-dimension list — for cross-component regression
/// tests + dashboard widget configuration.
pub const KEY_DIMENSION_LIST: [KeyDimension; 3] = [
    KeyDimension::PerTenant,
    KeyDimension::PerIp,
    KeyDimension::PerTenantPerEndpoint,
];

/// Composite bucket key surfacing the per-instance HashMap shape +
/// the durable D1 PK columns simultaneously. The `scope_key` is opaque
/// per-dimension (empty for `PerTenant`; IPv4/IPv6 literal for `PerIp`;
/// endpoint route id for `PerTenantPerEndpoint`).
///
/// Tenant-leftmost ordering: the `tenant_id` is the first field so that
/// a `BTreeMap` keyed on `BucketKey` naturally clusters rows for one
/// tenant — supports per-tenant cold-start reload AND the dashboard
/// per-tenant aggregation widget.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BucketKey {
    /// Tenant scope (PK component 1; preserved as a dedicated field so
    /// the cross-tenant injection guard cannot be bypassed by a clever
    /// `scope_key`).
    pub tenant_id: Uuid,
    /// Bucket dimension.
    pub dimension: KeyDimension,
    /// Opaque per-dimension scope.
    pub scope_key: String,
}

impl BucketKey {
    /// Build a `PerTenant` bucket key (the camada 1 canonical aggregate).
    #[must_use]
    pub fn per_tenant(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            dimension: KeyDimension::PerTenant,
            scope_key: String::new(),
        }
    }

    /// Build a `PerIp` bucket key. The IP literal is the textual IPv4 /
    /// IPv6 form (e.g. `"203.0.113.5"` or `"2001:db8::1"`) — production
    /// wiring extracts via `request.headers.cf-connecting-ip`.
    #[must_use]
    pub fn per_ip(tenant_id: Uuid, ip_literal: impl Into<String>) -> Self {
        Self {
            tenant_id,
            dimension: KeyDimension::PerIp,
            scope_key: ip_literal.into(),
        }
    }

    /// Build a `PerTenantPerEndpoint` bucket key. The endpoint id is
    /// the canonical route literal (e.g. `"cas.put"` / `"ac.get"`).
    #[must_use]
    pub fn per_tenant_per_endpoint(
        tenant_id: Uuid,
        endpoint_id: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            dimension: KeyDimension::PerTenantPerEndpoint,
            scope_key: endpoint_id.into(),
        }
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
    fn dimensions_have_unique_canonical_strings() {
        let mut set = std::collections::HashSet::new();
        for d in KEY_DIMENSION_LIST {
            assert!(set.insert(d.as_str()), "duplicate canonical: {d}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn canonical_mnemonics_align_with_sql_literals() {
        assert_eq!(KeyDimension::PerTenant.as_str(), "per_tenant");
        assert_eq!(KeyDimension::PerIp.as_str(), "per_ip");
        assert_eq!(
            KeyDimension::PerTenantPerEndpoint.as_str(),
            "per_tenant_per_endpoint"
        );
    }

    #[test]
    fn per_tenant_key_has_empty_scope() {
        let t = Uuid::from_u128(0x1);
        let key = BucketKey::per_tenant(t);
        assert_eq!(key.tenant_id, t);
        assert_eq!(key.dimension, KeyDimension::PerTenant);
        assert!(key.scope_key.is_empty());
    }

    #[test]
    fn per_ip_key_carries_ip_literal() {
        let t = Uuid::from_u128(0x1);
        let key = BucketKey::per_ip(t, "203.0.113.5");
        assert_eq!(key.dimension, KeyDimension::PerIp);
        assert_eq!(key.scope_key, "203.0.113.5");
    }

    #[test]
    fn per_endpoint_key_carries_endpoint_literal() {
        let t = Uuid::from_u128(0x1);
        let key = BucketKey::per_tenant_per_endpoint(t, "cas.put");
        assert_eq!(key.dimension, KeyDimension::PerTenantPerEndpoint);
        assert_eq!(key.scope_key, "cas.put");
    }

    #[test]
    fn cross_tenant_keys_compare_unequal() {
        let a = BucketKey::per_tenant(Uuid::from_u128(0xa));
        let b = BucketKey::per_tenant(Uuid::from_u128(0xb));
        assert_ne!(a, b);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", KeyDimension::PerTenant), "per_tenant");
    }
}

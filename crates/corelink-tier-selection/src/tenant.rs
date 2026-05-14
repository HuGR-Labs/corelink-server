//! Tenant + correlation identifiers.

/// Opaque tenant identifier (matches `tenant` column in D1).
///
/// Newtype prevents accidental cross-domain mixing (`StripeCustomerId`
/// vs `TenantId` etc.).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(String);

impl TenantId {
    /// Construct a [`TenantId`] from an opaque slug.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for TenantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stripe customer id mapping (set atomically per WI §6.7).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StripeCustomerId(String);

impl StripeCustomerId {
    /// Construct a [`StripeCustomerId`].
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Per-tenant onboarding context handed to `select_tier`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TenantCtx {
    /// Canonical tenant identifier.
    pub tenant_id: TenantId,
    /// `now()` in milliseconds since epoch (caller injected for
    /// determinism in tests).
    pub now_ms: u64,
    /// Correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
}

impl TenantCtx {
    /// Construct a [`TenantCtx`].
    #[must_use]
    pub fn new(tenant_id: TenantId, now_ms: u64, correlation_id: impl Into<String>) -> Self {
        Self {
            tenant_id,
            now_ms,
            correlation_id: correlation_id.into(),
        }
    }
}

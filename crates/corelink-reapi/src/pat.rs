//! PAT validation seam — auth_stub_contract.md S-02↔S-03 bridge.
//!
//! Per `auth_stub_contract.md §3` the production interface is a single trait
//! `AuthMiddleware` with `authenticate` + `require_scope` + `require_mfa_fresh`.
//! S-01 only needs the PAT validation half (the REAPI handlers don't gate on
//! MFA freshness — that lives in S-13 admin plane). We expose:
//!
//! - [`PatValidator`] — minimal trait with `authenticate(token) -> TenantContext`.
//!   Real Clerk impl lands in S-03 alongside JWKS cache + Argon2id verify;
//!   the trait stays binding-stable.
//! - [`StubPatValidator`] — fixture-backed test fake; not for production use
//!   (no JWT verify, no revocation check).
//! - [`TenantContext`] — per-request bundle. Immutable; carries tenant_id,
//!   region, scope-set, request_id, principal_id (PAT owner UUID).
//! - [`AuthScope`] — typed enum mirror of the `auth_stub_contract.md §2`
//!   `Scope` enum; covers `cache:r` / `cache:w` for S-01.
//!
//! ## Why scope-set, not single string
//!
//! WI-S01-005 §6.1.3 mandates `scopes: PatScopes` (set, not single label).
//! Future S-03 fine-grained PAT will add `cache:w:cas` / `cache:w:ac` —
//! a set tolerates the future without API churn.
//!
//! ## Cross-tenant isolation
//!
//! [`TenantContext`] does NOT carry the per-region [`corelink_tenant_path::TenantDerivationKey`];
//! the auth layer applies the TDK once at construction time when it builds
//! the downstream [`corelink_worker::TenantCtx`] for storage adapters.
//! That separation matches `tenant.rs §"Why the constructor takes a
//! &TenantDerivationKey"` reasoning: TDK lives in the auth layer, not in the
//! per-request ctx, so a misrouted request can't accidentally re-derive a
//! prefix under a wrong-region TDK.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fmt;

use corelink_replication::region_resolver::Region;
use thiserror::Error;
use uuid::Uuid;

/// Typed PAT scope. S-01 + S-02 consumes [`AuthScope::CacheRead`],
/// [`AuthScope::CacheWrite`], and [`AuthScope::CacheFindMissing`]; the
/// others (Admin, Billing, Privacy) are declared so a fixture-backed
/// `StubPatValidator` can carry full-fidelity PAT scope-sets that match
/// the canonical `auth_stub_contract.md §2` enum without requiring
/// re-typing in S-03.
///
/// `CacheFindMissing` is a discovery-only scope (`cache:find-missing`):
/// it grants the bearer permission to invoke `FindMissingBlobs` (a
/// pre-upload existence check) without implying read access to the
/// underlying blobs. This split honours auth_model.md §scope L188
/// "Default de CLI/CI tokens: cache-rw + cache-find-missing".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuthScope {
    /// `cache:r` — read CAS blobs / AC results.
    CacheRead,
    /// `cache:w` — write CAS blobs / AC results.
    CacheWrite,
    /// `cache:find-missing` — invoke `FindMissingBlobs` (existence check).
    /// Does NOT imply `CacheRead` (download). Added WI-S02-002 SEAL.
    CacheFindMissing,
    /// `admin:read`. Out of scope for S-01 handlers; present for parity.
    AdminRead,
    /// `admin:write`. Out of scope for S-01 handlers.
    AdminWrite,
    /// `billing:admin`. Out of scope for S-01 handlers.
    BillingAdmin,
    /// `privacy:admin`. Out of scope for S-01 handlers.
    PrivacyAdmin,
}

impl AuthScope {
    /// Stable canonical text representation. Matches the literals in
    /// `auth_stub_contract.md §2` byte-for-byte so existing PAT issuers can
    /// emit the same labels into JWT claims.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CacheRead => "cache:r",
            Self::CacheWrite => "cache:w",
            Self::CacheFindMissing => "cache:find-missing",
            Self::AdminRead => "admin:read",
            Self::AdminWrite => "admin:write",
            Self::BillingAdmin => "billing:admin",
            Self::PrivacyAdmin => "privacy:admin",
        }
    }
}

impl fmt::Display for AuthScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-request authenticated context. Constructed by [`PatValidator::authenticate`].
///
/// All fields are immutable. The struct deliberately omits the
/// per-region [`corelink_tenant_path::TenantDerivationKey`]: the auth layer
/// holds the TDK and applies it once when wiring the storage-side
/// [`corelink_worker::TenantCtx`]. Keeping the TDK out of this struct means
/// per-request ctx cannot accidentally route TDKs cross-region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantContext {
    /// Stable tenant UUID. Derived from PAT validation (Clerk JWT claim
    /// `tenant_id` in S-03; fixture lookup in [`StubPatValidator`]).
    tenant_id: Uuid,
    /// Stable PAT-owner UUID. May differ from `tenant_id` in multi-user
    /// tenants. Used in audit envelopes (`source` field) and per-PAT rate
    /// limit (S-08).
    principal_id: Uuid,
    /// Region this request is pinned to (residency layer; S-14 forward).
    region: Region,
    /// Scope-set; checked by [`TenantContext::require_scope`].
    scopes: BTreeSet<AuthScope>,
    /// Correlation id (gRPC `x-request-id` header, propagated client →
    /// handler → audit_outbox row).
    request_id: String,
}

impl TenantContext {
    /// Construct a context. Used by [`PatValidator`] impls; not callable
    /// from outside this crate without `[doc(hidden)]` test gates.
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        principal_id: Uuid,
        region: Region,
        scopes: BTreeSet<AuthScope>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            principal_id,
            region,
            scopes,
            request_id: request_id.into(),
        }
    }

    /// Tenant UUID this request authenticated as.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// PAT-owner UUID. May equal `tenant_id` in single-user tenants.
    #[must_use]
    pub const fn principal_id(&self) -> Uuid {
        self.principal_id
    }

    /// Pinned residency region.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Borrow the request correlation id.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Iterate the scope set in canonical (BTreeSet) order. Useful for
    /// audit envelope serialization.
    pub fn scopes(&self) -> impl Iterator<Item = &AuthScope> {
        self.scopes.iter()
    }

    /// Verify a required scope is present. Returns
    /// `Err(AuthStubError::ScopeInsufficient)` if absent — matching the
    /// `auth_stub_contract.md §3` default impl semantics so S-03 swap stays
    /// behavior-compatible.
    ///
    /// # Errors
    ///
    /// Returns [`AuthStubError::ScopeInsufficient`] when `required` is
    /// not in the bound scope set.
    pub fn require_scope(&self, required: AuthScope) -> Result<(), AuthStubError> {
        if self.scopes.contains(&required) {
            Ok(())
        } else {
            Err(AuthStubError::ScopeInsufficient { required })
        }
    }
}

/// Auth-layer error variants — mirrors `auth_stub_contract.md §4` so the
/// stub returns the same error vocabulary as S-03 will. error-taxonomy
/// codes are surfaced via [`AuthStubError::code`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AuthStubError {
    /// Bearer token absent / wrong format / not in fixture map. Maps to
    /// `COR_AUTH_PAT_INVALID` (HTTP 401).
    #[error("PAT invalid or expired")]
    PatInvalid,
    /// Scope-set does not contain `required`. Maps to
    /// `COR_AUTH_SCOPE_INSUFFICIENT` (HTTP 403).
    #[error("PAT scope insufficient: required {required}")]
    ScopeInsufficient {
        /// The scope the handler asked for and that the PAT did not carry.
        required: AuthScope,
    },
}

impl AuthStubError {
    /// Stable canonical error code matching `error_taxonomy.md`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PatInvalid => crate::error_map::COR_AUTH_PAT_INVALID,
            Self::ScopeInsufficient { .. } => crate::error_map::COR_AUTH_SCOPE_INSUFFICIENT,
        }
    }
}

/// Auth seam used by REAPI handlers.
///
/// The trait surface is intentionally narrow — `authenticate(token,
/// request_id)` returning a [`TenantContext`]. S-03 will plug a real Clerk
/// adapter behind the same trait; the handlers above this layer never call
/// concrete Clerk APIs.
pub trait PatValidator: Send + Sync {
    /// Validate a bearer token and return the authenticated context. The
    /// handler is responsible for extracting the bearer token from the
    /// `Authorization` header *before* calling this trait.
    ///
    /// `request_id` is propagated into the resulting `TenantContext` so
    /// every audit envelope carries the same correlation id end-to-end.
    ///
    /// # Errors
    ///
    /// Returns [`AuthStubError::PatInvalid`] if the token is unknown,
    /// expired, or revoked.
    fn authenticate(&self, token: &str, request_id: &str) -> Result<TenantContext, AuthStubError>;
}

/// Fixture-backed [`PatValidator`] for unit + integration tests.
///
/// Production deploys MUST use the S-03 Clerk-backed validator. The stub
/// performs no crypto, no JWKS fetch, no revocation check — it is a pure
/// in-memory map from `token` → `(tenant_id, region, scopes, principal_id)`.
///
/// The stub deliberately retains the **same `TenantContext` shape** as the
/// real S-03 adapter (per `auth_stub_contract.md §1` "Frozen ABI") so a
/// drop-in swap doesn't churn handler code.
#[derive(Debug, Default, Clone)]
pub struct StubPatValidator {
    fixtures: HashMap<String, FixtureEntry>,
}

#[derive(Debug, Clone)]
struct FixtureEntry {
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    scopes: BTreeSet<AuthScope>,
}

impl StubPatValidator {
    /// Construct an empty stub. Add fixtures with [`Self::insert`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `(token, tenant context bundle)` mapping. The tuple form
    /// keeps the public API friction-free for tests. Returns `&mut Self`
    /// so calls can chain.
    pub fn insert(
        &mut self,
        token: impl Into<String>,
        tenant_id: Uuid,
        principal_id: Uuid,
        region: Region,
        scopes: impl IntoIterator<Item = AuthScope>,
    ) -> &mut Self {
        self.fixtures.insert(
            token.into(),
            FixtureEntry {
                tenant_id,
                principal_id,
                region,
                scopes: scopes.into_iter().collect(),
            },
        );
        self
    }

    /// Number of registered fixtures. Mirror of `HashMap::len`. Useful for
    /// test assertions that the validator was wired correctly.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    /// True iff no fixtures are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fixtures.is_empty()
    }
}

impl PatValidator for StubPatValidator {
    fn authenticate(&self, token: &str, request_id: &str) -> Result<TenantContext, AuthStubError> {
        let entry = self.fixtures.get(token).ok_or(AuthStubError::PatInvalid)?;
        Ok(TenantContext::new(
            entry.tenant_id,
            entry.principal_id,
            entry.region,
            entry.scopes.clone(),
            request_id,
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    fn fixture_validator() -> StubPatValidator {
        let mut v = StubPatValidator::new();
        v.insert(
            "test-token-write",
            Uuid::from_u128(1),
            Uuid::from_u128(11),
            Region::Wnam,
            [AuthScope::CacheRead, AuthScope::CacheWrite],
        );
        v.insert(
            "test-token-read-only",
            Uuid::from_u128(2),
            Uuid::from_u128(12),
            Region::Wnam,
            [AuthScope::CacheRead],
        );
        v
    }

    #[test]
    fn unknown_token_is_pat_invalid() {
        let v = fixture_validator();
        let err = v.authenticate("nope", "req-1").unwrap_err();
        assert_eq!(err, AuthStubError::PatInvalid);
        assert_eq!(err.code(), "COR_AUTH_PAT_INVALID");
    }

    #[test]
    fn known_token_yields_ctx() {
        let v = fixture_validator();
        let ctx = v.authenticate("test-token-write", "req-2").unwrap();
        assert_eq!(ctx.tenant_id(), Uuid::from_u128(1));
        assert_eq!(ctx.principal_id(), Uuid::from_u128(11));
        assert_eq!(ctx.region(), Region::Wnam);
        assert_eq!(ctx.request_id(), "req-2");
        ctx.require_scope(AuthScope::CacheWrite).unwrap();
    }

    #[test]
    fn scope_check_rejects_missing_scope() {
        let v = fixture_validator();
        let ctx = v.authenticate("test-token-read-only", "req-3").unwrap();
        let err = ctx.require_scope(AuthScope::CacheWrite).unwrap_err();
        assert_eq!(
            err,
            AuthStubError::ScopeInsufficient {
                required: AuthScope::CacheWrite
            }
        );
        assert_eq!(err.code(), "COR_AUTH_SCOPE_INSUFFICIENT");
    }

    #[test]
    fn auth_scope_canonical_strings() {
        assert_eq!(AuthScope::CacheRead.as_str(), "cache:r");
        assert_eq!(AuthScope::CacheWrite.as_str(), "cache:w");
        assert_eq!(AuthScope::CacheFindMissing.as_str(), "cache:find-missing");
        assert_eq!(AuthScope::AdminRead.as_str(), "admin:read");
        assert_eq!(AuthScope::AdminWrite.as_str(), "admin:write");
        assert_eq!(AuthScope::BillingAdmin.as_str(), "billing:admin");
        assert_eq!(AuthScope::PrivacyAdmin.as_str(), "privacy:admin");
    }

    #[test]
    fn cache_find_missing_does_not_imply_cache_read() {
        // Critical: cache:find-missing grants existence-check capability
        // ONLY. A PAT carrying ONLY `CacheFindMissing` MUST NOT pass a
        // `require_scope(CacheRead)` check (defense against scope
        // creep — discovery-only tokens can list, not download).
        let mut v = StubPatValidator::new();
        v.insert(
            "find-only",
            Uuid::from_u128(99),
            Uuid::from_u128(199),
            Region::Wnam,
            [AuthScope::CacheFindMissing],
        );
        let ctx = v.authenticate("find-only", "req-find").unwrap();
        ctx.require_scope(AuthScope::CacheFindMissing).unwrap();
        // Read scope MUST be denied — no implicit upgrade.
        assert!(ctx.require_scope(AuthScope::CacheRead).is_err());
        assert!(ctx.require_scope(AuthScope::CacheWrite).is_err());
    }

    #[test]
    fn empty_validator_default_is_empty() {
        let v = StubPatValidator::default();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }
}

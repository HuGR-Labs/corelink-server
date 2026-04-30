//! Immutable authenticated request context — [`AuthCtx`] (WI-S03-003 §1).
//!
//! ## Why a separate context from the storage-layer [`crate::TenantCtx`]
//!
//! The storage-layer [`crate::TenantCtx`] (WI-S01-003) carries the
//! minimum identity surface the storage adapters need (tenant UUID,
//! residency region, derived [`corelink_tenant_path::TenantPrefix`]).
//! The auth middleware (this WI) needs a richer surface — granted
//! scopes, principal id, auth method, request id — that is irrelevant
//! to the storage layer and would inflate the storage-layer struct
//! across every R2 / D1 call.
//!
//! The canonical pattern (WI-S03-003 §9.2 + ADR-0029):
//!
//! 1. Auth middleware constructs an immutable [`AuthCtx`] post-verify.
//! 2. Middleware injects the [`AuthCtx`] into the request extensions.
//! 3. Handlers downstream call [`AuthCtx::tenant_ctx`] to obtain the
//!    storage-layer [`crate::TenantCtx`] derived from the same
//!    `(TDK, tenant_id, region)` triple — by construction the prefix
//!    field is always `derive_prefix(tdk, tenant_id)`.
//!
//! ## Immutability invariant (INV-AUTH-TENANTCTX-IMMUTABLE)
//!
//! Every field is private. The single legal construction path is
//! [`AuthCtxBuilder::build`], called only from inside this crate's
//! middleware module. Handlers downstream consume via getters
//! ([`AuthCtx::tenant_id`], etc.) and the storage-layer projection
//! [`AuthCtx::tenant_ctx`]. There is **no** mutator, **no** public
//! field, **no** `DerefMut`. `Clone` is supported (cheap; all fields
//! are `Copy` or `Arc`-shared) so axum / tonic handlers can spread
//! the context across spawned tasks without breaking immutability.
//!
//! ## `#[non_exhaustive]` evolution
//!
//! [`AuthCtx`] is `#[non_exhaustive]`: future fields (e.g.
//! `executor_id` for Phase 2 executor tokens, `ip_address` for audit
//! enrichment) can land without breaking handler pattern-matching.
//! Downstream handlers MUST use the dot-dot rest pattern
//! `match ctx { AuthCtx { tenant_id, .. } => ... }` — a CI lint
//! enforces this in the host-server crate.

use std::sync::Arc;
use std::time::SystemTime;

use corelink_clerk::{ClerkOrgId, ClerkSessionId, ClerkUserId};
use corelink_pat::{PatEnv, PatId, PatScopes};
use corelink_tenant_path::{TenantDerivationKey, TenantPrefix};
use uuid::Uuid;

use crate::region::Region;
use crate::tenant::TenantCtx;

/// How the request was authenticated.
///
/// The `Pat { env, pat_id }` arm carries the canonical PAT environment
/// (`pat | ci | ro`) and the Neon `pat.id` UUIDv7 row primary key.
/// The `Jwt { clerk_session_id, user_id, org_id }` arm carries the
/// `sid`, `sub`, and (optional) `org_id` claims from the validated
/// Clerk JWT.
#[derive(Clone, Debug)]
pub enum AuthMethod {
    /// PAT-authenticated request.
    Pat {
        /// Canonical PAT environment (`pat | ci | ro`).
        env: PatEnv,
        /// PAT row primary key (Neon `pat.id`).
        pat_id: PatId,
    },
    /// Clerk JWT-authenticated request.
    Jwt {
        /// Clerk session id (`sid` claim).
        clerk_session_id: ClerkSessionId,
        /// Clerk user id (`sub` claim).
        user_id: ClerkUserId,
        /// Clerk organisation id (`org_id` claim) when present.
        org_id: Option<ClerkOrgId>,
    },
}

/// Canonical principal identifier — UUID surfacing the authenticated
/// human / machine identity. PAT path: `pat.principal_id` (Neon
/// column). JWT path: a deterministic UUIDv5 derived from the Clerk
/// `sub` claim (the JWT `sub` is an opaque string; the v5 derivation
/// gives the audit chain a stable UUID surface without leaking the
/// raw `sub` PII into observability pipelines).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrincipalId(pub Uuid);

/// Canonical request identifier (UUIDv7; time-ordered).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub Uuid);

impl RequestId {
    /// Generate a fresh request id (UUIDv7).
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

/// Immutable per-request authenticated context (WI-S03-003 §1).
///
/// Fields are private; the only legal construction path is
/// [`AuthCtxBuilder::build`]. Handlers consume via getters or via
/// the storage-layer projection [`AuthCtx::tenant_ctx`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct AuthCtx {
    principal: PrincipalId,
    tenant_id: Uuid,
    region: Region,
    tenant_prefix: TenantPrefix,
    scopes: PatScopes,
    auth_method: AuthMethod,
    request_id: RequestId,
    issued_at: SystemTime,
    /// Canonical TDK reference held for storage-layer projection.
    /// The TDK is wrapped in an `Arc` so cloning the [`AuthCtx`]
    /// across spawned tasks does not duplicate the secret key
    /// material.
    tdk: Arc<TenantDerivationKey>,
}

impl AuthCtx {
    /// The verified principal id.
    #[must_use]
    pub const fn principal(&self) -> PrincipalId {
        self.principal
    }

    /// The verified tenant UUID. Load-bearing field for INV-TENANT-ISOLATION.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// The R2 residency region this request is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// The HMAC-derived tenant path prefix. Pre-computed at
    /// construction time; identical to `derive_prefix(tdk, tenant_id)`.
    #[must_use]
    pub const fn tenant_prefix(&self) -> &TenantPrefix {
        &self.tenant_prefix
    }

    /// The granted scopes (u64 bitset).
    #[must_use]
    pub const fn scopes(&self) -> PatScopes {
        self.scopes
    }

    /// The verification method (PAT vs JWT).
    #[must_use]
    pub fn auth_method(&self) -> &AuthMethod {
        &self.auth_method
    }

    /// The canonical request id (UUIDv7).
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// The verification timestamp.
    #[must_use]
    pub const fn issued_at(&self) -> SystemTime {
        self.issued_at
    }

    /// Project to the storage-layer [`TenantCtx`].
    ///
    /// The projection re-derives the prefix from the held TDK so the
    /// returned [`TenantCtx`] satisfies the same construction
    /// invariant as the canonical [`TenantCtx::new`] path —
    /// `ctx.prefix() == derive_prefix(tdk, tenant_id)` by
    /// construction. Callers cannot accidentally pair a tenant id
    /// with a foreign prefix.
    #[must_use]
    pub fn tenant_ctx(&self) -> TenantCtx {
        TenantCtx::new(&self.tdk, self.tenant_id, self.region)
    }

    /// Returns `true` iff every bit in `required` is also set in
    /// [`Self::scopes`]. Hot-path bitwise check; no allocation.
    #[must_use]
    pub fn has_scope(&self, required: u64) -> bool {
        self.scopes.has(required)
    }
}

/// Builder for [`AuthCtx`]. The `build` method is `pub(crate)` so the
/// only legal construction site is the auth middleware. Tests in
/// integration-test crates can opt into the `__test_helpers` module
/// (gated by the `tower-middleware` feature; the `__` prefix marks it
/// as internal-only API).
#[derive(Clone, Debug)]
pub struct AuthCtxBuilder {
    principal: PrincipalId,
    tenant_id: Uuid,
    region: Region,
    scopes: PatScopes,
    auth_method: AuthMethod,
    request_id: RequestId,
    issued_at: SystemTime,
    tdk: Arc<TenantDerivationKey>,
}

impl AuthCtxBuilder {
    /// Start a new builder. Caller MUST be the auth middleware OR
    /// the gated test-helper module.
    pub(crate) fn new(
        principal: PrincipalId,
        tenant_id: Uuid,
        region: Region,
        scopes: PatScopes,
        auth_method: AuthMethod,
        tdk: Arc<TenantDerivationKey>,
    ) -> Self {
        Self {
            principal,
            tenant_id,
            region,
            scopes,
            auth_method,
            request_id: RequestId::new_v7(),
            issued_at: SystemTime::now(),
            tdk,
        }
    }

    /// Override the request id (typically used to propagate a
    /// client-supplied `x-request-id` header). The auth middleware
    /// calls this when the request carries a valid UUID in the
    /// header; otherwise the auto-generated v7 id stands.
    #[must_use]
    pub fn with_request_id(mut self, request_id: RequestId) -> Self {
        self.request_id = request_id;
        self
    }

    /// Override the `issued_at` timestamp. Test-only — production
    /// always uses the wall-clock time at builder construction.
    #[doc(hidden)]
    #[must_use]
    pub fn with_issued_at(mut self, issued_at: SystemTime) -> Self {
        self.issued_at = issued_at;
        self
    }

    /// Finalise into an immutable [`AuthCtx`]. Derives the canonical
    /// tenant prefix from `(tdk, tenant_id)` so the resulting context
    /// upholds INV-TENANT-ISOLATION at the type level.
    #[must_use]
    pub(crate) fn build(self) -> AuthCtx {
        let tenant_prefix = corelink_tenant_path::derive_prefix(&self.tdk, self.tenant_id);
        AuthCtx {
            principal: self.principal,
            tenant_id: self.tenant_id,
            region: self.region,
            tenant_prefix,
            scopes: self.scopes,
            auth_method: self.auth_method,
            request_id: self.request_id,
            issued_at: self.issued_at,
            tdk: self.tdk,
        }
    }
}

/// Test-only construction helpers exposed under the
/// `__test_helpers` module. The leading `__` marks the module as
/// internal-only; consumers MUST NOT use this in production code
/// paths.
#[doc(hidden)]
pub mod __test_helpers {
    use super::{
        AuthCtx, AuthCtxBuilder, AuthMethod, PrincipalId, Region, TenantDerivationKey,
    };
    use corelink_pat::PatScopes;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Construct an [`AuthCtx`] directly. Visible only when the
    /// `tower-middleware` feature is enabled; downstream handlers in
    /// production code paths MUST go through the canonical
    /// [`super::super::auth::AuthLayer`] middleware.
    #[must_use]
    pub fn make_auth_ctx(
        principal: PrincipalId,
        tenant_id: Uuid,
        region: Region,
        scopes: PatScopes,
        auth_method: AuthMethod,
        tdk: Arc<TenantDerivationKey>,
    ) -> AuthCtx {
        AuthCtxBuilder::new(principal, tenant_id, region, scopes, auth_method, tdk).build()
    }
}

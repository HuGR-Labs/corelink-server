//! [`TenantCtx`] — minimal per-request tenant context for storage adapters.
//!
//! WI-S01-003 §1 names this `TenantCtx`. The shape is intentionally small —
//! tenant_id (UUID), residency region, and the canonical [`TenantPrefix`].
//!
//! ## Why the constructor takes a `&TenantDerivationKey` (not a prefix)
//!
//! The earlier draft of this crate exposed `TenantCtx::new(tenant_id,
//! region, prefix)` — accepting a caller-supplied prefix. Codex round 1 of
//! WI-S01-003 flagged that as a P0 cross-tenant misrouting risk: a caller
//! could (accidentally or maliciously) pass a prefix that does not match
//! `tenant_id`, and the storage adapters would happily land blobs at the
//! wrong key. Rather than relying on the auth layer to "always pass a
//! consistent pair", we close the door at the type level: the only
//! constructor takes `&TenantDerivationKey + tenant_id` and derives the
//! prefix internally — by construction, `ctx.prefix()` is always
//! `derive_prefix(tdk, ctx.tenant_id())`.
//!
//! That is REG-NAMESPACE-002 ("no code path may construct a key without
//! going through `derive_prefix`") enforced at the type-system level rather
//! than at the convention level.

use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix};
use uuid::Uuid;

use crate::region::Region;

/// Per-request tenant context handed to [`crate::storage::r2`] adapters.
///
/// Contains:
/// - `tenant_id` (a UUID already authenticated by the auth layer),
/// - the [`Region`] this request is pinned to (residency layer),
/// - the [`TenantPrefix`], **derived from `tenant_id` under the per-region
///   TDK at construction time**.
///
/// The prefix field is private and only the constructor [`TenantCtx::new`]
/// can populate it, so cross-tenant misrouting via a mismatched
/// `(tenant_id, prefix)` pair is structurally impossible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TenantCtx {
    tenant_id: Uuid,
    region: Region,
    prefix: TenantPrefix,
}

impl TenantCtx {
    /// Construct a `TenantCtx` by deriving the per-tenant prefix from
    /// `(tdk, tenant_id)` at the type-system level.
    ///
    /// Callers in the auth layer (S-03) hold the per-region [`TenantDerivationKey`]
    /// behind a Cloudflare Secrets binding. Each request hits this
    /// constructor exactly once; thereafter the storage layer operates on
    /// the immutable, internally-consistent ctx without ever touching the
    /// TDK again.
    #[must_use]
    pub fn new(tdk: &TenantDerivationKey, tenant_id: Uuid, region: Region) -> Self {
        let prefix = derive_prefix(tdk, tenant_id);
        Self {
            tenant_id,
            region,
            prefix,
        }
    }

    /// The authenticated tenant UUID.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// The R2 residency region for this tenant request.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// The HMAC-derived 16-char prefix for R2/KV/D1 namespacing.
    #[must_use]
    pub const fn prefix(&self) -> &TenantPrefix {
        &self.prefix
    }
}

//! Adapter-local port traits (hex-arch style).
//!
//! The cargo adapter consumes two side-effecting subsystems through
//! abstract ports: a tenant-scoped content-addressable store and a
//! `Bearer` PAT → tenant resolver. Defining the ports inside this
//! crate (rather than depending on a not-yet-extracted workspace
//! trait) lets the Wave-34 adapter campaign land in parallel with
//! the Wave-33 Stream-A `corelink-cas` umbrella consolidation
//! without coupling either work item to the other's review cycle.
//!
//! ## Why local ports, not workspace types
//!
//! Per the Wave-34 sibling charter, each adapter (`cargo`, `npm`,
//! `pip`, `oci`, `brew`) defines its own minimum-viable port surface
//! so concurrent agents stay source-disjoint. When the canonical
//! workspace `CasStore` / `TenantResolver` traits land (post-Wave-34
//! consolidation), each adapter's `ports` module becomes a `pub use`
//! re-export — call sites stay stable. See dispatch packet §5 for
//! the conceptual signatures this module realizes.

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;

/// Per-tenant content-addressable byte store.
///
/// Implementors MUST namespace by `tenant_id` (no cross-tenant read
/// fall-through) and MUST treat `(tenant_id, digest_hex)` as the unique
/// identity of a stored blob. `digest_hex` is the sccache BLAKE3 key
/// (lowercase hex, 64 chars).
///
/// All methods are async to permit network / disk backends.
#[async_trait]
pub trait CasStore: Send + Sync + Debug {
    /// Return the bytes stored under `(tenant_id, digest_hex)`, or
    /// `None` on miss. `Err` is reserved for backend failure (network,
    /// disk, permission) and surfaces as
    /// [`crate::cargo::CargoAdapterError::Cas`].
    async fn get(&self, tenant_id: &str, digest_hex: &str) -> Result<Option<Vec<u8>>, CasError>;

    /// Durably store `bytes` under `(tenant_id, digest_hex)`. Existing
    /// entries are overwritten idempotently (content-addressable equality
    /// is presumed at the caller via the BLAKE3 key).
    async fn put(&self, tenant_id: &str, digest_hex: &str, bytes: Vec<u8>) -> Result<(), CasError>;

    /// Cheap existence probe for `HEAD /<key>` (sccache HTTP backend) —
    /// MUST NOT download or rehash the blob.
    ///
    /// sccache issues a `HEAD` before every cache write to skip an upload it
    /// already has; serving that probe via [`Self::get`] pulls the FULL blob
    /// (and discards it), doubling R2 GET egress per probe (a COGS leak). The
    /// HEAD handler calls this instead of `get`.
    ///
    /// # Default
    ///
    /// The default falls back to [`Self::get`] (downloads the body) and maps
    /// the outcome to a boolean — CORRECT but not cheap; it exists so existing
    /// in-memory/test impls keep working. A storage-backed impl SHOULD override
    /// it with a true metadata/HEAD lookup (the production bridge does, via the
    /// workspace `CasReadHandler::exists`).
    async fn exists(&self, tenant_id: &str, digest_hex: &str) -> Result<bool, CasError> {
        Ok(self.get(tenant_id, digest_hex).await?.is_some())
    }
}

/// CAS backend failure surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CasError {
    /// Backend reported a recoverable failure (transient I/O, retry-able).
    #[error("backend: {0}")]
    Backend(String),
}

/// A resolved PAT: the owning tenant id plus whether the PAT carries cache
/// WRITE capability. Returned by [`TenantResolver::resolve_with_capability`]
/// so write-gate callers can enforce the PAT's real rights independently of
/// the Worker-injected `x-corelink-scope` header (two-layer write enforcement).
///
/// Mirrors the `ResolvedPat` shape in the OCI adapter's ports — all adapters
/// use the same two-layer model after F27.
#[derive(Debug, Clone)]
pub struct ResolvedTenant {
    /// Tenant id the PAT belongs to.
    pub tenant_id: String,
    /// `true` iff the PAT grants cache WRITE (e.g. `cas:rw` / `admin`).
    pub can_write: bool,
}

/// Resolves a PAT plaintext to its owning tenant.
///
/// Implementors MUST compare the supplied PAT against stored values in
/// constant time (see `subtle::ConstantTimeEq`) — this trait surface
/// hides that detail from callers but enforces it by convention in the
/// `auth.rs` call site.
///
/// PAT scopes are `cas:read` + `cas:write`; the resolver itself returns
/// only the tenant id so the scope check stays local to this crate.
#[async_trait]
pub trait TenantResolver: Send + Sync + Debug {
    /// Return the tenant id owning `pat_plaintext`, or
    /// `Err(TenantResolveError::InvalidPat)` if the PAT is unknown /
    /// expired / out-of-scope.
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError>;

    /// Resolve a PAT to its tenant AND its cache write-capability.
    ///
    /// The write gate uses this to enforce `scope_ok_from_header AND
    /// can_write_from_pat_reverify` (two-layer write enforcement, mirroring
    /// OCI). Default impl: resolve the tenant and report `can_write = false`
    /// (FAIL-SAFE — an impl that cannot determine write capability denies
    /// writes). Impls backed by the container's `PatVerifier` override this
    /// to return the PAT's real capability.
    async fn resolve_with_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<ResolvedTenant, TenantResolveError> {
        let tenant_id = self.resolve(pat_plaintext).await?;
        Ok(ResolvedTenant {
            tenant_id,
            can_write: false,
        })
    }
}

/// PAT resolution failure surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TenantResolveError {
    /// PAT not found, expired, or lacking `cas:read` / `cas:write` scope.
    /// Surfaces as HTTP 401 via [`crate::cargo::CargoAdapterError::Auth`].
    #[error("invalid PAT")]
    InvalidPat,

    /// Resolver backend (e.g. D1, in-memory directory) failed.
    /// Surfaces as HTTP 503.
    #[error("resolver backend: {0}")]
    Backend(String),
}

/// Type-erased CAS handle bundled into [`crate::cargo::CargoAdapterConfig`].
pub type SharedCasStore = Arc<dyn CasStore>;

/// Type-erased tenant resolver bundled into
/// [`crate::cargo::CargoAdapterConfig`].
pub type SharedTenantResolver = Arc<dyn TenantResolver>;

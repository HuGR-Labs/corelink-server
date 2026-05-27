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
    async fn get(
        &self,
        tenant_id: &str,
        digest_hex: &str,
    ) -> Result<Option<Vec<u8>>, CasError>;

    /// Durably store `bytes` under `(tenant_id, digest_hex)`. Existing
    /// entries are overwritten idempotently (content-addressable equality
    /// is presumed at the caller via the BLAKE3 key).
    async fn put(
        &self,
        tenant_id: &str,
        digest_hex: &str,
        bytes: Vec<u8>,
    ) -> Result<(), CasError>;
}

/// CAS backend failure surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CasError {
    /// Backend reported a recoverable failure (transient I/O, retry-able).
    #[error("backend: {0}")]
    Backend(String),
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
    async fn resolve(
        &self,
        pat_plaintext: &str,
    ) -> Result<String, TenantResolveError>;
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

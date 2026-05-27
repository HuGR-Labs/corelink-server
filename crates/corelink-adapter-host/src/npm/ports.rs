//! Locally-declared port traits the npm adapter consumes.
//!
//! Per the Wave-34 charter "Concurrent agents (parallel-safe per
//! Section 0.6)" decision, each per-package-manager adapter (cargo /
//! npm / brew / oci / pip) declares its own minimal port surface
//! inline rather than editing a shared `corelink-adapter-ports`
//! crate. This keeps the five sibling campaigns trivially
//! parallel-safe (zero source overlap) and lets each adapter ship to
//! the workspace independently. Consolidation into a shared ports
//! crate is deferred to a follow-up sequential wave once all five
//! adapters have landed and the trait surfaces have stabilised.
//!
//! The trait surfaces here are intentionally narrow — they cover
//! only the operations the npm adapter calls, nothing else. Each
//! adapter's port traits intentionally name-collide across crates
//! (every adapter defines its own `CasStore`, `KvStore`,
//! `TenantResolver`); consumers wire them up via concrete adapter
//! implementations that bridge to the production `corelink-cas` /
//! `corelink-adapters-cloud` surfaces at boot time.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;

use crate::npm::error::NpmAdapterError;

/// Tenant-scoped content-addressable blob store as the npm adapter
/// sees it. Production implementations bridge to the workspace
/// `corelink-cas` umbrella; in-memory test fakes back the unit /
/// adversarial test suite.
#[async_trait]
pub trait CasStore: Send + Sync + fmt::Debug {
    /// Fetch a stored blob by `(tenant, digest)`. `Ok(None)` is a
    /// cache miss; `Ok(Some(bytes))` is a hit.
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Cas`] on any backend transport or
    /// encoding failure.
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, NpmAdapterError>;

    /// Persist `bytes` under `(tenant, digest)`. The adapter
    /// guarantees the integrity check has already passed before
    /// calling `put`.
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Cas`] on any backend failure.
    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), NpmAdapterError>;
}

/// Tenant-scoped key-value store the adapter uses to cache npm package
/// metadata JSON. Returns `(value, inserted_at_unix_ms)` so the TTL
/// check in [`crate::npm::metadata`] is pure-logic.
#[async_trait]
pub trait KvStore: Send + Sync + fmt::Debug {
    /// Fetch `(value, inserted_at_unix_ms)` for `(tenant, key)`.
    /// `Ok(None)` is a miss; `Ok(Some((bytes, ts)))` is a hit.
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Kv`] on any backend failure.
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError>;

    /// Store `value` at `(tenant, key)` with `inserted_at_unix_ms`.
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Kv`] on any backend failure.
    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), NpmAdapterError>;
}

/// PAT → tenant resolver. The adapter calls
/// [`TenantResolver::resolve`] on every request before any CAS / KV
/// touch; an `Err` short-circuits to `401`.
#[async_trait]
pub trait TenantResolver: Send + Sync + fmt::Debug {
    /// Resolve a (presumed-valid) PAT plaintext to its tenant.
    ///
    /// Implementations MUST compare the PAT bytes in constant time
    /// (e.g. via [`subtle::ConstantTimeEq`]).
    ///
    /// # Errors
    ///
    /// Returns [`NpmAdapterError::Auth`] for any PAT verification
    /// failure (missing, malformed, expired, revoked, wrong scope).
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, NpmAdapterError>;
}

/// Convenience type alias for the `Arc<dyn ...>` CAS store wiring shape.
pub type CasStoreHandle = Arc<dyn CasStore>;
/// Convenience type alias for the `Arc<dyn ...>` KV store wiring shape.
pub type KvStoreHandle = Arc<dyn KvStore>;
/// Convenience type alias for the `Arc<dyn ...>` tenant resolver shape.
pub type TenantResolverHandle = Arc<dyn TenantResolver>;

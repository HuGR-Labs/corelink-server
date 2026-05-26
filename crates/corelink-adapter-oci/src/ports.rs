//! Hexagonal ports the OCI adapter depends on.
//!
//! The wave-34 dispatch contract (`specs/_proposals/adapters/oci.md`)
//! names three external surfaces — `CasStore`, `KvStore`,
//! `TenantResolver` — none of which currently exist as canonical
//! workspace traits. Per the consumer-migration deferral pattern from
//! Wave-33 Stage 2.A-v2, we declare them HERE as adapter-local ports
//! and surface in-memory test impls; production wiring will bridge
//! them onto the canonical surfaces once those land.
//!
//! ## Why local ports rather than blocking on the canonical traits
//!
//! The canonical `corelink-cas` CAS surface is currently the
//! Option-A re-export façade over 10 absorbed crates; adding a
//! `CasStore` trait there would force a charter "behaviour-preserving
//! refactor ONLY" violation review. Mirror choice was made by the
//! sibling wave-34 adapter campaigns (cargo / npm / pip / brew) per
//! the Section 0.6 parallel-safety conjecture: each adapter owns its
//! own ports + per-adapter test fakes; bridge code lands at the
//! production wiring site only.
//!
//! ## Charter constraints preserved
//!
//! - All trait methods are `async` + return [`PortResult`] so the
//!   audit-fail-CLOSED contract surfaces at the type level (no way to
//!   silently succeed past a backend failure).
//! - Implementations MUST NOT panic (`#![forbid(unsafe_code)]` + the
//!   `unwrap_used = "deny"` lint apply transitively to any in-tree
//!   impl).
//! - The test fakes in [`testing`] are NEVER reachable from
//!   production — they live behind `#[cfg(any(test, feature = "test-support"))]`
//!   gates per Stage 0 sub-step 4.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;

use corelink_core::{SecretWrap, TenantId};

/// Adapter-local port-call result.
///
/// `Send + Sync + 'static` boundary on the error string keeps the
/// trait object-safe across axum's `tower::Service` requirements.
pub type PortResult<T> = Result<T, String>;

/// Wave-33 Stage 1 Stream A canonical CAS bounded-context surface is
/// today re-exported but does NOT yet expose a `CasStore` trait. This
/// local port stands in until the canonical surface lands.
///
/// The OCI adapter calls these methods at:
///
/// | OCI op | port call |
/// |---|---|
/// | `POST /v2/<name>/blobs/uploads/` | [`Self::open_upload`] |
/// | `PATCH /v2/<name>/blobs/uploads/<uuid>` | [`Self::append_chunk`] |
/// | `PUT /v2/<name>/blobs/uploads/<uuid>?digest=X` | [`Self::finalize_upload`] |
/// | `GET /v2/<name>/blobs/<digest>` | [`Self::get_blob`] |
/// | `HEAD /v2/<name>/blobs/<digest>` | [`Self::blob_exists`] |
///
/// `tenant` is always non-empty — repo extraction in
/// [`crate::server`] resolves the bearer token to a [`TenantId`]
/// before any blob op is dispatched.
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync + fmt::Debug {
    /// Open an upload session. Returns the server-allocated UUID the
    /// client subsequently uses in `PATCH` + `PUT` URLs.
    async fn open_upload(&self, tenant: &TenantId) -> PortResult<String>;

    /// Append a chunk to an existing upload. Returns the new
    /// cumulative byte count (used to emit the OCI `Range:` header).
    /// Returning `Err` with the `"upload session not found: <uuid>"`
    /// shape triggers the [`crate::error::OciAdapterError::UploadSessionMissing`]
    /// wire mapping.
    async fn append_chunk(
        &self,
        tenant: &TenantId,
        upload_uuid: &str,
        chunk: Bytes,
    ) -> PortResult<u64>;

    /// Finalize an upload: concatenate all chunks, return the assembled
    /// bytes for declared-digest verification at the call site, AND
    /// persist under the `(tenant, blob_key)` slot. `blob_key` is the
    /// OCI digest wire string (`"sha256:<hex>"`); the implementation
    /// MAY further translate to a CAS-internal address.
    async fn finalize_upload(
        &self,
        tenant: &TenantId,
        upload_uuid: &str,
        blob_key: &str,
    ) -> PortResult<Bytes>;

    /// Cancel + reap an upload session (called when declared-digest
    /// verification fails; the half-uploaded bytes MUST be discarded).
    async fn cancel_upload(&self, tenant: &TenantId, upload_uuid: &str) -> PortResult<()>;

    /// Fetch a persisted blob by `(tenant, blob_key)`. Returns `None`
    /// for not-found; the [`crate::pull::blob`] handler translates to
    /// `404`.
    async fn get_blob(&self, tenant: &TenantId, blob_key: &str) -> PortResult<Option<Bytes>>;

    /// Cheap existence check (`HEAD /blobs/<digest>`).
    async fn blob_exists(&self, tenant: &TenantId, blob_key: &str) -> PortResult<bool>;
}

/// KV port for manifests + tag lists. Wave-33's `corelink-adapters-cloud::cf::kv`
/// is the production binding (CF Worker KV); this trait is the
/// abstraction the OCI adapter writes against.
///
/// Keys are constructed per oci.md §3:
///
/// - `oci_manifest:<repo>:<reference>` (tenant prefix is implicit via the
///   tenant arg — every key is per-tenant scoped at the impl layer).
/// - `oci_tags:<repo>` for the tag-list cache.
/// - `oci_blob_index:<oci-digest>` for the OCI-digest → CAS-key bridge.
#[async_trait::async_trait]
pub trait ManifestKvStore: Send + Sync + fmt::Debug {
    /// Get raw bytes for `(tenant, key)`.
    async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>>;

    /// Put bytes at `(tenant, key)`. Overwrites without CAS — the
    /// OCI adapter relies on per-key serial atomicity at this port.
    async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()>;

    /// List keys under a prefix (used for `/tags/list` derivation
    /// when a tag-list cache is absent). Returns `Vec<String>` of
    /// matching keys, full form (NOT stripped of prefix).
    async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>>;
}

/// PAT → [`TenantId`] resolver port. Wave-33 Stream B's
/// `corelink-auth` aggregator hosts the canonical PAT verify
/// implementation; this port is the adapter-local shim until a
/// canonical `TenantResolver` trait lands.
#[async_trait::async_trait]
pub trait TenantResolver: Send + Sync + fmt::Debug {
    /// Resolve a PAT (the secret form a client puts in `Authorization:
    /// Basic <base64(user:pat)>`) to a [`TenantId`]. Returns `Err`
    /// with a wire-shape message on invalid / expired / revoked.
    async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId>;
}

/// Test fakes used by the integration tests and by [`crate::config`]
/// doc-style smoke tests. NEVER reachable from production wiring.
pub mod testing {
    use super::*;

    /// In-memory [`BlobStore`] backed by a `HashMap` per tenant.
    #[derive(Default)]
    pub struct InMemoryBlobStore {
        inner: Arc<Mutex<InMemoryBlobStoreState>>,
    }

    impl fmt::Debug for InMemoryBlobStore {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("InMemoryBlobStore").finish_non_exhaustive()
        }
    }

    #[derive(Default)]
    struct InMemoryBlobStoreState {
        /// `(tenant, blob_key) → bytes`
        blobs: HashMap<(TenantId, String), Bytes>,
        /// `(tenant, upload_uuid) → accumulated chunks`
        uploads: HashMap<(TenantId, String), Vec<u8>>,
    }

    #[async_trait::async_trait]
    impl BlobStore for InMemoryBlobStore {
        async fn open_upload(&self, tenant: &TenantId) -> PortResult<String> {
            let uuid = uuid::Uuid::new_v4().simple().to_string();
            let mut g = self.inner.lock();
            g.uploads.insert((*tenant, uuid.clone()), Vec::new());
            Ok(uuid)
        }

        async fn append_chunk(
            &self,
            tenant: &TenantId,
            upload_uuid: &str,
            chunk: Bytes,
        ) -> PortResult<u64> {
            let mut g = self.inner.lock();
            let buf = g
                .uploads
                .get_mut(&(*tenant, upload_uuid.to_string()))
                .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?;
            buf.extend_from_slice(&chunk);
            let len = u64::try_from(buf.len()).map_err(|e| e.to_string())?;
            Ok(len)
        }

        async fn finalize_upload(
            &self,
            tenant: &TenantId,
            upload_uuid: &str,
            blob_key: &str,
        ) -> PortResult<Bytes> {
            let mut g = self.inner.lock();
            let buf = g
                .uploads
                .remove(&(*tenant, upload_uuid.to_string()))
                .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?;
            let bytes = Bytes::from(buf);
            g.blobs.insert((*tenant, blob_key.to_string()), bytes.clone());
            Ok(bytes)
        }

        async fn cancel_upload(&self, tenant: &TenantId, upload_uuid: &str) -> PortResult<()> {
            let mut g = self.inner.lock();
            g.uploads.remove(&(*tenant, upload_uuid.to_string()));
            Ok(())
        }

        async fn get_blob(
            &self,
            tenant: &TenantId,
            blob_key: &str,
        ) -> PortResult<Option<Bytes>> {
            let g = self.inner.lock();
            Ok(g.blobs.get(&(*tenant, blob_key.to_string())).cloned())
        }

        async fn blob_exists(&self, tenant: &TenantId, blob_key: &str) -> PortResult<bool> {
            let g = self.inner.lock();
            Ok(g.blobs.contains_key(&(*tenant, blob_key.to_string())))
        }
    }

    /// In-memory [`ManifestKvStore`] backed by a `BTreeMap` per
    /// tenant (BTreeMap so `list_prefix` is deterministic).
    #[derive(Default)]
    pub struct InMemoryKv {
        inner: Arc<Mutex<std::collections::BTreeMap<(TenantId, String), Bytes>>>,
    }

    impl fmt::Debug for InMemoryKv {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("InMemoryKv").finish_non_exhaustive()
        }
    }

    #[async_trait::async_trait]
    impl ManifestKvStore for InMemoryKv {
        async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
            let g = self.inner.lock();
            Ok(g.get(&(*tenant, key.to_string())).cloned())
        }

        async fn put(
            &self,
            tenant: &TenantId,
            key: &str,
            value: Bytes,
        ) -> PortResult<()> {
            let mut g = self.inner.lock();
            g.insert((*tenant, key.to_string()), value);
            Ok(())
        }

        async fn list_prefix(
            &self,
            tenant: &TenantId,
            prefix: &str,
        ) -> PortResult<Vec<String>> {
            let g = self.inner.lock();
            let mut out = Vec::new();
            for (k_tenant, k_str) in g.keys() {
                if k_tenant == tenant && k_str.starts_with(prefix) {
                    out.push(k_str.clone());
                }
            }
            Ok(out)
        }
    }

    /// Static [`TenantResolver`] backed by a `HashMap<PatHex, TenantId>`.
    ///
    /// Comparison is by raw-string equality on the SECRET — fine for
    /// tests; production goes through Argon2id via the real
    /// `corelink-pat` consumer.
    pub struct StaticTenantResolver {
        inner: Arc<Mutex<HashMap<String, TenantId>>>,
    }

    impl Default for StaticTenantResolver {
        fn default() -> Self {
            // Default: one tenant pinned to PAT `"hugr-pat-test"`.
            let mut m = HashMap::new();
            m.insert(
                String::from("hugr-pat-test"),
                TenantId::from_uuid(uuid::Uuid::nil()),
            );
            Self {
                inner: Arc::new(Mutex::new(m)),
            }
        }
    }

    impl fmt::Debug for StaticTenantResolver {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("StaticTenantResolver").finish_non_exhaustive()
        }
    }

    impl StaticTenantResolver {
        /// Add or overwrite a `(pat → tenant)` mapping.
        pub fn insert(&self, pat: impl Into<String>, tenant: TenantId) {
            self.inner.lock().insert(pat.into(), tenant);
        }
    }

    #[async_trait::async_trait]
    impl TenantResolver for StaticTenantResolver {
        async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId> {
            use secrecy::ExposeSecret as _;
            let g = self.inner.lock();
            let plaintext = pat.as_secret_string().expose_secret();
            g.get(plaintext)
                .copied()
                .ok_or_else(|| String::from("unknown pat"))
        }
    }
}

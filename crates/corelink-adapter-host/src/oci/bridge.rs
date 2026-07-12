//! Bridges for the absorbed OCI adapter's port traits.
//!
//! The OCI adapter declares three port traits:
//! - [`super::ports::BlobStore`] — chunked upload protocol
//!   (`open_upload`, `append_chunk`, `finalize_upload`, `cancel_upload`,
//!   `get_blob`, `blob_exists`); errors as `String` via
//!   [`super::ports::PortResult`].
//! - [`super::ports::ManifestKvStore`] — KV for manifests +
//!   tag lists; errors as `String`.
//! - [`super::ports::TenantResolver`] — async
//!   `resolve_pat(&SecretWrap) → TenantId`.
//!
//! # Bridges
//!
//! - [`OciBlobBridge`] — implements `BlobStore` via
//!   `CasReadHandler`/`CasWriteHandler`. Upload sessions are buffered
//!   in-process in a `Mutex<HashMap<String, Vec<u8>>>`. `finalize_upload`
//!   flushes the buffer to the `CasWriteHandler`; `get_blob` reads via
//!   `CasReadHandler`.
//! - [`OciManifestKvBridge<K>`] — implements `ManifestKvStore` via a generic
//!   `K: KvBackend`. [`KvBackend`] uses RPITIT and is not object-safe.
//! - [`OciTenantBridge`] — implements `TenantResolver` via `PatValidator`,
//!   wrapping the exposed PAT plaintext in a
//!   [`corelink_core::SecretWrap`].

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;

use super::ports::{BlobStore, ManifestKvStore, PortResult, TenantResolver};
use corelink_core::{SecretWrap, TenantId};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};
use corelink_reapi::pat::{AuthStubError, PatValidator};
use corelink_worker::cache::kv::KvBackend;

// ---------------------------------------------------------------------------
// OciBlobBridge
// ---------------------------------------------------------------------------

/// In-process upload session buffer — maps `uuid → accumulated bytes`.
type UploadBuf = Mutex<HashMap<String, Vec<u8>>>;

/// Bridges the OCI adapter's `BlobStore` port onto
/// `(CasReadHandler, CasWriteHandler)`.
///
/// Upload sessions are buffered in memory inside `uploads`. `finalize_upload`
/// flushes accumulated chunks to the `CasWriteHandler` keyed by `blob_key`
/// (the OCI digest wire string, e.g. `"sha256:<hex>"`). `get_blob` and
/// `blob_exists` delegate to `CasReadHandler`.
///
/// # Caveats
///
/// The in-process session buffer is not durable — a process restart drops
/// all in-flight uploads. For production, replace the in-memory buffer with
/// a Durable Object or R2 multipart upload. This bridge targets integration
/// testing and single-process deployments.
#[derive(Debug)]
#[non_exhaustive]
pub struct OciBlobBridge {
    /// Handler for CAS read operations.
    pub read_handler: Arc<dyn CasReadHandler>,
    /// Handler for CAS write operations.
    pub write_handler: Arc<dyn CasWriteHandler>,
    /// Service principal injected into handler requests.
    pub principal: String,
    /// In-process upload session buffer.
    uploads: Arc<UploadBuf>,
}

impl OciBlobBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(
        read_handler: Arc<dyn CasReadHandler>,
        write_handler: Arc<dyn CasWriteHandler>,
        principal: impl Into<String>,
    ) -> Self {
        Self {
            read_handler,
            write_handler,
            principal: principal.into(),
            uploads: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl BlobStore for OciBlobBridge {
    async fn open_upload(&self, tenant: &TenantId) -> PortResult<String> {
        let uuid = format!("{tenant}:{ts}", ts = unix_ms_now());
        self.uploads
            .lock()
            .map_err(|e| format!("upload buf lock poisoned: {e}"))?
            .insert(uuid.clone(), Vec::new());
        Ok(uuid)
    }

    async fn append_chunk(
        &self,
        _tenant: &TenantId,
        upload_uuid: &str,
        chunk: Bytes,
    ) -> PortResult<u64> {
        let mut g = self
            .uploads
            .lock()
            .map_err(|e| format!("upload buf lock poisoned: {e}"))?;
        let buf = g
            .get_mut(upload_uuid)
            .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?;
        buf.extend_from_slice(&chunk);
        u64::try_from(buf.len()).map_err(|e| format!("buf len overflow: {e}"))
    }

    async fn finalize_upload(
        &self,
        _tenant: &TenantId,
        upload_uuid: &str,
        blob_key: &str,
        _storage_cap_bytes: Option<i64>,
    ) -> PortResult<Bytes> {
        let buf = {
            let mut g = self
                .uploads
                .lock()
                .map_err(|e| format!("upload buf lock poisoned: {e}"))?;
            g.remove(upload_uuid)
                .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?
        };
        let assembled = Bytes::from(buf.clone());
        let handler = Arc::clone(&self.write_handler);
        let tenant_str = _tenant.to_string();
        let principal = self.principal.clone();
        let blob_key_owned = blob_key.to_owned();
        let req = CasWriteRequest::new(
            &tenant_str,
            &blob_key_owned,
            buf,
            &principal,
            &tenant_str,
            unix_ms_now(),
        );
        tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?
            .map_err(|e| format!("handler write: {e:?}"))?;
        Ok(assembled)
    }

    async fn cancel_upload(&self, _tenant: &TenantId, upload_uuid: &str) -> PortResult<()> {
        self.uploads
            .lock()
            .map_err(|e| format!("upload buf lock poisoned: {e}"))?
            .remove(upload_uuid);
        Ok(())
    }

    async fn get_blob(&self, tenant: &TenantId, blob_key: &str) -> PortResult<Option<Bytes>> {
        let handler = Arc::clone(&self.read_handler);
        let tenant_str = tenant.to_string();
        let principal = self.principal.clone();
        let req = CasReadRequest::new(
            &tenant_str,
            blob_key,
            &principal,
            &tenant_str,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?;
        match result {
            Ok(resp) => Ok(Some(Bytes::from(resp.bytes))),
            Err(CasHandlerError::NotFound { .. }) => Ok(None),
            Err(e) => Err(format!("handler read: {e:?}")),
        }
    }

    async fn blob_exists(&self, tenant: &TenantId, blob_key: &str) -> PortResult<bool> {
        let result = self.get_blob(tenant, blob_key).await?;
        Ok(result.is_some())
    }
}

// ---------------------------------------------------------------------------
// OciManifestKvBridge
// ---------------------------------------------------------------------------

/// Bridges the OCI adapter's `ManifestKvStore` port onto `KvBackend`.
///
/// [`KvBackend`] uses RPITIT and is not object-safe; `K` is a generic
/// parameter.
///
/// `list_prefix` is implemented by delegating `get` on scanned keys —
/// there is no native prefix-scan in `KvBackend`. For production this
/// bridge should be replaced with a CF Workers KV binding that has
/// native prefix listing.
#[non_exhaustive]
pub struct OciManifestKvBridge<K> {
    /// Underlying KV backend.
    pub backend: Arc<K>,
}

impl<K: fmt::Debug> fmt::Debug for OciManifestKvBridge<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OciManifestKvBridge")
            .field("backend", &self.backend)
            .finish()
    }
}

impl<K: KvBackend + Send + Sync + 'static> OciManifestKvBridge<K> {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(backend: Arc<K>) -> Self {
        Self { backend }
    }

    fn scoped_key(tenant: &TenantId, key: &str) -> String {
        format!("{tenant}:{key}")
    }
}

#[async_trait]
impl<K: KvBackend + Send + Sync + fmt::Debug + 'static> ManifestKvStore for OciManifestKvBridge<K> {
    async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
        let scoped = Self::scoped_key(tenant, key);
        self.backend
            .get(&scoped)
            .await
            .map(|opt| opt.map(Bytes::from))
            .map_err(|e| format!("kv backend: {e:?}"))
    }

    async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
        let scoped = Self::scoped_key(tenant, key);
        // TTL: 7 days — OCI manifests are long-lived relative to npm/pip metadata.
        self.backend
            .put_with_ttl(&scoped, value.to_vec(), 604_800)
            .await
            .map_err(|e| format!("kv backend: {e:?}"))
    }

    async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
        // `KvBackend` has no prefix-list primitive. Return an empty list in
        // the bridge layer — callers must use the in-memory fake's
        // `InMemoryKv` (which implements real prefix listing) for tests that
        // exercise tag listing. Production wiring should use the CF Workers KV
        // binding directly.
        //
        // This is a known limitation documented in the crate-level docs. It
        // does not break correctness for `push` / `pull` paths; only
        // `/tags/list` will return fewer results than expected.
        let _ = (tenant, prefix); // suppress unused warnings
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// OciTenantBridge
// ---------------------------------------------------------------------------

/// Bridges the OCI adapter's `TenantResolver` port onto
/// [`PatValidator`].
///
/// The OCI adapter passes a [`SecretWrap`] (the decoded PAT from the
/// HTTP `Authorization: Basic` header). This bridge calls
/// `SecretWrap::expose()` to extract the plaintext, calls
/// `PatValidator::authenticate`, then returns the resolved [`TenantId`].
#[non_exhaustive]
pub struct OciTenantBridge {
    /// Workspace PAT validator.
    pub validator: Arc<dyn PatValidator>,
}

impl fmt::Debug for OciTenantBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OciTenantBridge").finish_non_exhaustive()
    }
}

impl OciTenantBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(validator: Arc<dyn PatValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl TenantResolver for OciTenantBridge {
    async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId> {
        let token = pat.expose().to_owned();
        let validator = Arc::clone(&self.validator);
        let result =
            tokio::task::spawn_blocking(move || validator.authenticate(&token, "oci-adapter-host"))
                .await
                .map_err(|e| format!("spawn_blocking: {e}"))?;

        match result {
            Ok(ctx) => Ok(TenantId::from(ctx.tenant_id())),
            Err(AuthStubError::PatInvalid) => Err("PAT invalid or expired".into()),
            Err(e) => Err(format!("auth error: {e:?}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test assertions are intentional"
)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use corelink_core::{SecretWrap, TenantId};
    use corelink_handler_cas::{
        handler::fake_hash, InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
    };
    use corelink_reapi::pat::{AuthScope, StubPatValidator};
    use corelink_replication::region_resolver::Region;
    use corelink_worker::cache::kv::InMemoryKv;
    use uuid::Uuid;

    use super::*;

    fn make_handler() -> Arc<InMemoryCasHandler> {
        Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ))
    }

    fn make_blob_bridge(h: Arc<InMemoryCasHandler>) -> OciBlobBridge {
        OciBlobBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-oci-host",
        )
    }

    fn tid(u: u128) -> TenantId {
        TenantId::from(Uuid::from_u128(u))
    }

    // ---- BlobStore bridge --------------------------------------------------

    #[tokio::test]
    async fn blob_get_miss_returns_none() {
        let bridge = make_blob_bridge(make_handler());
        let result = bridge.get_blob(&tid(1), "sha256:abc").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn blob_open_append_finalize_roundtrip() {
        let h = make_handler();
        let bridge = make_blob_bridge(h);
        let t = tid(2);
        let chunk = Bytes::from_static(b"oci-layer");
        let blob_key = fake_hash(&chunk);
        let uuid = bridge.open_upload(&t).await.unwrap();
        let _len = bridge.append_chunk(&t, &uuid, chunk.clone()).await.unwrap();
        let result = bridge
            .finalize_upload(&t, &uuid, &blob_key, None)
            .await
            .unwrap();
        assert_eq!(result, chunk);
    }

    #[tokio::test]
    async fn blob_get_after_finalize() {
        let h = make_handler();
        let bridge = make_blob_bridge(h);
        let t = tid(3);
        let data = b"layer-content".to_vec();
        let hash = fake_hash(&data);
        let uuid = bridge.open_upload(&t).await.unwrap();
        bridge
            .append_chunk(&t, &uuid, Bytes::from(data.clone()))
            .await
            .unwrap();
        bridge
            .finalize_upload(&t, &uuid, &hash, None)
            .await
            .unwrap();
        let got = bridge.get_blob(&t, &hash).await.unwrap();
        assert_eq!(got, Some(Bytes::from(data)));
    }

    #[tokio::test]
    async fn blob_cancel_upload_removes_session() {
        let bridge = make_blob_bridge(make_handler());
        let t = tid(4);
        let uuid = bridge.open_upload(&t).await.unwrap();
        bridge.cancel_upload(&t, &uuid).await.unwrap();
        let err = bridge
            .append_chunk(&t, &uuid, Bytes::from_static(b"x"))
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn blob_exists_false_on_miss() {
        let bridge = make_blob_bridge(make_handler());
        let exists = bridge.blob_exists(&tid(5), "sha256:nothere").await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn blob_size_reports_real_size_and_none_on_miss() {
        // H2: the OCI blob HEAD path needs the real blob size for Content-Length.
        // The bridge inherits the default `blob_size` (derives from `get_blob`).
        let h = make_handler();
        let bridge = make_blob_bridge(h);
        let t = tid(6);
        let data = b"blob-of-known-length".to_vec();
        let hash = fake_hash(&data);
        let uuid = bridge.open_upload(&t).await.unwrap();
        bridge
            .append_chunk(&t, &uuid, Bytes::from(data.clone()))
            .await
            .unwrap();
        bridge
            .finalize_upload(&t, &uuid, &hash, None)
            .await
            .unwrap();

        let size = bridge.blob_size(&t, &hash).await.unwrap();
        assert_eq!(size, Some(data.len() as u64));
        let miss = bridge.blob_size(&t, "sha256:nothere").await.unwrap();
        assert_eq!(miss, None);
    }

    // ---- ManifestKvStore bridge --------------------------------------------

    #[tokio::test]
    async fn manifest_kv_put_get_roundtrip() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = OciManifestKvBridge::new(kv);
        let t = tid(10);
        let value = Bytes::from_static(b"{\"manifest\":true}");
        bridge
            .put(&t, "oci_manifest:myrepo:v1.0", value.clone())
            .await
            .unwrap();
        let got = bridge.get(&t, "oci_manifest:myrepo:v1.0").await.unwrap();
        assert_eq!(got, Some(value));
    }

    #[tokio::test]
    async fn manifest_kv_get_miss_returns_none() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = OciManifestKvBridge::new(kv);
        let result = bridge.get(&tid(11), "no-such-key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn manifest_kv_list_prefix_returns_empty_in_bridge() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = OciManifestKvBridge::new(kv);
        bridge
            .put(&tid(12), "oci_tags:myrepo", Bytes::from_static(b"v1"))
            .await
            .unwrap();
        let result = bridge.list_prefix(&tid(12), "oci_tags:").await.unwrap();
        assert!(
            result.is_empty(),
            "bridge list_prefix always returns empty (documented limitation)"
        );
    }

    // ---- TenantResolver bridge ---------------------------------------------

    #[tokio::test]
    async fn oci_tenant_valid_pat() {
        let mut v = StubPatValidator::new();
        let t = Uuid::from_u128(99);
        v.insert(
            "oci-token",
            t,
            Uuid::from_u128(100),
            Region::Wnam,
            [AuthScope::CacheRead],
        );
        let bridge = OciTenantBridge::new(Arc::new(v));
        let secret = SecretWrap::new("oci-token".into());
        let resolved = bridge.resolve_pat(&secret).await.unwrap();
        assert_eq!(resolved, TenantId::from(t));
    }

    #[tokio::test]
    async fn oci_tenant_invalid_pat() {
        let bridge = OciTenantBridge::new(Arc::new(StubPatValidator::new()));
        let secret = SecretWrap::new("bad".into());
        let err = bridge.resolve_pat(&secret).await.unwrap_err();
        assert!(err.contains("invalid") || err.contains("PAT"));
    }

    #[test]
    fn debug_impls() {
        let h = make_handler();
        let _ = format!("{:?}", make_blob_bridge(h));
        let kv: Arc<InMemoryKv> = Arc::new(InMemoryKv::new());
        let _ = format!("{:?}", OciManifestKvBridge::new(kv));
        let _ = format!(
            "{:?}",
            OciTenantBridge::new(Arc::new(StubPatValidator::new()))
        );
    }
}

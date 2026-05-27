//! Bridges for the absorbed pip adapter's port traits.
//!
//! The pip adapter declares three port traits:
//! - [`super::ports::CasStore`] — async CAS `get`/`put` by
//!   `(&TenantId, &Digest)`, errors as [`super::error::PipAdapterError`].
//! - [`super::ports::KvStore`] — async metadata KV `get`/`put` with
//!   a `(value, inserted_at_unix_ms)` tuple.
//! - [`super::ports::TenantResolver`] — async PAT → [`TenantId`].
//!
//! # Bridges
//!
//! - [`PipCasBridge`] — implements `CasStore` via `CasReadHandler`/`CasWriteHandler`.
//! - [`PipKvBridge<K>`] — implements `KvStore` via a generic `K: KvBackend`. Note:
//!   [`corelink_worker::cache::kv::KvBackend`] uses RPITIT and is not object-safe.
//! - [`PipTenantBridge`] — implements `TenantResolver` via `PatValidator`.
//!
//! The timestamp encoding is identical to the npm bridge: `inserted_at_unix_ms`
//! is prepended as an 8-byte big-endian prefix in the stored KV value.

use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::error::PipAdapterError;
use super::ports::{CasStore, KvStore, TenantResolver};
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};
use corelink_reapi::pat::{AuthStubError, PatValidator};
use corelink_worker::cache::kv::KvBackend;

// ---------------------------------------------------------------------------
// PipCasBridge
// ---------------------------------------------------------------------------

/// Bridges the pip adapter's `CasStore` port onto
/// `(CasReadHandler, CasWriteHandler)`.
#[derive(Debug)]
#[non_exhaustive]
pub struct PipCasBridge {
    /// Handler for CAS read operations.
    pub read_handler: Arc<dyn CasReadHandler>,
    /// Handler for CAS write operations.
    pub write_handler: Arc<dyn CasWriteHandler>,
    /// Service principal injected into handler requests.
    pub principal: String,
}

impl PipCasBridge {
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
        }
    }
}

#[async_trait]
impl CasStore for PipCasBridge {
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, PipAdapterError> {
        let handler = Arc::clone(&self.read_handler);
        let tenant_str = tenant.to_string();
        let hash_str = digest.to_hex();
        let principal = self.principal.clone();
        let req = CasReadRequest::new(&tenant_str, &hash_str, &principal, &tenant_str, unix_ms_now());
        let result = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .map_err(|e| PipAdapterError::Cas(format!("spawn_blocking: {e}")))?;
        match result {
            Ok(resp) => Ok(Some(resp.bytes)),
            Err(CasHandlerError::NotFound { .. }) => Ok(None),
            Err(e) => Err(PipAdapterError::Cas(format!("handler: {e:?}"))),
        }
    }

    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), PipAdapterError> {
        let handler = Arc::clone(&self.write_handler);
        let tenant_str = tenant.to_string();
        let hash_str = digest.to_hex();
        let principal = self.principal.clone();
        let req = CasWriteRequest::new(&tenant_str, &hash_str, bytes, &principal, &tenant_str, unix_ms_now());
        let result = tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| PipAdapterError::Cas(format!("spawn_blocking: {e}")))?;
        result.map(|_| ()).map_err(|e| PipAdapterError::Cas(format!("handler: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// PipKvBridge
// ---------------------------------------------------------------------------

/// Bridges the pip adapter's `KvStore` port onto `KvBackend`.
///
/// [`KvBackend`] uses RPITIT and is not object-safe; `K` is a generic
/// parameter. Use [`corelink_worker::cache::kv::InMemoryKv`] for tests.
///
/// # Encoding
///
/// `inserted_at_unix_ms` is stored as an 8-byte big-endian prefix before
/// the value bytes. This matches the npm bridge encoding exactly.
#[non_exhaustive]
pub struct PipKvBridge<K> {
    /// Underlying KV backend.
    pub backend: Arc<K>,
}

impl<K: fmt::Debug> fmt::Debug for PipKvBridge<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipKvBridge")
            .field("backend", &self.backend)
            .finish()
    }
}

impl<K: KvBackend + Send + Sync + 'static> PipKvBridge<K> {
    /// Construct a new bridge wrapping `backend`.
    #[must_use]
    pub fn new(backend: Arc<K>) -> Self {
        Self { backend }
    }

    fn scoped_key(tenant: &TenantId, key: &str) -> String {
        format!("{tenant}:{key}")
    }

    fn encode(inserted_at_unix_ms: u64, bytes: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + bytes.len());
        out.extend_from_slice(&inserted_at_unix_ms.to_be_bytes());
        out.extend_from_slice(&bytes);
        out
    }

    fn decode(raw: Vec<u8>) -> Result<(Vec<u8>, u64), PipAdapterError> {
        let prefix = raw
            .get(..8)
            .ok_or_else(|| PipAdapterError::Kv("stored value too short".into()))?;
        let ts_bytes: [u8; 8] = prefix
            .try_into()
            .map_err(|_| PipAdapterError::Kv("timestamp decode failed".into()))?;
        let ts = u64::from_be_bytes(ts_bytes);
        let value = raw
            .get(8..)
            .ok_or_else(|| PipAdapterError::Kv("stored value body missing".into()))?
            .to_vec();
        Ok((value, ts))
    }
}

#[async_trait]
impl<K: KvBackend + Send + Sync + fmt::Debug + 'static> KvStore for PipKvBridge<K> {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, PipAdapterError> {
        let scoped = Self::scoped_key(tenant, key);
        let raw = self
            .backend
            .get(&scoped)
            .await
            .map_err(|e| PipAdapterError::Kv(format!("backend: {e:?}")))?;
        match raw {
            None => Ok(None),
            Some(bytes) => {
                let (value, ts) = Self::decode(bytes)?;
                Ok(Some((value, ts)))
            }
        }
    }

    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), PipAdapterError> {
        let scoped = Self::scoped_key(tenant, key);
        let encoded = Self::encode(inserted_at_unix_ms, value);
        // TTL: 1 h — PyPI index pages are short-lived relative to npm metadata.
        self.backend
            .put_with_ttl(&scoped, encoded, 3_600)
            .await
            .map_err(|e| PipAdapterError::Kv(format!("backend: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// PipTenantBridge
// ---------------------------------------------------------------------------

/// Bridges the pip adapter's `TenantResolver` port onto
/// [`PatValidator`].
#[non_exhaustive]
pub struct PipTenantBridge {
    /// Workspace PAT validator.
    pub validator: Arc<dyn PatValidator>,
}

impl fmt::Debug for PipTenantBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipTenantBridge").finish_non_exhaustive()
    }
}

impl PipTenantBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(validator: Arc<dyn PatValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl TenantResolver for PipTenantBridge {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, PipAdapterError> {
        let validator = Arc::clone(&self.validator);
        let token = pat_plaintext.to_owned();
        let result = tokio::task::spawn_blocking(move || {
            validator.authenticate(&token, "pip-adapter-host")
        })
        .await
        .map_err(|e| PipAdapterError::Auth(format!("spawn_blocking: {e}")))?;

        match result {
            Ok(ctx) => Ok(TenantId::from(ctx.tenant_id())),
            Err(AuthStubError::PatInvalid) => Err(PipAdapterError::Auth("PAT invalid".into())),
            Err(e) => Err(PipAdapterError::Auth(format!("auth: {e:?}"))),
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

    use corelink_core::{Digest, TenantId};
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

    fn make_digest(b: &[u8]) -> Digest {
        Digest::from_hex(&fake_hash(b)).expect("fake_hash produces valid hex")
    }

    fn tid(u: u128) -> TenantId {
        TenantId::from(Uuid::from_u128(u))
    }

    // ---- CasStore bridge ---------------------------------------------------

    #[tokio::test]
    async fn pip_cas_miss_returns_none() {
        let h = make_handler();
        let bridge = PipCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-pip",
        );
        let d = make_digest(b"no-data");
        assert!(bridge.get(&tid(1), &d).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn pip_cas_roundtrip() {
        let h = make_handler();
        let bridge = PipCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-pip",
        );
        let bytes = b"wheel-bytes".to_vec();
        let d = make_digest(&bytes);
        bridge.put(&tid(1), &d, bytes.clone()).await.unwrap();
        assert_eq!(bridge.get(&tid(1), &d).await.unwrap(), Some(bytes));
    }

    #[tokio::test]
    async fn pip_cas_wrong_hash_errors() {
        let h = make_handler();
        let bridge = PipCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-pip",
        );
        let wrong = make_digest(b"wrong");
        let err = bridge.put(&tid(1), &wrong, b"actual".to_vec()).await;
        assert!(err.is_err());
    }

    // ---- KvStore bridge ---------------------------------------------------

    #[tokio::test]
    async fn pip_kv_roundtrip_preserves_timestamp() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = PipKvBridge::new(kv);
        let t = tid(5);
        let val = b"pypi-index".to_vec();
        let ts: u64 = 9_999_999;
        bridge.put(&t, "simple/requests", val.clone(), ts).await.unwrap();
        let got = bridge.get(&t, "simple/requests").await.unwrap();
        assert_eq!(got, Some((val, ts)));
    }

    #[tokio::test]
    async fn pip_kv_miss_returns_none() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = PipKvBridge::new(kv);
        assert!(bridge.get(&tid(6), "nothing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn pip_kv_tenant_isolated() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = PipKvBridge::new(kv);
        bridge.put(&tid(7), "k", b"v".to_vec(), 1).await.unwrap();
        assert!(bridge.get(&tid(8), "k").await.unwrap().is_none());
    }

    // ---- TenantResolver bridge ---------------------------------------------

    #[tokio::test]
    async fn pip_tenant_valid_pat() {
        let mut v = StubPatValidator::new();
        let t = Uuid::from_u128(77);
        v.insert("pip-tok", t, Uuid::from_u128(78), Region::Wnam, [AuthScope::CacheWrite]);
        let bridge = PipTenantBridge::new(Arc::new(v));
        let resolved = bridge.resolve("pip-tok").await.unwrap();
        assert_eq!(resolved, TenantId::from(t));
    }

    #[tokio::test]
    async fn pip_tenant_invalid_pat() {
        let bridge = PipTenantBridge::new(Arc::new(StubPatValidator::new()));
        let err = bridge.resolve("x").await.unwrap_err();
        assert!(matches!(err, PipAdapterError::Auth(_)));
    }

    #[test]
    fn debug_impls() {
        let h = make_handler();
        let _ = format!(
            "{:?}",
            PipCasBridge::new(
                Arc::clone(&h) as Arc<dyn CasReadHandler>,
                Arc::clone(&h) as Arc<dyn CasWriteHandler>,
                "p"
            )
        );
        let kv: Arc<InMemoryKv> = Arc::new(InMemoryKv::new());
        let _ = format!("{:?}", PipKvBridge::new(kv));
        let _ = format!("{:?}", PipTenantBridge::new(Arc::new(StubPatValidator::new())));
    }
}

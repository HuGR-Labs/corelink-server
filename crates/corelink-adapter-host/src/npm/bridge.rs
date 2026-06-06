//! Bridges for the absorbed npm adapter's port traits.
//!
//! The npm adapter declares three port traits:
//! - [`super::ports::CasStore`] — async CAS `get`/`put` by
//!   `(&TenantId, &Digest)`, errors as [`super::error::NpmAdapterError`].
//! - [`super::ports::KvStore`] — async metadata KV `get`/`put` with
//!   a `(value, inserted_at_unix_ms)` tuple.
//! - [`super::ports::TenantResolver`] — async PAT → [`TenantId`].
//!
//! # Bridges
//!
//! - [`NpmCasBridge`] — implements `CasStore` via `CasReadHandler`/`CasWriteHandler`.
//! - [`NpmKvBridge<K>`] — implements `KvStore` via a generic `K: KvBackend`. Note:
//!   [`corelink_worker::cache::kv::KvBackend`] uses RPITIT and is not object-safe;
//!   the bridge is therefore generic rather than `Arc<dyn KvBackend>`.
//! - [`NpmTenantBridge`] — implements `TenantResolver` via `PatValidator`.
//!
//! The `KvStore` semantic difference: the npm `KvStore::get` returns
//! `(bytes, inserted_at_unix_ms)` while `KvBackend` is a raw byte store
//! without timestamps. The bridge encodes the `inserted_at_unix_ms` as a
//! big-endian 8-byte prefix in the stored value so the round-trip is
//! lossless. Format: `[u64 BE: inserted_at_unix_ms][...bytes]`.

use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::error::NpmAdapterError;
use super::ports::{CasStore, KvStore, TenantResolver};
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};
use corelink_reapi::pat::{AuthStubError, PatValidator};
use corelink_worker::cache::kv::KvBackend;

// ---------------------------------------------------------------------------
// NpmCasBridge
// ---------------------------------------------------------------------------

/// Bridges the npm adapter's `CasStore` port onto
/// `(CasReadHandler, CasWriteHandler)`.
#[derive(Debug)]
#[non_exhaustive]
pub struct NpmCasBridge {
    /// Handler for CAS read operations.
    pub read_handler: Arc<dyn CasReadHandler>,
    /// Handler for CAS write operations.
    pub write_handler: Arc<dyn CasWriteHandler>,
    /// Service principal injected into handler requests.
    pub principal: String,
}

impl NpmCasBridge {
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
impl CasStore for NpmCasBridge {
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, NpmAdapterError> {
        let handler = Arc::clone(&self.read_handler);
        let tenant_str = tenant.to_string();
        let hash_str = digest.to_hex();
        let principal = self.principal.clone();
        let req = CasReadRequest::new(
            &tenant_str,
            &hash_str,
            &principal,
            &tenant_str,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .map_err(|e| NpmAdapterError::Cas(format!("spawn_blocking: {e}")))?;
        match result {
            Ok(resp) => Ok(Some(resp.bytes)),
            Err(CasHandlerError::NotFound { .. }) => Ok(None),
            Err(e) => Err(NpmAdapterError::Cas(format!("handler: {e:?}"))),
        }
    }

    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), NpmAdapterError> {
        let handler = Arc::clone(&self.write_handler);
        let tenant_str = tenant.to_string();
        let hash_str = digest.to_hex();
        let principal = self.principal.clone();
        let req = CasWriteRequest::new(
            &tenant_str,
            &hash_str,
            bytes,
            &principal,
            &tenant_str,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| NpmAdapterError::Cas(format!("spawn_blocking: {e}")))?;
        result
            .map(|_| ())
            .map_err(|e| NpmAdapterError::Cas(format!("handler: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// NpmKvBridge
// ---------------------------------------------------------------------------

/// Bridges the npm adapter's `KvStore` port onto `KvBackend`.
///
/// [`KvBackend`] uses RPITIT and is not object-safe; `K` is a generic
/// parameter rather than `Arc<dyn KvBackend>`. Use
/// [`corelink_worker::cache::kv::InMemoryKv`] for tests.
///
/// # Encoding
///
/// `KvStore::get` returns `(bytes, inserted_at_unix_ms)` but `KvBackend`
/// stores only raw bytes. The bridge prepends `inserted_at_unix_ms` as an
/// 8-byte big-endian prefix before delegating to `KvBackend::put_with_ttl`.
/// On `get`, the prefix is stripped and the timestamp is returned.
#[non_exhaustive]
pub struct NpmKvBridge<K> {
    /// Underlying KV backend.
    pub backend: Arc<K>,
}

impl<K: fmt::Debug> fmt::Debug for NpmKvBridge<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NpmKvBridge")
            .field("backend", &self.backend)
            .finish()
    }
}

impl<K: KvBackend + Send + Sync + 'static> NpmKvBridge<K> {
    /// Construct a new bridge wrapping `backend`.
    #[must_use]
    pub fn new(backend: Arc<K>) -> Self {
        Self { backend }
    }

    /// Build the scoped key: `"{tenant}:{key}"`.
    fn scoped_key(tenant: &TenantId, key: &str) -> String {
        format!("{tenant}:{key}")
    }

    /// Encode `(bytes, inserted_at_unix_ms)` into the wire format:
    /// `[u64 BE timestamp][bytes]`.
    fn encode(inserted_at_unix_ms: u64, bytes: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + bytes.len());
        out.extend_from_slice(&inserted_at_unix_ms.to_be_bytes());
        out.extend_from_slice(&bytes);
        out
    }

    /// Decode the wire format back into `(bytes, inserted_at_unix_ms)`.
    fn decode(raw: Vec<u8>) -> Result<(Vec<u8>, u64), NpmAdapterError> {
        let prefix = raw.get(..8).ok_or_else(|| {
            NpmAdapterError::Kv("stored value too short to decode timestamp".into())
        })?;
        let ts_bytes: [u8; 8] = prefix
            .try_into()
            .map_err(|_| NpmAdapterError::Kv("timestamp decode failed".into()))?;
        let ts = u64::from_be_bytes(ts_bytes);
        let value = raw
            .get(8..)
            .ok_or_else(|| NpmAdapterError::Kv("stored value too short to extract body".into()))?
            .to_vec();
        Ok((value, ts))
    }
}

#[async_trait]
impl<K: KvBackend + Send + Sync + fmt::Debug + 'static> KvStore for NpmKvBridge<K> {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
        let scoped = Self::scoped_key(tenant, key);
        let raw = self
            .backend
            .get(&scoped)
            .await
            .map_err(|e| NpmAdapterError::Kv(format!("backend: {e:?}")))?;
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
    ) -> Result<(), NpmAdapterError> {
        let scoped = Self::scoped_key(tenant, key);
        let encoded = Self::encode(inserted_at_unix_ms, value);
        // TTL: 24 h — matches npm metadata cache TTL from npm adapter config.
        self.backend
            .put_with_ttl(&scoped, encoded, 86_400)
            .await
            .map_err(|e| NpmAdapterError::Kv(format!("backend: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// NpmTenantBridge
// ---------------------------------------------------------------------------

/// Bridges the npm adapter's `TenantResolver` port onto
/// [`PatValidator`].
#[non_exhaustive]
pub struct NpmTenantBridge {
    /// Workspace PAT validator.
    pub validator: Arc<dyn PatValidator>,
}

impl fmt::Debug for NpmTenantBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NpmTenantBridge").finish_non_exhaustive()
    }
}

impl NpmTenantBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(validator: Arc<dyn PatValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl TenantResolver for NpmTenantBridge {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, NpmAdapterError> {
        let validator = Arc::clone(&self.validator);
        let token = pat_plaintext.to_owned();
        let result =
            tokio::task::spawn_blocking(move || validator.authenticate(&token, "npm-adapter-host"))
                .await
                .map_err(|e| NpmAdapterError::Auth(format!("spawn_blocking: {e}")))?;

        match result {
            Ok(ctx) => {
                let tid = TenantId::from(ctx.tenant_id());
                Ok(tid)
            }
            Err(AuthStubError::PatInvalid) => Err(NpmAdapterError::Auth("PAT invalid".into())),
            Err(e) => Err(NpmAdapterError::Auth(format!("auth: {e:?}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current wall-clock unix milliseconds.
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
    use corelink_worker::cache::kv::InMemoryKv;
    use uuid::Uuid;

    use super::*;

    fn make_handler() -> Arc<InMemoryCasHandler> {
        Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ))
    }

    fn make_digest_from_bytes(b: &[u8]) -> Digest {
        let hex = fake_hash(b);
        Digest::from_hex(&hex).expect("valid hex from fake_hash")
    }

    fn tenant(u: u128) -> TenantId {
        TenantId::from(Uuid::from_u128(u))
    }

    // ---- CasStore bridge ---------------------------------------------------

    #[tokio::test]
    async fn cas_get_miss_returns_none() {
        let h = make_handler();
        let bridge = NpmCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-npm",
        );
        let digest = make_digest_from_bytes(b"empty");
        let result = bridge.get(&tenant(1), &digest).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cas_put_then_get_roundtrip() {
        let h = make_handler();
        let bridge = NpmCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-npm",
        );
        let bytes = b"tarball-bytes".to_vec();
        let digest = make_digest_from_bytes(&bytes);
        bridge
            .put(&tenant(1), &digest, bytes.clone())
            .await
            .unwrap();
        let got = bridge.get(&tenant(1), &digest).await.unwrap();
        assert_eq!(got, Some(bytes));
    }

    #[tokio::test]
    async fn cas_put_wrong_hash_errors() {
        let h = make_handler();
        let bridge = NpmCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-npm",
        );
        let wrong_digest = make_digest_from_bytes(b"something-else");
        let err = bridge
            .put(&tenant(1), &wrong_digest, b"actual".to_vec())
            .await;
        assert!(err.is_err());
    }

    // ---- KvStore bridge ----------------------------------------------------

    #[tokio::test]
    async fn kv_put_then_get_roundtrip_with_timestamp() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = NpmKvBridge::new(kv);
        let t = tenant(2);
        let value = b"metadata-json".to_vec();
        let ts: u64 = 1_717_000_000_000;
        bridge
            .put(&t, "pkg@1.0.0", value.clone(), ts)
            .await
            .unwrap();
        let got = bridge.get(&t, "pkg@1.0.0").await.unwrap();
        assert_eq!(got, Some((value, ts)));
    }

    #[tokio::test]
    async fn kv_get_miss_returns_none() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = NpmKvBridge::new(kv);
        let result = bridge.get(&tenant(3), "no-such-key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn kv_tenant_scoping_isolates_keys() {
        let kv = Arc::new(InMemoryKv::new());
        let bridge = NpmKvBridge::new(kv);
        let ta = tenant(10);
        let tb = tenant(11);
        bridge
            .put(&ta, "key", b"ta-value".to_vec(), 1)
            .await
            .unwrap();
        let result = bridge.get(&tb, "key").await.unwrap();
        assert!(result.is_none());
    }

    // ---- TenantResolver bridge ---------------------------------------------

    #[tokio::test]
    async fn tenant_valid_pat_returns_tenant_id() {
        let mut v = StubPatValidator::new();
        let tid = Uuid::from_u128(55);
        v.insert(
            "npm-token",
            tid,
            Uuid::from_u128(56),
            corelink_replication::region_resolver::Region::Wnam,
            [AuthScope::CacheRead],
        );
        let bridge = NpmTenantBridge::new(Arc::new(v));
        let resolved = bridge.resolve("npm-token").await.unwrap();
        assert_eq!(resolved, TenantId::from(tid));
    }

    #[tokio::test]
    async fn tenant_invalid_pat_returns_auth_error() {
        let bridge = NpmTenantBridge::new(Arc::new(StubPatValidator::new()));
        let err = bridge.resolve("bad").await.unwrap_err();
        assert!(matches!(err, NpmAdapterError::Auth(_)));
    }

    #[test]
    fn debug_impls() {
        let h = make_handler();
        let _ = format!(
            "{:?}",
            NpmCasBridge::new(
                Arc::clone(&h) as Arc<dyn CasReadHandler>,
                Arc::clone(&h) as Arc<dyn CasWriteHandler>,
                "p"
            )
        );
        let kv: Arc<InMemoryKv> = Arc::new(InMemoryKv::new());
        let _ = format!("{:?}", NpmKvBridge::new(kv));
        let _ = format!(
            "{:?}",
            NpmTenantBridge::new(Arc::new(StubPatValidator::new()))
        );
    }
}

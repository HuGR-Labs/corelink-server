//! Bridges for `corelink-adapter-brew` port traits.
//!
//! The brew adapter declares two port traits in its `ports` module:
//! - [`corelink_adapter_brew::ports::CasStore`] — async `get`/`put` by
//!   `(tenant_id: &str, cas_key: &str)`, errors as
//!   [`corelink_adapter_brew::ports::CasError`].
//! - [`corelink_adapter_brew::ports::TenantResolver`] — async `resolve`
//!   a PAT plaintext to a tenant-id `String`, errors as
//!   [`corelink_adapter_brew::ports::TenantResolveError`].
//!
//! # Bridges
//!
//! - [`BrewCasBridge`] — implements `CasStore` by delegating `get` →
//!   [`corelink_handler_cas::CasReadHandler::read`] and `put` →
//!   [`corelink_handler_cas::CasWriteHandler::write`].
//! - [`BrewTenantBridge`] — implements `TenantResolver` by delegating to
//!   [`corelink_reapi::pat::PatValidator::authenticate`].
//!
//! The brew adapter's port surface is structurally identical to the cargo
//! adapter's, with the only differences being the error type names
//! (`BrewAdapterError`-adjacent `CasError`/`TenantResolveError`) and the
//! CAS key semantics (BLAKE3 of the canonicalized bottle URL rather than
//! a sccache BLAKE3 key).

use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use corelink_adapter_brew::ports::{
    CasError, CasStore, TenantResolveError, TenantResolver,
};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};
use corelink_reapi::pat::{AuthStubError, PatValidator};

// ---------------------------------------------------------------------------
// BrewCasBridge
// ---------------------------------------------------------------------------

/// Bridges `corelink-adapter-brew::ports::CasStore` onto the workspace
/// split-handler pair `(CasReadHandler, CasWriteHandler)`.
///
/// `principal` is the service-account identity injected into every
/// handler request, representing the adapter-host service.
#[derive(Debug)]
#[non_exhaustive]
pub struct BrewCasBridge {
    /// Handler for CAS read operations.
    pub read_handler: Arc<dyn CasReadHandler>,
    /// Handler for CAS write operations.
    pub write_handler: Arc<dyn CasWriteHandler>,
    /// Service principal injected into handler requests.
    pub principal: String,
}

impl BrewCasBridge {
    /// Construct a new bridge from the given split-handler pair.
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
impl CasStore for BrewCasBridge {
    async fn get(
        &self,
        tenant_id: &str,
        cas_key: &str,
    ) -> Result<Option<Vec<u8>>, CasError> {
        let handler = Arc::clone(&self.read_handler);
        let req = CasReadRequest::new(
            tenant_id,
            cas_key,
            self.principal.as_str(),
            tenant_id,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .map_err(|e| CasError::Backend(format!("spawn_blocking join: {e}")))?;
        match result {
            Ok(resp) => Ok(Some(resp.bytes)),
            Err(CasHandlerError::NotFound { .. }) => Ok(None),
            Err(e) => Err(CasError::Backend(format!("handler: {e:?}"))),
        }
    }

    async fn put(
        &self,
        tenant_id: &str,
        cas_key: &str,
        bytes: Vec<u8>,
    ) -> Result<(), CasError> {
        let handler = Arc::clone(&self.write_handler);
        let req = CasWriteRequest::new(
            tenant_id,
            cas_key,
            bytes,
            self.principal.as_str(),
            tenant_id,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| CasError::Backend(format!("spawn_blocking join: {e}")))?;
        result.map(|_| ()).map_err(|e| CasError::Backend(format!("handler: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// BrewTenantBridge
// ---------------------------------------------------------------------------

/// Bridges `corelink-adapter-brew::ports::TenantResolver` onto the
/// workspace [`PatValidator`].
#[non_exhaustive]
pub struct BrewTenantBridge {
    /// Workspace PAT validator.
    pub validator: Arc<dyn PatValidator>,
}

impl fmt::Debug for BrewTenantBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrewTenantBridge").finish_non_exhaustive()
    }
}

impl BrewTenantBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(validator: Arc<dyn PatValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl TenantResolver for BrewTenantBridge {
    async fn resolve(
        &self,
        pat_plaintext: &str,
    ) -> Result<String, TenantResolveError> {
        let validator = Arc::clone(&self.validator);
        let token = pat_plaintext.to_owned();
        let result = tokio::task::spawn_blocking(move || {
            validator.authenticate(&token, "brew-adapter-host")
        })
        .await
        .map_err(|e| TenantResolveError::Backend(format!("spawn_blocking join: {e}")))?;

        match result {
            Ok(ctx) => Ok(ctx.tenant_id().to_string()),
            Err(AuthStubError::PatInvalid) => Err(TenantResolveError::InvalidPat),
            Err(e) => Err(TenantResolveError::Backend(format!("auth: {e:?}"))),
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

    use corelink_handler_cas::{
        handler::fake_hash, InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
    };
    use corelink_reapi::pat::{AuthScope, StubPatValidator};
    use corelink_replication::region_resolver::Region;
    use uuid::Uuid;

    use super::*;

    fn make_handler() -> Arc<InMemoryCasHandler> {
        Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ))
    }

    fn make_cas_bridge(h: Arc<InMemoryCasHandler>) -> BrewCasBridge {
        BrewCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-brew-host",
        )
    }

    #[tokio::test]
    async fn cas_get_miss_returns_none() {
        let bridge = make_cas_bridge(make_handler());
        let result = bridge.get("brew-tenant", &"c".repeat(64)).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cas_put_then_get_roundtrip() {
        let h = make_handler();
        let bridge = make_cas_bridge(h);
        let bytes = b"bottle-data".to_vec();
        let hash = fake_hash(&bytes);
        bridge.put("brew-t1", &hash, bytes.clone()).await.unwrap();
        let got = bridge.get("brew-t1", &hash).await.unwrap();
        assert_eq!(got, Some(bytes));
    }

    #[tokio::test]
    async fn cas_put_wrong_hash_errors() {
        let bridge = make_cas_bridge(make_handler());
        let err = bridge.put("brew-t1", &"e".repeat(64), b"bottle".to_vec()).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn cas_multiple_tenants_isolated() {
        let h = make_handler();
        let bridge = make_cas_bridge(h);
        let bytes = b"brew-payload".to_vec();
        let hash = fake_hash(&bytes);
        bridge.put("tenant-a", &hash, bytes.clone()).await.unwrap();
        // tenant-b sees a miss for tenant-a's key
        let miss = bridge.get("tenant-b", &hash).await.unwrap();
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn tenant_bridge_valid_pat() {
        let mut v = StubPatValidator::new();
        let tid = Uuid::from_u128(100);
        v.insert("brew-token", tid, Uuid::from_u128(200), Region::Wnam, [AuthScope::CacheRead]);
        let bridge = BrewTenantBridge::new(Arc::new(v));
        let resolved = bridge.resolve("brew-token").await.unwrap();
        assert_eq!(resolved, tid.to_string());
    }

    #[tokio::test]
    async fn tenant_bridge_invalid_pat() {
        let bridge = BrewTenantBridge::new(Arc::new(StubPatValidator::new()));
        let err = bridge.resolve("no-such-pat").await.unwrap_err();
        assert!(matches!(err, TenantResolveError::InvalidPat));
    }

    #[test]
    fn debug_impls() {
        let h = make_handler();
        let _ = format!("{:?}", make_cas_bridge(h));
        let _ = format!("{:?}", BrewTenantBridge::new(Arc::new(StubPatValidator::new())));
    }
}

//! Bridges for the absorbed cargo adapter's port traits.
//!
//! The cargo adapter declares two port traits in its `ports` module:
//! - [`super::ports::CasStore`] — async `get`/`put` by
//!   `(tenant_id: &str, digest_hex: &str)`, errors as
//!   [`super::ports::CasError`].
//! - [`super::ports::TenantResolver`] — async `resolve`
//!   a PAT plaintext to a tenant-id `String`, errors as
//!   [`super::ports::TenantResolveError`].
//!
//! # Bridges
//!
//! - [`CargoCasBridge`] — implements `CasStore` by delegating `get` →
//!   [`corelink_handler_cas::CasReadHandler::read`] and `put` →
//!   [`corelink_handler_cas::CasWriteHandler::write`]. Both workspace
//!   handlers are sync; the bridge calls them via
//!   `tokio::task::spawn_blocking`.
//! - [`CargoTenantBridge`] — implements `TenantResolver` by delegating to
//!   [`corelink_reapi::pat::PatValidator::authenticate`] (also sync,
//!   called via `spawn_blocking`).

use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::ports::{CasError, CasStore, TenantResolveError, TenantResolver};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};
use corelink_reapi::pat::{AuthStubError, PatValidator};

// ---------------------------------------------------------------------------
// CargoCasBridge
// ---------------------------------------------------------------------------

/// Bridges the cargo adapter's `CasStore` port onto the workspace
/// split-handler pair `(CasReadHandler, CasWriteHandler)`.
///
/// `principal` is the service-account identity injected into every
/// handler request; it represents the adapter-host service identity, not
/// the end-user PAT (which was validated upstream by [`CargoTenantBridge`]).
#[derive(Debug)]
#[non_exhaustive]
pub struct CargoCasBridge {
    /// Handler for CAS read operations.
    pub read_handler: Arc<dyn CasReadHandler>,
    /// Handler for CAS write operations.
    pub write_handler: Arc<dyn CasWriteHandler>,
    /// Service principal injected into handler requests.
    pub principal: String,
}

impl CargoCasBridge {
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
impl CasStore for CargoCasBridge {
    async fn get(&self, tenant_id: &str, digest_hex: &str) -> Result<Option<Vec<u8>>, CasError> {
        let handler = Arc::clone(&self.read_handler);
        let req = CasReadRequest::new(
            tenant_id,
            digest_hex,
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

    async fn put(&self, tenant_id: &str, digest_hex: &str, bytes: Vec<u8>) -> Result<(), CasError> {
        let handler = Arc::clone(&self.write_handler);
        let req = CasWriteRequest::new(
            tenant_id,
            digest_hex,
            bytes,
            self.principal.as_str(),
            tenant_id,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| CasError::Backend(format!("spawn_blocking join: {e}")))?;
        result
            .map(|_| ())
            .map_err(|e| CasError::Backend(format!("handler: {e:?}")))
    }
}

// ---------------------------------------------------------------------------
// CargoTenantBridge
// ---------------------------------------------------------------------------

/// Bridges the cargo adapter's `TenantResolver` port onto the
/// workspace [`PatValidator`].
///
/// On a valid PAT, the resolved `TenantContext` is unwrapped to its
/// `tenant_id` UUID string, which is what the cargo adapter expects.
#[non_exhaustive]
pub struct CargoTenantBridge {
    /// Workspace PAT validator.
    pub validator: Arc<dyn PatValidator>,
}

impl fmt::Debug for CargoTenantBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CargoTenantBridge").finish_non_exhaustive()
    }
}

impl CargoTenantBridge {
    /// Construct a new bridge.
    #[must_use]
    pub fn new(validator: Arc<dyn PatValidator>) -> Self {
        Self { validator }
    }
}

#[async_trait]
impl TenantResolver for CargoTenantBridge {
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        let validator = Arc::clone(&self.validator);
        let token = pat_plaintext.to_owned();
        let result = tokio::task::spawn_blocking(move || {
            validator.authenticate(&token, "cargo-adapter-host")
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

/// Current wall-clock unix milliseconds. Falls back to `u64::MAX` on
/// overflow (defensive; all callers accept any u64).
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

    // ---- helpers -----------------------------------------------------------

    fn make_inmem_handler() -> Arc<InMemoryCasHandler> {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        Arc::new(InMemoryCasHandler::new(audit, sli))
    }

    fn make_cas_bridge(h: Arc<InMemoryCasHandler>) -> CargoCasBridge {
        CargoCasBridge::new(
            Arc::clone(&h) as Arc<dyn CasReadHandler>,
            Arc::clone(&h) as Arc<dyn CasWriteHandler>,
            "svc-cargo-host",
        )
    }

    // ---- CasStore bridge tests ---------------------------------------------

    #[tokio::test]
    async fn cas_get_miss_returns_none() {
        let h = make_inmem_handler();
        let bridge = make_cas_bridge(h);
        let result = bridge.get("t1", &"a".repeat(64)).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cas_put_then_get_roundtrip() {
        let h = make_inmem_handler();
        let bridge = make_cas_bridge(h);
        let bytes = b"hello cargo".to_vec();
        let hash = fake_hash(&bytes);
        bridge.put("t1", &hash, bytes.clone()).await.unwrap();
        let got = bridge.get("t1", &hash).await.unwrap();
        assert_eq!(got, Some(bytes));
    }

    #[tokio::test]
    async fn cas_get_different_tenant_returns_none() {
        let h = make_inmem_handler();
        let bridge = make_cas_bridge(Arc::clone(&h));
        let bytes = b"data".to_vec();
        let hash = fake_hash(&bytes);
        h.seed("t1", &hash, bytes).expect("seed");
        let result = bridge.get("t2", &hash).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cas_put_hash_mismatch_surfaces_as_cas_error() {
        let h = make_inmem_handler();
        let bridge = make_cas_bridge(h);
        let err = bridge.put("t1", &"f".repeat(64), b"data".to_vec()).await;
        assert!(err.is_err(), "hash mismatch must surface as Err");
        let e = err.unwrap_err();
        assert!(matches!(e, CasError::Backend(_)));
    }

    #[tokio::test]
    async fn cas_put_idempotent_second_write_succeeds() {
        let h = make_inmem_handler();
        let bridge = make_cas_bridge(h);
        let bytes = b"idempotent".to_vec();
        let hash = fake_hash(&bytes);
        bridge.put("t1", &hash, bytes.clone()).await.unwrap();
        bridge.put("t1", &hash, bytes).await.unwrap();
    }

    // ---- TenantResolver bridge tests ---------------------------------------

    #[tokio::test]
    async fn tenant_resolver_valid_pat_returns_tenant_id_string() {
        let mut v = StubPatValidator::new();
        let tid = Uuid::from_u128(42);
        let pid = Uuid::from_u128(7);
        v.insert("my-pat", tid, pid, Region::Wnam, [AuthScope::CacheRead]);
        let bridge = CargoTenantBridge::new(Arc::new(v));
        let result = bridge.resolve("my-pat").await.unwrap();
        assert_eq!(result, tid.to_string());
    }

    #[tokio::test]
    async fn tenant_resolver_invalid_pat_returns_invalid_pat_error() {
        let v = StubPatValidator::new();
        let bridge = CargoTenantBridge::new(Arc::new(v));
        let err = bridge.resolve("bad-pat").await.unwrap_err();
        assert!(matches!(err, TenantResolveError::InvalidPat));
    }

    // ---- Debug impl verification -------------------------------------------

    #[test]
    fn bridges_implement_debug() {
        let h = make_inmem_handler();
        let cas = make_cas_bridge(h);
        let _ = format!("{cas:?}");
        let tenant = CargoTenantBridge::new(Arc::new(StubPatValidator::new()));
        let _ = format!("{tenant:?}");
    }
}

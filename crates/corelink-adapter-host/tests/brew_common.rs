//! Shared test fixtures: in-memory CAS, in-memory PAT resolver,
//! adapter spin-up helpers.
//!
//! Used by `smoke`, `adversarial`, and any other integration test that
//! needs to drive the brew adapter end-to-end.

#![allow(
    dead_code,
    missing_docs,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items,
    clippy::len_without_is_empty
)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use corelink_adapter_host::brew::config::DEFAULT_BOTTLE_SIZE_LIMIT_BYTES;
use corelink_adapter_host::brew::ports::{CasError, CasStore, TenantResolveError, TenantResolver};
use corelink_adapter_host::brew::server::build_router;
use corelink_adapter_host::brew::BrewAdapterConfig;
use corelink_audit::ports::InMemoryAuditEmitter;
use parking_lot_helper::Mutex;
use url::Url;

mod parking_lot_helper {
    pub use std::sync::Mutex;
}

#[derive(Debug, Default)]
pub struct InMemoryCas {
    inner: Mutex<HashMap<(String, String), Vec<u8>>>,
}

impl InMemoryCas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn keys(&self) -> Vec<(String, String)> {
        self.inner.lock().unwrap().keys().cloned().collect()
    }
}

#[async_trait]
impl CasStore for InMemoryCas {
    async fn get(
        &self,
        tenant_id: &str,
        cas_key: &str,
    ) -> Result<Option<Vec<u8>>, CasError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(&(tenant_id.to_owned(), cas_key.to_owned()))
            .cloned())
    }

    async fn put(
        &self,
        tenant_id: &str,
        cas_key: &str,
        bytes: Vec<u8>,
    ) -> Result<(), CasError> {
        self.inner
            .lock()
            .unwrap()
            .insert((tenant_id.to_owned(), cas_key.to_owned()), bytes);
        Ok(())
    }
}

/// A CAS implementor that always fails — used to exercise the
/// `BrewAdapterError::Cas` path.
#[derive(Debug, Default)]
pub struct FailingCas;

#[async_trait]
impl CasStore for FailingCas {
    async fn get(
        &self,
        _tenant_id: &str,
        _cas_key: &str,
    ) -> Result<Option<Vec<u8>>, CasError> {
        Err(CasError::Backend("intentional get failure".to_owned()))
    }

    async fn put(
        &self,
        _tenant_id: &str,
        _cas_key: &str,
        _bytes: Vec<u8>,
    ) -> Result<(), CasError> {
        Err(CasError::Backend("intentional put failure".to_owned()))
    }
}

#[derive(Debug, Default)]
pub struct StaticTenantResolver {
    map: HashMap<String, String>,
}

impl StaticTenantResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, pat: impl Into<String>, tenant: impl Into<String>) -> Self {
        self.map.insert(pat.into(), tenant.into());
        self
    }
}

#[async_trait]
impl TenantResolver for StaticTenantResolver {
    async fn resolve(
        &self,
        pat_plaintext: &str,
    ) -> Result<String, TenantResolveError> {
        self.map
            .get(pat_plaintext)
            .cloned()
            .ok_or(TenantResolveError::InvalidPat)
    }
}

/// Build a fully wired adapter pointed at `upstream_url`, with the
/// shared in-memory CAS / resolver / audit sink. Returns the bind
/// address (`127.0.0.1:<ephemeral>`) and the live `JoinHandle`.
pub async fn spin_adapter(
    upstream_url: Url,
    cas: Arc<dyn CasStore>,
    resolver: Arc<dyn TenantResolver>,
    audit: Arc<InMemoryAuditEmitter>,
    bottle_size_limit_bytes: u64,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = BrewAdapterConfig::new(
        bind_addr,
        upstream_url,
        bottle_size_limit_bytes,
        cas,
        resolver,
        audit,
    );

    let router = build_router(config).unwrap();
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    let actual = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (actual, handle)
}

/// Default bottle size limit (2 GiB) wrapped for ergonomics.
pub fn default_bottle_limit() -> u64 {
    DEFAULT_BOTTLE_SIZE_LIMIT_BYTES
}

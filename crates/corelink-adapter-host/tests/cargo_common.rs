//! Shared test fixtures: in-memory CAS, fixed-tenant PAT resolver,
//! adapter spin-up helpers.
//!
//! Used by `smoke`, `adversarial`, and `prop_translate` integration tests
//! that need to drive the cargo adapter end-to-end.

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
use corelink_adapter_host::cargo::config::DEFAULT_BODY_SIZE_LIMIT_BYTES;
use corelink_adapter_host::cargo::ports::{CasError, CasStore, TenantResolveError, TenantResolver};
use corelink_adapter_host::cargo::server::build_router;
use corelink_adapter_host::cargo::CargoAdapterConfig;
use corelink_audit::ports::InMemoryAuditEmitter;
use std::sync::Mutex;

/// In-memory CAS backed by a `HashMap<(tenant_id, digest_hex), Vec<u8>>`.
/// Thread-safe via `std::sync::Mutex`.
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
    async fn get(&self, tenant_id: &str, digest_hex: &str) -> Result<Option<Vec<u8>>, CasError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(&(tenant_id.to_owned(), digest_hex.to_owned()))
            .cloned())
    }

    async fn put(&self, tenant_id: &str, digest_hex: &str, bytes: Vec<u8>) -> Result<(), CasError> {
        self.inner
            .lock()
            .unwrap()
            .insert((tenant_id.to_owned(), digest_hex.to_owned()), bytes);
        Ok(())
    }
}

/// A CAS implementor that always fails — used to exercise the
/// `CargoAdapterError::Cas` path.
#[derive(Debug, Default)]
pub struct FailingCas;

#[async_trait]
impl CasStore for FailingCas {
    async fn get(&self, _tenant_id: &str, _digest_hex: &str) -> Result<Option<Vec<u8>>, CasError> {
        Err(CasError::Backend("intentional get failure".to_owned()))
    }

    async fn put(
        &self,
        _tenant_id: &str,
        _digest_hex: &str,
        _bytes: Vec<u8>,
    ) -> Result<(), CasError> {
        Err(CasError::Backend("intentional put failure".to_owned()))
    }
}

/// Fixed-tenant PAT resolver. Resolves a set of known PAT → tenant_id
/// mappings; uses `subtle::ConstantTimeEq` for constant-time compare.
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
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        use subtle::ConstantTimeEq;
        // Constant-time compare: iterate all entries to avoid early-exit
        // timing oracle. We return the first match found.
        let mut result: Option<String> = None;
        for (k, v) in &self.map {
            let ct_eq = k.as_bytes().ct_eq(pat_plaintext.as_bytes()).unwrap_u8();
            if ct_eq == 1 {
                result = Some(v.clone());
            }
        }
        result.ok_or(TenantResolveError::InvalidPat)
    }
}

/// Build a fully wired adapter with the shared in-memory CAS / resolver
/// / audit sink. Returns the bind address (`127.0.0.1:<ephemeral>`)
/// and the live `JoinHandle`.
pub async fn spin_adapter(
    cas: Arc<dyn CasStore>,
    resolver: Arc<dyn TenantResolver>,
    audit: Arc<InMemoryAuditEmitter>,
    body_size_limit_bytes: u64,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = CargoAdapterConfig::new(bind_addr, body_size_limit_bytes, cas, resolver, audit);

    let router = build_router(config);
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    let actual = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (actual, handle)
}

/// Default body size limit for tests.
pub fn default_body_limit() -> u64 {
    DEFAULT_BODY_SIZE_LIMIT_BYTES
}

/// A valid 64-char BLAKE3 hex key for use in tests.
pub fn test_key() -> String {
    "a".repeat(64)
}

/// A second distinct valid 64-char BLAKE3 hex key.
pub fn test_key_b() -> String {
    "b".repeat(64)
}

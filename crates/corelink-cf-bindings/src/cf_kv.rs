//! Production [`KvBackend`] implementation backed by a Cloudflare KV namespace.
//!
//! Mirrors `corelink-clerk-cf::CfKvJwksCache` but targets the generic
//! byte-slot [`KvBackend`] surface used by the negative-cache (S-02)
//! and any future hot-lookup path. The KV namespace binding is
//! resolved by name at construction time via `env.kv(binding_name)`.
//!
//! # TTL clamp
//!
//! Cloudflare KV requires `expiration_ttl >= 60s`. We clamp defensively
//! — the negative cache uses 300s, well above the floor, but a future
//! caller passing a 30s TTL would otherwise hit a CF runtime error
//! that surfaces as `KvError::Backend(...)`. Clamping at the adapter
//! keeps the trait surface identical to the in-memory fake.
//!
//! # `Send` bridging
//!
//! Same `worker::send::SendFuture` pattern as the R2 adapter — CF
//! Workers are single-threaded so wrapping the `!Send` JS futures is
//! sound. See module docs in `cf_r2.rs` for the full rationale.

use corelink_cas::cache::kv::{KvBackend, KvError};
use std::future::Future;
use worker::kv::KvStore;

/// Minimum KV TTL enforced by Cloudflare (60 seconds).
const CF_KV_MIN_TTL_SECS: u64 = 60;

/// Production [`KvBackend`] backed by Cloudflare KV.
///
/// Construct via [`CfKvNamespaceAdapter::new`] from a `worker::kv::KvStore`
/// obtained via `env.kv("METADATA_KV")` (or any other declared binding).
#[derive(Clone, Debug)]
pub struct CfKvNamespaceAdapter {
    store: KvStore,
}

impl CfKvNamespaceAdapter {
    /// Wrap a `KvStore` binding.
    #[must_use]
    pub fn new(store: KvStore) -> Self {
        Self { store }
    }

    /// Borrow the underlying `worker::kv::KvStore`.
    #[must_use]
    pub fn inner(&self) -> &KvStore {
        &self.store
    }
}

impl KvBackend for CfKvNamespaceAdapter {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            let bytes = self
                .store
                .get(key)
                .bytes()
                .await
                .map_err(|e| KvError::Backend(format!("kv get: {e}")))?;
            Ok(bytes)
        })
    }

    fn put_with_ttl<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> impl Future<Output = Result<(), KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            let ttl = ttl_secs.max(CF_KV_MIN_TTL_SECS);
            self.store
                .put_bytes(key, &value)
                .map_err(|e| KvError::Backend(format!("kv put build: {e}")))?
                .expiration_ttl(ttl)
                .execute()
                .await
                .map_err(|e| KvError::Backend(format!("kv put execute: {e}")))?;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<(), KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            self.store
                .delete(key)
                .await
                .map_err(|e| KvError::Backend(format!("kv delete: {e}")))?;
            Ok(())
        })
    }
}

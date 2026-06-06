//! Production [`KvJwksCache`] implementation backed by a Cloudflare KV namespace.
//!
//! The KV namespace binding is resolved by name at construction time via
//! `worker::Env::kv(binding_name)`. A single instance of [`CfKvJwksCache`]
//! is cheaply clonable (the inner `worker::kv::KvStore` is itself clone).
//!
//! # Key format
//!
//! Keys are `clerk:jwks:<instance_hash>` — the same prefix the adapter
//! uses for cache-key derivation (see `ClerkConfig::instance_hash`).
//!
//! # Trait surface analysis
//!
//! The [`KvJwksCache`] trait uses `&self` on `get`, `set`, and `delete`.
//! This is correct for CF Workers: `KvStore` is not `Send` in the usual
//! Rust sense (it wraps a JS object), but CF Workers are single-threaded
//! so the `unsafe impl Send for KvStore` in workers-rs is intentional.
//! The trait taking `&self` (not `&mut self`) is the RIGHT design for
//! this CF-stateless environment — no interior mutability issue found.
//!
//! # TTL notes
//!
//! CF KV `put` expiration_ttl is expressed in seconds and must be >= 60.
//! The adapter passes `jwks_ttl` which defaults to 86400s (24h) — well
//! above the 60s minimum. We clamp to at least 60s defensively.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corelink_clerk::jwks_cache::{KvJwksCache, KvJwksCacheFuture};
use corelink_clerk::{CachedJwks, Jwks, KvJwksCacheError};
use worker::kv::KvStore;

/// Minimum KV TTL enforced by Cloudflare (60 seconds).
const CF_KV_MIN_TTL_SECS: u64 = 60;

/// Key prefix for all JWKS cache entries.
const KEY_PREFIX: &str = "clerk:jwks:";

/// Production [`KvJwksCache`] backed by Cloudflare KV.
///
/// Construct via [`CfKvJwksCache::new`] from a `worker::kv::KvStore`
/// obtained via `env.kv("CLERK_JWKS_KV")`.
#[derive(Clone, Debug)]
pub struct CfKvJwksCache {
    store: KvStore,
}

impl CfKvJwksCache {
    /// Wrap a `KvStore` binding.
    ///
    /// ```ignore
    /// // In a CF Worker fetch handler:
    /// let kv = env.kv("CLERK_JWKS_KV")?;
    /// let cache = CfKvJwksCache::new(kv);
    /// ```
    #[must_use]
    pub fn new(store: KvStore) -> Self {
        Self { store }
    }

    fn make_key(instance_hash: &str) -> String {
        let mut key = String::with_capacity(KEY_PREFIX.len() + instance_hash.len());
        key.push_str(KEY_PREFIX);
        key.push_str(instance_hash);
        key
    }
}

impl KvJwksCache for CfKvJwksCache {
    /// Read a cached JWKS document from CF KV.
    ///
    /// Returns `Ok(None)` if the key is absent or CF KV returns null
    /// (i.e. the TTL-based expiry fired on CF's side).
    ///
    /// The `stored_at` timestamp is embedded in the payload JSON as a
    /// wrapper object `{"stored_at_unix_secs":<u64>,"jwks":{...}}` so
    /// the adapter can perform its belt-and-suspenders freshness check.
    ///
    /// # Send wrapping
    ///
    /// `KvJwksCacheFuture` requires `Send`. The CF KV `.text().await`
    /// future contains `Rc<RefCell<...>>` from `js-sys::JsFuture` and
    /// is therefore `!Send` by Rust's type system. Workers-rs explicitly
    /// documents that CF Workers are single-threaded and provides
    /// `worker::send::SendFuture` to wrap non-Send futures and satisfy
    /// `Send` bounds. We use it here — this is safe because Workers
    /// never actually move the future across threads.
    fn get<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, Option<CachedJwks>> {
        Box::pin(worker::send::SendFuture::new(async move {
            let key = Self::make_key(instance_hash);
            let raw = self
                .store
                .get(&key)
                .text()
                .await
                .map_err(|e| KvJwksCacheError::Backend(format!("kv get: {e}")))?;
            match raw {
                None => Ok(None),
                Some(text) => {
                    let envelope: StoredEnvelope = serde_json::from_str(&text)
                        .map_err(|e| KvJwksCacheError::Corrupt(format!("json parse: {e}")))?;
                    let jwks = Jwks::parse(envelope.jwks_json.as_bytes())
                        .map_err(|e| KvJwksCacheError::Corrupt(e.to_string()))?;
                    let stored_at = UNIX_EPOCH
                        .checked_add(Duration::from_secs(envelope.stored_at_unix_secs))
                        .unwrap_or(UNIX_EPOCH);
                    Ok(Some(CachedJwks { jwks, stored_at }))
                }
            }
        }))
    }

    /// Write a JWKS document to CF KV with a TTL.
    ///
    /// The CF KV `expiration_ttl` is set to `max(ttl_secs, 60)` to
    /// satisfy CF's minimum TTL requirement. The `stored_at` timestamp
    /// is captured from `SystemTime::now()` at write time and embedded
    /// in the stored payload so the adapter's belt-and-suspenders
    /// freshness check works correctly.
    fn set<'a>(
        &'a self,
        instance_hash: &'a str,
        jwks: &'a Jwks,
        ttl: Duration,
    ) -> KvJwksCacheFuture<'a, ()> {
        Box::pin(worker::send::SendFuture::new(async move {
            let key = Self::make_key(instance_hash);
            let now = SystemTime::now();
            let stored_at_unix_secs = now
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            let jwks_json = jwks.to_json();
            let envelope = StoredEnvelope {
                stored_at_unix_secs,
                jwks_json,
            };
            let payload = serde_json::to_string(&envelope)
                .map_err(|e| KvJwksCacheError::Backend(format!("json serialize: {e}")))?;
            let ttl_secs = ttl.as_secs().max(CF_KV_MIN_TTL_SECS);
            self.store
                .put(&key, payload)
                .map_err(|e| KvJwksCacheError::Backend(format!("kv put build: {e}")))?
                .expiration_ttl(ttl_secs)
                .execute()
                .await
                .map_err(|e| KvJwksCacheError::Backend(format!("kv put execute: {e}")))?;
            Ok(())
        }))
    }

    /// Delete a JWKS cache slot from CF KV (manual rotation trigger).
    fn delete<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, ()> {
        Box::pin(worker::send::SendFuture::new(async move {
            let key = Self::make_key(instance_hash);
            self.store
                .delete(&key)
                .await
                .map_err(|e| KvJwksCacheError::Backend(format!("kv delete: {e}")))?;
            Ok(())
        }))
    }
}

/// JSON envelope stored in KV.
///
/// We embed `stored_at_unix_secs` alongside the raw JWKS JSON so the
/// adapter's `is_fresh` check can work independently of KV's own TTL.
/// This is belt-and-suspenders: CF KV will also expire the key after
/// `expiration_ttl`, but we don't want to trust the external clock
/// solely.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct StoredEnvelope {
    /// Unix timestamp (seconds) captured at `set` time.
    stored_at_unix_secs: u64,
    /// Raw JWKS JSON string (output of `Jwks::to_json()`).
    jwks_json: String,
}

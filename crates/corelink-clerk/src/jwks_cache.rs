//! TTL-bounded JWKS cache trait + in-memory test fake.
//!
//! Production impl wraps a Cloudflare KV namespace (key
//! `clerk:jwks:<instance_hash>`, TTL 86_400 s per WI §1 / §9.2).
//! Tests use [`crate::fakes::InMemoryKvCache`].
//!
//! The cache stores opaque `Vec<u8>` payloads (canonical JWKS JSON
//! produced by [`crate::Jwks::to_json`]); the adapter reparses on
//! load via [`crate::Jwks::parse`]. This split lets the trait stay
//! storage-shape-agnostic (KV PUT/GET takes bytes; we don't hand the
//! KV layer parsed Rust structs).

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use thiserror::Error;

use crate::jwks::Jwks;

/// In-memory cached JWKS — the parsed form alongside the wall-clock
/// instant the cache row was written. Used as the canonical "warm"
/// snapshot the adapter consults before a fetch.
#[derive(Clone, Debug)]
pub struct CachedJwks {
    /// Parsed JWKS.
    pub jwks: Jwks,
    /// Stored-at instant (the cache layer's clock).
    pub stored_at: SystemTime,
}

/// Errors surfaced by the [`KvJwksCache`] trait.
#[derive(Debug, Error)]
pub enum KvJwksCacheError {
    /// Underlying KV error (network / rate limit / shape).
    #[error("KV error: {0}")]
    Backend(String),
    /// Stored payload could not be deserialised back to a JWKS doc.
    /// On encountering this, the adapter treats the slot as missing
    /// and triggers a fresh fetch (defensive — never proceed with
    /// undefined-state).
    #[error("cached payload corrupt: {0}")]
    Corrupt(String),
}

/// Boxed-future signature for the cache trait methods.
pub type KvJwksCacheFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KvJwksCacheError>> + Send + 'a>>;

/// TTL-bounded JWKS cache.
///
/// Implementations MUST honour `ttl` semantics: a `get` after `ttl`
/// elapsed since `set` MUST surface `Ok(None)` (or surface the
/// underlying KV expiry — both are valid). The adapter does its own
/// freshness re-check via [`CachedJwks::stored_at`] as belt-and-
/// suspenders, so a slightly-late KV expiry is harmless.
pub trait KvJwksCache: Send + Sync + 'static {
    /// Read a cached JWKS for `instance_hash`. Returns `Ok(None)` if
    /// the slot is empty / expired.
    fn get<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, Option<CachedJwks>>;

    /// Persist a JWKS for `instance_hash` with `ttl`. Overwrites the
    /// existing slot atomically.
    fn set<'a>(
        &'a self,
        instance_hash: &'a str,
        jwks: &'a Jwks,
        ttl: Duration,
    ) -> KvJwksCacheFuture<'a, ()>;

    /// Explicitly evict the slot (manual rotation trigger).
    fn delete<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, ()>;
}

/// Returns true iff `cached.stored_at + ttl > now` (with the obvious
/// arithmetic-saturation guard for malformed clocks).
#[must_use]
pub fn is_fresh(cached: &CachedJwks, ttl: Duration, now: SystemTime) -> bool {
    let expires_at = cached.stored_at.checked_add(ttl);
    match expires_at {
        Some(exp) => exp > now,
        None => false,
    }
}

//! Shared D1-backed mutable KV for the npm metadata cache surface.
//!
//! # Why this exists (and is NOT the moat)
//!
//! The 2-level content-dedup [`crate::adapter_cache::MoatCache`] stores
//! IMMUTABLE, content-addressed bytes (tarballs, bottles) keyed by
//! `blake3(content)`. npm package METADATA is different: it is mutable JSON
//! that is refreshed on a TTL (a package gains new versions over time), so
//! it cannot live in a content-addressed store — a new version would change
//! the content hash and orphan the old map row, and there is no stable
//! content identity to key on. The npm adapter's `KvStore` port therefore
//! needs a small mutable key→`(value, inserted_at_unix_ms)` store with
//! upsert semantics; the TTL/freshness decision stays pure-logic in the
//! adapter (it reads `inserted_at_unix_ms` back).
//!
//! # Shape
//!
//! Mirrors [`crate::adapter_cache::UrlMapStore`] exactly: a [`NpmKvBackend`]
//! trait (so [`NpmKvStore`] can be unit-tested hermetically with a fake)
//! plus a production impl on [`crate::storage::d1_http::D1HttpClient`] over
//! the `adapter_npm_meta` D1 table, and a [`npm_kv_from_env`] builder that
//! mirrors [`crate::adapter_cache::d1_map_from_env`] (fail-CLOSED when the
//! storage env is unset). Namespacing is the SAME `_public` vs `<tenant>`
//! convention as the moat: unscoped packages are PUBLIC (cross-tenant
//! share — public registry data), scoped `@org/…` packages are per-tenant
//! (isolated). The route shell
//! ([`crate::routes::npm`]) chooses the namespace from the package name.

use std::sync::Arc;

use async_trait::async_trait;

use crate::storage::d1_http::D1HttpClient;

/// Backend for the npm metadata KV: maps `(namespace, key) →
/// (value_bytes, inserted_at_unix_ms)` with upsert semantics.
///
/// Abstracted as a trait so [`NpmKvStore`] is hermetically unit-testable
/// with a fake backend; the production impl is on [`D1HttpClient`].
#[async_trait]
pub trait NpmKvBackend: Send + Sync + std::fmt::Debug {
    /// Fetch `(value, inserted_at_unix_ms)` for `(namespace, key)`, or `None`.
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<(Vec<u8>, u64)>, String>;
    /// Upsert `(namespace, key) → (value, inserted_at_unix_ms)`.
    async fn put(
        &self,
        namespace: &str,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), String>;
}

#[async_trait]
impl NpmKvBackend for D1HttpClient {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<(Vec<u8>, u64)>, String> {
        // Value is stored as hex (D1 over HTTP is JSON-only; npm metadata is
        // UTF-8 JSON, but hex keeps the column binary-safe + avoids escaping
        // surprises). `inserted_ms` is the freshness anchor the adapter TTLs.
        let rows = self
            .query(
                "SELECT value_hex, inserted_ms FROM adapter_npm_meta \
                 WHERE namespace = ?1 AND meta_key = ?2 LIMIT 1",
                &[
                    serde_json::Value::String(namespace.to_owned()),
                    serde_json::Value::String(key.to_owned()),
                ],
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let value_hex = row
            .get("value_hex")
            .and_then(|v| v.as_str())
            .ok_or("D1 adapter_npm_meta: missing `value_hex` column")?;
        let value = hex::decode(value_hex)
            .map_err(|e| format!("D1 adapter_npm_meta: value_hex not hex: {e}"))?;
        let inserted_ms = row
            .get("inserted_ms")
            .and_then(serde_json::Value::as_u64)
            .ok_or("D1 adapter_npm_meta: missing/!u64 `inserted_ms` column")?;
        Ok(Some((value, inserted_ms)))
    }

    async fn put(
        &self,
        namespace: &str,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), String> {
        let value_hex = hex::encode(value);
        self.query(
            "INSERT INTO adapter_npm_meta \
               (namespace, meta_key, value_hex, inserted_ms) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(namespace, meta_key) DO UPDATE SET \
               value_hex   = excluded.value_hex, \
               inserted_ms = excluded.inserted_ms",
            &[
                serde_json::Value::String(namespace.to_owned()),
                serde_json::Value::String(key.to_owned()),
                serde_json::Value::String(value_hex),
                serde_json::Value::from(inserted_at_unix_ms),
            ],
        )
        .await?;
        Ok(())
    }
}

/// Failure surface of [`NpmKvStore`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NpmKvError {
    /// Backend fault (D1 query / decode).
    #[error("npm metadata kv backend: {0}")]
    Backend(String),
}

/// The npm metadata KV: a thin typed wrapper over a [`NpmKvBackend`].
///
/// Kept as a struct (rather than using the backend trait directly in the
/// route) so the error surface is a typed [`NpmKvError`] the route maps
/// onto `NpmAdapterError::Kv`, mirroring how [`crate::adapter_cache::MoatCache`]
/// wraps [`crate::adapter_cache::UrlMapStore`].
pub struct NpmKvStore {
    backend: Arc<dyn NpmKvBackend>,
}

impl std::fmt::Debug for NpmKvStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NpmKvStore").finish_non_exhaustive()
    }
}

impl NpmKvStore {
    /// Construct from an explicit backend (production wiring + tests).
    #[must_use]
    pub fn new(backend: Arc<dyn NpmKvBackend>) -> Self {
        Self { backend }
    }

    /// Fetch cached metadata `(value, inserted_at_unix_ms)`, or `None`.
    ///
    /// # Errors
    ///
    /// [`NpmKvError::Backend`] on a backend fault.
    pub async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmKvError> {
        self.backend
            .get(namespace, key)
            .await
            .map_err(NpmKvError::Backend)
    }

    /// Upsert cached metadata.
    ///
    /// # Errors
    ///
    /// [`NpmKvError::Backend`] on a backend fault.
    pub async fn put(
        &self,
        namespace: &str,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), NpmKvError> {
        self.backend
            .put(namespace, key, value, inserted_at_unix_ms)
            .await
            .map_err(NpmKvError::Backend)
    }
}

/// Build the production D1-backed [`NpmKvStore`] from process env
/// ([`crate::storage::StorageEnv`]). Returns `None` (fail-CLOSED — the npm
/// route is not mounted) when the storage env is unset/invalid. Mirrors
/// [`crate::adapter_cache::d1_map_from_env`].
#[must_use]
pub fn npm_kv_from_env() -> Option<Arc<NpmKvStore>> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = D1HttpClient::new(&storage_env)
        .map_err(|e| tracing::warn!(error = %e, "npm metadata kv: D1 client init failed"))
        .ok()?;
    Some(Arc::new(NpmKvStore::new(Arc::new(d1))))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    // `(ns, key) → (value, inserted_ms)` rows for the in-memory KV fake.
    type FakeRows = HashMap<(String, String), (Vec<u8>, u64)>;

    #[derive(Default, Debug)]
    struct FakeBackend(Mutex<FakeRows>);
    #[async_trait]
    impl NpmKvBackend for FakeBackend {
        async fn get(&self, ns: &str, key: &str) -> Result<Option<(Vec<u8>, u64)>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), key.to_owned()))
                .cloned())
        }
        async fn put(&self, ns: &str, key: &str, value: Vec<u8>, ts: u64) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .insert((ns.to_owned(), key.to_owned()), (value, ts));
            Ok(())
        }
    }

    #[tokio::test]
    async fn put_then_get_round_trip_with_timestamp() {
        let store = NpmKvStore::new(Arc::new(FakeBackend::default()));
        store
            .put("_public", "npm:meta:lodash", b"{}".to_vec(), 1234)
            .await
            .unwrap();
        let got = store.get("_public", "npm:meta:lodash").await.unwrap();
        assert_eq!(got, Some((b"{}".to_vec(), 1234)));
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let store = NpmKvStore::new(Arc::new(FakeBackend::default()));
        assert_eq!(store.get("_public", "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn namespaces_isolate() {
        let store = NpmKvStore::new(Arc::new(FakeBackend::default()));
        store
            .put("tenant-a", "npm:meta:@org/x", b"a".to_vec(), 1)
            .await
            .unwrap();
        assert_eq!(
            store.get("tenant-b", "npm:meta:@org/x").await.unwrap(),
            None
        );
        assert_eq!(
            store.get("tenant-a", "npm:meta:@org/x").await.unwrap(),
            Some((b"a".to_vec(), 1))
        );
    }
}

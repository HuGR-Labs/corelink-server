//! The 2-level content-dedup "moat" cache store for the URL-keyed cache
//! adapters (brew / npm / pip).
//!
//! # Why two levels
//!
//! Those adapters key their cache by `blake3(upstream-URL)`, but the
//! content-addressed CAS keys bytes by `blake3(content)` (and the in-memory
//! handler verifies it). So a URL-derived key cannot be a CAS key. Instead:
//!
//! - **bytes** live in the content-addressed CAS, keyed by `blake3(content)`.
//!   PUBLIC content (Homebrew bottles, public npm/PyPI packages) is stored
//!   under a SHARED namespace ⇒ identical content is stored ONCE and served to
//!   every authenticated tenant — the **network-effect moat**. PRIVATE content
//!   uses a per-tenant namespace (isolated).
//! - **url→content-hash** lives in the D1 `adapter_cache_map` table
//!   ([`UrlMapStore`]). A GET resolves `(namespace, url_hash) → content_hash`
//!   then reads the deduped bytes from CAS.
//!
//! Access is PAT-gated (Option-B re-verify, see [`crate::adapter_pat`]); only
//! the public CONTENT is shared (safe — it is already public upstream).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest,
};

use crate::storage::d1_http::D1HttpClient;

/// Namespace for public, deterministic, cross-tenant-shareable content
/// (Homebrew bottles, public npm/PyPI packages). Storing public content under
/// one namespace is the dedup that powers the network-effect moat.
pub const PUBLIC_NAMESPACE: &str = "_public";

// ── Level 2: the url→content-hash map ───────────────────────────────────────

/// Maps `(namespace, url_hash) → content_hash` for the 2-level cache.
///
/// Abstracted as a trait so [`MoatCache`] can be unit-tested hermetically with
/// a fake map (mirrors `crate::adapter_pat::PatRowLookup`); the production impl
/// is on [`D1HttpClient`].
#[async_trait]
pub trait UrlMapStore: Send + Sync {
    /// Resolve the CAS content-hash for `(namespace, url_hash)`, or `None`.
    async fn get(&self, namespace: &str, url_hash: &str) -> Result<Option<String>, String>;
    /// Upsert `(namespace, url_hash) → (content_hash, content_len)`.
    async fn put(
        &self,
        namespace: &str,
        url_hash: &str,
        content_hash: &str,
        content_len: u64,
    ) -> Result<(), String>;

    /// Remove the `(namespace, url_hash)` mapping so a subsequent [`Self::get`]
    /// misses. Idempotent: deleting an absent mapping succeeds.
    ///
    /// This backs the cargo/sccache WebDAV `DELETE` (opendal's write-check
    /// cleanup). Only the url→content-hash row is removed; the content-addressed
    /// CAS blob is left for GC (see [`MoatCache::delete`]).
    ///
    /// # Default
    ///
    /// The default returns `Err` — a store must OPT IN to deletion by overriding
    /// this. The production [`D1HttpClient`] and the cargo WebDAV surface do; the
    /// read-through caches (brew/npm/pip/oci) never issue a `DELETE`, so they keep
    /// the fail-LOUD default rather than a silent no-op.
    async fn delete(&self, _namespace: &str, _url_hash: &str) -> Result<(), String> {
        Err("UrlMapStore backend does not support delete".to_owned())
    }
}

#[async_trait]
impl UrlMapStore for D1HttpClient {
    async fn get(&self, namespace: &str, url_hash: &str) -> Result<Option<String>, String> {
        // F3.2 BLOCKER-1: a `_public` lookup must NEVER resolve a content_hash that
        // has been revoked (`public_blocklist`, migration 0097). The blocklist filter
        // is a `NOT EXISTS` join in the SAME statement — not a second query after the
        // map read — so there is no map-read → blocklist-read TOCTOU: the lookup is
        // linearized either before the revocation's blocklist insert (returns the
        // hash, whose bytes may then be deleted → a later CAS read misses, still safe)
        // or after it (returns None). The private-namespace path is UNCHANGED.
        let sql = if namespace == PUBLIC_NAMESPACE {
            "SELECT c.content_hash FROM adapter_cache_map c \
             WHERE c.namespace = ?1 AND c.url_hash = ?2 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM public_blocklist pb WHERE pb.content_hash = c.content_hash \
               ) \
             LIMIT 1"
        } else {
            "SELECT content_hash FROM adapter_cache_map \
             WHERE namespace = ?1 AND url_hash = ?2 LIMIT 1"
        };
        let rows = self
            .query(
                sql,
                &[
                    serde_json::Value::String(namespace.to_owned()),
                    serde_json::Value::String(url_hash.to_owned()),
                ],
            )
            .await?;
        Ok(rows.into_iter().next().and_then(|r| {
            r.get("content_hash")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        }))
    }

    async fn put(
        &self,
        namespace: &str,
        url_hash: &str,
        content_hash: &str,
        content_len: u64,
    ) -> Result<(), String> {
        self.query(
            "INSERT INTO adapter_cache_map \
               (namespace, url_hash, content_hash, content_len, created_ms) \
             VALUES (?1, ?2, ?3, ?4, unixepoch('now', 'subsec') * 1000) \
             ON CONFLICT(namespace, url_hash) DO UPDATE SET \
               content_hash = excluded.content_hash, \
               content_len  = excluded.content_len, \
               created_ms   = unixepoch('now', 'subsec') * 1000",
            &[
                serde_json::Value::String(namespace.to_owned()),
                serde_json::Value::String(url_hash.to_owned()),
                serde_json::Value::String(content_hash.to_owned()),
                serde_json::Value::from(content_len),
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete(&self, namespace: &str, url_hash: &str) -> Result<(), String> {
        self.query(
            "DELETE FROM adapter_cache_map WHERE namespace = ?1 AND url_hash = ?2",
            &[
                serde_json::Value::String(namespace.to_owned()),
                serde_json::Value::String(url_hash.to_owned()),
            ],
        )
        .await?;
        Ok(())
    }
}

// ── Level 1: the content-addressed bytes + the 2-level composition ──────────

/// Canonical content hash (production): the same `blake3` hex the CAS write
/// path expects, via `corelink_hash::Digest::compute`.
#[must_use]
pub fn canonical_hash_hex(bytes: &[u8]) -> String {
    corelink_hash::Digest::compute(bytes).to_hex()
}

/// Build the production D1-backed [`UrlMapStore`] from process env
/// ([`crate::storage::StorageEnv`]). Returns `None` (fail-CLOSED — the
/// adapter route is not mounted) when the storage env is unset/invalid, so
/// dev/CI runs without D1 simply don't expose the adapter surfaces. Mirrors
/// [`crate::adapter_pat::PatVerifier::from_env`].
#[must_use]
pub fn d1_map_from_env() -> Option<Arc<D1HttpClient>> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    D1HttpClient::new(&storage_env)
        .map_err(|e| tracing::warn!(error = %e, "adapter url-map: D1 client init failed"))
        .ok()
        .map(Arc::new)
}

/// Failure surface of [`MoatCache`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MoatError {
    /// Backend fault (CAS handler, D1 map, or join error).
    #[error("moat cache backend: {0}")]
    Backend(String),
}

/// The 2-level content-dedup cache: a [`UrlMapStore`] (url→content-hash) over
/// the content-addressed CAS handlers (content-hash→bytes).
///
/// `hasher` MUST match the CAS write path's hash function: production wires
/// [`canonical_hash_hex`] (matches the real R2/blake3 handler); tests wire
/// `corelink_handler_cas::handler::fake_hash` (matches `InMemoryCasHandler`).
pub struct MoatCache {
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    hasher: fn(&[u8]) -> String,
    principal: String,
}

impl std::fmt::Debug for MoatCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoatCache")
            .field("principal", &self.principal)
            .finish_non_exhaustive()
    }
}

impl MoatCache {
    /// Construct with an explicit hasher (production wiring + tests).
    #[must_use]
    pub fn new(
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        map: Arc<dyn UrlMapStore>,
        hasher: fn(&[u8]) -> String,
        principal: impl Into<String>,
    ) -> Self {
        Self {
            cas_read,
            cas_write,
            map,
            hasher,
            principal: principal.into(),
        }
    }

    /// Production constructor: hashes with [`canonical_hash_hex`] (blake3),
    /// matching the real CAS handler.
    #[must_use]
    pub fn production(
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        map: Arc<dyn UrlMapStore>,
        principal: impl Into<String>,
    ) -> Self {
        Self::new(cas_read, cas_write, map, canonical_hash_hex, principal)
    }

    /// Resolve cached bytes for `(namespace, url_hash)`, or `None` on miss.
    ///
    /// A map hit whose CAS blob has been GC'd is treated as a miss (the caller
    /// re-fetches upstream + re-stores), not an error.
    ///
    /// Timed as the `ostore` sub-phase of the Worker's `origin` block (see
    /// [`crate::origin_timing`]): the whole storage cost of a cache lookup —
    /// the url-map read AND, on a map hit, the CAS/R2 blob fetch. The work is
    /// in [`Self::get_untimed`]; this wrapper only starts and stops a clock.
    ///
    /// On the cargo read path the url-map read is now served from the `pat`
    /// read's co-read ([`crate::d1_coread`]), so `ostore` measures ~0 on a miss
    /// and the CAS/R2 fetch alone on a hit; the round trip it used to measure
    /// moved into `opat`, which now carries both statements. The phases still
    /// sum exactly — the millisecond changed phase, it did not disappear.
    pub async fn get(&self, namespace: &str, url_hash: &str) -> Result<Option<Vec<u8>>, MoatError> {
        crate::origin_timing::timed(
            crate::origin_timing::Phase::Store,
            self.get_untimed(namespace, url_hash),
        )
        .await
    }

    /// The unwrapped body of [`Self::get`] — see it for the contract.
    async fn get_untimed(
        &self,
        namespace: &str,
        url_hash: &str,
    ) -> Result<Option<Vec<u8>>, MoatError> {
        // The url-map row may already be in hand: on the cargo read path the
        // container's per-request D1 `pat` read carries it in the SAME round
        // trip (see [`crate::d1_coread`]), which is what collapsed `opat` +
        // `ostore` from two RTTs to one.
        //
        // ⚠️ AUTH BEFORE ACT. `namespace` here is the tenant the container
        // derived from the PAT (the moat is NEVER keyed by the Worker's tenant
        // header), and `take` serves the prefetched row ONLY under an exact
        // `(namespace, url_hash)` match with the key it was fetched under. So a
        // prefetch is consumable only after the PAT verify that produced this
        // very `namespace` has already succeeded, and a hint that named a
        // different tenant is discarded unread — we fall through to the real,
        // correctly-keyed read below. There is no path on which a row fetched
        // under one namespace is served under another.
        let prefetched = crate::d1_coread::take(namespace, url_hash);
        let content_hash = match prefetched {
            Some(hit) => match hit {
                Some(h) => h,
                // A co-read MISS is a real answer, and it is the hot one: the
                // whole 404 path now costs zero storage round trips.
                None => return Ok(None),
            },
            None => match self
                .map
                .get(namespace, url_hash)
                .await
                .map_err(MoatError::Backend)?
            {
                Some(h) => h,
                None => return Ok(None),
            },
        };
        let handler = Arc::clone(&self.cas_read);
        let req = CasReadRequest::new(
            namespace,
            &content_hash,
            self.principal.as_str(),
            namespace,
            unix_ms_now(),
        );
        let result = tokio::task::spawn_blocking(move || handler.read(req))
            .await
            .map_err(|e| MoatError::Backend(format!("cas read join: {e}")))?;
        match result {
            Ok(resp) => {
                // Defense-in-depth integrity check on the READ path. The write
                // path stores bytes content-addressed under `(self.hasher)(bytes)`,
                // so a served blob MUST re-hash to the mapped `content_hash`. A
                // mismatch means CAS corruption, a poisoned map row, or a write
                // that bypassed content-addressing — NEVER serve such bytes. We
                // treat it as a miss so the caller re-fetches the authentic
                // (SSRF-pinned) upstream and re-stores, self-healing the entry;
                // logged at error level because it must not happen in normal op.
                let actual = (self.hasher)(&resp.bytes);
                if actual != content_hash {
                    tracing::error!(
                        namespace,
                        url_hash,
                        expected = %content_hash,
                        actual = %actual,
                        "moat: CAS bytes do not match the mapped content-hash; \
                         refusing to serve (treating as miss for self-heal)"
                    );
                    return Ok(None);
                }
                Ok(Some(resp.bytes))
            }
            // Map points at a blob the CAS no longer has ⇒ cache miss.
            Err(CasHandlerError::NotFound { .. }) => Ok(None),
            Err(e) => Err(MoatError::Backend(format!("cas read: {e:?}"))),
        }
    }

    /// Store `bytes` content-addressed + map `(namespace, url_hash)` to the
    /// content-hash. Identical bytes (any namespace/url) dedup to one CAS blob.
    ///
    /// Timed as `ostore`, like [`Self::get`] — a write's storage cost belongs
    /// to the same phase as a read's, so a probe against the write path
    /// attributes without a second convention.
    pub async fn put(
        &self,
        namespace: &str,
        url_hash: &str,
        bytes: Vec<u8>,
        storage_quota_bytes: Option<i64>,
    ) -> Result<(), MoatError> {
        crate::origin_timing::timed(
            crate::origin_timing::Phase::Store,
            self.put_untimed(namespace, url_hash, bytes, storage_quota_bytes),
        )
        .await
    }

    /// The unwrapped body of [`Self::put`] — see it for the contract.
    async fn put_untimed(
        &self,
        namespace: &str,
        url_hash: &str,
        bytes: Vec<u8>,
        storage_quota_bytes: Option<i64>,
    ) -> Result<(), MoatError> {
        let content_hash = (self.hasher)(&bytes);
        let content_len = bytes.len() as u64;
        let handler = Arc::clone(&self.cas_write);
        // Storage-cap seeding: `storage_quota_bytes` is the caller's RESOLVED
        // per-tier cap, threaded into the byte-accounting reservation
        // (`CasWriteRequest::with_storage_quota_bytes`) so a fresh
        // `tenant_storage_state` row is seeded with the REAL cap and a
        // DOWNGRADED tenant's stored cap is reconciled on this write (rt-nuclear
        // #16). The OCI surface (WP #10) resolves the cap at `/token` mint and
        // passes it here via the verified bearer; the brew/npm/pip surfaces pass
        // `None` (their writes accrue against an EXISTING row's stored cap; a
        // tenant with NO row yet is seeded on its first NATIVE write, which
        // carries the Worker cap header). `None` ⇒ fail-closed on an unseeded
        // tenant — absence is never treated as unlimited.
        let req = CasWriteRequest::new(
            namespace,
            &content_hash,
            bytes,
            self.principal.as_str(),
            namespace,
            unix_ms_now(),
        )
        .with_storage_quota_bytes(storage_quota_bytes);
        let result = tokio::task::spawn_blocking(move || handler.write(req))
            .await
            .map_err(|e| MoatError::Backend(format!("cas write join: {e}")))?;
        result.map_err(|e| MoatError::Backend(format!("cas write: {e:?}")))?;
        self.map
            .put(namespace, url_hash, &content_hash, content_len)
            .await
            .map_err(MoatError::Backend)?;
        Ok(())
    }

    /// Remove the `(namespace, url_hash)` map entry so a later [`Self::get`]
    /// misses — the WebDAV `DELETE` on the cargo/sccache surface.
    ///
    /// Only the url→content-hash map row is removed; the content-addressed CAS
    /// blob is intentionally LEFT for GC. The blob may be shared by other keys
    /// (content dedup), so removing it here could break an unrelated mapping;
    /// unreferenced blobs are reclaimed by the storage GC, not by this path.
    /// Idempotent: deleting an absent key succeeds.
    ///
    /// Timed as `ostore`, like [`Self::get`] / [`Self::put`] — sccache's
    /// write-probe cleanup issues one of these per build, so it is on the
    /// measured surface.
    pub async fn delete(&self, namespace: &str, url_hash: &str) -> Result<(), MoatError> {
        crate::origin_timing::timed(
            crate::origin_timing::Phase::Store,
            self.map.delete(namespace, url_hash),
        )
        .await
        .map_err(MoatError::Backend)
    }
}

/// Current wall-clock unix milliseconds.
fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
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

    use corelink_handler_cas::handler::fake_hash;
    use corelink_handler_cas::{InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver};

    use super::*;

    /// Hermetic in-memory url→hash map.
    #[derive(Default)]
    struct FakeUrlMap {
        rows: Mutex<HashMap<(String, String), (String, u64)>>,
    }

    #[async_trait]
    impl UrlMapStore for FakeUrlMap {
        async fn get(&self, namespace: &str, url_hash: &str) -> Result<Option<String>, String> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .get(&(namespace.to_owned(), url_hash.to_owned()))
                .map(|(h, _)| h.clone()))
        }
        async fn put(
            &self,
            namespace: &str,
            url_hash: &str,
            content_hash: &str,
            content_len: u64,
        ) -> Result<(), String> {
            self.rows.lock().unwrap().insert(
                (namespace.to_owned(), url_hash.to_owned()),
                (content_hash.to_owned(), content_len),
            );
            Ok(())
        }
    }

    fn in_memory_cas() -> Arc<InMemoryCasHandler> {
        Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ))
    }

    /// Build a `MoatCache` over an in-memory CAS + fake map, using `fake_hash`
    /// (matches the in-memory handler's verification).
    fn moat(cas: Arc<InMemoryCasHandler>, map: Arc<FakeUrlMap>) -> MoatCache {
        MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            "moat-test",
        )
    }

    #[tokio::test]
    async fn put_then_get_round_trip() {
        let map = Arc::new(FakeUrlMap::default());
        let m = moat(in_memory_cas(), Arc::clone(&map));
        let bytes = b"bottle-bytes".to_vec();
        m.put(PUBLIC_NAMESPACE, "urlhash1", bytes.clone(), None)
            .await
            .unwrap();
        let got = m.get(PUBLIC_NAMESPACE, "urlhash1").await.unwrap();
        assert_eq!(got, Some(bytes));
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let m = moat(in_memory_cas(), Arc::new(FakeUrlMap::default()));
        assert_eq!(m.get(PUBLIC_NAMESPACE, "nope").await.unwrap(), None);
    }

    /// The `UrlMapStore::delete` DEFAULT is fail-LOUD: a store that does not
    /// override it (here the brew/npm/pip/oci-style `FakeUrlMap`, which does not)
    /// reports the unsupported error verbatim — never a silent success. Asserting
    /// the EXACT message pins both the Ok and the Err mutant on the default body.
    #[tokio::test]
    async fn urlmapstore_delete_default_is_unsupported_err() {
        let m = FakeUrlMap::default();
        assert_eq!(
            UrlMapStore::delete(&m, "ns", "k").await,
            Err("UrlMapStore backend does not support delete".to_owned())
        );
    }

    #[tokio::test]
    async fn identical_content_dedups_to_one_cas_blob() {
        // Two DIFFERENT url hashes with IDENTICAL bytes → same content_hash →
        // ONE CAS blob, two map rows. This is the moat.
        let map = Arc::new(FakeUrlMap::default());
        let m = moat(in_memory_cas(), Arc::clone(&map));
        let bytes = b"shared-public-dep".to_vec();
        m.put(PUBLIC_NAMESPACE, "urlA", bytes.clone(), None)
            .await
            .unwrap();
        m.put(PUBLIC_NAMESPACE, "urlB", bytes.clone(), None)
            .await
            .unwrap();
        // Both map rows resolve to the SAME content_hash.
        let ch_a = map
            .rows
            .lock()
            .unwrap()
            .get(&(PUBLIC_NAMESPACE.to_owned(), "urlA".to_owned()))
            .unwrap()
            .0
            .clone();
        let ch_b = map
            .rows
            .lock()
            .unwrap()
            .get(&(PUBLIC_NAMESPACE.to_owned(), "urlB".to_owned()))
            .unwrap()
            .0
            .clone();
        assert_eq!(ch_a, ch_b, "identical content must dedup to one CAS hash");
        // Both URLs serve the bytes.
        assert_eq!(
            m.get(PUBLIC_NAMESPACE, "urlA").await.unwrap(),
            Some(bytes.clone())
        );
        assert_eq!(m.get(PUBLIC_NAMESPACE, "urlB").await.unwrap(), Some(bytes));
    }

    #[tokio::test]
    async fn map_hit_but_cas_gone_is_miss_not_error() {
        // Map points at a content_hash the CAS doesn't have → treated as miss.
        let map = Arc::new(FakeUrlMap::default());
        map.rows.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), "dangling".to_owned()),
            (fake_hash(b"never-stored"), 12),
        );
        let m = moat(in_memory_cas(), Arc::clone(&map));
        assert_eq!(m.get(PUBLIC_NAMESPACE, "dangling").await.unwrap(), None);
    }

    #[tokio::test]
    async fn served_bytes_failing_content_hash_check_are_refused() {
        // Defense-in-depth (L6 review): if the CAS hands back bytes that do NOT
        // re-hash to the mapped content_hash (a poisoned map row, CAS
        // corruption, or a write that bypassed content-addressing), the moat
        // MUST refuse to serve them. It returns a miss so the caller re-fetches
        // the authentic (SSRF-pinned) upstream and self-heals — never serving
        // un-content-addressed bytes across the shared `_public` namespace.
        use corelink_handler_cas::{CasReadResponse, CasWriteResponse};

        // A CAS that returns FIXED bytes for ANY hash — i.e. the stored blob is
        // not content-addressed (the case the in-memory handler rejects on
        // write, but which corruption/poisoning could produce in prod).
        #[derive(Debug)]
        struct PoisonCas(Vec<u8>);
        impl CasReadHandler for PoisonCas {
            fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
                Ok(CasReadResponse::new(self.0.clone(), req.hash))
            }
        }
        impl CasWriteHandler for PoisonCas {
            fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
                Ok(CasWriteResponse::new(req.claimed_hash, true))
            }
        }

        let map = Arc::new(FakeUrlMap::default());
        // Map row points at the content-hash of the AUTHENTIC bytes…
        let good_hash = fake_hash(b"authentic-public-dep");
        map.rows.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), "urlX".to_owned()),
            (good_hash, 42),
        );
        // …but the CAS hands back DIFFERENT (tampered) bytes for that hash.
        let cas = Arc::new(PoisonCas(b"tampered-bytes".to_vec()));
        let m = MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            "moat-test",
        );
        // fake_hash(tampered) != good_hash ⇒ the read re-check fails ⇒ miss.
        assert_eq!(m.get(PUBLIC_NAMESPACE, "urlX").await.unwrap(), None);
    }
}

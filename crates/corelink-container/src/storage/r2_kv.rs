//! R2-backed opaque key→value store for the Turborepo (and future cargo/
//! sccache) cache surface.
//!
//! Unlike the content-addressed CAS, this store does **NOT** verify that
//! the key equals the hash of the bytes — the key is opaque (a Turborepo
//! artifact hash / an sccache cache key). It implements the
//! [`corelink_turbo_bridge::adapter::CasReadStore`] +
//! [`CasWriteStore`](corelink_turbo_bridge::adapter::CasWriteStore) ports
//! over a pluggable [`KvBackend`], closing the `routes/turbo_v8.rs`
//! `TODO(v2): R2KvStore` (in-memory artifacts did not persist across
//! container restarts).
//!
//! ## Key layout
//!
//! `<tenant_prefix_16>/<opaque_key>`
//!
//! The `tenant_prefix_16` is HMAC-derived from the tenant UUID via
//! [`corelink_tenant_path::derive_prefix`] (same scheme as the CAS/AC
//! stores) so cross-tenant keys are unguessable even with raw bucket
//! access. On the production path this is the ONLY accepted prefix: a
//! missing TDK or a non-UUID tenant fails CLOSED
//! ([`TurboBridgeError::Internal`]) rather than degrade to a public,
//! predictable prefix — mirroring [`r2_s3::R2AcHandler`]. The padded,
//! truncated raw-string fallback is gated behind `#[cfg(test)]` and is
//! unreachable in production (a same-millisecond UUIDv7 prefix collision
//! would otherwise share a keyspace).
//!
//! ## Testability seam
//!
//! The store holds an `Arc<dyn KvBackend>` rather than a concrete R2
//! client, so the full behavioral suite (round-trip, isolation,
//! durability-across-rebuild, fault injection) runs against an in-process
//! fake — no network. Production wires the R2-backed [`R2Backend`].
//!
//! ## Sync-over-async
//!
//! The `CasReadStore` / `CasWriteStore` trait methods are synchronous but
//! the backend is async. We bridge with
//! `tokio::task::block_in_place(|| Handle::current().block_on(..))` — the
//! identical rationale documented on `r2_s3::R2AcHandler` (a bare
//! `block_on` from inside the running multi-thread runtime would hang).

use std::sync::Arc;

use async_trait::async_trait;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use corelink_turbo_bridge::adapter::{CasReadStore, CasWriteStore};
use corelink_turbo_bridge::TurboBridgeError;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::r2_s3::R2S3Client;
use super::StorageEnv;

/// Opaque async key→value backend the store reads/writes through. Returns
/// `Ok(None)` for an absent key on `get`; `Err(String)` for a backend
/// failure (transient I/O, auth, etc.) — the store maps that to
/// [`TurboBridgeError::Internal`], NEVER to a false `NotFound`.
#[async_trait]
pub trait KvBackend: Send + Sync + core::fmt::Debug {
    /// Fetch raw bytes for the fully-qualified object key.
    async fn get(&self, object_key: &str) -> Result<Option<Vec<u8>>, String>;
    /// Upsert raw bytes under the fully-qualified object key.
    async fn put(&self, object_key: &str, bytes: Vec<u8>) -> Result<(), String>;
}

/// Production backend: the R2 S3-compatible client.
#[derive(Debug)]
pub struct R2Backend {
    client: R2S3Client,
}

#[async_trait]
impl KvBackend for R2Backend {
    async fn get(&self, object_key: &str) -> Result<Option<Vec<u8>>, String> {
        self.client.get(object_key).await
    }
    async fn put(&self, object_key: &str, bytes: Vec<u8>) -> Result<(), String> {
        self.client.put(object_key, bytes).await
    }
}

/// Opaque key→value store with per-tenant prefix isolation.
pub struct R2KvStore {
    backend: Arc<dyn KvBackend>,
    /// Tenant derivation key for `derive_prefix`. `None` in dev/test mode
    /// → raw padded-truncated prefix fallback.
    tdk: Option<TenantDerivationKey>,
}

impl core::fmt::Debug for R2KvStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2KvStore").finish_non_exhaustive()
    }
}

impl R2KvStore {
    /// Construct from a backend and optional 32-byte TDK.
    #[must_use]
    pub fn new(backend: Arc<dyn KvBackend>, tdk_bytes: Option<Zeroizing<[u8; 32]>>) -> Self {
        let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);
        Self { backend, tdk }
    }

    /// Derive the per-tenant R2 object key for an opaque `(tenant, key)`.
    /// Mirrors `r2_s3::R2AcHandler::r2_key`; no region segment (the turbo
    /// cache is not region-sharded). The opaque `key` is preserved
    /// verbatim after the prefix — traversal sequences (`../`, leading
    /// `/`) are stored as literal key bytes, never resolved, so they can
    /// never escape the caller's `<prefix16>/` namespace.
    ///
    /// # Errors
    ///
    /// On the production path the prefix is ALWAYS the secret-keyed
    /// `derive_prefix(tdk, uuid)`. When the TDK is absent or the tenant
    /// is not a canonical UUID the key is NOT derivable — this returns
    /// [`TurboBridgeError::Internal`] (fail CLOSED) rather than a public,
    /// predictable `pad16` prefix. The `pad16` fallback would let two
    /// tenants onboarded in the same millisecond (UUIDv7) collide into a
    /// SHARED Turbo keyspace (cross-tenant cache leak/poisoning). This
    /// mirrors the CAS/AC `R2CasHandler`/`R2AcHandler` posture exactly.
    fn object_key(&self, tenant: &str, key: &str) -> Result<String, TurboBridgeError> {
        let prefix = match &self.tdk {
            Some(tdk) => match Uuid::try_parse(tenant) {
                Ok(uid) => derive_prefix(tdk, uid).to_string(),
                // Non-UUID tenant: test fixtures use the raw padded
                // prefix; the production path fails CLOSED.
                #[cfg(test)]
                Err(_) => pad16(tenant),
                #[cfg(not(test))]
                Err(_) => return Err(non_derivable_tenant_err()),
            },
            // No TDK is only reachable under `#[cfg(test)]`:
            // `build_r2_kv_from_env` fails closed when `R2_TDK_HEX` is
            // unset/invalid, so the production store is never built with
            // `tdk = None`.
            #[cfg(test)]
            None => pad16(tenant),
            #[cfg(not(test))]
            None => return Err(non_derivable_tenant_err()),
        };
        Ok(format!("{prefix}/{key}"))
    }

    /// Block on an async backend op from inside the running multi-thread
    /// runtime (see module docs).
    fn block_on<F, T>(fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(fut))
    }
}

impl CasReadStore for R2KvStore {
    fn read(&self, tenant: &str, key: &str) -> Result<Vec<u8>, TurboBridgeError> {
        let object_key = self.object_key(tenant, key)?;
        match Self::block_on(self.backend.get(&object_key)) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(TurboBridgeError::NotFound {
                hash: key.to_owned(),
            }),
            // A backend error MUST surface as Internal — never a false
            // NotFound (do not mask present data as absent). [the AC-500 /
            // no-false-404 discipline]
            Err(e) => Err(TurboBridgeError::Internal(format!("r2 kv get: {e}"))),
        }
    }
}

impl CasWriteStore for R2KvStore {
    fn write(
        &self,
        tenant: &str,
        key: &str,
        bytes: Vec<u8>,
    ) -> Result<Option<u64>, TurboBridgeError> {
        let object_key = self.object_key(tenant, key)?;
        // rt34 finding #3/#4/#5/#6: Turbo keys are OPAQUE/client-chosen (NOT
        // content-addressed), so an overwrite can change the stored SIZE. The
        // route accrues the full new bytes up front, then RELEASES the PRIOR
        // size on an overwrite — netting the true on-disk delta (`new - prior`).
        // Probe presence BEFORE the PUT and capture the prior byte length; a
        // probe error fails CLOSED to `Ok(None)` (treat as fresh ⇒ charge the
        // full new bytes — a missed prior-release over-counts the tenant,
        // conservative, never under). (The `KvBackend` port exposes only
        // get/put; the prod R2 backend's `get` is the presence probe — the same
        // call `read` already uses.)
        let prior_len = match Self::block_on(self.backend.get(&object_key)) {
            Ok(Some(prior)) => Some(prior.len() as u64),
            Ok(None) => None,
            // Fail CLOSED: treat a probe error as a fresh insert ⇒ charge the
            // full new bytes (do not release anything we cannot confirm).
            Err(_) => None,
        };
        Self::block_on(self.backend.put(&object_key, bytes))
            .map_err(|e| TurboBridgeError::Internal(format!("r2 kv put: {e}")))?;
        Ok(prior_len)
    }
}

/// Pad-or-truncate a non-UUID tenant string to a stable 16-char prefix.
/// PUBLIC, predictable namespace — TEST FIXTURES ONLY. It must NEVER be
/// used on the production path: two tenants whose first 16 chars collide
/// (trivial for same-millisecond UUIDv7 ids) would share a keyspace. The
/// production path fails CLOSED instead (see [`R2KvStore::object_key`]).
#[cfg(test)]
fn pad16(tenant: &str) -> String {
    let mut p = tenant.to_owned();
    p.truncate(16);
    while p.len() < 16 {
        p.push('0');
    }
    p
}

/// The fail-CLOSED error for a non-derivable tenant on the production
/// Turbo path (no TDK, or a non-UUID tenant). Surfaced as
/// [`TurboBridgeError::Internal`] so the op NEVER writes/reads under a
/// public, predictable `pad16` prefix (INV-TENANT-ISOLATION). Mirrors the
/// CAS/AC `R2*Handler` 500 posture.
#[cfg(not(test))]
fn non_derivable_tenant_err() -> TurboBridgeError {
    tracing::error!(
        "Turbo R2KvStore: tenant prefix is not derivable on the production path \
         (missing TDK or non-UUID tenant); refusing a public predictable prefix \
         (fail-closed, INV-TENANT-ISOLATION)"
    );
    TurboBridgeError::Internal("non-derivable tenant prefix (INV-TENANT-ISOLATION)".to_owned())
}

/// Build an [`R2KvStore`] (R2-backed) from environment variables, or
/// `None` when storage credentials are not configured (dev/CI → caller
/// falls back to `InMemoryKvStore`). Mirrors
/// `r2_s3::build_r2_ac_handler_from_env`.
///
/// Bucket: `R2_TURBO_BUCKET` (default `corelink-turbo-prod`).
///
/// # Errors
///
/// Returns `Some(Err(..))` when creds are present but the S3 client cannot
/// be constructed.
#[must_use]
pub async fn build_r2_kv_from_env() -> Option<Result<R2KvStore, String>> {
    let env = StorageEnv::from_env()?;
    let bucket =
        super::non_empty_env("R2_TURBO_BUCKET").unwrap_or_else(|| "corelink-turbo-prod".to_owned());
    // FAIL CLOSED — mirror `build_r2_cas_handler_from_env`: storage creds
    // are present, so this is the production data plane and the secret
    // TDK is MANDATORY. Without it the per-tenant prefix would degrade to
    // a public, predictable scheme and let two tenants onboarded in the
    // same millisecond (UUIDv7) collide into one SHARED Turbo keyspace
    // (cross-tenant cache leak/poisoning). Refuse to construct the store
    // (the route falls back / does not serve) rather than serve in the
    // silently-degraded public-prefix mode (INV-TENANT-ISOLATION).
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to build the R2 Turbo KV store (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, &bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    let backend: Arc<dyn KvBackend> = Arc::new(R2Backend { client });
    Some(Ok(R2KvStore::new(backend, Some(tdk_bytes))))
}

/// Load the tenant derivation key from `R2_TDK_HEX` (64 hex / 32 bytes).
/// `None` when unset/malformed → raw-prefix fallback. Self-contained to
/// keep this module disjoint from `r2_s3.rs`.
fn load_tdk_from_env() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let raw = hex::decode(hex_str.trim()).ok()?;
    let arr: [u8; 32] = raw.try_into().ok()?;
    Some(Zeroizing::new(arr))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // ── FakeBackend: in-process KV with fault injection + shared store ──
    // The store is an Arc<Mutex<HashMap>> so a SECOND R2KvStore built on a
    // CLONE of the backend sees prior writes → proves durability is
    // decoupled from store/handler lifetime (the R2KvStore-vs-InMemory point).

    #[derive(Debug, Clone)]
    struct FakeBackend {
        store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fault: Arc<Mutex<Option<String>>>,
        /// When set, ONLY `get` errors (the presence probe) — `put` still
        /// succeeds. Used to exercise the rt34 probe-error fail-CLOSED path
        /// where `write` must report `None` (charge full) yet the body is stored.
        get_only_fault: Arc<Mutex<Option<String>>>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                store: Arc::new(Mutex::new(HashMap::new())),
                fault: Arc::new(Mutex::new(None)),
                get_only_fault: Arc::new(Mutex::new(None)),
            }
        }
        /// Inject a backend failure: subsequent get/put return Err(msg).
        fn fail(&self, msg: &str) {
            *self.fault.lock().unwrap() = Some(msg.to_owned());
        }
        /// Inject a GET-only failure: subsequent `get` returns Err(msg) but
        /// `put` still succeeds.
        fn fail_get_only(&self, msg: &str) {
            *self.get_only_fault.lock().unwrap() = Some(msg.to_owned());
        }
    }

    #[async_trait]
    impl KvBackend for FakeBackend {
        async fn get(&self, object_key: &str) -> Result<Option<Vec<u8>>, String> {
            if let Some(e) = self.fault.lock().unwrap().clone() {
                return Err(e);
            }
            if let Some(e) = self.get_only_fault.lock().unwrap().clone() {
                return Err(e);
            }
            Ok(self.store.lock().unwrap().get(object_key).cloned())
        }
        async fn put(&self, object_key: &str, bytes: Vec<u8>) -> Result<(), String> {
            if let Some(e) = self.fault.lock().unwrap().clone() {
                return Err(e);
            }
            self.store
                .lock()
                .unwrap()
                .insert(object_key.to_owned(), bytes);
            Ok(())
        }
    }

    fn store_with(backend: FakeBackend) -> R2KvStore {
        R2KvStore::new(Arc::new(backend), None)
    }

    // ── F1/F2 — production posture: a TDK + UUID tenant HMACs the prefix,
    //    never the public pad16 of the tenant string ──

    /// With a TDK configured (the PRODUCTION posture — `build_r2_kv_from_env`
    /// now fails closed without one), a canonical UUID tenant resolves to
    /// the secret-keyed `derive_prefix` HMAC, NOT the public, predictable
    /// `pad16` prefix. This is the regression pin for finding #4: the
    /// Turbo store mirrors CAS/AC and never serves under a predictable
    /// 16-char-truncated prefix that two same-millisecond UUIDv7 tenants
    /// could collide into.
    #[test]
    fn iso_tdk_path_uses_hmac_prefix_not_pad16() {
        let tdk = Zeroizing::new([0x5au8; 32]);
        let s = R2KvStore::new(Arc::new(FakeBackend::new()), Some(tdk.clone()));
        let tenant = "0190abcd-1234-75ab-8def-0123456789ab";
        let ok = s.object_key(tenant, "artifact").unwrap();
        let prefix = ok.split('/').next().unwrap();
        assert_eq!(prefix.len(), 16, "prefix must be 16 chars: {ok}");
        // Must NOT be the public pad16 of the raw tenant string.
        assert_ne!(
            prefix,
            pad16(tenant),
            "production prefix leaked the public pad16 tenant prefix (F1/F2)"
        );
        // And it must equal the canonical secret-keyed derivation.
        let expected = derive_prefix(
            &TenantDerivationKey::from_bytes(tdk),
            Uuid::try_parse(tenant).unwrap(),
        )
        .to_string();
        assert_eq!(prefix, expected, "prefix must be derive_prefix(tdk, uuid)");
    }

    // ── §1.2 Tenant isolation [P0][sec] — pure object_key, no runtime ──

    #[test]
    fn iso_object_key_isolates_by_tenant_prefix() {
        let s = store_with(FakeBackend::new());
        let k = s.object_key("teamA", "turbo-artifact-hash-xyz").unwrap();
        assert_eq!(k, "teamA00000000000/turbo-artifact-hash-xyz");
        // Different tenant → different prefix → no full-key collision.
        assert_ne!(k, s.object_key("teamB", "turbo-artifact-hash-xyz").unwrap());
    }

    #[test]
    fn iso_object_key_pads_short_truncates_long_tenants() {
        let s = store_with(FakeBackend::new());
        assert!(s
            .object_key("x", "k")
            .unwrap()
            .starts_with("x000000000000000/"));
        assert!(s
            .object_key("0123456789abcdefGHIJ", "k")
            .unwrap()
            .starts_with("0123456789abcdef/"));
    }

    #[test]
    fn iso_traversal_key_stays_literal_inside_prefix() {
        // A-ISO-5 / S-A-06: `../` and leading `/` are opaque key bytes,
        // preserved verbatim AFTER the tenant prefix — they cannot escape
        // the `<prefix16>/` namespace.
        let s = store_with(FakeBackend::new());
        for evil in [
            "../otherprefix/k",
            "/abs/k",
            "..%2F..%2Fvictim",
            "a/../../b",
        ] {
            let ok = s.object_key("teamA", evil).unwrap();
            assert!(
                ok.starts_with("teamA00000000000/"),
                "key {evil:?} escaped prefix: {ok}"
            );
            assert!(ok.ends_with(evil), "opaque key {evil:?} was mutated: {ok}");
        }
    }

    // ── §1.1 round-trip + §1.3 LWW + §1.4 no-false-404 (need a runtime) ──

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rt_put_get_roundtrip_binary_and_empty() {
        let s = store_with(FakeBackend::new());
        for bytes in [vec![], vec![0u8], vec![0xFFu8; 4096], (0..=255u8).collect()] {
            s.write("t", "k", bytes.clone()).unwrap();
            assert_eq!(s.read("t", "k").unwrap(), bytes);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rt_last_write_wins_and_empty_overwrite() {
        let s = store_with(FakeBackend::new());
        // Fresh insert ⇒ no prior (None). 2-byte body.
        assert_eq!(s.write("t", "k", b"v1".to_vec()).unwrap(), None);
        // Overwrite ⇒ reports the PRIOR byte length (2).
        assert_eq!(s.write("t", "k", b"v2".to_vec()).unwrap(), Some(2));
        assert_eq!(s.read("t", "k").unwrap(), b"v2");
        // Empty PUT is a real overwrite, not a no-op; prior was 2 bytes.
        assert_eq!(s.write("t", "k", vec![]).unwrap(), Some(2));
        assert_eq!(s.read("t", "k").unwrap(), Vec::<u8>::new());
        // Re-write over the now-empty object ⇒ prior is Some(0).
        assert_eq!(s.write("t", "k", b"new".to_vec()).unwrap(), Some(0));
    }

    /// rt34 finding #3/#4/#5/#6 — the byte-delta contract `write` exposes:
    ///   fresh insert ⇒ None (route charges full new);
    ///   same-size overwrite ⇒ Some(n) where n == new len (route releases n ⇒ net 0);
    ///   GROW 1→big ⇒ Some(1) (route releases 1 ⇒ net +(big-1));
    ///   SHRINK big→1 ⇒ Some(big) (route releases big ⇒ net -(big-1)).
    /// The route accrues the full NEW len up front and releases `prior_len`;
    /// this test pins the prior-len values that drive that reconciliation.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rt_write_reports_prior_len_for_byte_delta() {
        let s = store_with(FakeBackend::new());
        // Fresh insert: no prior ⇒ route keeps the full new charge.
        assert_eq!(
            s.write("t", "k", b"x".to_vec()).unwrap(),
            None,
            "fresh ⇒ None"
        );
        // Same-size overwrite (1→1): prior = 1 ⇒ release 1, net 0.
        assert_eq!(
            s.write("t", "k", b"y".to_vec()).unwrap(),
            Some(1),
            "same-size overwrite ⇒ prior len 1 (net 0 after release)"
        );
        // GROW 1 → 1000: prior = 1 ⇒ release 1, net +(1000-1).
        let big = vec![0u8; 1000];
        assert_eq!(
            s.write("t", "k", big.clone()).unwrap(),
            Some(1),
            "grow ⇒ prior len 1 (net +999)"
        );
        // SHRINK 1000 → 1: prior = 1000 ⇒ release 1000, net -(1000-1).
        assert_eq!(
            s.write("t", "k", b"z".to_vec()).unwrap(),
            Some(1000),
            "shrink ⇒ prior len 1000 (net -999)"
        );
    }

    /// rt34: a probe (get) ERROR on the presence check fails CLOSED to `None`
    /// (treat as fresh ⇒ the route charges the full new bytes, never releasing
    /// an unconfirmed prior), while the PUT body is still stored. A missed
    /// prior-release over-counts the tenant — conservative, never under-counts.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rt_write_probe_error_fails_closed_to_none() {
        let backend = FakeBackend::new();
        let s = R2KvStore::new(Arc::new(backend.clone()), None);
        // Seed a real prior object so a WORKING probe would return Some(11).
        assert_eq!(s.write("t", "k", b"prior-bytes".to_vec()).unwrap(), None);
        // Now error ONLY the presence-probe `get`; `put` still succeeds.
        backend.fail_get_only("R2 503 on probe");
        // Probe error ⇒ write reports None (fail CLOSED: do NOT release the
        // unconfirmed prior), even though a prior object DID exist.
        assert_eq!(
            s.write("t", "k", b"new-body".to_vec()).unwrap(),
            None,
            "probe error must fail CLOSED to None (charge full, release nothing)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn miss_returns_notfound_not_fabricated_empty() {
        let s = store_with(FakeBackend::new());
        match s.read("t", "never-written") {
            Err(TurboBridgeError::NotFound { .. }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // ── §1.2 isolation at the READ/WRITE level [P0][sec] ──

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn iso_cross_tenant_invisible_and_independent() {
        // Shared backend → both tenants hit the same physical store; only
        // the prefix keeps them apart. This is the breach test.
        let backend = FakeBackend::new();
        let a = R2KvStore::new(Arc::new(backend.clone()), None);
        let b = R2KvStore::new(Arc::new(backend), None);
        a.write("tenantA", "k", b"secretA".to_vec()).unwrap();
        // B cannot see A's object under the same opaque key.
        match b.read("tenantB", "k") {
            Err(TurboBridgeError::NotFound { .. }) => {}
            other => panic!("cross-tenant leak: {other:?}"),
        }
        // Same key, different tenants, different bytes → each its own.
        b.write("tenantB", "k", b"valueB".to_vec()).unwrap();
        assert_eq!(a.read("tenantA", "k").unwrap(), b"secretA");
        assert_eq!(b.read("tenantB", "k").unwrap(), b"valueB");
    }

    // ── §1.4 DURABILITY across handler rebuild [durability] ──

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dur_object_survives_store_rebuild() {
        let backend = FakeBackend::new();
        {
            let first = R2KvStore::new(Arc::new(backend.clone()), None);
            first.write("t", "k", b"durable".to_vec()).unwrap();
        } // first store dropped — simulates container/handler restart.
        let rebuilt = R2KvStore::new(Arc::new(backend), None);
        assert_eq!(
            rebuilt.read("t", "k").unwrap(),
            b"durable",
            "R2KvStore must persist across rebuild (the whole point vs InMemory)"
        );
    }

    // ── §1.4 fault injection: backend error → Internal, NEVER false 404 ──

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn res_backend_get_error_is_internal_not_notfound() {
        let backend = FakeBackend::new();
        let s = R2KvStore::new(Arc::new(backend.clone()), None);
        s.write("t", "k", b"present".to_vec()).unwrap();
        backend.fail("R2 503 transient");
        match s.read("t", "k") {
            Err(TurboBridgeError::Internal(_)) => {} // correct: present data NOT masked as absent
            Err(TurboBridgeError::NotFound { .. }) => {
                panic!("false 404: backend error masked present data as absent")
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn res_backend_put_error_surfaces_no_false_success() {
        let backend = FakeBackend::new();
        let s = R2KvStore::new(Arc::new(backend.clone()), None);
        backend.fail("R2 PUT 500");
        match s.write("t", "k", b"x".to_vec()) {
            Err(TurboBridgeError::Internal(_)) => {}
            other => panic!("write must surface backend error, got {other:?}"),
        }
    }

    // ── Property-based (pure object_key): determinism, injectivity, opacity ──

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// A-RT-5: object_key is a pure deterministic function.
        #[test]
        fn prop_object_key_deterministic(t in "[a-zA-Z0-9_-]{1,40}", k in ".{0,200}") {
            let s = store_with(FakeBackend::new());
            proptest::prop_assert_eq!(
                s.object_key(&t, &k).unwrap(),
                s.object_key(&t, &k).unwrap()
            );
        }

        /// A-ISO-3 [sec]: distinct tenants never share a full object key for
        /// the same opaque key (prefix injectivity over the pad16 domain).
        #[test]
        fn prop_distinct_tenants_no_keyspace_collision(
            t1 in "[a-zA-Z0-9_-]{1,40}", t2 in "[a-zA-Z0-9_-]{1,40}", k in ".{0,80}"
        ) {
            proptest::prop_assume!(pad16(&t1) != pad16(&t2));
            let s = store_with(FakeBackend::new());
            proptest::prop_assert_ne!(
                s.object_key(&t1, &k).unwrap(),
                s.object_key(&t2, &k).unwrap()
            );
        }

        /// A-RT-2 [sec]: key opacity — the opaque key appears verbatim after
        /// the tenant prefix; no normalization/aliasing alters which slot is
        /// addressed, and no key can escape the prefix.
        #[test]
        fn prop_key_opacity_no_escape(t in "[a-zA-Z0-9_-]{1,16}", k in ".{1,200}") {
            let s = store_with(FakeBackend::new());
            let ok = s.object_key(&t, &k).unwrap();
            let expected_prefix = format!("{}/", pad16(&t));
            proptest::prop_assert!(ok.starts_with(&expected_prefix));
            proptest::prop_assert_eq!(&ok[expected_prefix.len()..], &k);
        }
    }
}

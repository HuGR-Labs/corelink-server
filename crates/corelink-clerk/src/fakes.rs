//! In-memory fakes for unit + property tests.
//!
//! These fakes implement the public traits without any network /
//! Cloudflare runtime dependency. Production wiring (`worker::Fetch`,
//! `worker::kv::Store`) lives in `corelink-worker` (S-03 wiring).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::jwks::{Jwks, JwksFetchError, JwksFetchFuture, JwksFetcher};
use crate::jwks_cache::{CachedJwks, KvJwksCache, KvJwksCacheError, KvJwksCacheFuture};

/// In-memory KV cache backed by a `HashMap<String, (Vec<u8>, expires_at)>`.
///
/// Honours TTL via wall-clock comparison against an injectable clock
/// (see [`InMemoryKvCache::with_clock`]); production wiring uses
/// `SystemTime::now`.
pub struct InMemoryKvCache {
    store: Mutex<HashMap<String, Slot>>,
    clock: Box<dyn Fn() -> SystemTime + Send + Sync>,
}

impl std::fmt::Debug for InMemoryKvCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryKvCache")
            .field("entries", &self.store.lock().map(|g| g.len()).unwrap_or(0))
            .finish()
    }
}

#[derive(Clone, Debug)]
struct Slot {
    payload: Vec<u8>,
    stored_at: SystemTime,
    expires_at: SystemTime,
}

impl InMemoryKvCache {
    /// New cache with the canonical `SystemTime::now` clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            clock: Box::new(SystemTime::now),
        }
    }

    /// New cache with a deterministic clock (test helper). The clock
    /// is read on every `get`/`set` so a test driver can advance time
    /// between calls.
    #[must_use]
    pub fn with_clock<F>(clock: F) -> Self
    where
        F: Fn() -> SystemTime + Send + Sync + 'static,
    {
        Self {
            store: Mutex::new(HashMap::new()),
            clock: Box::new(clock),
        }
    }
}

impl Default for InMemoryKvCache {
    fn default() -> Self {
        Self::new()
    }
}

impl KvJwksCache for InMemoryKvCache {
    fn get<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, Option<CachedJwks>> {
        Box::pin(async move {
            let now = (self.clock)();
            let guard = self
                .store
                .lock()
                .map_err(|e| KvJwksCacheError::Backend(format!("mutex poisoned: {e}")))?;
            let slot = match guard.get(instance_hash) {
                Some(s) => s.clone(),
                None => return Ok(None),
            };
            drop(guard);
            if slot.expires_at <= now {
                return Ok(None);
            }
            let jwks =
                Jwks::parse(&slot.payload).map_err(|e| KvJwksCacheError::Corrupt(e.to_string()))?;
            Ok(Some(CachedJwks {
                jwks,
                stored_at: slot.stored_at,
            }))
        })
    }

    fn set<'a>(
        &'a self,
        instance_hash: &'a str,
        jwks: &'a Jwks,
        ttl: Duration,
    ) -> KvJwksCacheFuture<'a, ()> {
        Box::pin(async move {
            let now = (self.clock)();
            let expires_at = now
                .checked_add(ttl)
                .ok_or_else(|| KvJwksCacheError::Backend("clock overflow".into()))?;
            let payload = jwks.to_json().into_bytes();
            let mut guard = self
                .store
                .lock()
                .map_err(|e| KvJwksCacheError::Backend(format!("mutex poisoned: {e}")))?;
            guard.insert(
                instance_hash.to_owned(),
                Slot {
                    payload,
                    stored_at: now,
                    expires_at,
                },
            );
            Ok(())
        })
    }

    fn delete<'a>(&'a self, instance_hash: &'a str) -> KvJwksCacheFuture<'a, ()> {
        Box::pin(async move {
            let mut guard = self
                .store
                .lock()
                .map_err(|e| KvJwksCacheError::Backend(format!("mutex poisoned: {e}")))?;
            guard.remove(instance_hash);
            Ok(())
        })
    }
}

/// JWKS fetcher with a fixed in-memory document.
#[derive(Debug)]
pub struct StaticJwksFetcher {
    jwks: Mutex<Jwks>,
    fetch_count: AtomicUsize,
    forced_error: Mutex<Option<JwksFetchError>>,
}

impl StaticJwksFetcher {
    /// New fetcher serving a fixed [`Jwks`].
    #[must_use]
    pub fn new(jwks: Jwks) -> Self {
        Self {
            jwks: Mutex::new(jwks),
            fetch_count: AtomicUsize::new(0),
            forced_error: Mutex::new(None),
        }
    }

    /// Empty (no keys) fetcher — useful for negative tests.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(Jwks::default())
    }

    /// Replace the served JWKS atomically (test rotation helper).
    pub fn rotate_to(&self, jwks: Jwks) {
        if let Ok(mut guard) = self.jwks.lock() {
            *guard = jwks;
        }
    }

    /// Force the next fetch (and all subsequent fetches until cleared)
    /// to fail with the given error. `None` clears the override.
    pub fn force_error(&self, err: Option<JwksFetchError>) {
        if let Ok(mut guard) = self.forced_error.lock() {
            *guard = err;
        }
    }

    /// How many times `fetch` has been invoked.
    #[must_use]
    pub fn fetch_count(&self) -> usize {
        self.fetch_count.load(Ordering::SeqCst)
    }
}

impl JwksFetcher for StaticJwksFetcher {
    fn fetch<'a>(&'a self, _url: &'a str) -> JwksFetchFuture<'a> {
        Box::pin(async move {
            self.fetch_count.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut guard) = self.forced_error.lock() {
                if let Some(err) = guard.take() {
                    // Re-arm; we want sticky errors until explicitly cleared.
                    let cloned = match &err {
                        JwksFetchError::Transport(s) => JwksFetchError::Transport(s.clone()),
                        JwksFetchError::HttpStatus { status } => {
                            JwksFetchError::HttpStatus { status: *status }
                        }
                        JwksFetchError::Malformed(s) => JwksFetchError::Malformed(s.clone()),
                    };
                    *guard = Some(cloned);
                    return Err(err);
                }
            }
            let jwks = self
                .jwks
                .lock()
                .map_err(|e| JwksFetchError::Transport(format!("mutex poisoned: {e}")))?
                .clone();
            Ok(jwks)
        })
    }
}

/// JWKS fetcher backed by a scripted sequence of `Jwks` results
/// (one per call). Useful for rotation tests that need to surface
/// "first call returns kid_v1 only, second call returns kid_v1+v2".
#[derive(Debug)]
pub struct ScriptedJwksFetcher {
    script: Mutex<Vec<Jwks>>,
    fetch_count: AtomicUsize,
    last_default: Mutex<Jwks>,
}

impl ScriptedJwksFetcher {
    /// Build a fetcher serving `script[i]` on call `i`. After the
    /// script is exhausted, the LAST scripted entry is replayed
    /// (mirrors KV cache stickiness in production).
    #[must_use]
    pub fn new(script: Vec<Jwks>) -> Self {
        let last = script.last().cloned().unwrap_or_default();
        Self {
            script: Mutex::new(script),
            fetch_count: AtomicUsize::new(0),
            last_default: Mutex::new(last),
        }
    }

    /// Number of fetches dispatched so far.
    #[must_use]
    pub fn fetch_count(&self) -> usize {
        self.fetch_count.load(Ordering::SeqCst)
    }
}

impl JwksFetcher for ScriptedJwksFetcher {
    fn fetch<'a>(&'a self, _url: &'a str) -> JwksFetchFuture<'a> {
        Box::pin(async move {
            let idx = self.fetch_count.fetch_add(1, Ordering::SeqCst);
            let script = self
                .script
                .lock()
                .map_err(|e| JwksFetchError::Transport(format!("mutex poisoned: {e}")))?;
            if let Some(jwks) = script.get(idx) {
                return Ok(jwks.clone());
            }
            drop(script);
            let last = self
                .last_default
                .lock()
                .map_err(|e| JwksFetchError::Transport(format!("mutex poisoned: {e}")))?
                .clone();
            Ok(last)
        })
    }
}

/// Wrapper that adds an injectable clock to any [`JwksFetcher`]. Used
/// in chaos tests where we want to verify the adapter respects
/// `stored_at + ttl` even when the underlying KV layer reports
/// absurdly stale rows.
pub struct ClockedFetcher<F>
where
    F: JwksFetcher,
{
    inner: F,
    /// Public so tests can swap the clock on the fly.
    pub clock: Box<dyn Fn() -> SystemTime + Send + Sync>,
}

impl<F: JwksFetcher + std::fmt::Debug> std::fmt::Debug for ClockedFetcher<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClockedFetcher")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<F: JwksFetcher> ClockedFetcher<F> {
    /// Wrap `inner` with the supplied clock fn.
    pub fn new(inner: F, clock: impl Fn() -> SystemTime + Send + Sync + 'static) -> Self {
        Self {
            inner,
            clock: Box::new(clock),
        }
    }
}

impl<F: JwksFetcher> JwksFetcher for ClockedFetcher<F> {
    fn fetch<'a>(&'a self, url: &'a str) -> JwksFetchFuture<'a> {
        let _now = (self.clock)();
        // Currently the clock is purely informational; reserved for
        // future "stale fetch" simulation hooks.
        let fut: Pin<Box<dyn Future<Output = _> + Send + 'a>> = self.inner.fetch(url);
        fut
    }
}

/// Test-only RSA keypair + JWKS-key projection.
///
/// The module is feature-gated on `test-utils`; the integration test
/// suite enables this feature via `dev-dependencies = { features =
/// ["test-utils"] }`. Production crates must NEVER depend on this
/// surface.
#[cfg(feature = "test-utils")]
pub mod test_keys {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs8::EncodePublicKey;
    use rsa::traits::PublicKeyParts;
    use rsa::{RsaPrivateKey, RsaPublicKey};

    /// Test-only RSA-2048 keypair with PEM serialisation + JWKS-key
    /// projection. Uses `rand::thread_rng` for keygen — never wired
    /// into the production validate path.
    #[derive(Debug)]
    #[allow(missing_docs, reason = "test-only fields; full doc in module rustdoc")]
    pub struct TestRsaKey {
        pub kid: String,
        pub private_pem: String,
        pub public_pem: String,
        pub jwks_key: crate::jwks::JwksKey,
    }

    impl TestRsaKey {
        /// Generate a fresh RSA-2048 keypair tagged `kid`.
        #[allow(
            clippy::expect_used,
            reason = "test-only: keygen failures here are catastrophic test-environment misconfig"
        )]
        pub fn generate(kid: &str) -> Self {
            let mut rng = rand::thread_rng();
            let private = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
            let public = RsaPublicKey::from(&private);
            let private_pem = private
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .expect("serialise private")
                .to_string();
            let public_pem = public
                .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
                .expect("serialise public");
            // JWKS components are base64url-no-pad of the big-endian
            // modulus + exponent.
            let n_be = public.n().to_bytes_be();
            let e_be = public.e().to_bytes_be();
            let n_b64url = URL_SAFE_NO_PAD.encode(n_be);
            let e_b64url = URL_SAFE_NO_PAD.encode(e_be);
            let jwks_key = crate::jwks::JwksKey {
                kid: kid.to_owned(),
                n_b64url,
                e_b64url,
            };
            Self {
                kid: kid.to_owned(),
                private_pem,
                public_pem,
                jwks_key,
            }
        }

        /// Build a JWKS containing only this key.
        pub fn into_jwks(&self) -> crate::jwks::Jwks {
            crate::jwks::Jwks::from_keys(vec![self.jwks_key.clone()])
        }
    }
}

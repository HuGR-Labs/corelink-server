//! Minimal KV-namespace abstraction for the negative cache (WI-S02-005).
//!
//! Mirrors the trait-abstraction pattern used by `corelink-worker::storage::r2`
//! ([`crate::storage::r2::R2Backend`]) and `corelink-meta::MetaStore`: the
//! crate exposes a tiny [`KvBackend`] surface (`get_with_metadata`,
//! `put_with_ttl`, `delete`) plus an in-memory test fake
//! ([`InMemoryKv`]) that preserves the documented semantics. The real
//! Cloudflare Workers KV binding adapter (`worker::KvNamespace`) only
//! exists inside the Workers runtime; it is wired in at integration tier
//! alongside the REAPI handler.
//!
//! ## Why a custom trait (and not, say, a serde/JSON shape)
//!
//! The cached value is a single byte (a [`crate::cache::miss_reason::MissReason::tag`]); no
//! serialization framework is justified. The trait operates on raw bytes
//! so the in-memory fake and the future production binding adapter share
//! exactly the same sub-byte-level semantics — no JSON encoder bug can
//! creep between the two.
//!
//! ## TTL semantics
//!
//! [`KvBackend::put_with_ttl`] takes an absolute TTL in **seconds**. The
//! in-memory fake honors the TTL by recording a UNIX-epoch expiry
//! timestamp at write time and lazily evicting on read. Production
//! Cloudflare Workers KV maps the same parameter onto the
//! `expirationTtl` argument of `KV.put(...)` (Cloudflare runtime cap:
//! ≥ 60 s; the negative cache caps at 300 s and is comfortably above
//! the floor).
//!
//! ## Eventual consistency reality
//!
//! The trait surface intentionally exposes **no** `compare_and_swap`
//! primitive — Cloudflare Workers KV does not offer one (see WI §9.11).
//! Concurrent ops resolve last-write-wins; the negative cache's
//! correctness argument relies on TTL bound + explicit invalidation hook
//! + Bazel client retry semantics (WI §9.11), not on atomicity.

use core::fmt;
use core::future::Future;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use thiserror::Error;

/// Test-injectable wall-clock source so the TTL property tests can
/// fast-forward time without `std::thread::sleep`.
pub trait Clock: Send + Sync + fmt::Debug {
    /// Returns the current Unix epoch in **seconds**. The cache uses
    /// only second-resolution timestamps; Cloudflare Workers KV
    /// `expirationTtl` is similarly second-granular.
    fn now_unix_secs(&self) -> u64;
}

/// Default monotonic-ish clock backed by [`SystemTime::UNIX_EPOCH`]. Not
/// used by the property tests (those inject [`FakeClock`]), but it is
/// the production default the Cloudflare adapter wraps.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_secs(&self) -> u64 {
        // Wall clock can in principle return a pre-1970 instant on
        // catastrophic NTP misconfiguration; on Cloudflare Workers it is
        // monotonic+SNTP-synchronized so this branch is unreachable in
        // practice. We map the failure to "epoch 0" rather than panic
        // — the only consequence is that TTLs computed during such a
        // window run slightly long (bounded by the eventual NTP sync).
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// Test-only fake [`Clock`] whose `now` is owned by the test thread.
///
/// Used by [`crate::cache::negative`]'s property tests to simulate TTL
/// expiry deterministically.
#[derive(Debug)]
pub struct FakeClock {
    now: std::sync::atomic::AtomicU64,
}

impl FakeClock {
    /// Construct a fake clock pinned at `start_unix_secs`.
    #[must_use]
    pub const fn new(start_unix_secs: u64) -> Self {
        Self {
            now: std::sync::atomic::AtomicU64::new(start_unix_secs),
        }
    }

    /// Move the fake clock forward by `delta`.
    pub fn advance(&self, delta: Duration) {
        let secs = delta.as_secs();
        self.now
            .fetch_add(secs, std::sync::atomic::Ordering::AcqRel);
    }

    /// Set the absolute time.
    pub fn set(&self, unix_secs: u64) {
        self.now
            .store(unix_secs, std::sync::atomic::Ordering::Release);
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new(1_700_000_000)
    }
}

impl Clock for FakeClock {
    fn now_unix_secs(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Errors surfaced by [`KvBackend`] implementations.
#[derive(Debug, Error)]
pub enum KvError {
    /// Backend transport / runtime fault. Maps to `COR_SERVICE_DEGRADED`
    /// upstream; the negative-cache layer treats this as a soft miss
    /// (graceful fall-through to D1 + R2 per WI §15.1).
    #[error("KV backend fault: {0}")]
    Backend(String),

    /// The stored value did not parse as a known
    /// [`crate::cache::miss_reason::MissReason::tag`]. Treated as a soft
    /// miss + an
    /// emit of `corelink_cas_negative_cache_corrupt_total{region}` (the
    /// metric name is informational; the storage adapter does not own
    /// the metric pipe — observers in S-09 will).
    #[error("KV value corrupt: {detail}")]
    Corrupt {
        /// Diagnostic message — never surfaced to clients.
        detail: String,
    },
}

/// Minimal KV namespace surface used by [`crate::cache::negative::NegativeCache`].
///
/// All methods are async and operate on raw bytes; the negative cache
/// itself owns the byte-level encoding (a single-byte
/// [`crate::cache::miss_reason::MissReason::tag`]) so multiple cache
/// kinds can share the same backend in the future without ABI drift.
///
/// `get_with_metadata` is named to signal that a future revision may
/// surface the KV "metadata" object (Cloudflare Workers KV exposes one);
/// at S-02 GA the trait returns a single `Option<Vec<u8>>` and the
/// metadata channel is intentionally absent — adding it later is a
/// minor + additive trait change (the `default` body returns `None`).
pub trait KvBackend: Send + Sync {
    /// GET `key`. Returns the stored bytes if present (and not yet TTL-
    /// expired), or `Ok(None)` on miss / soft-eviction.
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send + 'a;

    /// PUT `value` at `key` with absolute TTL `ttl_secs` (Cloudflare
    /// Workers KV `expirationTtl` semantics). Idempotent under
    /// last-write-wins; no atomic CAS primitive is exposed.
    fn put_with_ttl<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> impl Future<Output = Result<(), KvError>> + Send + 'a;

    /// DELETE `key`. No-op on missing key (idempotent — the negative
    /// cache invalidation hook may fire repeatedly under client retry).
    fn delete<'a>(&'a self, key: &'a str)
        -> impl Future<Output = Result<(), KvError>> + Send + 'a;
}

// ---------------------------------------------------------------------------
// In-memory test backend
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct StoredEntry {
    value: Vec<u8>,
    expires_at_unix_secs: u64,
}

/// Host-side test fake for [`KvBackend`].
///
/// Backed by a `Mutex<HashMap<String, StoredEntry>>`. Honors TTL by
/// stamping an absolute UNIX-epoch expiry at write time and lazily
/// evicting on read (we deliberately do not run a background sweeper —
/// the test surface is identical to the production CF binding's lazy
/// expiry semantics).
///
/// The fake is **not** hardened for high concurrency; it exists to drive
/// the same `cargo test` paths as the production binding adapter under
/// `cargo test`.
pub struct InMemoryKv<C: Clock = SystemClock> {
    inner: Mutex<HashMap<String, StoredEntry>>,
    clock: C,
}

impl<C: Clock> fmt::Debug for InMemoryKv<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryKv")
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}

impl InMemoryKv<SystemClock> {
    /// Construct a fresh, empty KV fake backed by [`SystemClock`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(SystemClock)
    }
}

impl Default for InMemoryKv<SystemClock> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock> InMemoryKv<C> {
    /// Construct a fresh, empty KV fake backed by an arbitrary clock.
    /// Intended for property/integration tests that need
    /// [`FakeClock`].
    #[must_use]
    pub fn with_clock(clock: C) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            clock,
        }
    }

    /// Borrow the embedded clock. Used by the negative-cache unit
    /// tests to fast-forward [`FakeClock`] without re-routing
    /// through `with_clock(Arc<C>)`.
    #[must_use]
    pub const fn clock(&self) -> &C {
        &self.clock
    }

    /// Number of stored (and not yet expired) entries. Test-only
    /// helper.
    #[must_use]
    #[allow(
        clippy::expect_used,
        reason = "test fake: a poisoned mutex in test code is itself a test failure"
    )]
    pub fn live_len(&self) -> usize {
        let now = self.clock.now_unix_secs();
        let guard = self
            .inner
            .lock()
            .expect("InMemoryKv mutex must not be poisoned in test code");
        guard
            .values()
            .filter(|e| e.expires_at_unix_secs > now)
            .count()
    }

    /// Total stored entries (including expired-but-not-yet-evicted).
    /// Test-only.
    #[must_use]
    #[allow(
        clippy::expect_used,
        reason = "test fake: a poisoned mutex in test code is itself a test failure"
    )]
    pub fn raw_len(&self) -> usize {
        let guard = self
            .inner
            .lock()
            .expect("InMemoryKv mutex must not be poisoned in test code");
        guard.len()
    }

    /// Snapshot the current set of keys (live + expired). Test-only.
    #[must_use]
    #[allow(
        clippy::expect_used,
        reason = "test fake: a poisoned mutex in test code is itself a test failure"
    )]
    pub fn keys_snapshot(&self) -> Vec<String> {
        let guard = self
            .inner
            .lock()
            .expect("InMemoryKv mutex must not be poisoned in test code");
        guard.keys().cloned().collect()
    }

    /// Test-only escape hatch: poison the value at `key` with arbitrary
    /// bytes, simulating a corrupted KV entry. Used by the corruption
    /// regression tests.
    #[allow(
        clippy::expect_used,
        reason = "test fake: a poisoned mutex in test code is itself a test failure"
    )]
    pub fn poison_for_test(&self, key: &str, raw: Vec<u8>, ttl_secs: u64) {
        let mut guard = self
            .inner
            .lock()
            .expect("InMemoryKv mutex must not be poisoned in test code");
        let now = self.clock.now_unix_secs();
        guard.insert(
            key.to_owned(),
            StoredEntry {
                value: raw,
                expires_at_unix_secs: now.saturating_add(ttl_secs),
            },
        );
    }
}

impl<C: Clock> KvBackend for InMemoryKv<C> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| KvError::Backend("in-memory KV mutex poisoned".to_owned()))?;
        let now = self.clock.now_unix_secs();
        if let Some(entry) = guard.get(key) {
            if entry.expires_at_unix_secs <= now {
                // Lazy eviction (CF KV semantics).
                guard.remove(key);
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }
        Ok(None)
    }

    async fn put_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> Result<(), KvError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| KvError::Backend("in-memory KV mutex poisoned".to_owned()))?;
        let now = self.clock.now_unix_secs();
        let expires_at_unix_secs = now.saturating_add(ttl_secs);
        guard.insert(
            key.to_owned(),
            StoredEntry {
                value,
                expires_at_unix_secs,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| KvError::Backend("in-memory KV mutex poisoned".to_owned()))?;
        guard.remove(key);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Failure-injecting test backend
// ---------------------------------------------------------------------------

/// Test backend that returns [`KvError::Backend`] on every call. Used to
/// drive the WI §15.1 chaos test ("KV outage breaks read path → graceful
/// fall-through to D1 + R2").
#[derive(Debug)]
pub struct AlwaysFailingKv {
    /// Stable diagnostic carried back to the caller for assertion.
    pub diagnostic: String,
}

impl AlwaysFailingKv {
    /// Construct a failing backend with a fixed diagnostic message.
    #[must_use]
    pub fn new(diagnostic: impl Into<String>) -> Self {
        Self {
            diagnostic: diagnostic.into(),
        }
    }
}

impl KvBackend for AlwaysFailingKv {
    async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, KvError> {
        Err(KvError::Backend(self.diagnostic.clone()))
    }

    async fn put_with_ttl(
        &self,
        _key: &str,
        _value: Vec<u8>,
        _ttl_secs: u64,
    ) -> Result<(), KvError> {
        Err(KvError::Backend(self.diagnostic.clone()))
    }

    async fn delete(&self, _key: &str) -> Result<(), KvError> {
        Err(KvError::Backend(self.diagnostic.clone()))
    }
}

// ---------------------------------------------------------------------------
// Counting test backend (call observability for property tests)
// ---------------------------------------------------------------------------

/// Test wrapper that counts `get` / `put` / `delete` calls per inner
/// backend. Mirrors [`crate::storage::r2::CountingR2`]; lets the
/// property tests assert "the cache was consulted exactly N times" or
/// "the invalidate hook fired exactly once" without relying on
/// final-state inspection.
#[derive(Debug)]
pub struct CountingKv<B: KvBackend> {
    inner: B,
    get_calls: std::sync::atomic::AtomicUsize,
    put_calls: std::sync::atomic::AtomicUsize,
    delete_calls: std::sync::atomic::AtomicUsize,
}

impl<B: KvBackend> CountingKv<B> {
    /// Wrap an existing backend with call counters.
    #[must_use]
    pub const fn new(inner: B) -> Self {
        Self {
            inner,
            get_calls: std::sync::atomic::AtomicUsize::new(0),
            put_calls: std::sync::atomic::AtomicUsize::new(0),
            delete_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Borrow the inner backend.
    #[must_use]
    pub const fn inner(&self) -> &B {
        &self.inner
    }

    /// Number of `get` invocations observed.
    #[must_use]
    pub fn get_calls(&self) -> usize {
        self.get_calls.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Number of `put_with_ttl` invocations observed.
    #[must_use]
    pub fn put_calls(&self) -> usize {
        self.put_calls.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Number of `delete` invocations observed.
    #[must_use]
    pub fn delete_calls(&self) -> usize {
        self.delete_calls.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl<B: KvBackend> KvBackend for CountingKv<B> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        self.get_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.get(key).await
    }

    async fn put_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> Result<(), KvError> {
        self.put_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.put_with_ttl(key, value, ttl_secs).await
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.delete_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.delete(key).await
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: unwrap/panic on a failing assertion is itself a test failure"
)]
mod unit_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn put_then_get_within_ttl_returns_value() {
        let kv = InMemoryKv::new();
        kv.put_with_ttl("k", vec![0xab], 60).await.unwrap();
        let got = kv.get("k").await.unwrap();
        assert_eq!(got, Some(vec![0xab]));
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_key() {
        let kv = InMemoryKv::new();
        let got = kv.get("missing").await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn ttl_expiry_evicts_entry_via_fake_clock() {
        let clock = FakeClock::new(1_000_000);
        let kv = InMemoryKv::with_clock(clock);
        kv.put_with_ttl("k", vec![0x01], 60).await.unwrap();
        assert_eq!(kv.get("k").await.unwrap(), Some(vec![0x01]));
        // Advance past TTL.
        kv.clock.advance(Duration::from_secs(61));
        assert_eq!(kv.get("k").await.unwrap(), None);
        // Lazy eviction collapses to raw 0.
        assert_eq!(kv.raw_len(), 0);
    }

    #[tokio::test]
    async fn delete_is_idempotent_on_missing_key() {
        let kv = InMemoryKv::new();
        kv.delete("nonexistent").await.unwrap();
        kv.delete("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn delete_removes_existing_entry() {
        let kv = InMemoryKv::new();
        kv.put_with_ttl("k", vec![0xff], 60).await.unwrap();
        kv.delete("k").await.unwrap();
        assert_eq!(kv.get("k").await.unwrap(), None);
    }

    #[tokio::test]
    async fn last_write_wins_on_overwrite() {
        let kv = InMemoryKv::new();
        kv.put_with_ttl("k", vec![0x01], 60).await.unwrap();
        kv.put_with_ttl("k", vec![0x02], 60).await.unwrap();
        assert_eq!(kv.get("k").await.unwrap(), Some(vec![0x02]));
    }

    #[tokio::test]
    async fn counting_kv_observes_call_counts() {
        let kv = CountingKv::new(InMemoryKv::new());
        kv.put_with_ttl("k", vec![0x01], 60).await.unwrap();
        let _ = kv.get("k").await.unwrap();
        let _ = kv.get("k").await.unwrap();
        kv.delete("k").await.unwrap();
        assert_eq!(kv.put_calls(), 1);
        assert_eq!(kv.get_calls(), 2);
        assert_eq!(kv.delete_calls(), 1);
    }

    #[tokio::test]
    async fn always_failing_kv_emits_backend_diagnostic() {
        let kv = AlwaysFailingKv::new("simulated KV outage");
        let err = kv.get("k").await.unwrap_err();
        match err {
            KvError::Backend(d) => assert_eq!(d, "simulated KV outage"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fake_clock_set_overrides_advance() {
        let clock = FakeClock::new(100);
        clock.advance(Duration::from_secs(50));
        assert_eq!(clock.now_unix_secs(), 150);
        clock.set(1_000_000);
        assert_eq!(clock.now_unix_secs(), 1_000_000);
    }
}

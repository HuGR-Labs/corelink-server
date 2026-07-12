//! Container-side **tenant-suspend gate** for the OCI registry plane
//! (go-live gap G4b).
//!
//! # Why this exists — the worker-side gate does NOT cover OCI
//!
//! The worker-side tenant-suspend gate (`tenant_suspend_gate.ts`, PR #677) denies
//! a suspended/erased tenant at the Worker edge — but it does NOT cover the OCI
//! surface. The Worker forwards `/v2/*` + `/token` **RAW** (the two-leg
//! pass-through carve-out: it cannot resolve the PAT scope for the OCI flow, so
//! it strips no tenant and runs no per-tenant block before returning). The tenant
//! is resolved CONTAINER-side. So a SUSPENDED tenant could still push AND pull OCI
//! blobs — the exact sibling of the `$`-ceiling bypass ([`crate::routes::oci`])
//! and the request-count bypass ([`crate::request_count`]) that were already
//! closed container-side.
//!
//! # Invariant
//!
//! A tenant whose `tenant_offboarding_state.state ∈ {suspended, erased}` is DENIED
//! on the OCI plane — push AND pull — **fail-closed**. An `active` tenant, or one
//! in an EARLIER offboarding phase (`cancel_requested` / `grace_period` /
//! `read_only` — the export/restoration windows), is NOT denied here: those states
//! keep read/write access by design. Only the terminal `suspended` (T+45..T+90,
//! admin-revoke only) and `erased` (T+90+) states deny.
//!
//! # Two enforcement points (see [`crate::routes::oci`])
//!
//! 1. **PRIMARY — the `/token` mint** ([`crate::routes::oci`]'s `OciPatResolver`):
//!    a suspended tenant's token exchange returns the port error → NO bearer is
//!    minted → push AND pull are blocked at the door.
//! 2. **RESIDUAL — the `/v2/*` legs** (`oci_quota_gate`): a bearer minted BEFORE
//!    suspension stays valid for `TOKEN_TTL_SECS` (300 s), so a mid-session
//!    suspend would otherwise leak up to 5 min. The gate re-checks on every op
//!    and 403s a suspended tenant before the request runs.
//!
//! # Fail-CLOSED (deliberately — a suspension is a hard revocation)
//!
//! Unlike the SLO-style fail-OPEN request-count cap, the suspend gate is
//! fail-CLOSED for a tenant we KNOW is suspended:
//!
//! - a fresh cache hit is authoritative (suspended → deny; active → allow);
//! - a D1 read is issued on a miss/stale entry; a success updates the cache;
//! - on a D1 read ERROR, a tenant we have ALREADY seen suspended (cached, even
//!   stale) STAYS denied — the suspended verdict is sticky across a transient
//!   fault, so an outage cannot re-open a known-suspended tenant;
//! - the error is NEVER cached, so a successful contradicting read (e.g. an ops
//!   force-revert `suspended → active`) flips the tenant back to allowed on the
//!   next op;
//! - we fail-OPEN only for an UNKNOWN tenant (never confirmed suspended) whose
//!   read errors — denying every unclassifiable tenant during a D1 outage would
//!   take down the whole active fleet, so availability wins there. The data plane
//!   remains the deeper net.
//!
//! # Wiring
//!
//! Production ([`crate::routes`]) builds a [`CachedSuspendResolver`] over the SAME
//! [`crate::storage::d1_http::D1HttpClient`] the OCI moat map + cap resolver use,
//! and threads it into BOTH the `OciPatResolver` (token leg) and the `OciCostGate`
//! (v2 legs). In dev/CI (no D1 storage env) it is `None` and the gate is inert.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::Mutex as AsyncMutex;

use crate::storage::d1_http::D1HttpClient;
use crate::wall_clock::WallClock;

/// Resolves whether a tenant is in a DENIED offboarding state on the OCI plane.
///
/// `Ok(true)` ⇒ suspended/erased (DENY); `Ok(false)` ⇒ allowed; `Err` ⇒ a D1
/// fault the caller treats as fail-OPEN for an UNKNOWN tenant (the
/// [`CachedSuspendResolver`] folds a KNOWN-suspended read-error into `Ok(true)`
/// so it stays denied — see the module doc).
#[async_trait]
pub trait SuspendResolver: std::fmt::Debug + Send + Sync {
    /// `Ok(true)` when `tenant_id`'s offboarding state ∈ {suspended, erased}.
    async fn suspended_state(&self, tenant_id: &str) -> Result<bool, String>;
}

/// The two offboarding states that DENY on the OCI plane. Earlier phases
/// (`cancel_requested` / `grace_period` / `read_only`) intentionally do NOT deny.
fn state_denies(state: &str) -> bool {
    matches!(state, "suspended" | "erased")
}

/// Production [`SuspendResolver`] over the `tenant_offboarding_state` D1 table
/// (migration 0046), reached via [`D1HttpClient`].
///
/// The query mirrors [`crate::oci_cap::D1TenantCapResolver`]'s tier read: a single
/// parameterised positional lookup. An ACTIVE tenant has either no row or a row
/// whose state is not terminal, so a missing row ⇒ NOT suspended (`Ok(false)`).
/// A transport/decode error ⇒ `Err` (the caller / cache applies the fail-closed
/// policy).
#[derive(Debug)]
pub struct D1SuspendResolver {
    client: Arc<D1HttpClient>,
}

impl D1SuspendResolver {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SuspendResolver for D1SuspendResolver {
    async fn suspended_state(&self, tenant_id: &str) -> Result<bool, String> {
        let rows = self
            .client
            .query(
                "SELECT state FROM tenant_offboarding_state WHERE tenant_id = ?1 LIMIT 1",
                &[serde_json::Value::String(tenant_id.to_owned())],
            )
            .await?;
        // No row ⇒ the tenant has never offboarded ⇒ NOT suspended.
        let Some(row) = rows.into_iter().next() else {
            return Ok(false);
        };
        // A row with a non-string / missing `state` is a backend fault — treat as
        // an error so the fail-closed policy (not a silent allow) applies.
        let state = row
            .get("state")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                "D1 tenant_offboarding_state: missing or non-string `state`".to_owned()
            })?;
        Ok(state_denies(state))
    }
}

/// Default TTL for a cached suspend decision, in milliseconds. Mirrors
/// [`crate::request_count::DEFAULT_TIER_CACHE_TTL_MS`] (30 s): amortises a cold
/// burst's per-op reads while keeping a fresh suspension near-immediate — and the
/// `/v2` residual gate bounds a mid-session suspend to at most one TTL on the
/// warm path (the bearer TTL is a much larger 300 s, so the cache never widens
/// the leak window).
pub const DEFAULT_SUSPEND_CACHE_TTL_MS: i64 = 30_000;

/// Upper bound on distinct tenants held in the cache / single-flight map, so
/// neither grows without bound under tenant churn. Mirrors
/// [`crate::request_count`]'s `TIER_CACHE_CAP`.
const SUSPEND_CACHE_CAP: usize = 50_000;

/// One cached suspend decision: the verdict plus when it was fetched.
#[derive(Debug, Clone)]
struct CachedSuspend {
    suspended: bool,
    fetched_at_ms: i64,
}

/// A single-flight + short-TTL cache in front of an inner [`SuspendResolver`],
/// mirroring [`crate::request_count::CachedTierResolver`] — plus the fail-CLOSED
/// sticky-deny twist (a KNOWN-suspended tenant stays denied through a read error).
///
/// The OCI plane resolves the suspend verdict on EVERY token mint AND every `/v2`
/// op; against [`D1SuspendResolver`] that is one D1-over-HTTP read per op. A cold
/// parallel burst would thunder the herd onto D1 (the 2026-07-08 668 MB cold
/// hydrate is the reference incident). This wrapper collapses that: a warm tenant
/// resolves in memory, and a cold burst COALESCES into ONE inner read via a
/// per-tenant single-flight lock.
#[derive(Debug)]
pub struct CachedSuspendResolver {
    inner: Arc<dyn SuspendResolver>,
    clock: Arc<dyn WallClock>,
    ttl_ms: i64,
    /// `tenant -> (suspended, fetched_at_ms)`. Sync mutex; no `.await` across it.
    cache: Mutex<HashMap<String, CachedSuspend>>,
    /// `tenant -> per-tenant single-flight lock`. The async mutex is held across
    /// the inner read so a cold burst for one tenant does exactly one D1 read.
    inflight: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl CachedSuspendResolver {
    /// Wrap `inner` with a `ttl_ms`-TTL single-flight cache.
    #[must_use]
    pub fn new(inner: Arc<dyn SuspendResolver>, clock: Arc<dyn WallClock>, ttl_ms: i64) -> Self {
        Self {
            inner,
            clock,
            ttl_ms,
            cache: Mutex::new(HashMap::new()),
            inflight: AsyncMutex::new(HashMap::new()),
        }
    }

    /// The tenant's cached decision if present AND fresh at `now_ms`, else `None`.
    fn cached_fresh(&self, tenant_id: &str, now_ms: i64) -> Option<bool> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(tenant_id)?;
        (now_ms.saturating_sub(entry.fetched_at_ms) < self.ttl_ms).then_some(entry.suspended)
    }

    /// Whether the tenant is KNOWN suspended from ANY cache entry (fresh OR stale)
    /// — the sticky fail-closed signal used when the inner read errors.
    fn known_suspended(&self, tenant_id: &str) -> bool {
        self.cache
            .lock()
            .ok()
            .and_then(|c| c.get(tenant_id).map(|e| e.suspended))
            .unwrap_or(false)
    }

    /// Store a freshly-resolved decision, evicting (expired-then-oldest) at the cap.
    fn store(&self, tenant_id: &str, suspended: bool, now_ms: i64) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        if cache.len() >= SUSPEND_CACHE_CAP && !cache.contains_key(tenant_id) {
            cache.retain(|_, e| now_ms.saturating_sub(e.fetched_at_ms) < self.ttl_ms);
            if cache.len() >= SUSPEND_CACHE_CAP {
                if let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, e)| e.fetched_at_ms)
                    .map(|(k, _)| k.clone())
                {
                    let _ = cache.remove(&oldest);
                }
            }
        }
        let _ = cache.insert(
            tenant_id.to_owned(),
            CachedSuspend {
                suspended,
                fetched_at_ms: now_ms,
            },
        );
    }

    /// The per-tenant single-flight lock, created on demand (bounded).
    async fn inflight_lock(&self, tenant_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.inflight.lock().await;
        if map.len() >= SUSPEND_CACHE_CAP && !map.contains_key(tenant_id) {
            // Drop only UNCONTENDED locks (no waiter besides the map's own Arc).
            map.retain(|_, l| Arc::strong_count(l) > 1);
        }
        Arc::clone(
            map.entry(tenant_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }
}

#[async_trait]
impl SuspendResolver for CachedSuspendResolver {
    async fn suspended_state(&self, tenant_id: &str) -> Result<bool, String> {
        let now_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        // Warm fast path — fresh cache entry, zero D1.
        if let Some(suspended) = self.cached_fresh(tenant_id, now_ms) {
            return Ok(suspended);
        }
        // Cold: serialise per tenant so a burst does ONE inner read, not N.
        let lock = self.inflight_lock(tenant_id).await;
        let _guard = lock.lock().await;
        // Re-read the clock and re-check: a peer holding the lock just before us
        // may already have warmed the entry.
        let now_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        if let Some(suspended) = self.cached_fresh(tenant_id, now_ms) {
            return Ok(suspended);
        }
        // Still cold — do the ONE inner read.
        match self.inner.suspended_state(tenant_id).await {
            Ok(suspended) => {
                self.store(tenant_id, suspended, now_ms);
                Ok(suspended)
            }
            // Fail-CLOSED sticky: a tenant we ALREADY know suspended (cached, even
            // stale) STAYS denied through a transient read fault. The error is
            // NEVER cached, so a later successful read can flip it back to allowed.
            // An UNKNOWN tenant's error propagates → the caller fails OPEN.
            Err(e) => {
                if self.known_suspended(tenant_id) {
                    Ok(true)
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::wall_clock::InMemoryFakeWallClock;

    const T0: u64 = 1_781_481_600_000; // 2026-06-15T00:00:00Z

    #[test]
    fn only_terminal_states_deny() {
        assert!(state_denies("suspended"));
        assert!(state_denies("erased"));
        // Earlier offboarding phases keep OCI access.
        assert!(!state_denies("active"));
        assert!(!state_denies("cancel_requested"));
        assert!(!state_denies("grace_period"));
        assert!(!state_denies("read_only"));
        assert!(!state_denies("bogus"));
    }

    /// A resolver returning a fixed verdict, counting inner calls.
    #[derive(Debug)]
    struct FixedSuspend {
        suspended: bool,
        calls: Arc<AtomicUsize>,
        delay_ms: u64,
    }
    #[async_trait]
    impl SuspendResolver for FixedSuspend {
        async fn suspended_state(&self, _tenant_id: &str) -> Result<bool, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            Ok(self.suspended)
        }
    }

    /// A resolver whose verdict is swappable at runtime (drives mid-session flip)
    /// and can be forced to error.
    #[derive(Debug)]
    struct SwitchableSuspend {
        suspended: std::sync::atomic::AtomicBool,
        error: std::sync::atomic::AtomicBool,
        calls: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl SuspendResolver for SwitchableSuspend {
        async fn suspended_state(&self, _tenant_id: &str) -> Result<bool, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.error.load(Ordering::SeqCst) {
                return Err("d1 down".to_owned());
            }
            Ok(self.suspended.load(Ordering::SeqCst))
        }
    }

    fn cached(
        inner: Arc<dyn SuspendResolver>,
        clock: &Arc<InMemoryFakeWallClock>,
    ) -> CachedSuspendResolver {
        CachedSuspendResolver::new(
            inner,
            Arc::clone(clock) as Arc<dyn WallClock>,
            DEFAULT_SUSPEND_CACHE_TTL_MS,
        )
    }

    #[tokio::test]
    async fn active_tenant_is_allowed() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(FixedSuspend {
                suspended: false,
                calls: Arc::clone(&calls),
                delay_ms: 0,
            }),
            &clock,
        );
        assert!(!r.suspended_state("t1").await.unwrap());
    }

    #[tokio::test]
    async fn suspended_tenant_is_denied() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(FixedSuspend {
                suspended: true,
                calls: Arc::clone(&calls),
                delay_ms: 0,
            }),
            &clock,
        );
        assert!(r.suspended_state("t1").await.unwrap());
    }

    #[tokio::test]
    async fn warm_hit_serves_from_memory() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(FixedSuspend {
                suspended: false,
                calls: Arc::clone(&calls),
                delay_ms: 0,
            }),
            &clock,
        );
        let _ = r.suspended_state("t1").await.unwrap();
        let _ = r.suspended_state("t1").await.unwrap();
        let _ = r.suspended_state("t1").await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "warm ops must not re-read D1"
        );
    }

    #[tokio::test]
    async fn cold_parallel_burst_coalesces_to_one_read() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = Arc::new(cached(
            Arc::new(FixedSuspend {
                suspended: true,
                calls: Arc::clone(&calls),
                delay_ms: 30,
            }),
            &clock,
        ));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..24 {
            let rr = Arc::clone(&r);
            set.spawn(async move { rr.suspended_state("hot").await.unwrap() });
        }
        while let Some(res) = set.join_next().await {
            assert!(res.unwrap());
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "a cold burst must coalesce into ONE read"
        );
    }

    #[tokio::test]
    async fn ttl_expiry_re_resolves_and_flips() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let inner = Arc::new(SwitchableSuspend {
            suspended: std::sync::atomic::AtomicBool::new(false),
            error: std::sync::atomic::AtomicBool::new(false),
            calls: Arc::clone(&calls),
        });
        let r = cached(Arc::clone(&inner) as Arc<dyn SuspendResolver>, &clock);
        assert!(!r.suspended_state("t1").await.unwrap());
        // Suspend mid-session; still within TTL → serves the STALE `active` verdict.
        inner.suspended.store(true, Ordering::SeqCst);
        clock.advance(Duration::from_millis(
            u64::try_from(DEFAULT_SUSPEND_CACHE_TTL_MS).unwrap() - 1,
        ));
        assert!(
            !r.suspended_state("t1").await.unwrap(),
            "pre-TTL op still cached"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Cross the TTL → re-read picks up the suspension.
        clock.advance(Duration::from_millis(2));
        assert!(
            r.suspended_state("t1").await.unwrap(),
            "post-TTL op re-reads → denied"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn known_suspended_read_error_stays_denied_fail_closed() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let inner = Arc::new(SwitchableSuspend {
            suspended: std::sync::atomic::AtomicBool::new(true),
            error: std::sync::atomic::AtomicBool::new(false),
            calls: Arc::clone(&calls),
        });
        let r = cached(Arc::clone(&inner) as Arc<dyn SuspendResolver>, &clock);
        // First op: confirm suspended (caches the verdict).
        assert!(r.suspended_state("t1").await.unwrap());
        // Now D1 errors AND the TTL has expired → the sticky verdict keeps denying.
        inner.error.store(true, Ordering::SeqCst);
        clock.advance(Duration::from_millis(
            u64::try_from(DEFAULT_SUSPEND_CACHE_TTL_MS).unwrap() + 1,
        ));
        assert!(
            r.suspended_state("t1").await.unwrap(),
            "a known-suspended tenant STAYS denied through a read error (fail-closed)"
        );
    }

    #[tokio::test]
    async fn unknown_tenant_read_error_fails_open() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let inner = Arc::new(SwitchableSuspend {
            suspended: std::sync::atomic::AtomicBool::new(false),
            error: std::sync::atomic::AtomicBool::new(true),
            calls: Arc::clone(&calls),
        });
        let r = cached(Arc::clone(&inner) as Arc<dyn SuspendResolver>, &clock);
        // Never seen this tenant + D1 errors → Err (caller fails OPEN); nothing cached.
        assert!(r.suspended_state("t-unknown").await.is_err());
        assert!(r.suspended_state("t-unknown").await.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "errors are NOT cached → each op retries"
        );
    }
}

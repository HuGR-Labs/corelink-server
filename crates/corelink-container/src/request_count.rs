//! Per-tenant monthly **request-count** middleware primitive
//! (rt-nuclear #8 — the request-count half).
//!
//! # Why — `request count` is not `$` and is not `rate`
//!
//! CoreLink enforces THREE orthogonal abuse axes, each with its own backing:
//!
//! - `ratelimit_buckets` (token bucket) bounds **velocity** (req/s + burst),
//!   enforced in-app by [`crate::routes::ratelimit_layer`];
//! - `tenant_quota` (migration 0066, [`crate::tenant_quota`]) bounds the
//!   cumulative **dollar** spend per cycle (ADR-0068);
//! - the `monthly_request_counts` table (migration 0071) bounds the cumulative
//!   **request count** per calendar month — the signed rate-card
//!   `requestsPerMonthMax` axis (free 500K … max 80M; team/enterprise
//!   uncapped). A slow-but-steady tenant stays under the per-second limit AND
//!   under the $-tripwire yet could still blow past the contracted monthly
//!   request allowance.
//!
//! # Why this lives in the container (the rt-nuclear #8 gap)
//!
//! The monthly request-count cap is normally enforced at the **Worker edge**
//! (`worker/src/lib/quota.ts::checkRequestQuota`). The OCI surface, however, is
//! a two-leg pass-through: the Worker forwards `/v2/*` + `/token` RAW and
//! returns BEFORE its quota block (it also strips `x-corelink-tenant-id`), so
//! OCI billable writes were never counted against the monthly request cap — the
//! exact sibling of the $-ceiling bypass that PR #318 closed for the dollar
//! axis. This module is the container-side mirror of `checkRequestQuota`,
//! wired into the OCI router and keyed on the SAME verified-HMAC-bearer tenant
//! that #318's `oci_quota_gate` already resolves (never a request header).
//!
//! # Fail-OPEN (deliberately — unlike the $-ceiling)
//!
//! The request-count cap is an SLO-style allowance limiter, NOT a cost cap, so
//! its uncertainty posture is fail-**OPEN**, matching `checkRequestQuota`
//! byte-for-byte:
//!
//! - store transport / decode error → allow (availability; the request is not
//!   counted);
//! - tier unconfirmed (a tier-resolution D1 fault) → allow, no count (we cannot
//!   know which cap applies; counting against a fallback `free` cap would
//!   false-positive a paid tenant during a partial outage);
//! - uncapped tier (team / enterprise) → allow, and skip the counter write
//!   entirely (nothing to enforce, no reason to pay a D1 write);
//! - over the cap → **429 Too Many Requests** + `Retry-After` =
//!   [`seconds_until_next_month_start`].
//!
//! This is the OPPOSITE of [`crate::tenant_quota`]'s fail-CLOSED `$`-ceiling —
//! the trade-off is intentional and matches the Worker per-axis.
//!
//! # Wiring (the seam the OCI router registers)
//!
//! Production builds a [`RequestCountGate`] from process env
//! ([`RequestCountGate::from_env`]); it is `None` in dev/CI (no D1 storage env),
//! mirroring [`crate::routes::QuotaGate::from_env`]. The OCI router stashes it
//! and calls [`RequestCountGate::check_and_increment`] on each write method,
//! keyed on the verified-bearer tenant.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::Mutex as AsyncMutex;

use crate::wall_clock::WallClock;

/// The free-tier monthly request cap (the floor). Mirrors
/// `worker/src/lib/quota.ts` `QUOTAS.free.requestsPerMonthMax`.
pub const CAP_FREE: i64 = 500_000;
/// Solo-tier monthly request cap. Mirrors `QUOTAS.solo`.
pub const CAP_SOLO: i64 = 2_000_000;
/// Starter-tier monthly request cap. Mirrors `QUOTAS.starter`.
pub const CAP_STARTER: i64 = 6_000_000;
/// Pro/org-tier monthly request cap. Mirrors `QUOTAS.pro` / `QUOTAS.org`.
pub const CAP_PRO: i64 = 20_000_000;
/// Max-tier monthly request cap. Mirrors `QUOTAS.max`.
pub const CAP_MAX: i64 = 80_000_000;

/// Resolve the monthly request cap for a tier slug, mirroring the Worker's
/// `QUOTAS[tier].requestsPerMonthMax`.
///
/// `None` ⇒ the tier is **uncapped** (team / enterprise → `MAX_SAFE_INTEGER`
/// in the Worker): the caller skips the counter write entirely. An unknown
/// slug maps to the most-restrictive `free` cap (defence-in-depth — the tier
/// strings come from [`crate::routes::auth_introspect::tier_for_tenant`], which
/// already filters to the canonical set, so this branch is unreachable in
/// practice but stays fail-safe).
#[must_use]
pub fn cap_for_tier(tier: &str) -> Option<i64> {
    match tier {
        "solo" => Some(CAP_SOLO),
        "starter" => Some(CAP_STARTER),
        "pro" | "org" => Some(CAP_PRO),
        "max" => Some(CAP_MAX),
        // team / enterprise are uncapped (MAX_SAFE_INTEGER in the Worker).
        "team" | "enterprise" => None,
        // free + any unknown slug → the floor.
        _ => Some(CAP_FREE),
    }
}

/// The UTC calendar month as a fixed `YYYY-MM` string (e.g. `2026-06`), the
/// `monthly_request_counts.year_month` bucket key. Derived from a Unix-epoch-ms
/// wall clock so the boundary matches the Worker's
/// `new Date().toISOString().slice(0, 7)` and
/// [`seconds_until_next_month_start`].
#[must_use]
pub fn year_month_utc(now_ms: i64) -> String {
    let (year, month) = year_month_parts(now_ms);
    format!("{year:04}-{month:02}")
}

/// Seconds from `now_ms` until the first instant of the next UTC calendar
/// month — the `Retry-After` for a 429, mirroring the Worker's
/// `secondsUntilNextMonthStart()`.
#[must_use]
pub fn seconds_until_next_month_start(now_ms: i64) -> i64 {
    let (year, month) = year_month_parts(now_ms);
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next_start_ms = days_from_civil(next_year, next_month, 1) * 86_400_000;
    next_start_ms.saturating_sub(now_ms).max(0) / 1000
}

/// Split a Unix-epoch-ms instant into its UTC `(year, month)` components.
fn year_month_parts(now_ms: i64) -> (i64, u32) {
    let days = now_ms.div_euclid(86_400_000);
    civil_from_days(days)
}

/// Days since the Unix epoch for a civil `(y, m, d)` date — Howard Hinnant's
/// branchless `days_from_civil` (proleptic Gregorian, day 1).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`] returning only `(year, month)` — Hinnant's
/// `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32)
}

/// Backing store for the per-tenant per-month request counter.
///
/// One method — the atomic increment-and-check — because, unlike the
/// $-ceiling, there is no roll/seed/absolute-write shape to drive: a new
/// calendar month is implicitly a fresh row (the `year_month` key changes), so
/// a single `INSERT … ON CONFLICT … DO UPDATE … RETURNING` covers every case.
#[async_trait::async_trait]
pub trait RequestCountStore: std::fmt::Debug + Send + Sync {
    /// Atomically increment the `(tenant_id, year_month)` counter by one and
    /// return the POST-increment count. Seeds a fresh row at `1` when none
    /// exists. `Err` ⇒ transport / decode failure (the gate fail-OPENs).
    ///
    /// Mirrors `checkRequestQuota`'s single atomic UPSERT:
    ///
    /// ```sql
    /// INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms)
    /// VALUES (?1, ?2, 1, ?3)
    /// ON CONFLICT(tenant_id, year_month)
    /// DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3
    /// RETURNING request_count;
    /// ```
    async fn increment(
        &self,
        tenant_id: &str,
        year_month: &str,
        now_ms: i64,
    ) -> Result<i64, String>;
}

/// Resolves a tenant's effective tier (the cap selector). The production impl
/// wraps [`crate::routes::auth_introspect::tier_for_tenant`] over D1; tests
/// inject a fixed-tier fake.
#[async_trait::async_trait]
pub trait TierResolver: std::fmt::Debug + Send + Sync {
    /// Resolve the tenant's tier slug. `Err` ⇒ a genuine D1 fault; the gate
    /// fail-OPENs (cannot know which cap applies — F21 symmetry).
    async fn tier(&self, tenant_id: &str) -> Result<String, String>;
}

/// The per-tenant monthly request-count gate.
///
/// Cheap to clone (two `Arc`s + clock) so it drops into the OCI router state
/// alongside the $-ceiling [`crate::routes::QuotaGate`].
#[derive(Clone, Debug)]
pub struct RequestCountGate {
    store: Arc<dyn RequestCountStore>,
    tiers: Arc<dyn TierResolver>,
    clock: Arc<dyn WallClock>,
}

impl RequestCountGate {
    /// Construct a gate from its collaborators.
    #[must_use]
    pub fn new(
        store: Arc<dyn RequestCountStore>,
        tiers: Arc<dyn TierResolver>,
        clock: Arc<dyn WallClock>,
    ) -> Self {
        Self {
            store,
            tiers,
            clock,
        }
    }

    /// Build the production gate from process env (the same
    /// [`crate::storage::StorageEnv`] the D1 adapters use), or `None` in dev/CI
    /// (no D1 storage env) — the request-count cap is then not enforced,
    /// mirroring [`crate::routes::QuotaGate::from_env`].
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let client = Arc::new(
            crate::storage::d1_http::D1HttpClient::new(&storage_env)
                .map_err(|e| tracing::warn!(error = %e, "request-count: D1 client init failed"))
                .ok()?,
        );
        let store: Arc<dyn RequestCountStore> =
            Arc::new(D1RequestCountStore::new(Arc::clone(&client)));
        let clock: Arc<dyn WallClock> = Arc::new(crate::wall_clock::SystemWallClock::new());
        // Front the per-op D1 tier read (`tenant.tier` + `tier_selections`) with a
        // single-flight + short-TTL cache: a warm tenant resolves in memory, and a
        // cold PARALLEL burst coalesces into ONE inner resolve instead of the
        // thundering herd that D1-saturated introspect/billing during the
        // 2026-07-08 668 MB cold hydrate. See [`CachedTierResolver`].
        let tiers: Arc<dyn TierResolver> = Arc::new(CachedTierResolver::new(
            Arc::new(D1TierResolver::new(client)),
            Arc::clone(&clock),
            DEFAULT_TIER_CACHE_TTL_MS,
        ));
        Some(Self::new(store, tiers, clock))
    }

    /// Count ONE billable op against `tenant`'s monthly request cap and
    /// enforce it. `Some(resp)` ⇒ REJECT (**429** over cap); `None` ⇒ proceed.
    ///
    /// Fail-OPEN on every uncertain path (store error, tier-resolution fault,
    /// uncapped tier, unavailable clock) — see the module doc; this matches
    /// `checkRequestQuota`'s posture exactly. Uncapped tiers and the
    /// no-clock path skip the counter write.
    pub async fn check_and_increment(&self, tenant: &str) -> Option<Response> {
        let now_ms_u64 = self.clock.now_ms();
        if now_ms_u64 == 0 {
            // No trustworthy clock → fail-OPEN (this is an availability
            // limiter, not a cost cap); skip the count rather than 429.
            return None;
        }
        let now_ms = i64::try_from(now_ms_u64).unwrap_or(i64::MAX);

        // Resolve the cap from the tenant's tier. A tier-resolution fault →
        // fail-OPEN, no count (we cannot know which cap applies — F21).
        let tier = match self.tiers.tier(tenant).await {
            Ok(t) => t,
            Err(_) => return None,
        };
        // Uncapped tier (team / enterprise) → nothing to enforce; skip the
        // write (no reason to pay a D1 round-trip).
        let cap = cap_for_tier(&tier)?;

        // Atomic increment-and-check. A store error → fail-OPEN (the op is not
        // counted; the data plane remains the deeper net).
        let year_month = year_month_utc(now_ms);
        let count = match self.store.increment(tenant, &year_month, now_ms).await {
            Ok(c) => c,
            Err(_) => return None,
        };

        // The cap is a maximum allowance: reject only requests BEYOND it
        // (count > cap). The request whose increment lands exactly ON the cap
        // is still served — identical to `checkRequestQuota`.
        if count <= cap {
            return None;
        }

        let retry_after = seconds_until_next_month_start(now_ms);
        Some(
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
                "monthly request quota exceeded; wait for the cycle to reset",
            )
                .into_response(),
        )
    }
}

/// Production [`RequestCountStore`] over the `monthly_request_counts` D1 table
/// (migration 0071), reached via [`crate::storage::d1_http::D1HttpClient`].
///
/// The SQL is identical to the Worker's `checkRequestQuota` UPSERT so both
/// surfaces increment the SAME counter (an OCI write and a Worker-edge native
/// write both count against the one monthly allowance). Parameterised positional
/// binds; the tenant scope rides in the conflict key (INV-TENANT-ISOLATION).
#[derive(Debug)]
pub struct D1RequestCountStore {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1RequestCountStore {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl RequestCountStore for D1RequestCountStore {
    async fn increment(
        &self,
        tenant_id: &str,
        year_month: &str,
        now_ms: i64,
    ) -> Result<i64, String> {
        let rows = self
            .client
            .query(
                "INSERT INTO monthly_request_counts \
                     (tenant_id, year_month, request_count, updated_at_ms) \
                 VALUES (?1, ?2, 1, ?3) \
                 ON CONFLICT(tenant_id, year_month) \
                 DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3 \
                 RETURNING request_count",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::String(year_month.to_owned()),
                    serde_json::Value::from(now_ms),
                ],
            )
            .await?;
        // A `RETURNING` UPSERT always yields exactly one row.
        rows.into_iter()
            .next()
            .and_then(|r| r.get("request_count").and_then(serde_json::Value::as_i64))
            .ok_or_else(|| {
                "D1 monthly_request_counts: missing or non-integer `request_count`".to_owned()
            })
    }
}

/// Production [`TierResolver`] over D1, delegating to the existing
/// `getTierForTenant` Rust mirror.
#[derive(Debug)]
pub struct D1TierResolver {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1TierResolver {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl TierResolver for D1TierResolver {
    async fn tier(&self, tenant_id: &str) -> Result<String, String> {
        crate::routes::auth_introspect::tier_for_tenant(&self.client, tenant_id).await
    }
}

/// Default TTL for a cached tier entry, in milliseconds.
///
/// The tier is the cap SELECTOR only — the durable `monthly_request_counts`
/// table stays the sole count authority — so a bounded `≤ TTL` staleness on the
/// selector is an accepted approximation (a just-upgraded/downgraded tenant sees
/// its new cap within one TTL). `30 s` amortises a cold burst's per-op tier
/// reads while keeping a cap change near-immediate; it mirrors the bounded
/// staleness the `$`-ceiling lease already accepts ([`crate::tenant_quota`]).
pub const DEFAULT_TIER_CACHE_TTL_MS: i64 = 30_000;

/// Upper bound on distinct tenants held in the tier cache / single-flight map,
/// so neither grows without bound under tenant churn (~64 B per entry → MB-scale
/// at the cap). Over the cap, expired entries are pruned first, then the oldest.
const TIER_CACHE_CAP: usize = 50_000;

/// One cached tier decision: the resolved slug plus when it was fetched.
#[derive(Debug, Clone)]
struct CachedTier {
    tier: String,
    fetched_at_ms: i64,
}

/// A single-flight + short-TTL cache in front of an inner [`TierResolver`].
///
/// `RequestCountGate::check_and_increment` resolves the tenant's tier on EVERY
/// billable op; against [`D1TierResolver`] that is two synchronous D1-over-HTTP
/// reads (`tenant.tier` + `tier_selections`) per op. A **cold PARALLEL burst**
/// — the 2026-07-08 668 MB cold hydrate, where ~135 of the D1 read-queries that
/// saturated introspect/billing were exactly these per-op tier reads — has every
/// concurrent op miss at once and thunder the herd onto D1.
///
/// This wrapper collapses that: a warm tenant resolves in memory (zero D1), and
/// a cold burst is COALESCED into ONE inner resolve via a per-tenant
/// single-flight lock (the rest wait, then read the now-warm entry).
///
/// **Correctness.** The tier only SELECTS which cap applies; the durable counter
/// remains the sole authority, so a `≤ TTL`-stale selector is a bounded, accepted
/// approximation (like the `$`-ceiling lease). The **fail-OPEN** posture is
/// preserved unchanged: an inner `Err` is returned to the caller (which fail-OPENs
/// per F21) and is NEVER cached, so a transient D1 fault self-heals on the next op.
#[derive(Debug)]
pub struct CachedTierResolver {
    inner: Arc<dyn TierResolver>,
    clock: Arc<dyn WallClock>,
    ttl_ms: i64,
    /// `tenant -> (tier, fetched_at_ms)`. Sync mutex; no `.await` is held across it.
    cache: Mutex<HashMap<String, CachedTier>>,
    /// `tenant -> per-tenant single-flight lock`. The async mutex is held across
    /// the inner resolve so concurrent cold ops for one tenant do exactly one D1 read.
    inflight: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl CachedTierResolver {
    /// Wrap `inner` with a `ttl_ms`-TTL single-flight cache.
    #[must_use]
    pub fn new(inner: Arc<dyn TierResolver>, clock: Arc<dyn WallClock>, ttl_ms: i64) -> Self {
        Self {
            inner,
            clock,
            ttl_ms,
            cache: Mutex::new(HashMap::new()),
            inflight: AsyncMutex::new(HashMap::new()),
        }
    }

    /// The tenant's cached tier if present AND fresh at `now_ms`, else `None`.
    fn cached_fresh(&self, tenant_id: &str, now_ms: i64) -> Option<String> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(tenant_id)?;
        (now_ms.saturating_sub(entry.fetched_at_ms) < self.ttl_ms).then(|| entry.tier.clone())
    }

    /// Store a freshly-resolved tier, evicting (expired-then-oldest) at the cap.
    fn store(&self, tenant_id: &str, tier: String, now_ms: i64) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        if cache.len() >= TIER_CACHE_CAP && !cache.contains_key(tenant_id) {
            cache.retain(|_, e| now_ms.saturating_sub(e.fetched_at_ms) < self.ttl_ms);
            if cache.len() >= TIER_CACHE_CAP {
                if let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, e)| e.fetched_at_ms)
                    .map(|(k, _)| k.clone())
                {
                    let _ = cache.remove(&oldest);
                }
            }
        }
        let _ = cache.insert(tenant_id.to_owned(), CachedTier { tier, fetched_at_ms: now_ms });
    }

    /// The per-tenant single-flight lock, created on demand (bounded).
    async fn inflight_lock(&self, tenant_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.inflight.lock().await;
        if map.len() >= TIER_CACHE_CAP && !map.contains_key(tenant_id) {
            // Drop only UNCONTENDED locks (no waiter besides the map's own Arc).
            map.retain(|_, l| Arc::strong_count(l) > 1);
        }
        Arc::clone(
            map.entry(tenant_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }
}

#[async_trait::async_trait]
impl TierResolver for CachedTierResolver {
    async fn tier(&self, tenant_id: &str) -> Result<String, String> {
        let now_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        // Warm fast path — fresh cache entry, zero D1.
        if let Some(tier) = self.cached_fresh(tenant_id, now_ms) {
            return Ok(tier);
        }
        // Cold: serialise per tenant so a burst does ONE inner resolve, not N.
        let lock = self.inflight_lock(tenant_id).await;
        let _guard = lock.lock().await;
        // Re-read the clock and re-check: a peer holding the lock just before us
        // may already have warmed the entry.
        let now_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        if let Some(tier) = self.cached_fresh(tenant_id, now_ms) {
            return Ok(tier);
        }
        // Still cold — do the ONE inner resolve. Fail-OPEN: propagate any `Err`
        // and never cache it, so a transient D1 fault self-heals next op.
        let tier = self.inner.tier(tenant_id).await?;
        self.store(tenant_id, tier.clone(), now_ms);
        Ok(tier)
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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::wall_clock::InMemoryFakeWallClock;

    /// 2026-06-15T00:00:00Z, in Unix-epoch ms. (1_750_000_000_000 — the value
    /// this constant carried before — is actually 2025-06-14T22:13:20Z, NOT a
    /// 2026-06 midnight; it made the fixed-clock tests compute year_month
    /// "2025-06" and miss every "2026-06"-keyed pre-load. Cross-check:
    /// 2026-06-16T00:00:00Z = 1_781_568_000_000 (the tier-seed epoch), so
    /// 2026-06-15T00:00:00Z = that minus one day.)
    const T0: u64 = 1_781_481_600_000;

    /// In-memory counter keyed by `(tenant, year_month)`.
    #[derive(Debug, Default)]
    struct FakeCounter(Mutex<HashMap<(String, String), i64>>);
    #[async_trait::async_trait]
    impl RequestCountStore for FakeCounter {
        async fn increment(
            &self,
            tenant_id: &str,
            year_month: &str,
            _now_ms: i64,
        ) -> Result<i64, String> {
            let mut m = self.0.lock().unwrap();
            let c = m
                .entry((tenant_id.to_owned(), year_month.to_owned()))
                .or_insert(0);
            *c += 1;
            Ok(*c)
        }
    }

    /// A store that always errors (drives the fail-OPEN path).
    #[derive(Debug)]
    struct ErroringCounter;
    #[async_trait::async_trait]
    impl RequestCountStore for ErroringCounter {
        async fn increment(&self, _t: &str, _ym: &str, _n: i64) -> Result<i64, String> {
            Err("boom".to_owned())
        }
    }

    /// Fixed-tier resolver.
    #[derive(Debug)]
    struct FixedTier(&'static str);
    #[async_trait::async_trait]
    impl TierResolver for FixedTier {
        async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
            Ok(self.0.to_owned())
        }
    }

    /// A tier resolver that always errors (drives the fail-OPEN path).
    #[derive(Debug)]
    struct ErroringTier;
    #[async_trait::async_trait]
    impl TierResolver for ErroringTier {
        async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
            Err("d1 down".to_owned())
        }
    }

    fn gate(store: Arc<dyn RequestCountStore>, tier: &'static str) -> RequestCountGate {
        RequestCountGate::new(
            store,
            Arc::new(FixedTier(tier)),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(T0)),
        )
    }

    #[test]
    fn cap_table_matches_worker_quotas() {
        assert_eq!(cap_for_tier("free"), Some(CAP_FREE));
        assert_eq!(cap_for_tier("solo"), Some(CAP_SOLO));
        assert_eq!(cap_for_tier("starter"), Some(CAP_STARTER));
        assert_eq!(cap_for_tier("pro"), Some(CAP_PRO));
        assert_eq!(cap_for_tier("org"), Some(CAP_PRO));
        assert_eq!(cap_for_tier("max"), Some(CAP_MAX));
        // Uncapped tiers skip the counter.
        assert_eq!(cap_for_tier("team"), None);
        assert_eq!(cap_for_tier("enterprise"), None);
        // Unknown → most-restrictive floor.
        assert_eq!(cap_for_tier("bogus"), Some(CAP_FREE));
    }

    #[test]
    fn year_month_and_retry_after_are_utc_calendar_aligned() {
        // 2026-06-15T00:00:00Z.
        assert_eq!(year_month_utc(T0 as i64), "2026-06");
        // Next month start = 2026-07-01T00:00:00Z; remaining ≈ 16 days.
        let secs = seconds_until_next_month_start(T0 as i64);
        assert_eq!(secs, 16 * 86_400);
    }

    #[tokio::test]
    async fn under_cap_allows_and_counts() {
        let store = Arc::new(FakeCounter::default());
        let g = gate(store, "free");
        // First op against a fresh tenant — allowed.
        assert!(g.check_and_increment("tenant-a").await.is_none());
        assert!(g.check_and_increment("tenant-a").await.is_none());
    }

    #[tokio::test]
    async fn over_cap_rejects_429_with_retry_after() {
        // Pre-load the counter to exactly the free cap so the next op is the
        // (cap+1)-th — the request that crosses the line → 429.
        let store = Arc::new(FakeCounter::default());
        {
            let mut m = store.0.lock().unwrap();
            m.insert(("tenant-b".to_owned(), "2026-06".to_owned()), CAP_FREE);
        }
        let g = gate(store, "free");
        let resp = g
            .check_and_increment("tenant-b")
            .await
            .expect("over-cap op must be rejected");
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().contains_key(axum::http::header::RETRY_AFTER));
    }

    #[tokio::test]
    async fn the_op_landing_exactly_on_the_cap_is_served() {
        // count == cap is within allowance (count <= cap); only count > cap rejects.
        let store = Arc::new(FakeCounter::default());
        {
            let mut m = store.0.lock().unwrap();
            m.insert(("tenant-c".to_owned(), "2026-06".to_owned()), CAP_FREE - 1);
        }
        let g = gate(store, "free");
        // This increment lands the count exactly on the cap → still served.
        assert!(g.check_and_increment("tenant-c").await.is_none());
    }

    #[tokio::test]
    async fn uncapped_tier_skips_counter_and_allows() {
        // An enterprise tenant is uncapped: never 429, and the store is never
        // touched (use the erroring store to prove the write is skipped).
        let g = gate(Arc::new(ErroringCounter), "enterprise");
        assert!(g.check_and_increment("tenant-ent").await.is_none());
    }

    #[tokio::test]
    async fn store_error_fails_open() {
        let g = gate(Arc::new(ErroringCounter), "free");
        // A capped tier whose counter store errors → fail-OPEN (allow), the
        // op is uncounted (availability over enforcement on this axis).
        assert!(g.check_and_increment("tenant-d").await.is_none());
    }

    #[tokio::test]
    async fn tier_resolution_error_fails_open() {
        // A tier-resolution D1 fault → fail-OPEN, no count (F21 symmetry: we
        // cannot know which cap applies). Erroring store proves no write runs.
        let g = RequestCountGate::new(
            Arc::new(ErroringCounter),
            Arc::new(ErroringTier),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(T0)),
        );
        assert!(g.check_and_increment("tenant-e").await.is_none());
    }

    #[tokio::test]
    async fn no_clock_fails_open() {
        let g = RequestCountGate::new(
            Arc::new(ErroringCounter),
            Arc::new(FixedTier("free")),
            Arc::new(InMemoryFakeWallClock::at_unix_ms(0)),
        );
        assert!(g.check_and_increment("tenant-f").await.is_none());
    }

    // ── CachedTierResolver (WP: cold-hydrate D1 thundering-herd hardening) ──────

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A [`TierResolver`] fake that counts inner resolves (to prove the cache/
    /// single-flight coalesces them) and can inject a delay so a burst overlaps.
    #[derive(Debug)]
    struct CountingTier {
        tier: &'static str,
        calls: Arc<AtomicUsize>,
        delay_ms: u64,
    }
    #[async_trait::async_trait]
    impl TierResolver for CountingTier {
        async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            Ok(self.tier.to_owned())
        }
    }

    /// A counting [`TierResolver`] that always errors (fail-OPEN path).
    #[derive(Debug)]
    struct CountingErroringTier(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl TierResolver for CountingErroringTier {
        async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err("D1 down".to_owned())
        }
    }

    fn cached(
        inner: Arc<dyn TierResolver>,
        clock: &Arc<InMemoryFakeWallClock>,
    ) -> CachedTierResolver {
        CachedTierResolver::new(inner, Arc::clone(clock) as Arc<dyn WallClock>, DEFAULT_TIER_CACHE_TTL_MS)
    }

    #[tokio::test]
    async fn cached_tier_warm_hit_serves_from_memory() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(CountingTier { tier: "free", calls: Arc::clone(&calls), delay_ms: 0 }),
            &clock,
        );
        assert_eq!(r.tier("t1").await.unwrap(), "free");
        assert_eq!(r.tier("t1").await.unwrap(), "free");
        assert_eq!(r.tier("t1").await.unwrap(), "free");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "warm ops must not re-read D1");
    }

    #[tokio::test]
    async fn cached_tier_cold_parallel_burst_coalesces_to_one_resolve() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = Arc::new(cached(
            // A delay makes the burst genuinely overlap on the single-flight lock.
            Arc::new(CountingTier { tier: "pro", calls: Arc::clone(&calls), delay_ms: 30 }),
            &clock,
        ));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..24 {
            let rr = Arc::clone(&r);
            set.spawn(async move { rr.tier("hot-tenant").await.unwrap() });
        }
        while let Some(res) = set.join_next().await {
            assert_eq!(res.unwrap(), "pro");
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "a cold parallel burst must coalesce into ONE inner D1 resolve"
        );
    }

    #[tokio::test]
    async fn cached_tier_ttl_expiry_re_resolves() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(CountingTier { tier: "free", calls: Arc::clone(&calls), delay_ms: 0 }),
            &clock,
        );
        let _ = r.tier("t1").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Still within TTL → cached.
        clock.advance(Duration::from_millis(u64::try_from(DEFAULT_TIER_CACHE_TTL_MS).unwrap() - 1));
        let _ = r.tier("t1").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "pre-TTL op is still cached");
        // Cross the TTL → re-resolve.
        clock.advance(Duration::from_millis(2));
        let _ = r.tier("t1").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "post-TTL op re-reads D1");
    }

    #[tokio::test]
    async fn cached_tier_inner_error_is_not_cached_fail_open() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(Arc::new(CountingErroringTier(Arc::clone(&calls))), &clock);
        assert!(r.tier("t1").await.is_err());
        assert!(r.tier("t1").await.is_err());
        // Each op re-tried the inner (nothing cached) → a transient fault self-heals.
        assert_eq!(calls.load(Ordering::SeqCst), 2, "errors must NOT be cached (fail-open)");
    }

    #[tokio::test]
    async fn cached_tier_distinct_tenants_resolve_independently() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let r = cached(
            Arc::new(CountingTier { tier: "free", calls: Arc::clone(&calls), delay_ms: 0 }),
            &clock,
        );
        let _ = r.tier("t1").await.unwrap();
        let _ = r.tier("t2").await.unwrap();
        let _ = r.tier("t1").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one resolve per distinct tenant");
    }
}

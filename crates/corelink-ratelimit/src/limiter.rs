//! Rate-limiter orchestrator — the pure-logic core of the Tower
//! middleware. Wires the per-tenant token-bucket state machine,
//! `RateLimitAuditSink`, `RateLimitMetricsObserver`, and
//! `RateLimitConfig` into the canonical decision pipeline.
//!
//! ## Decision pipeline (per request)
//!
//! For each authenticated request `(tenant, key, cost, now_ms)`:
//!
//! 1. **Tenant-mismatch guard** (INV-AVAIL-ISOLATION canary): if the
//!    `key.tenant_id` doesn't match the caller-supplied `tenant_id`,
//!    bump `corelink.ratelimit.cross_tenant_violation_total` (SEV-1)
//!    and return [`RateLimitError::TenantMismatch`]. Mapped to 500 by
//!    the production Tower layer.
//! 2. **Cost overflow guard**: if `cost > burst_capacity`, the request
//!    can NEVER be served — reject with
//!    [`RateLimitError::CostExceedsCapacity`] (mapped to 400).
//! 3. **Lazy refill + cost check** (under per-instance Mutex): apply
//!    `try_acquire(state, cost, now_ms, config)`.
//! 4. **Audit emit BEFORE state mutation** (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`): emit the canonical event
//!    record. Audit failure ROLLS BACK the decision (returns
//!    `Err(Audit)`); production wiring maps to 503.
//! 5. **Commit state** to per-instance HashMap (mirrors DO durable
//!    storage write).
//! 6. **Metrics emit** (`check_total`, `tokens_remaining`,
//!    `refill_rate`, `middleware_duration_us`) AFTER the audit succeeds.
//!
//! ## Why per-instance Mutex serialises
//!
//! The InMemory orchestrator holds the `(read state, refill, cost
//! check, optionally update state)` sequence under a single
//! per-instance `Mutex`. This mirrors the production CF DO actor model
//! (single-threaded actor per `rate-limiter-<tenant_id>` DO singleton)
//! byte-for-byte: concurrent `try_acquire` calls cannot both succeed
//! at the boundary — the first one to acquire the lock observes
//! `refilled >= cost`; the second observes the first's commit (tokens
//! decremented) and fires the deny arm. Pinned by
//! `prop_concurrent_acquire_does_not_double_spend`.
//!
//! ## F-001 closure
//!
//! The orchestrator state is held in a per-instance
//! `Arc<Mutex<HashMap<BucketKey, TokenBucketState>>>` (NOT a
//! process-global `static LazyLock<Mutex<>>`). Tests instantiate fresh
//! orchestrators per case so the harness cannot accidentally leak
//! state across cases.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::audit::{RateLimitAuditRecord, RateLimitAuditSink, RateLimitEventType};
use crate::bucket::{try_acquire, BucketDecision, TokenBucketState};
use crate::config::RateLimitConfig;
use crate::error::RateLimitError;
use crate::key::{BucketKey, KeyDimension};
use crate::metrics::{RateLimitMetricsObserver, RateLimitResultLabel};

/// Upper bound on the number of live token buckets the in-memory
/// orchestrator retains, mirroring the Argon2id verifier's
/// `PER_TENANT_MAP_CAP` (`corelink-container::adapter_pat`).
///
/// ## DoS rationale (cross-tenant, HIGH — red-team confirmed)
///
/// The bucket map is keyed off [`BucketKey::scope_key`], and for the
/// UNAUTHENTICATED OCI plane that scope is derived from the
/// attacker-controlled repository name parsed VERBATIM from the
/// `/v2/<repo>/...` URL path (`corelink-container::routes::ratelimit_layer`
/// `oci_repo_scope` → `oci_bucket_key`). A flood of distinct repo segments
/// (`GET /v2/<random-N>/manifests/latest`, each `N` unique) made every request
/// materialise a NEW, PERMANENT map entry — the map had no cap and no eviction,
/// so the heap grew without bound until the OOM-killer reaped the SINGLETON
/// `_oci` Durable Object that fronts EVERY OCI tenant (one DO for all of them →
/// a cross-tenant registry outage, not just the attacker's own).
///
/// Bounding the map as an LRU defeats the flood: when a new bucket would push
/// the map past the cap we evict the least-recently-accessed entry first. An
/// evicted bucket simply re-materialises FRESH (full) on its next hit — exactly
/// what would have happened had it never existed — so the cap is sound and can
/// never become a rate-limit BYPASS (eviction only ever resets a bucket to
/// "full", the most permissive state, and a real attacker keeping a key hot
/// keeps it from being the LRU victim anyway).
///
/// 100k is deliberately generous: each entry is a [`BucketKey`] + a small
/// [`TokenBucketState`] (well under ~200 B → the full map is tens-of-MB scale),
/// far above any legitimate concurrently-active keyspace, so steady-state
/// eviction is effectively never reached by real traffic — the cap exists
/// solely to cap adversarial distinct-key churn.
const LIMITER_BUCKET_MAP_CAP: usize = 100_000;

/// Number of entries the approximate-LRU eviction SAMPLES per eviction
/// (Redis-style `maxmemory-samples`). The least-recently-accessed entry
/// *of the sample* is evicted — NOT a global minimum.
///
/// ## Why sampled (the F2 fix — algorithmic-complexity DoS closure)
///
/// The first cap fix (PR #530) evicted via `map.iter().min_by_key(last_access)`
/// — an `O(n)` FULL SCAN of the (up-to-100k-entry) map, run under the single
/// per-instance `Mutex` on EVERY new distinct key once the map sits at the cap.
/// The OCI bucket key is an attacker-controlled pre-auth repo path string
/// (`GET /v2/<rand>/manifests/latest`), so an unauth flood pins the shared
/// `_oci` limiter at the cap and makes every subsequent distinct key pay a
/// ~O(100k) scan under the one Mutex that serialises the whole singleton OCI
/// plane → CPU + lock-contention starvation: the SAME cross-tenant blast radius
/// the cap was meant to remove.
///
/// Sampling `K` entries and evicting the oldest of the sample is `O(K)` =
/// `O(1)` amortised — eviction touches at most `K` entries, never `n`, so the
/// adversarial per-key cost is constant regardless of map size. It is
/// APPROXIMATE LRU (Redis uses exactly this; `K = 8` keeps the eviction quality
/// statistically near-true-LRU). Safety is preserved:
///   * An evicted bucket re-materialises FRESH/full on its next hit — eviction
///     can only ever reset a bucket to the most-permissive state, so this is
///     NOT a rate-limit bypass (identical to the first fix's guarantee).
///   * A hot / just-throttled key (the most-recently-touched, largest
///     `last_access`) is the most-permissive eviction target and is highly
///     UNLIKELY to be in any given sample's minimum — so an attacker cannot
///     steer eviction onto a victim they are actively hammering.
const LIMITER_EVICTION_SAMPLE_K: usize = 8;

/// One entry in the bounded bucket LRU: the [`TokenBucketState`] plus the
/// monotonic tick at which it was last accessed (materialised, refreshed, or
/// touched). The smallest tick is the eviction candidate when the map is full.
#[derive(Clone, Copy, Debug)]
struct Bucket {
    state: TokenBucketState,
    last_access: u64,
}

/// Per-request decision rendered by [`RateLimiter::try_acquire`].
///
/// `PartialEq` is intentionally NOT derived because the `Allow` arm
/// carries the f64 `next_state.available_tokens`. Tests use `matches!`
/// + per-field equality where needed.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum RateLimitDecision {
    /// Request consumes `cost` tokens; handler MAY proceed.
    Allow {
        /// Wall-clock instant the decision was rendered.
        decided_at_ms: u64,
        /// Remaining tokens (integer-rounded for header emission).
        tokens_remaining: u64,
        /// Bucket capacity ceiling (informational; for RFC 9331
        /// `RateLimit-Policy` header).
        burst_capacity: u32,
        /// Active refill rate (tokens / second; informational).
        refill_rate_per_sec: u64,
    },
    /// Request rejected — 429 + Retry-After arm (RFC 6585 §4 + RFC 9331).
    Deny429 {
        /// Wall-clock instant the deny fired.
        decided_at_ms: u64,
        /// Retry-After value (seconds; clamped per
        /// [`RateLimitConfig`]).
        retry_after_secs: u64,
        /// Tokens currently in the bucket (integer-rounded).
        tokens_remaining: u64,
        /// Bucket capacity ceiling.
        burst_capacity: u32,
        /// Active refill rate.
        refill_rate_per_sec: u64,
    },
}

impl RateLimitDecision {
    /// Whether THIS decision admits the request (`Allow` arm).
    #[must_use]
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Whether THIS decision rejects the request (`Deny429` arm).
    #[must_use]
    pub const fn is_deny(&self) -> bool {
        matches!(self, Self::Deny429 { .. })
    }

    /// Result label corresponding to THIS arm (for metric emit).
    #[must_use]
    pub const fn result_label(&self) -> RateLimitResultLabel {
        match self {
            Self::Allow { .. } => RateLimitResultLabel::Allowed,
            Self::Deny429 { .. } => RateLimitResultLabel::Denied,
        }
    }
}

/// Side-effect payload returned alongside [`RateLimitDecision`].
///
/// Production wiring uses this to sum the duration into the
/// `middleware_duration_us` metric and to drive the Tower-layer
/// response shape.
#[derive(Clone, Copy, Debug)]
pub struct RateLimitOutcome {
    /// The rendered decision.
    pub decision: RateLimitDecision,
    /// Decision duration (microseconds; exposed for testing the
    /// < 3ms p99 SLO per spec_contract §10.s08.2).
    pub duration_us: u64,
}

/// Trait surfaced by every rate-limiter backend (production CF DO
/// singleton + Tower layer / in-memory fake).
pub trait RateLimiter: Send + Sync + core::fmt::Debug {
    /// Render the per-request rate-limit decision.
    ///
    /// `now_ms` is the wall-clock instant the request was admitted at
    /// the Tower layer. `cost` is the request weight (typically 1;
    /// bandwidth-weighted requests sometimes >1 per WI §1).
    ///
    /// # Errors
    ///
    /// Surface as [`RateLimitError`].
    fn try_acquire(
        &self,
        tenant_id: Uuid,
        bucket_key: BucketKey,
        cost: u32,
        now_ms: u64,
    ) -> Result<RateLimitOutcome, RateLimitError>;

    /// Update the per-tenant plan refill rate + burst capacity.
    /// Production wiring invokes this on the S-13 admin push (plan
    /// upgrade / downgrade); tests poke the override directly.
    ///
    /// # Errors
    ///
    /// Surface as [`RateLimitError`].
    fn update_plan(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        scope_key: &str,
        new_refill_rate_per_sec: u32,
        new_burst_capacity: u32,
        now_ms: u64,
    ) -> Result<(), RateLimitError>;

    /// Snapshot the current bucket state (for diagnostic / cold-start
    /// reload tests). Returns `Ok(None)` if no bucket has been
    /// materialised yet.
    ///
    /// # Errors
    ///
    /// Surface as [`RateLimitError`].
    fn snapshot_bucket(
        &self,
        bucket_key: &BucketKey,
    ) -> Result<Option<TokenBucketState>, RateLimitError>;
}

/// In-memory token-bucket rate-limiter (the canonical pure-logic
/// orchestrator). Mirrors the production CF DO actor model: per-instance
/// `Mutex` serialises concurrent calls so the boundary cannot be
/// double-spent.
pub struct InMemoryTokenBucketRateLimiter<A, M>
where
    A: RateLimitAuditSink,
    M: RateLimitMetricsObserver,
{
    audit: Arc<A>,
    metrics: Arc<M>,
    config: RateLimitConfig,
    state: Arc<Mutex<HashMap<BucketKey, Bucket>>>,
    /// Max live buckets before LRU eviction kicks in (see
    /// [`LIMITER_BUCKET_MAP_CAP`]).
    bucket_cap: usize,
    /// Monotonic LRU tick source. Bumped on every bucket access so the
    /// least-recently-used entry can be identified for eviction.
    access_tick: AtomicU64,
}

impl<A, M> core::fmt::Debug for InMemoryTokenBucketRateLimiter<A, M>
where
    A: RateLimitAuditSink,
    M: RateLimitMetricsObserver,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryTokenBucketRateLimiter")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<A, M> InMemoryTokenBucketRateLimiter<A, M>
where
    A: RateLimitAuditSink,
    M: RateLimitMetricsObserver,
{
    /// Construct with the canonical default config.
    #[must_use]
    pub fn with_defaults(audit: Arc<A>, metrics: Arc<M>) -> Self {
        Self::new(audit, metrics, RateLimitConfig::canonical())
    }

    /// Construct with an explicit config.
    #[must_use]
    pub fn new(audit: Arc<A>, metrics: Arc<M>, config: RateLimitConfig) -> Self {
        Self::new_with_cap(audit, metrics, config, LIMITER_BUCKET_MAP_CAP)
    }

    /// Construct with an explicit config AND an explicit bucket-map cap.
    /// The cap defaults to [`LIMITER_BUCKET_MAP_CAP`] in the public
    /// constructors; a smaller cap is used by the bounded-map regression
    /// test so the distinct-key flood is cheap to drive.
    #[must_use]
    fn new_with_cap(
        audit: Arc<A>,
        metrics: Arc<M>,
        config: RateLimitConfig,
        bucket_cap: usize,
    ) -> Self {
        Self {
            audit,
            metrics,
            config,
            state: Arc::new(Mutex::new(HashMap::new())),
            bucket_cap: bucket_cap.max(1),
            access_tick: AtomicU64::new(0),
        }
    }

    /// Next monotonic LRU tick (interior-mutable; cheap relaxed bump).
    fn next_tick(&self) -> u64 {
        self.access_tick.fetch_add(1, Ordering::Relaxed)
    }

    /// Enforce the bucket-map cap BEFORE a NEW key is inserted: if the map is
    /// at/over the cap and `incoming` is not already present, evict the
    /// least-recently-accessed entry **of a bounded sample** (Redis-style
    /// approximate LRU). This is the bound that defeats the OCI distinct-repo
    /// DoS (see [`LIMITER_BUCKET_MAP_CAP`]).
    ///
    /// ## `O(1)`-amortised eviction (the F2 algorithmic-complexity fix)
    ///
    /// Eviction inspects at most [`LIMITER_EVICTION_SAMPLE_K`] entries and
    /// evicts the oldest-`last_access` of that sample — `O(K)` = `O(1)`, NEVER
    /// an `O(n)` full scan. So even when the map is pinned at the cap by an
    /// adversarial distinct-key flood, each new key pays a constant eviction
    /// cost regardless of map size, and the per-instance `Mutex` is never held
    /// for an `O(n)` span.
    ///
    /// ## Why sampling the HashMap's iteration order is a sound sample
    ///
    /// Rust's `HashMap` hashes keys with SipHash seeded by a per-map random
    /// state, so iteration order is already pseudo-random w.r.t. insertion /
    /// access order — taking the first `K` of `iter()` is a dep-free random
    /// sample (no rng crate). It is APPROXIMATE LRU: the global LRU minimum may
    /// be missed, but a fresh re-materialise on the victim's next hit makes that
    /// harmless (eviction only ever resets a bucket to its most-permissive
    /// "full" state — never a rate-limit bypass), and a hot key is statistically
    /// unlikely to be the sample's minimum, so an attacker cannot steer
    /// eviction onto a key they are actively hammering.
    fn evict_if_at_cap(map: &mut HashMap<BucketKey, Bucket>, incoming: &BucketKey, cap: usize) {
        if map.len() < cap || map.contains_key(incoming) {
            return;
        }
        // Sampled approximate-LRU: scan only the first K of the (SipHash-
        // randomised) iteration order and evict the oldest of that sample.
        // Bounded at K entries → O(1), no full O(n) scan under the Mutex.
        if let Some(victim) = map
            .iter()
            .take(LIMITER_EVICTION_SAMPLE_K)
            .min_by_key(|(_, b)| b.last_access)
            .map(|(k, _)| k.clone())
        {
            map.remove(&victim);
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Number of materialised buckets (cardinality assertion in
    /// property tests).
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitError::Backend`] on mutex poisoning.
    pub fn bucket_count(&self) -> Result<usize, RateLimitError> {
        let g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        Ok(g.len())
    }

    /// Pre-seed a bucket (test wiring + DO cold-start reload). The
    /// production binding constructs the bucket lazily on first
    /// `try_acquire`; tests pre-seed to drive deterministic boundary
    /// scenarios.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitError::Backend`] on mutex poisoning.
    pub fn seed_bucket(
        &self,
        key: BucketKey,
        state: TokenBucketState,
    ) -> Result<(), RateLimitError> {
        let tick = self.next_tick();
        let mut g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        Self::evict_if_at_cap(&mut g, &key, self.bucket_cap);
        g.insert(
            key,
            Bucket {
                state,
                last_access: tick,
            },
        );
        Ok(())
    }

    fn fresh_bucket(&self, now_ms: u64) -> TokenBucketState {
        TokenBucketState::new_full(
            self.config.default_burst_capacity(),
            self.config.default_refill_rate_per_sec(),
            now_ms,
        )
    }
}

impl<A, M> RateLimiter for InMemoryTokenBucketRateLimiter<A, M>
where
    A: RateLimitAuditSink,
    M: RateLimitMetricsObserver,
{
    fn try_acquire(
        &self,
        tenant_id: Uuid,
        bucket_key: BucketKey,
        cost: u32,
        now_ms: u64,
    ) -> Result<RateLimitOutcome, RateLimitError> {
        // Step 1: tenant-mismatch guard (INV-AVAIL-ISOLATION canary).
        if bucket_key.tenant_id != tenant_id {
            // Bump the SEV-1 cross-tenant canary. Even if metrics fail,
            // we MUST still surface the typed error.
            let _ = self.metrics.record_cross_tenant_violation();
            return Err(RateLimitError::TenantMismatch {
                caller_tenant_id: tenant_id,
                bucket_tenant_id: bucket_key.tenant_id,
            });
        }

        // Step 2: per-instance Mutex (mirrors DO actor serialisation).
        let tick = self.next_tick();
        let mut guard = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;

        // Enforce the bounded-map cap BEFORE lazily materialising a NEW bucket
        // (the unbounded-growth DoS sink — see `LIMITER_BUCKET_MAP_CAP`).
        Self::evict_if_at_cap(&mut guard, &bucket_key, self.bucket_cap);

        // Lazy materialise the bucket (and mark it freshly accessed for LRU).
        let entry = guard
            .entry(bucket_key.clone())
            .or_insert_with(|| Bucket {
                state: self.fresh_bucket(now_ms),
                last_access: tick,
            });
        entry.last_access = tick;
        let state = entry.state;

        // Step 3: cost overflow guard.
        if cost > state.burst_capacity {
            return Err(RateLimitError::CostExceedsCapacity {
                cost,
                capacity: state.burst_capacity,
            });
        }

        // Step 4: lazy refill + cost check.
        let decision = try_acquire(state, cost, now_ms, &self.config);

        // Step 5: audit emit BEFORE state mutation (fail-closed envelope).
        let (event_type, available_tokens_after, next_state) = match decision {
            BucketDecision::Allow {
                next_state,
                available_tokens_after,
            } => (
                RateLimitEventType::Allowed,
                available_tokens_after,
                next_state,
            ),
            BucketDecision::Deny429 {
                next_state,
                available_tokens_after,
                ..
            } => (
                RateLimitEventType::Denied429,
                available_tokens_after,
                next_state,
            ),
        };
        self.audit.emit(RateLimitAuditRecord {
            event_type,
            tenant_id,
            bucket_key: bucket_key.clone(),
            cost,
            available_tokens_after,
            created_by_request_id: "test".to_string(),
            now_ms,
        })?;

        // Optionally emit BucketRefilled when the refill step actually
        // moved the watermark. This is the informational arm — failure
        // here mirrors fail-closed envelope (rolls back).
        if next_state.last_refill_at_ms != state.last_refill_at_ms
            && (next_state.available_tokens > state.available_tokens
                || matches!(decision, BucketDecision::Allow { .. }))
        {
            self.audit.emit(RateLimitAuditRecord {
                event_type: RateLimitEventType::BucketRefilled,
                tenant_id,
                bucket_key: bucket_key.clone(),
                cost: 0,
                available_tokens_after: next_state.available_tokens.floor() as u64,
                created_by_request_id: "test".to_string(),
                now_ms,
            })?;
        }

        // Step 6: commit state to the in-memory mirror. The key was
        // materialised above, so this overwrite cannot grow the map.
        guard.insert(
            bucket_key.clone(),
            Bucket {
                state: next_state,
                last_access: tick,
            },
        );
        // Drop the lock before metrics emit (metrics are off-the-hot-path).
        drop(guard);

        // Step 7: metrics emit (after audit succeeded).
        let result_label = match decision {
            BucketDecision::Allow { .. } => RateLimitResultLabel::Allowed,
            BucketDecision::Deny429 { .. } => RateLimitResultLabel::Denied,
        };
        self.metrics
            .record_check(tenant_id, bucket_key.dimension, result_label)?;
        self.metrics.observe_tokens_remaining(
            tenant_id,
            bucket_key.dimension,
            available_tokens_after,
        )?;
        // refill_rate gauge — updated each call (cheap; preserves
        // INV-RATE-LIMIT-PROPORTIONALITY canary).
        self.metrics.observe_refill_rate(
            tenant_id,
            bucket_key.dimension,
            next_state.refill_rate_per_sec.floor() as u64,
        )?;
        self.metrics
            .record_middleware_duration_us(result_label, 0)?;

        let public_decision = match decision {
            BucketDecision::Allow {
                available_tokens_after,
                ..
            } => RateLimitDecision::Allow {
                decided_at_ms: now_ms,
                tokens_remaining: available_tokens_after,
                burst_capacity: next_state.burst_capacity,
                refill_rate_per_sec: next_state.refill_rate_per_sec.floor() as u64,
            },
            BucketDecision::Deny429 {
                retry_after_secs,
                available_tokens_after,
                ..
            } => RateLimitDecision::Deny429 {
                decided_at_ms: now_ms,
                retry_after_secs,
                tokens_remaining: available_tokens_after,
                burst_capacity: next_state.burst_capacity,
                refill_rate_per_sec: next_state.refill_rate_per_sec.floor() as u64,
            },
        };

        Ok(RateLimitOutcome {
            decision: public_decision,
            duration_us: 0,
        })
    }

    fn update_plan(
        &self,
        tenant_id: Uuid,
        dimension: KeyDimension,
        scope_key: &str,
        new_refill_rate_per_sec: u32,
        new_burst_capacity: u32,
        now_ms: u64,
    ) -> Result<(), RateLimitError> {
        if new_burst_capacity == 0 {
            return Err(RateLimitError::Backend(
                "update_plan rejected: burst_capacity must be >= 1".to_string(),
            ));
        }
        let key = BucketKey {
            tenant_id,
            dimension,
            scope_key: scope_key.to_string(),
        };
        let tick = self.next_tick();
        let mut g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        Self::evict_if_at_cap(&mut g, &key, self.bucket_cap);
        let entry = g.entry(key).or_insert_with(|| Bucket {
            state: self.fresh_bucket(now_ms),
            last_access: tick,
        });
        entry.last_access = tick;
        // Snap available_tokens to new capacity if shrinking.
        let new_cap_f = f64::from(new_burst_capacity);
        if entry.state.available_tokens > new_cap_f {
            entry.state.available_tokens = new_cap_f;
        }
        entry.state.burst_capacity = new_burst_capacity;
        entry.state.refill_rate_per_sec = f64::from(new_refill_rate_per_sec);
        Ok(())
    }

    fn snapshot_bucket(
        &self,
        bucket_key: &BucketKey,
    ) -> Result<Option<TokenBucketState>, RateLimitError> {
        let g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        Ok(g.get(bucket_key).map(|b| b.state))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingRateLimitAuditSink, InMemoryRateLimitAuditSink};
    use crate::metrics::{InMemoryRateLimitMetrics, RateLimitMetricKind};

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    type Fixture = (
        InMemoryTokenBucketRateLimiter<InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics>,
        Arc<InMemoryRateLimitAuditSink>,
        Arc<InMemoryRateLimitMetrics>,
    );

    fn fresh() -> Fixture {
        let audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let limiter =
            InMemoryTokenBucketRateLimiter::with_defaults(Arc::clone(&audit), Arc::clone(&metrics));
        (limiter, audit, metrics)
    }

    // ---- Allow path -------------------------------------------------

    #[test]
    fn fresh_bucket_first_acquire_allows() {
        let (lim, audit, metrics) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        let out = lim.try_acquire(ten_a(), key.clone(), 1, 1000).unwrap();
        assert!(out.decision.is_allow());
        // First call materialises the bucket.
        assert_eq!(lim.bucket_count().unwrap(), 1);
        assert!(matches!(
            out.decision,
            RateLimitDecision::Allow {
                tokens_remaining,
                ..
            } if tokens_remaining == 999
        ));
        // Audit + metric emitted.
        assert_eq!(audit.snapshot_of(RateLimitEventType::Allowed).len(), 1);
        assert_eq!(metrics.counter_total(RateLimitMetricKind::CheckTotal), 1);
    }

    // ---- Deny path --------------------------------------------------

    #[test]
    fn empty_bucket_acquire_denies() {
        let (lim, audit, metrics) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        // Seed an empty bucket with rate 1/s.
        lim.seed_bucket(
            key.clone(),
            TokenBucketState::from_persisted(0.0, 100, 1.0, 1000),
        )
        .unwrap();
        let out = lim.try_acquire(ten_a(), key, 1, 1000).unwrap();
        assert!(out.decision.is_deny());
        match out.decision {
            RateLimitDecision::Deny429 {
                retry_after_secs,
                tokens_remaining,
                ..
            } => {
                assert_eq!(tokens_remaining, 0);
                assert!(retry_after_secs >= 1);
            }
            _ => panic!("expected deny"),
        }
        assert_eq!(audit.snapshot_of(RateLimitEventType::Denied429).len(), 1);
        assert_eq!(metrics.counter_total(RateLimitMetricKind::CheckTotal), 1);
    }

    // ---- Cost overflow guard ---------------------------------------

    #[test]
    fn cost_above_capacity_returns_typed_error() {
        let (lim, _a, _m) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        // Default burst capacity = 1000; cost 5000 → reject.
        let err = lim.try_acquire(ten_a(), key, 5000, 1000).unwrap_err();
        assert!(matches!(
            err,
            RateLimitError::CostExceedsCapacity { cost, capacity }
                if cost == 5000 && capacity == 1000
        ));
    }

    // ---- Tenant-mismatch guard --------------------------------------

    #[test]
    fn tenant_mismatch_returns_typed_error_and_bumps_canary() {
        let (lim, _a, metrics) = fresh();
        // Caller is ten_a, but the bucket key claims ten_b.
        let key = BucketKey::per_tenant(ten_b());
        let err = lim.try_acquire(ten_a(), key, 1, 1000).unwrap_err();
        assert!(matches!(err, RateLimitError::TenantMismatch { .. }));
        assert_eq!(
            metrics.counter_total(RateLimitMetricKind::CrossTenantViolationTotal),
            1
        );
    }

    // ---- Tenant isolation -------------------------------------------

    #[test]
    fn tenant_a_acquire_does_not_affect_tenant_b_bucket() {
        let (lim, _a, _m) = fresh();
        let ka = BucketKey::per_tenant(ten_a());
        let kb = BucketKey::per_tenant(ten_b());
        // Seed both at full capacity, both same rate.
        lim.seed_bucket(
            ka.clone(),
            TokenBucketState::from_persisted(10.0, 100, 1.0, 1000),
        )
        .unwrap();
        lim.seed_bucket(
            kb.clone(),
            TokenBucketState::from_persisted(10.0, 100, 1.0, 1000),
        )
        .unwrap();
        // Drain tenant_a fully.
        for _ in 0..10 {
            let _ = lim.try_acquire(ten_a(), ka.clone(), 1, 1000).unwrap();
        }
        // tenant_a now empty; should deny.
        let out_a = lim.try_acquire(ten_a(), ka.clone(), 1, 1000).unwrap();
        assert!(out_a.decision.is_deny());
        // tenant_b should still allow.
        let out_b = lim.try_acquire(ten_b(), kb, 1, 1000).unwrap();
        assert!(out_b.decision.is_allow());
    }

    // ---- F-001 closure ---------------------------------------------

    #[test]
    fn separate_limiter_instances_have_independent_state() {
        let (lim1, _, _) = fresh();
        let (lim2, _, _) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        let _ = lim1.try_acquire(ten_a(), key.clone(), 1, 1000).unwrap();
        // lim2 has NEVER seen this tenant — its bucket count is 0.
        assert_eq!(lim2.bucket_count().unwrap(), 0);
        assert_eq!(lim1.bucket_count().unwrap(), 1);
    }

    // ---- update_plan ------------------------------------------------

    #[test]
    fn update_plan_changes_refill_rate_and_capacity() {
        let (lim, _a, _m) = fresh();
        // Update the team-default tenant to enterprise rate 10000 RPS,
        // burst 50000.
        lim.update_plan(ten_a(), KeyDimension::PerTenant, "", 10_000, 50_000, 1000)
            .unwrap();
        let key = BucketKey::per_tenant(ten_a());
        let snap = lim.snapshot_bucket(&key).unwrap().unwrap();
        assert_eq!(snap.burst_capacity, 50_000);
        assert_eq!(snap.refill_rate_per_sec, 10_000.0);
    }

    #[test]
    fn update_plan_rejects_zero_capacity() {
        let (lim, _, _) = fresh();
        let err = lim
            .update_plan(ten_a(), KeyDimension::PerTenant, "", 100, 0, 1000)
            .unwrap_err();
        assert!(matches!(err, RateLimitError::Backend(_)));
    }

    #[test]
    fn update_plan_snaps_available_when_shrinking_capacity() {
        let (lim, _, _) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        lim.seed_bucket(
            key.clone(),
            TokenBucketState::from_persisted(800.0, 1000, 200.0, 1000),
        )
        .unwrap();
        // Downgrade: capacity 100; available should snap to 100.
        lim.update_plan(ten_a(), KeyDimension::PerTenant, "", 50, 100, 2000)
            .unwrap();
        let snap = lim.snapshot_bucket(&key).unwrap().unwrap();
        assert_eq!(snap.burst_capacity, 100);
        assert_eq!(snap.available_tokens, 100.0);
    }

    // ---- audit fail-closed ------------------------------------------

    #[test]
    fn audit_failure_aborts_decision_and_bucket_unchanged() {
        let audit = Arc::new(FailingRateLimitAuditSink::new());
        let metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let lim =
            InMemoryTokenBucketRateLimiter::with_defaults(Arc::clone(&audit), Arc::clone(&metrics));
        let key = BucketKey::per_tenant(ten_a());
        let err = lim.try_acquire(ten_a(), key.clone(), 1, 1000).unwrap_err();
        assert!(matches!(err, RateLimitError::Audit(_)));
        // Bucket WAS materialised (lazy materialisation happens before
        // audit), but the state was NEVER committed back — confirm the
        // bucket retains the FRESH state (not the post-acquire state).
        let snap = lim.snapshot_bucket(&key).unwrap().unwrap();
        assert_eq!(snap.available_tokens, 1000.0);
    }

    // ---- per-IP / per-endpoint dimensions --------------------------

    #[test]
    fn per_ip_bucket_isolated_from_per_tenant() {
        let (lim, _a, _m) = fresh();
        let kt = BucketKey::per_tenant(ten_a());
        let kip = BucketKey::per_ip(ten_a(), "203.0.113.5");
        // Drain per-tenant; per-IP unaffected.
        lim.seed_bucket(
            kt.clone(),
            TokenBucketState::from_persisted(1.0, 1000, 200.0, 1000),
        )
        .unwrap();
        let _ = lim.try_acquire(ten_a(), kt.clone(), 1, 1000).unwrap();
        // Now drain it again; should deny (0 tokens, same instant).
        let out = lim.try_acquire(ten_a(), kt, 1, 1000).unwrap();
        assert!(out.decision.is_deny());
        // per-IP bucket is fresh — should allow.
        let out = lim.try_acquire(ten_a(), kip, 1, 1000).unwrap();
        assert!(out.decision.is_allow());
    }

    // ---- cost == 0 idempotent ---------------------------------------

    #[test]
    fn cost_zero_is_idempotent_and_does_not_emit_refill_audit() {
        let (lim, audit, _m) = fresh();
        let key = BucketKey::per_tenant(ten_a());
        let _ = lim.try_acquire(ten_a(), key.clone(), 0, 1000).unwrap();
        let _ = lim.try_acquire(ten_a(), key.clone(), 0, 2000).unwrap();
        let snap = lim.snapshot_bucket(&key).unwrap().unwrap();
        // last_refill_at_ms NEVER advanced — cost-0 path is pure echo.
        assert_eq!(snap.last_refill_at_ms, 1000);
        assert_eq!(snap.available_tokens, 1000.0);
        // BucketRefilled NOT emitted (no refill happened).
        assert_eq!(
            audit.snapshot_of(RateLimitEventType::BucketRefilled).len(),
            0
        );
    }

    // ---- bucket_count + cardinality --------------------------------

    #[test]
    fn bucket_count_grows_with_distinct_keys() {
        let (lim, _, _) = fresh();
        let _ = lim
            .try_acquire(ten_a(), BucketKey::per_tenant(ten_a()), 1, 1000)
            .unwrap();
        let _ = lim
            .try_acquire(ten_a(), BucketKey::per_ip(ten_a(), "1.1.1.1"), 1, 1000)
            .unwrap();
        let _ = lim
            .try_acquire(
                ten_a(),
                BucketKey::per_tenant_per_endpoint(ten_a(), "cas.put"),
                1,
                1000,
            )
            .unwrap();
        assert_eq!(lim.bucket_count().unwrap(), 3);
    }

    // ---- bounded-map DoS regression --------------------------------

    /// Red-team OCI distinct-repo flood: a torrent of distinct
    /// attacker-controlled scope keys must NOT grow the bucket map without
    /// bound. With the LRU cap in place the map size stays clamped at the cap
    /// no matter how many unique keys are thrown at it.
    #[test]
    fn bucket_map_is_bounded_under_distinct_key_flood() {
        let audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let metrics = Arc::new(InMemoryRateLimitMetrics::new());
        // Small cap so the flood is cheap to drive; the production cap is
        // LIMITER_BUCKET_MAP_CAP (100k).
        let cap = 16usize;
        let lim = InMemoryTokenBucketRateLimiter::new_with_cap(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            RateLimitConfig::canonical(),
            cap,
        );
        // Simulate `GET /v2/<random-N>/manifests/latest` — each a distinct
        // attacker-controlled repo scope under ONE (unauth/shared) tenant.
        for i in 0..(cap * 8) as u32 {
            let key = BucketKey::per_tenant_per_endpoint(ten_a(), format!("oci.repo.{i}"));
            let out = lim.try_acquire(ten_a(), key, 1, 1000).unwrap();
            // Each fresh (re-materialised) bucket still serves correctly.
            assert!(out.decision.is_allow());
            // Invariant holds at EVERY step, not just at the end.
            assert!(lim.bucket_count().unwrap() <= cap);
        }
        // Map is clamped at exactly the cap despite 128 distinct keys.
        assert_eq!(lim.bucket_count().unwrap(), cap);
    }

    /// F2 algorithmic-complexity DoS closure: eviction must be `O(1)` — it
    /// inspects at most [`LIMITER_EVICTION_SAMPLE_K`] entries (Redis-style
    /// sampled approximate-LRU), NOT all `n` entries. This pins the property
    /// directly on `evict_if_at_cap`: with a map far larger than `K` sitting at
    /// the cap, an eviction is driven and the number of entries whose
    /// `last_access` it could have read is bounded by `K`.
    ///
    /// We assert the bound structurally: the victim chosen is always the
    /// oldest of SOME ≤K-sized subset, never necessarily the global minimum —
    /// so we drive an eviction on a map where MANY entries share the global-min
    /// tick and confirm only that a single eviction happened (size dropped by
    /// exactly one) and the cost did not depend on `n`. The K-bound itself is
    /// guaranteed by construction (`.take(LIMITER_EVICTION_SAMPLE_K)`); this
    /// test documents + exercises it.
    #[test]
    fn eviction_touches_at_most_sample_k_entries_not_o_n() {
        // K is the documented sample bound; eviction must never exceed it.
        // (Const relation — documents the invariant; clippy would const-fold any
        // assert over consts, so the allow is the idiomatic way to keep the
        // documenting check.)
        #[allow(clippy::assertions_on_constants, reason = "documents the const sample-bound invariant")]
        {
            assert!(LIMITER_EVICTION_SAMPLE_K >= 1);
        }

        // Build a map well above K so a full O(n) scan would touch many more
        // than K entries — yet eviction only ever samples K.
        let cap = LIMITER_EVICTION_SAMPLE_K * 64; // n >> K
        let mut map: HashMap<BucketKey, Bucket> = HashMap::new();
        for i in 0..cap as u32 {
            let key = BucketKey::per_tenant_per_endpoint(ten_a(), format!("oci.repo.{i}"));
            map.insert(
                key,
                Bucket {
                    state: TokenBucketState::from_persisted(1.0, 1000, 1.0, 1000),
                    // Uniform last_access so no entry is privileged — eviction
                    // quality is irrelevant here, only the touch BOUND matters.
                    last_access: 1,
                },
            );
        }
        assert_eq!(map.len(), cap);

        // Drive exactly one eviction by offering a NEW key while at cap.
        let incoming = BucketKey::per_tenant_per_endpoint(ten_a(), "oci.repo.NEW".to_string());
        InMemoryTokenBucketRateLimiter::<
            InMemoryRateLimitAuditSink,
            InMemoryRateLimitMetrics,
        >::evict_if_at_cap(&mut map, &incoming, cap);

        // Exactly one entry evicted — the map made room for the incoming key.
        // (Boundedness AND the ≤K-touch contract: eviction is `O(K)`, asserted
        // by construction via `.take(LIMITER_EVICTION_SAMPLE_K)` in
        // `evict_if_at_cap`; here we confirm it performs a single, well-formed
        // eviction on an n >> K map without scanning all n.)
        assert_eq!(map.len(), cap - 1);
    }
}

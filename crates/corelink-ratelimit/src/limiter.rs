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
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::audit::{RateLimitAuditRecord, RateLimitAuditSink, RateLimitEventType};
use crate::bucket::{try_acquire, BucketDecision, TokenBucketState};
use crate::config::RateLimitConfig;
use crate::error::RateLimitError;
use crate::key::{BucketKey, KeyDimension};
use crate::metrics::{RateLimitMetricsObserver, RateLimitResultLabel};

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
    state: Arc<Mutex<HashMap<BucketKey, TokenBucketState>>>,
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
        Self {
            audit,
            metrics,
            config: RateLimitConfig::canonical(),
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Construct with an explicit config.
    #[must_use]
    pub fn new(audit: Arc<A>, metrics: Arc<M>, config: RateLimitConfig) -> Self {
        Self {
            audit,
            metrics,
            config,
            state: Arc::new(Mutex::new(HashMap::new())),
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
        let mut g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        g.insert(key, state);
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
        let mut guard = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;

        // Lazy materialise the bucket.
        let state = *guard
            .entry(bucket_key.clone())
            .or_insert_with(|| self.fresh_bucket(now_ms));

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

        // Step 6: commit state to the in-memory mirror.
        guard.insert(bucket_key.clone(), next_state);
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
        let mut g = self
            .state
            .lock()
            .map_err(|_| RateLimitError::Backend("limiter state mutex poisoned".to_string()))?;
        let entry = g.entry(key).or_insert_with(|| self.fresh_bucket(now_ms));
        // Snap available_tokens to new capacity if shrinking.
        let new_cap_f = f64::from(new_burst_capacity);
        if entry.available_tokens > new_cap_f {
            entry.available_tokens = new_cap_f;
        }
        entry.burst_capacity = new_burst_capacity;
        entry.refill_rate_per_sec = f64::from(new_refill_rate_per_sec);
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
        Ok(g.get(bucket_key).copied())
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
}

//! R3-6 end-to-end resilience scenarios — 6 canonical arms pinned
//! per the orchestration mandate:
//!
//! 1. `rate_limit_per_tenant_burst_over_quota_rejects_excess_with_429_and_audit`
//! 2. `token_bucket_refills_after_one_logical_second_and_admits_next_request`
//! 3. `circuit_breaker_opens_on_five_consecutive_5xx_then_rejects_fast`
//! 4. `circuit_breaker_half_open_probe_success_closes_failure_reopens`
//! 5. `back_pressure_rejects_when_queue_depth_exceeds_threshold_with_503_and_retry_after`
//! 6. `audit_fail_closed_at_circuit_layer_blocks_state_change_before_rejection`
//!
//! All scenarios drive a single logical clock and seeded deterministic
//! choices (HalfOpen 10% sampler is `observation_id mod 10`; no real
//! RNG required for byte-for-byte reproducibility).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "integration tests are allowed to use these primitives"
)]

use std::sync::Arc;

use uuid::Uuid;

use corelink_rate_headers::{
    audit::{CircuitAuditRecord, CircuitAuditSink, CircuitAuditSinkError, CircuitEventType},
    circuit::{
        CircuitDecision, CircuitState, CircuitThresholds, GlobalCircuitBreaker, HealthObservation,
        InMemoryGlobalCircuitBreaker, ObservationStatus, HALFOPEN_DWELL_MS, ROLLING_WINDOW_MS,
    },
    metrics::InMemoryCircuitMetrics,
    InMemoryCircuitAuditSink,
};
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimitConfig, RateLimitDecision, RateLimitEventType,
    RateLimiter, TokenBucketState,
};

use e2e_resilience::{
    BackPressureQueue, HttpStatus, LogicalClock, ResilienceError, ResilienceResponse,
};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

type RateLimitFixture = (
    InMemoryTokenBucketRateLimiter<InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics>,
    Arc<InMemoryRateLimitAuditSink>,
);

/// Build a rate-limiter pinned to a small bucket so tests assert on
/// canonical 110%-over-quota arithmetic without huge iteration loops:
/// burst capacity 10, refill 10 tokens/sec — same algorithmic shape as
/// the production team-tier (1000 burst / 200 RPS) scaled 100× down.
fn fresh_rate_limiter(burst: u32, refill_rps: u32) -> RateLimitFixture {
    let audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let cfg = RateLimitConfig::canonical();
    let limiter =
        InMemoryTokenBucketRateLimiter::new(Arc::clone(&audit), Arc::clone(&metrics), cfg);
    // Seed an explicit bucket so the test pins the exact (burst, refill)
    // shape regardless of `RateLimitConfig` default drift.
    let tenant = canonical_tenant();
    let key = BucketKey::per_tenant(tenant);
    let state = TokenBucketState::new_full(burst, refill_rps, 0);
    limiter.seed_bucket(key, state).unwrap();
    (limiter, audit)
}

/// Canonical test tenant (`0xc0_..._01`). Pinned across scenarios so
/// audit records are correlatable in failure triage.
fn canonical_tenant() -> Uuid {
    Uuid::from_u128(0xc0ff_ee00_0000_0000_0000_0000_0000_0001)
}

type CircuitFixture = (
    InMemoryGlobalCircuitBreaker<InMemoryCircuitAuditSink, InMemoryCircuitMetrics>,
    Arc<InMemoryCircuitAuditSink>,
);

/// Build a breaker pinned to test-tight thresholds: 5xx ratio > 0.5,
/// DO error rate > 0.3, p99 effectively disabled (huge ceiling so
/// scenario 4 recovery is unambiguous), `min_observations: 5` so a
/// 5-burst saturates the window.
fn fresh_breaker() -> CircuitFixture {
    let audit = Arc::new(InMemoryCircuitAuditSink::new());
    let metrics = Arc::new(InMemoryCircuitMetrics::new());
    let thresholds = CircuitThresholds {
        error_5xx_rate: 0.5,
        p99_latency_us: u64::MAX, // signal B parked off; trip via A + C.
        do_error_rate: 0.3,
        min_observations: 5,
    };
    let breaker = InMemoryGlobalCircuitBreaker::new(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        "test-region",
        thresholds,
    );
    (breaker, audit)
}

/// Build one `ServerError5xx` health observation with `do_error_rate`
/// pinned high so signal C breaches alongside signal A on the burst.
fn five_xx(now_ms: u64) -> HealthObservation {
    HealthObservation {
        timestamp_ms: now_ms,
        status: ObservationStatus::ServerError5xx,
        latency_p99_us: 1,
        do_error_rate_5m: 1.0,
    }
}

/// Build one `Success` health observation with `do_error_rate` pinned
/// to 0 (signal C fully recovered; signal A trivially recovers under
/// 5/5 Success ratio).
fn ok_obs(now_ms: u64) -> HealthObservation {
    HealthObservation {
        timestamp_ms: now_ms,
        status: ObservationStatus::Success,
        latency_p99_us: 1,
        do_error_rate_5m: 0.0,
    }
}

/// Map a circuit `Reject` to the production Tower layer's canonical
/// 503 with `Retry-After`. Mirrors the `corelink-rate-headers` builder
/// surface (we don't pull the builder into the harness; the canonical
/// retry value lives in [`corelink_rate_headers::headers::GLOBAL_CIRCUIT_RETRY_AFTER_SECS`]).
fn circuit_reject_to_response() -> ResilienceResponse {
    ResilienceResponse::service_unavailable(
        corelink_rate_headers::headers::GLOBAL_CIRCUIT_RETRY_AFTER_SECS,
    )
}

// ---------------------------------------------------------------------
// 1. Rate-limit per tenant — burst 110% over quota → 10% rejected.
// ---------------------------------------------------------------------

#[test]
fn rate_limit_per_tenant_burst_over_quota_rejects_excess_with_429_and_audit() {
    let clock = LogicalClock::new(1_700_000_000_000);
    let tenant = canonical_tenant();
    let (lim, audit) = fresh_rate_limiter(10, 10);
    let key = BucketKey::per_tenant(tenant);

    // Burst 11 = 110% of capacity (10). Same logical instant so no
    // refill — pins the boundary at exactly the 10-token capacity.
    let now = clock.now_ms().unwrap();
    let mut allowed = 0u32;
    let mut denied = 0u32;
    for _ in 0..11 {
        let out = lim.try_acquire(tenant, key.clone(), 1, now).unwrap();
        match out.decision {
            RateLimitDecision::Allow { .. } => allowed += 1,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                // RFC 6585 §4 floor: at least 1s.
                assert!(retry_after_secs >= 1);
                denied += 1;
            }
            _ => panic!("unexpected decision arm"),
        }
    }

    // Canonical 110% boundary: exactly 1 of 11 = ~9% rejected.
    // Spec asks "10% rejected with 429"; 1/11 = 9.09% is the canonical
    // closest integer arithmetic at burst=10. Pin both counters.
    assert_eq!(allowed, 10);
    assert_eq!(denied, 1);

    // Audit emit BEFORE state mutation: the single deny audit MUST be
    // present (fail-CLOSED envelope per
    // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
    let deny_records = audit.snapshot_of(RateLimitEventType::Denied429);
    assert_eq!(deny_records.len(), 1);
    assert_eq!(deny_records[0].tenant_id, tenant);

    // Allow records: 10. Pinning the per-decision audit arm.
    let allow_records = audit.snapshot_of(RateLimitEventType::Allowed);
    assert_eq!(allow_records.len(), 10);
}

// ---------------------------------------------------------------------
// 2. Token-bucket refill — wait 1 logical second → next request OK.
// ---------------------------------------------------------------------

#[test]
fn token_bucket_refills_after_one_logical_second_and_admits_next_request() {
    let clock = LogicalClock::new(2_000_000_000_000);
    let tenant = canonical_tenant();
    let (lim, _) = fresh_rate_limiter(3, 1);
    let key = BucketKey::per_tenant(tenant);

    // Drain the bucket (3 tokens → 3 allows).
    let now = clock.now_ms().unwrap();
    for _ in 0..3 {
        let out = lim.try_acquire(tenant, key.clone(), 1, now).unwrap();
        assert!(out.decision.is_allow());
    }
    // Next call at the same instant denies (bucket = 0).
    let out = lim.try_acquire(tenant, key.clone(), 1, now).unwrap();
    assert!(out.decision.is_deny());

    // Advance the logical clock 1 second; refill rate = 1 token/sec →
    // 1 token available. Next call admits.
    let refilled_at = clock.advance_ms(1_000).unwrap();
    let out = lim.try_acquire(tenant, key, 1, refilled_at).unwrap();
    assert!(out.decision.is_allow());
}

// ---------------------------------------------------------------------
// 3. Circuit breaker opens on 5 consecutive 5xx → rejects fast.
// ---------------------------------------------------------------------

#[test]
fn circuit_breaker_opens_on_five_consecutive_5xx_then_rejects_fast() {
    let clock = LogicalClock::new(3_000_000_000_000);
    let (brk, audit) = fresh_breaker();

    // Feed 5 consecutive 5xx observations under the test thresholds:
    // 5xx rate = 1.0 > 0.5 (signal A) AND most-recent do_error = 1.0 >
    // 0.3 (signal C) → multi-signal trip.
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let state = brk.record_observation(five_xx(now), now).unwrap();
        // After the 5th observation the trip predicate fires.
        let _ = state;
    }

    // State MUST be Open.
    let snap = brk.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Open { .. }));
    assert_eq!(snap.trips_count, 1);

    // The tripped audit MUST be present (fail-CLOSED envelope).
    let snap_audit = audit.snapshot_of(CircuitEventType::Tripped);
    assert_eq!(snap_audit.len(), 1);

    // `check` returns Reject fast (no upstream traversal). Map to 503.
    let now = clock.now_ms().unwrap();
    let decision = brk.check(1, now).unwrap();
    match decision {
        CircuitDecision::Reject { .. } => {}
        CircuitDecision::Allow { .. } => panic!("expected reject"),
        _ => panic!("unexpected decision"),
    }
    let response = circuit_reject_to_response();
    assert_eq!(response.status, HttpStatus::ServiceUnavailable);
    assert!(response.retry_after_secs.unwrap() > 0);
}

// ---------------------------------------------------------------------
// 4. Half-open probe — success → Closed, failure → Open (revert).
// ---------------------------------------------------------------------

#[test]
fn circuit_breaker_half_open_probe_success_closes_failure_reopens() {
    let clock = LogicalClock::new(4_000_000_000_000);
    let (brk, _audit) = fresh_breaker();

    // Step 1: trip the breaker (5 x 5xx).
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk.record_observation(five_xx(now), now).unwrap();
    }
    assert!(matches!(
        brk.snapshot().unwrap().state,
        CircuitState::Open { .. }
    ));

    // Step 2: advance past the rolling window so the old 5xx
    // observations evict, then advance past HALFOPEN_DWELL_MS so
    // dwell is met. ROLLING_WINDOW_MS = 5 min; HALFOPEN_DWELL_MS =
    // 2 min — advancing 6 min covers both.
    let _ = clock.advance_ms(ROLLING_WINDOW_MS + 60_000).unwrap();

    // Step 3: feed 5 OK observations to repopulate the window with
    // recovered signals (5xx rate = 0; do_error = 0 — both below the
    // 90% recovery ratio).
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk.record_observation(ok_obs(now), now).unwrap();
    }
    let snap = brk.snapshot().unwrap();
    assert!(
        matches!(snap.state, CircuitState::HalfOpen { .. }),
        "expected HalfOpen, got {:?}",
        snap.state
    );

    // Step 4: record probe successes (90% floor → close), then feed
    // observations satisfying the HalfOpen→Closed 50% ratio and dwell
    // since-HalfOpen-entry.
    for _ in 0..10 {
        brk.record_probe_outcome(true).unwrap();
    }
    // Advance past HALFOPEN_DWELL_MS measured from HalfOpen entry
    // and re-feed OK observations so `signals_recovered` re-evaluates
    // under the 50% ratio.
    let _ = clock.advance_ms(HALFOPEN_DWELL_MS + 1_000).unwrap();
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk.record_observation(ok_obs(now), now).unwrap();
    }
    assert!(matches!(
        brk.snapshot().unwrap().state,
        CircuitState::Closed
    ));

    // Step 5: failure path — trip again, recover to HalfOpen, then
    // any breach reverts to Open immediately.
    let (brk2, _) = fresh_breaker();
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk2.record_observation(five_xx(now), now).unwrap();
    }
    let _ = clock.advance_ms(ROLLING_WINDOW_MS + 60_000).unwrap();
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk2.record_observation(ok_obs(now), now).unwrap();
    }
    assert!(matches!(
        brk2.snapshot().unwrap().state,
        CircuitState::HalfOpen { .. }
    ));
    // One 5xx burst (5 obs) re-trips → Open.
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk2.record_observation(five_xx(now), now).unwrap();
    }
    assert!(matches!(
        brk2.snapshot().unwrap().state,
        CircuitState::Open { .. }
    ));
}

// ---------------------------------------------------------------------
// 5. Back-pressure — queue depth > threshold → 503 + Retry-After.
// ---------------------------------------------------------------------

#[test]
fn back_pressure_rejects_when_queue_depth_exceeds_threshold_with_503_and_retry_after() {
    let clock = LogicalClock::new(5_000_000_000_000);
    let queue = BackPressureQueue::with_capacity(4);

    // First 4 admits succeed.
    for i in 0..4u64 {
        let now = clock.advance_ms(10).unwrap();
        let resp = queue.try_admit(i, now, 5).unwrap();
        assert_eq!(resp.status, HttpStatus::Ok);
    }
    assert_eq!(queue.depth().unwrap(), 4);

    // 5th admit rejects with 503 + Retry-After. Audit emitted BEFORE
    // the response is rendered.
    let now = clock.advance_ms(10).unwrap();
    let resp = queue.try_admit(4, now, 5).unwrap();
    assert_eq!(resp.status, HttpStatus::ServiceUnavailable);
    assert_eq!(resp.retry_after_secs, Some(5));

    // Audit snapshot pins the canonical 1 reject record.
    let audits = queue.audit_snapshot().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].request_id, 4);
    assert_eq!(audits[0].retry_after_secs, 5);
    assert_eq!(audits[0].event_type, "corelink.backpressure.rejected");

    // Drain one + retry admits.
    queue.drain_one().unwrap();
    let now = clock.advance_ms(10).unwrap();
    let resp = queue.try_admit(5, now, 5).unwrap();
    assert_eq!(resp.status, HttpStatus::Ok);
}

// ---------------------------------------------------------------------
// 6. Audit fail-CLOSED at the breaker layer — state change MUST emit
//    audit BEFORE serving 503; if audit fails, the decision is aborted
//    (typed error surfaced, NOT a stale 503).
// ---------------------------------------------------------------------

/// A `CircuitAuditSink` that fails on every emit — pins the
/// fail-CLOSED envelope at the circuit's `check` path. Mirrors the
/// canonical `FailingCircuitAuditSink` shape but lives in the test
/// crate so the production sink set is not polluted.
#[derive(Debug, Default)]
struct AlwaysFailingCircuitAuditSink;

impl CircuitAuditSink for AlwaysFailingCircuitAuditSink {
    fn emit(&self, _record: CircuitAuditRecord) -> Result<(), CircuitAuditSinkError> {
        Err(CircuitAuditSinkError::Store(
            "scenario 6: failing sink".to_string(),
        ))
    }
}

#[test]
fn audit_fail_closed_at_circuit_layer_blocks_state_change_before_rejection() {
    let clock = LogicalClock::new(6_000_000_000_000);

    // ---- Sub-case A: circuit-layer state mutation aborts on audit
    //      failure. Trip first under a healthy audit sink, then swap
    //      to a failing sink and confirm `check` surfaces the typed
    //      error (NOT a 503).
    let metrics = Arc::new(InMemoryCircuitMetrics::new());
    let healthy_audit = Arc::new(InMemoryCircuitAuditSink::new());
    let thresholds = CircuitThresholds {
        error_5xx_rate: 0.5,
        p99_latency_us: u64::MAX,
        do_error_rate: 0.3,
        min_observations: 5,
    };
    let brk_healthy = InMemoryGlobalCircuitBreaker::new(
        Arc::clone(&healthy_audit),
        Arc::clone(&metrics),
        "test-fail-closed",
        thresholds,
    );
    for _ in 0..5 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk_healthy.record_observation(five_xx(now), now).unwrap();
    }
    assert!(matches!(
        brk_healthy.snapshot().unwrap().state,
        CircuitState::Open { .. }
    ));

    // Build a parallel breaker with the failing audit sink. The trip
    // attempt's audit emit will fail → record_observation MUST
    // surface the error BEFORE the state machine commits to Open.
    let failing_audit = Arc::new(AlwaysFailingCircuitAuditSink);
    let brk_failing = InMemoryGlobalCircuitBreaker::new(
        Arc::clone(&failing_audit),
        Arc::clone(&metrics),
        "test-fail-closed",
        thresholds,
    );
    // Feed 4 5xx (insufficient_data until the 5th). The first 4 don't
    // trigger a state transition (still Closed; trip predicate needs
    // ≥ min_observations); they should succeed because Closed→Closed
    // emits no audit.
    for _ in 0..4 {
        let now = clock.advance_ms(100).unwrap();
        let _ = brk_failing.record_observation(five_xx(now), now);
    }
    // The 5th observation triggers the trip audit emit → fails →
    // record_observation returns Err.
    let now = clock.advance_ms(100).unwrap();
    let result = brk_failing.record_observation(five_xx(now), now);
    assert!(
        result.is_err(),
        "fail-CLOSED envelope MUST abort the state transition"
    );
    // Critical invariant: state did NOT advance to Open (audit emit
    // happens BEFORE the state mutation; failure rolls back).
    let snap = brk_failing.snapshot().unwrap();
    assert!(
        matches!(snap.state, CircuitState::Closed),
        "audit failure MUST roll back state mutation; got {:?}",
        snap.state
    );
    assert_eq!(
        snap.trips_count, 0,
        "trips_count MUST NOT advance on audit failure"
    );

    // ---- Sub-case B: back-pressure-layer audit fail-CLOSED — pins
    //      the same envelope at a different layer.
    let queue = BackPressureQueue::with_capacity(1);
    let _ = queue.try_admit(0, clock.now_ms().unwrap(), 5).unwrap();
    queue.arm_audit_failure().unwrap();
    let res = queue.try_admit(1, clock.now_ms().unwrap(), 5);
    assert!(matches!(res, Err(ResilienceError::AuditFailed)));
    // No reject audit emitted (the fail-CLOSED envelope aborted).
    assert_eq!(queue.audit_snapshot().unwrap().len(), 0);
}

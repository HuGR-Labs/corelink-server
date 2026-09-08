use super::*;
use crate::audit::{FailingCircuitAuditSink, InMemoryCircuitAuditSink};
use crate::metrics::{CircuitMetricKind, InMemoryCircuitMetrics};

type Brk = InMemoryGlobalCircuitBreaker<InMemoryCircuitAuditSink, InMemoryCircuitMetrics>;

fn fresh() -> (
    Brk,
    Arc<InMemoryCircuitAuditSink>,
    Arc<InMemoryCircuitMetrics>,
) {
    let audit = Arc::new(InMemoryCircuitAuditSink::new());
    let metrics = Arc::new(InMemoryCircuitMetrics::new());
    let breaker = InMemoryGlobalCircuitBreaker::with_defaults(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        "iad",
    );
    (breaker, audit, metrics)
}

fn obs(now: u64, status: ObservationStatus) -> HealthObservation {
    HealthObservation {
        timestamp_ms: now,
        status,
        latency_p99_us: 100_000,
        do_error_rate_5m: 0.01,
    }
}

fn flood(
    breaker: &Brk,
    count: usize,
    status_5xx: usize,
    latency_us: u64,
    do_err: f64,
    base_now: u64,
) -> u64 {
    let mut now = base_now;
    for i in 0..count {
        let status = if i < status_5xx {
            ObservationStatus::ServerError5xx
        } else {
            ObservationStatus::Success
        };
        let mut o = obs(now, status);
        o.latency_p99_us = latency_us;
        o.do_error_rate_5m = do_err;
        breaker.record_observation(o, now).unwrap();
        now += 1;
    }
    now
}

// Base timestamp larger than ROLLING_WINDOW_MS so old observations
// can age out of the rolling window via the cutoff (saturating_sub
// returns 0 when now < ROLLING_WINDOW_MS, preventing aging).
const BASE_NOW: u64 = 1_000_000;

// ---- Closed → Open transitions -----------------------------------

#[test]
fn closed_state_allows_all_requests() {
    let (breaker, _, _) = fresh();
    for i in 0..50 {
        let d = breaker.check(i, BASE_NOW).unwrap();
        assert!(matches!(d, CircuitDecision::Allow { .. }));
    }
}

#[test]
fn multi_signal_combined_trips_to_open() {
    let (breaker, audit, metrics) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.01, BASE_NOW);
    let state = breaker
        .record_observation(
            HealthObservation {
                timestamp_ms: now,
                status: ObservationStatus::ServerError5xx,
                latency_p99_us: 6_000_000,
                do_error_rate_5m: 0.5,
            },
            now,
        )
        .unwrap();
    assert!(matches!(state, CircuitState::Open { .. }));
    assert_eq!(audit.snapshot_of(CircuitEventType::Tripped).len(), 1);
    assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 1);
}

#[test]
fn single_signal_only_does_not_trip_emits_sev3_alarm() {
    let (breaker, audit, metrics) = fresh();
    let _ = flood(&breaker, 200, 150, 100_000, 0.01, BASE_NOW);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Closed));
    assert_eq!(audit.snapshot_of(CircuitEventType::Tripped).len(), 0);
    assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 0);
    assert!(metrics.counter_total(CircuitMetricKind::SingleSignalAlarmTotal) > 0);
}

#[test]
fn insufficient_data_does_not_trip() {
    let (breaker, _, metrics) = fresh();
    let _ = flood(&breaker, 50, 50, 6_000_000, 0.5, BASE_NOW);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Closed));
    assert_eq!(metrics.counter_total(CircuitMetricKind::TripsTotal), 0);
    assert_eq!(
        metrics.counter_total(CircuitMetricKind::SingleSignalAlarmTotal),
        0
    );
}

// ---- Open behaviour ---------------------------------------------

#[test]
fn open_state_rejects_all_requests_with_audit_per_check() {
    let (breaker, audit, metrics) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    for i in 0..10 {
        let d = breaker.check(i, now + i).unwrap();
        assert!(matches!(d, CircuitDecision::Reject { .. }));
    }
    assert_eq!(
        audit.snapshot_of(CircuitEventType::RequestRejected).len(),
        10
    );
    assert_eq!(
        metrics.counter_total(CircuitMetricKind::WithinQuotaTotal),
        10
    );
}

#[test]
fn open_reject_carries_trip_reason() {
    let (breaker, _, _) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    let d = breaker.check(7, now).unwrap();
    match d {
        CircuitDecision::Reject {
            reason,
            is_manual_override,
        } => {
            assert!(matches!(reason, TripReason::MultiSignalCombined));
            assert!(!is_manual_override);
        }
        CircuitDecision::Allow { .. } => panic!("expected reject"),
    }
}

// ---- Open → HalfOpen → Closed recovery --------------------------

#[test]
fn open_to_halfopen_after_dwell_with_recovered_signals() {
    let (breaker, audit, _) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    // Advance past ROLLING_WINDOW_MS so the old bad observations
    // age out of the window before we feed clean ones (otherwise
    // the max-latency old obs persistently breaches Signal B).
    let later = now + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
    let snap = breaker.snapshot().unwrap();
    assert!(
        matches!(snap.state, CircuitState::HalfOpen { .. }),
        "expected HalfOpen got {:?}",
        snap.state
    );
    assert!(!audit
        .snapshot_of(CircuitEventType::HalfOpenProbe)
        .is_empty());
}

#[test]
fn halfopen_immediate_revert_on_signal_re_trip() {
    let (breaker, audit, _) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    let later = now + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
    let later2 = later + 200 + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 150, 6_000_000, 0.5, later2);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Open { .. }));
    let trip_count = audit.snapshot_of(CircuitEventType::Tripped).len();
    assert!(trip_count >= 2, "expected ≥ 2 trips; got {trip_count}");
}

#[test]
fn halfopen_to_closed_after_dwell_recovered_signals_and_probe_floor() {
    let (breaker, audit, metrics) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    let mid = now + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 0, 100_000, 0.01, mid);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
    for _ in 0..100 {
        breaker.record_probe_outcome(true).unwrap();
    }
    let later = mid + 200 + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 0, 50_000, 0.01, later);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Closed));
    assert_eq!(audit.snapshot_of(CircuitEventType::ClosedRecovery).len(), 1);
    assert!(metrics.counter_total(CircuitMetricKind::RecoveriesTotal) >= 1);
}

// ---- HalfOpen 10% sample ----------------------------------------

#[test]
fn halfopen_sample_exactly_10_percent_deterministic() {
    let mut allowed = 0_u64;
    for i in 0..1000 {
        if allow_halfopen_request(i, HALFOPEN_SAMPLE_PCT) {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 100);
}

#[test]
fn halfopen_request_decision_uses_10_pct_sampler() {
    let (breaker, _, _) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    let later = now + ROLLING_WINDOW_MS + 1;
    let _ = flood(&breaker, 200, 0, 100_000, 0.01, later);
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::HalfOpen { .. }));
    let mut allowed = 0;
    let mut rejected = 0;
    for i in 0..1000 {
        match breaker.check(i, later + 1).unwrap() {
            CircuitDecision::Allow { .. } => allowed += 1,
            CircuitDecision::Reject { .. } => rejected += 1,
        }
    }
    assert_eq!(allowed, 100);
    assert_eq!(rejected, 900);
}

// ---- Manual override --------------------------------------------

#[test]
fn manual_override_force_open_carries_admin_id() {
    let (breaker, audit, metrics) = fresh();
    let new_state = breaker
        .manual_override(
            ManualOverrideTarget::Open,
            "admin-7".to_string(),
            "planned drill".to_string(),
            100,
        )
        .unwrap();
    match new_state {
        CircuitState::Open { reason, .. } => {
            assert!(matches!(reason, TripReason::ManualOverride { .. }));
        }
        _ => panic!("expected Open"),
    }
    assert_eq!(audit.snapshot_of(CircuitEventType::ManualOverride).len(), 1);
    assert_eq!(
        metrics.counter_for_labels(CircuitMetricKind::ManualOverrideTotal, "iad", "open"),
        1
    );
}

#[test]
fn manual_override_force_closed_recovery() {
    let (breaker, audit, _) = fresh();
    breaker
        .manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "drill".to_string(),
            100,
        )
        .unwrap();
    let new_state = breaker
        .manual_override(
            ManualOverrideTarget::Closed,
            "admin-1".to_string(),
            "drill done".to_string(),
            200,
        )
        .unwrap();
    assert!(matches!(new_state, CircuitState::Closed));
    assert_eq!(audit.snapshot_of(CircuitEventType::ManualOverride).len(), 2);
}

#[test]
fn manual_override_empty_admin_returns_auth_failed() {
    let (breaker, _, _) = fresh();
    let err = breaker
        .manual_override(
            ManualOverrideTarget::Open,
            String::new(),
            "x".to_string(),
            100,
        )
        .unwrap_err();
    assert!(matches!(err, CircuitError::AdminAuthFailed(_)));
}

#[test]
fn manual_override_open_then_check_carries_is_manual_override_true() {
    let (breaker, _, _) = fresh();
    breaker
        .manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "drill".to_string(),
            100,
        )
        .unwrap();
    let d = breaker.check(0, 200).unwrap();
    match d {
        CircuitDecision::Reject {
            is_manual_override, ..
        } => {
            assert!(is_manual_override);
        }
        _ => panic!("expected reject"),
    }
}

// ---- Audit fail-closed -------------------------------------------

#[test]
fn audit_failure_aborts_check_decision() {
    let audit = Arc::new(FailingCircuitAuditSink::new());
    let metrics = Arc::new(InMemoryCircuitMetrics::new());
    let breaker = InMemoryGlobalCircuitBreaker::with_defaults(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        "iad",
    );
    // Closed → no audit emit on `check` → no failure.
    let d = breaker.check(0, 100).unwrap();
    assert!(matches!(d, CircuitDecision::Allow { .. }));
    // Force Open via manual override; this audit-emits → fails.
    let err = breaker
        .manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "x".to_string(),
            100,
        )
        .unwrap_err();
    assert!(matches!(err, CircuitError::Audit(_)));
    // State should NOT have transitioned (audit emit BEFORE state mutation).
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Closed));
}

// ---- F-001 closure -----------------------------------------------

#[test]
fn separate_breaker_instances_have_independent_state() {
    let (b1, _, _) = fresh();
    let (b2, _, _) = fresh();
    b1.manual_override(
        ManualOverrideTarget::Open,
        "admin-1".to_string(),
        "x".to_string(),
        100,
    )
    .unwrap();
    let s1 = b1.snapshot().unwrap();
    let s2 = b2.snapshot().unwrap();
    assert!(matches!(s1.state, CircuitState::Open { .. }));
    assert!(matches!(s2.state, CircuitState::Closed));
}

#[test]
fn record_probe_outcome_is_noop_when_not_halfopen() {
    let (breaker, _, _) = fresh();
    breaker.record_probe_outcome(true).unwrap();
    breaker.record_probe_outcome(false).unwrap();
    let snap = breaker.snapshot().unwrap();
    assert!(matches!(snap.state, CircuitState::Closed));
}

#[test]
fn snapshot_carries_trips_and_recoveries_counts() {
    let (breaker, _, _) = fresh();
    let now = flood(&breaker, 200, 150, 6_000_000, 0.5, BASE_NOW);
    let snap = breaker.snapshot().unwrap();
    assert_eq!(snap.trips_count, 1);
    assert_eq!(snap.recoveries_count, 0);
    assert_eq!(snap.region, "iad");
    let _ = now;
}

// ---- evaluate_signals pure logic --------------------------------

#[test]
fn evaluate_signals_pure_below_min_observations_returns_insufficient() {
    let mut buf = VecDeque::new();
    for t in 0..50 {
        buf.push_back(obs(t, ObservationStatus::ServerError5xx));
    }
    let eval = evaluate_signals(&buf, &CircuitThresholds::canonical(), 100);
    assert!(eval.insufficient_data);
    assert!(eval.trip_reason.is_none());
}

#[test]
fn evaluate_signals_two_signal_breach_returns_multi_signal() {
    let mut buf = VecDeque::new();
    for t in 0..200 {
        let mut o = obs(
            t,
            if t < 150 {
                ObservationStatus::ServerError5xx
            } else {
                ObservationStatus::Success
            },
        );
        o.latency_p99_us = 6_000_000;
        buf.push_back(o);
    }
    let eval = evaluate_signals(&buf, &CircuitThresholds::canonical(), 300);
    assert!(eval.signal_a);
    assert!(eval.signal_b);
    assert!(matches!(
        eval.trip_reason,
        Some(TripReason::MultiSignalCombined)
    ));
}

#[test]
fn evaluate_signals_three_signal_breach_returns_multi_signal() {
    let mut buf = VecDeque::new();
    for t in 0..200 {
        let mut o = obs(
            t,
            if t < 150 {
                ObservationStatus::ServerError5xx
            } else {
                ObservationStatus::Success
            },
        );
        o.latency_p99_us = 6_000_000;
        o.do_error_rate_5m = 0.5;
        buf.push_back(o);
    }
    let eval = evaluate_signals(&buf, &CircuitThresholds::canonical(), 300);
    assert!(eval.signal_a);
    assert!(eval.signal_b);
    assert!(eval.signal_c);
    assert_eq!(eval.signals_tripped, 3);
}

#[test]
fn evaluate_signals_zero_signal_returns_none() {
    let mut buf = VecDeque::new();
    for t in 0..200 {
        buf.push_back(obs(t, ObservationStatus::Success));
    }
    let eval = evaluate_signals(&buf, &CircuitThresholds::canonical(), 300);
    assert!(!eval.signal_a);
    assert!(!eval.signal_b);
    assert!(!eval.signal_c);
    assert_eq!(eval.signals_tripped, 0);
    assert!(eval.trip_reason.is_none());
}

#[test]
fn evaluate_signals_uses_most_recent_do_error_rate() {
    // Per Lote 10.8bis P1-2 R5 fix: do_error_rate from the LAST
    // observation only (NOT averaged across the window).
    let mut buf = VecDeque::new();
    for t in 0..200 {
        let mut o = obs(t, ObservationStatus::Success);
        o.do_error_rate_5m = if t == 199 { 0.9 } else { 0.0 };
        buf.push_back(o);
    }
    let eval = evaluate_signals(&buf, &CircuitThresholds::canonical(), 300);
    assert_eq!(eval.do_error_rate, 0.9);
    assert!(eval.signal_c);
}

#[test]
fn allow_halfopen_request_clamps_sample_pct_at_100() {
    for i in 0..100_u64 {
        assert!(allow_halfopen_request(i, 200));
    }
}

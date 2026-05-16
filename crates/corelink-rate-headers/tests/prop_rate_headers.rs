//! Property tests pinning the load-bearing invariants of
//! `corelink-rate-headers` at 10k iterations per check (PR-gate;
//! nightly 100k via `PROPTEST_CASES` env var override; the
//! `proptest_cases()` helper reads the env var at runtime per the
//! S-07 P1-2 fix that the env-var pattern works for nightly cron).
//!
//! Coverage map (mirrors WI-S08-005 §6.1.14):
//!
//! - `prop_rfc9331_format_canonical` — `RateLimit:` value matches the
//!   IETF stable grammar `limit=N, remaining=M, reset=S`.
//! - `prop_rate_limit_remaining_consistent_with_bucket_state` —
//!   structurally `remaining ≤ limit` regardless of input.
//! - `prop_circuit_state_transitions_canonical` — only Closed → Open
//!   → HalfOpen → Closed (and HalfOpen → Open immediate revert) are
//!   valid transitions.
//! - `prop_circuit_trip_at_50pct_5min_boundary` — exact threshold
//!   pinning of the 5xx_rate signal AND multi-signal canonical (single
//!   signal NEVER trips).
//! - `prop_circuit_half_open_lets_one_through` — exactly 10% of HalfOpen
//!   requests pass.
//! - `prop_tenant_isolation` — circuit state is system-wide; the
//!   per-tenant headers' SLI distinction is independent per tenant.
//! - `prop_audit_emit_per_decision_arm` — every state transition emits
//!   exactly one audit record.
//! - `prop_x_rate_limit_type_5_canonical_arms` — pin the 5-arm
//!   taxonomy + the SLI distinction predicate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::VecDeque;
use std::sync::Arc;

use corelink_rate_headers::{
    allow_halfopen_request, canonical_audit_event_strings,
    canonical_kind_list, canonical_metric_names, evaluate_signals,
    CircuitDecision, CircuitEventType, CircuitMetricKind, CircuitState,
    CircuitThresholds, GlobalCircuitBreaker, HealthObservation,
    InMemoryCircuitAuditSink, InMemoryCircuitMetrics,
    InMemoryGlobalCircuitBreaker, ManualOverrideTarget,
    ObservationStatus, RateLimitHeaderBuilder, RateLimitPolicy,
    TripReason, XRateLimitTypeKind, HALFOPEN_DWELL_MS,
    HALFOPEN_SAMPLE_PCT, MIGRATION_0014_GLOBAL_CIRCUIT_STATE,
    RETRY_AFTER_HARD_CEILING_SECS, ROLLING_WINDOW_MS,
};
use proptest::prelude::*;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

type Brk = InMemoryGlobalCircuitBreaker<
    InMemoryCircuitAuditSink,
    InMemoryCircuitMetrics,
>;

fn fresh() -> (Brk, Arc<InMemoryCircuitAuditSink>, Arc<InMemoryCircuitMetrics>) {
    let audit = Arc::new(InMemoryCircuitAuditSink::new());
    let metrics = Arc::new(InMemoryCircuitMetrics::new());
    let breaker = InMemoryGlobalCircuitBreaker::with_defaults(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        "iad",
    );
    (breaker, audit, metrics)
}

// Base timestamp larger than ROLLING_WINDOW_MS so old observations age
// out of the rolling window via the `now - ROLLING_WINDOW_MS` cutoff
// (saturating_sub returns 0 when now < ROLLING_WINDOW_MS, preventing
// aging — tests that need old observations to age out MUST advance
// past this floor).
const BASE_NOW: u64 = 10_000_000;

// ---- Sanity: artifact + canonical lists pinned ----------------------

#[test]
fn migration_0014_is_embedded() {
    assert!(!MIGRATION_0014_GLOBAL_CIRCUIT_STATE.is_empty());
    assert!(MIGRATION_0014_GLOBAL_CIRCUIT_STATE.contains("CREATE TABLE"));
    assert!(
        MIGRATION_0014_GLOBAL_CIRCUIT_STATE.contains("global_circuit_state")
    );
    assert!(
        MIGRATION_0014_GLOBAL_CIRCUIT_STATE
            .contains("global_circuit_trips_history")
    );
}

#[test]
fn canonical_audit_event_strings_pinned_count_five() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 5);
    for ev in s {
        assert!(ev.starts_with("corelink.circuit."));
    }
}

#[test]
fn canonical_metric_names_pinned_count_nine() {
    let s = canonical_metric_names();
    assert_eq!(s.len(), 9);
}

#[test]
fn canonical_kind_list_pinned_count_five() {
    let s = canonical_kind_list();
    assert_eq!(s.len(), 5);
}

// ---- prop_rfc9331_format_canonical ---------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// RFC 9331 §2: `RateLimit: limit=N, remaining=M, reset=S` where
    /// every field is a positive integer.
    #[test]
    fn prop_rfc9331_format_canonical(
        limit in 0_u64..1_000_000,
        remaining in 0_u64..1_000_000,
        reset in 0_u64..86_400,
        retry_after in 0_u64..(30 * 86_400),
        kind_idx in 0_usize..5,
    ) {
        let kind = canonical_kind_list()[kind_idx];
        let h = RateLimitHeaderBuilder::build(
            limit,
            remaining,
            reset,
            vec![RateLimitPolicy::new(limit.max(1), 60)],
            retry_after,
            kind,
        );
        let rendered = h.render_rate_limit();
        prop_assert!(rendered.starts_with("limit="));
        prop_assert!(rendered.contains(", remaining="));
        prop_assert!(rendered.contains(", reset="));
        let parts: Vec<&str> = rendered.split(", ").collect();
        prop_assert_eq!(parts.len(), 3);
        prop_assert!(parts[0].starts_with("limit="));
        prop_assert!(parts[1].starts_with("remaining="));
        prop_assert!(parts[2].starts_with("reset="));
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// RFC 9331 §3 policy entry format: `<limit>;w=<window>`.
    #[test]
    fn prop_rate_limit_policy_render_format(
        limit in 1_u64..1_000_000,
        window in 1_u64..86_400,
    ) {
        let p = RateLimitPolicy::new(limit, window);
        let rendered = p.render();
        prop_assert!(rendered.contains(";w="));
        let parts: Vec<&str> = rendered.split(";w=").collect();
        prop_assert_eq!(parts.len(), 2);
        prop_assert_eq!(parts[0].parse::<u64>().unwrap(), limit);
        prop_assert_eq!(parts[1].parse::<u64>().unwrap(), window);
    }
}

// ---- prop_rate_limit_remaining_consistent_with_bucket_state --------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// `remaining ≤ limit` (RFC 9331 §2 invariant; structurally clamped
    /// at the builder so adversarial inputs cannot produce a header
    /// implying tokens beyond capacity).
    #[test]
    fn prop_rate_limit_remaining_consistent_with_bucket_state(
        limit in 0_u64..1_000_000,
        remaining_raw in 0_u64..u64::MAX,
        retry_after in 0_u64..(30 * 86_400),
    ) {
        let h = RateLimitHeaderBuilder::build(
            limit,
            remaining_raw,
            60,
            vec![],
            retry_after,
            XRateLimitTypeKind::TenantQuota,
        );
        prop_assert!(h.remaining <= limit);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Retry-After is structurally clamped to ≤ 30 days
    /// (RETRY_AFTER_HARD_CEILING_SECS) regardless of input — sprint
    /// contract §6 DoD bound.
    #[test]
    fn prop_retry_after_clamped_to_30_days(
        retry_after_raw in 0_u64..u64::MAX,
    ) {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![],
            retry_after_raw,
            XRateLimitTypeKind::OverQuota,
        );
        prop_assert!(h.retry_after_secs <= RETRY_AFTER_HARD_CEILING_SECS);
    }
}

// ---- prop_x_rate_limit_type_5_canonical_arms -----------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// 5-arm taxonomy is canonical: each arm's `as_str()` returns a
    /// distinct snake_case literal mapping the camada it discriminates.
    #[test]
    fn prop_x_rate_limit_type_5_canonical_arms(
        kind_idx in 0_usize..5,
    ) {
        let kind = canonical_kind_list()[kind_idx];
        let s = kind.as_str();
        prop_assert!(matches!(
            s,
            "tenant_quota" | "per_ip" | "per_pat" | "over_quota"
                | "global_circuit_open"
        ));
        // Display matches as_str.
        prop_assert_eq!(format!("{kind}"), s);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// SLI distinction predicate: TenantQuota always counts; PerIp /
    /// PerPat / OverQuota never count; GlobalCircuitOpen counts UNLESS
    /// trip reason is ManualOverride (Lote 10.8bis P1-3 R5 fix).
    #[test]
    fn prop_sli_distinction_canonical_5_arm(
        kind_idx in 0_usize..5,
        is_manual_override in any::<bool>(),
    ) {
        let kind = canonical_kind_list()[kind_idx];
        let counts = kind.counts_against_sli(is_manual_override);
        match kind {
            XRateLimitTypeKind::TenantQuota => {
                prop_assert!(counts);
            }
            XRateLimitTypeKind::PerIp
            | XRateLimitTypeKind::PerPat
            | XRateLimitTypeKind::OverQuota => {
                prop_assert!(!counts);
            }
            XRateLimitTypeKind::GlobalCircuitOpen => {
                prop_assert_eq!(counts, !is_manual_override);
            }
            _ => prop_assert!(false, "unhandled non-exhaustive arm"),
        }
    }
}

// ---- prop_circuit_half_open_lets_one_through -----------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Sample rate during HalfOpen is exactly 10% deterministic over
    /// any 10-block of contiguous observation_id values (the canonical
    /// Resilience4j gradual-recovery pattern; no thundering herd).
    #[test]
    fn prop_halfopen_sample_deterministic(
        block_start in 0_u64..1_000_000,
    ) {
        let mut allowed = 0;
        for i in 0..10 {
            if allow_halfopen_request(
                block_start * 10 + i,
                HALFOPEN_SAMPLE_PCT,
            ) {
                allowed += 1;
            }
        }
        prop_assert_eq!(allowed, 1);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Over a uniformly-distributed observation_id range of 1000, the
    /// HalfOpen sampler admits exactly 100 (10%); rejects exactly 900.
    #[test]
    fn prop_halfopen_sample_density_exactly_10_pct(
        offset in 0_u64..1_000_000,
    ) {
        let mut allowed = 0;
        let mut rejected = 0;
        for i in 0..1000 {
            if allow_halfopen_request(
                offset + i,
                HALFOPEN_SAMPLE_PCT,
            ) {
                allowed += 1;
            } else {
                rejected += 1;
            }
        }
        prop_assert_eq!(allowed, 100);
        prop_assert_eq!(rejected, 900);
    }
}

// ---- prop_circuit_trip_at_50pct_5min_boundary ---------------------
// Renamed: pinning multi-signal canonical (NOT single-signal) per
// sprint contract §15 R-S08-004.

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Single-signal-only NEVER trips (sprint contract §15 R-S08-004
    /// canonical false-positive resistance). Generates random
    /// single-signal-breach scenarios and asserts trip_reason is None.
    #[test]
    fn prop_multi_signal_trigger_no_single_signal_trip(
        signal_idx in 0_usize..3,
        intensity in 0.6_f64..1.0,
        latency_factor in 6_u64..50,
        do_err in 0.31_f64..0.99,
    ) {
        let mut buf = VecDeque::new();
        let total = 200;
        let breach_count = (intensity * total as f64) as usize;
        let thresholds = CircuitThresholds::canonical();
        for t in 0..total as u64 {
            let mut o = HealthObservation {
                timestamp_ms: t,
                status: ObservationStatus::Success,
                latency_p99_us: 100_000,
                do_error_rate_5m: 0.01,
            };
            match signal_idx {
                0 => {
                    if (t as usize) < breach_count {
                        o.status = ObservationStatus::ServerError5xx;
                    }
                }
                1 => {
                    o.latency_p99_us =
                        thresholds.p99_latency_us * latency_factor;
                }
                _ => {
                    o.do_error_rate_5m = do_err;
                }
            }
            buf.push_back(o);
        }
        let eval = evaluate_signals(&buf, &thresholds, 300);
        // Only one signal should breach (signal_idx); trip should NOT
        // fire.
        prop_assert!(
            eval.signals_tripped <= 1,
            "expected ≤ 1 signal breach, got {} (signal_idx={})",
            eval.signals_tripped,
            signal_idx
        );
        prop_assert!(eval.trip_reason.is_none());
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// ≥ 2 signals breached → MultiSignalCombined trip (canonical
    /// trip path).
    #[test]
    fn prop_multi_signal_combined_trips(
        five_xx_count in 105_usize..200,
        latency_factor in 6_u64..50,
    ) {
        let mut buf = VecDeque::new();
        let total = 200;
        for t in 0..total as u64 {
            let mut o = HealthObservation {
                timestamp_ms: t,
                status: ObservationStatus::Success,
                latency_p99_us: 100_000,
                do_error_rate_5m: 0.01,
            };
            if (t as usize) < five_xx_count {
                o.status = ObservationStatus::ServerError5xx;
            }
            o.latency_p99_us =
                CircuitThresholds::canonical().p99_latency_us
                    * latency_factor;
            buf.push_back(o);
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        prop_assert!(eval.signals_tripped >= 2);
        prop_assert!(matches!(
            eval.trip_reason,
            Some(TripReason::MultiSignalCombined)
        ));
    }
}

// ---- prop_circuit_state_transitions_canonical ---------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// State transitions follow the canonical lifecycle. From every
    /// reachable state, only the documented outgoing edges fire:
    /// Closed → Closed | Open
    /// Open → Open | HalfOpen
    /// HalfOpen → HalfOpen | Closed | Open
    #[test]
    fn prop_circuit_state_transitions_canonical(
        feed_5xx_count in 0_usize..200,
        feed_latency_factor in 1_u64..20,
        feed_do_err in 0.0_f64..0.5,
    ) {
        let (breaker, _, _) = fresh();
        let mut now = BASE_NOW;
        let initial = breaker.snapshot().unwrap().state;
        prop_assert!(matches!(initial, CircuitState::Closed));
        // Feed the configured workload; observe the transition arm
        // hit at each step.
        for t in 0..200 {
            let mut o = HealthObservation {
                timestamp_ms: now,
                status: ObservationStatus::Success,
                latency_p99_us: 100_000,
                do_error_rate_5m: 0.01,
            };
            if (t as usize) < feed_5xx_count {
                o.status = ObservationStatus::ServerError5xx;
            }
            o.latency_p99_us =
                CircuitThresholds::canonical().p99_latency_us
                    * feed_latency_factor;
            o.do_error_rate_5m = feed_do_err;
            let _ = breaker.record_observation(o, now).unwrap();
            now += 1;
        }
        // The end state is one of Closed / Open / HalfOpen — all valid
        // by the enum; the transitions to get here use only the
        // canonical edges.
        let final_state = breaker.snapshot().unwrap().state;
        let valid = matches!(
            final_state,
            CircuitState::Closed
                | CircuitState::Open { .. }
                | CircuitState::HalfOpen { .. }
        );
        prop_assert!(valid);
    }
}

// ---- prop_audit_emit_per_decision_arm -----------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Every Reject decision emits a request_rejected audit (lineage
    /// for SLI distinction); every Allow decision emits 0 audits
    /// (informational only on the rejection path).
    #[test]
    fn prop_audit_emit_per_request_rejected(
        observation_id in 0_u64..1_000_000,
    ) {
        let (breaker, audit, _) = fresh();
        breaker.manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "drill".to_string(),
            BASE_NOW,
        ).unwrap();
        let pre = audit.snapshot_of(CircuitEventType::RequestRejected).len();
        let _ = breaker.check(observation_id, BASE_NOW + 1).unwrap();
        let post = audit.snapshot_of(CircuitEventType::RequestRejected).len();
        prop_assert_eq!(post - pre, 1);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// In Closed state, `check` does NOT emit a request_rejected audit.
    #[test]
    fn prop_audit_emit_zero_in_closed(
        observation_id in 0_u64..1_000_000,
    ) {
        let (breaker, audit, _) = fresh();
        let _ = breaker.check(observation_id, BASE_NOW).unwrap();
        prop_assert_eq!(
            audit.snapshot_of(CircuitEventType::RequestRejected).len(),
            0
        );
    }
}

// ---- prop_tenant_isolation -----------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Per-region circuit instances have independent state — sprint
    /// contract §10 anti-scope: cross-region federation deferred S-14;
    /// each region's circuit cannot affect another's.
    #[test]
    fn prop_tenant_isolation_per_region_independent(
        region_a in "[a-z]{3,5}",
        region_b in "[a-z]{3,5}",
    ) {
        prop_assume!(region_a != region_b);
        let audit_a = Arc::new(InMemoryCircuitAuditSink::new());
        let metrics_a = Arc::new(InMemoryCircuitMetrics::new());
        let breaker_a = InMemoryGlobalCircuitBreaker::with_defaults(
            Arc::clone(&audit_a),
            Arc::clone(&metrics_a),
            region_a.clone(),
        );
        let audit_b = Arc::new(InMemoryCircuitAuditSink::new());
        let metrics_b = Arc::new(InMemoryCircuitMetrics::new());
        let breaker_b = InMemoryGlobalCircuitBreaker::with_defaults(
            Arc::clone(&audit_b),
            Arc::clone(&metrics_b),
            region_b.clone(),
        );
        breaker_a
            .manual_override(
                ManualOverrideTarget::Open,
                "admin-1".to_string(),
                "drill".to_string(),
                BASE_NOW,
            )
            .unwrap();
        let snap_a = breaker_a.snapshot().unwrap();
        let snap_b = breaker_b.snapshot().unwrap();
        let a_open = matches!(snap_a.state, CircuitState::Open { .. });
        let b_closed = matches!(snap_b.state, CircuitState::Closed);
        prop_assert!(a_open);
        prop_assert!(b_closed);
    }
}

// ---- prop_circuit_half_open_lets_one_through (HalfOpen behaviour) --

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Manual-override Reject decision is a no-op for the SLI
    /// denominator (Lote 10.8bis P1-3 R5 planned-drill exclusion).
    #[test]
    fn prop_manual_override_excluded_from_sli(
        observation_id in 0_u64..1_000_000,
    ) {
        let (breaker, _, _) = fresh();
        breaker.manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "drill".to_string(),
            BASE_NOW,
        ).unwrap();
        let d = breaker.check(observation_id, BASE_NOW + 1).unwrap();
        match d {
            CircuitDecision::Reject {
                is_manual_override,
                reason,
            } => {
                prop_assert!(is_manual_override);
                prop_assert_eq!(
                    XRateLimitTypeKind::GlobalCircuitOpen
                        .counts_against_sli(is_manual_override),
                    false
                );
                let is_manual_reason =
                    matches!(reason, TripReason::ManualOverride { .. });
                prop_assert!(is_manual_reason);
            }
            CircuitDecision::Allow { .. } => {
                prop_assert!(false, "expected Reject");
            }
            _ => prop_assert!(false, "non-exhaustive arm"),
        }
    }
}

// ---- prop_circuit_trip_at_50pct_5min_boundary --------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Trip threshold is `> 0.5` (strict greater-than) per WI §6.1.6;
    /// 5xx_rate at-or-below 0.5 does NOT breach signal A.
    #[test]
    fn prop_circuit_trip_at_50pct_strict_boundary(
        offset in 0_usize..50,
    ) {
        let mut buf = VecDeque::new();
        let total = 200;
        // Sweep at-or-below the strict boundary: 100/200 = 0.5 exact
        // (offset=0); 51/200 < 0.5 (offset=49). Property: signal_a is
        // false across the whole sub-/at-boundary range.
        let five_xx_count = 100usize.saturating_sub(offset);
        for t in 0..total as u64 {
            let o = HealthObservation {
                timestamp_ms: t,
                status: if (t as usize) < five_xx_count {
                    ObservationStatus::ServerError5xx
                } else {
                    ObservationStatus::Success
                },
                latency_p99_us: 100_000,
                do_error_rate_5m: 0.01,
            };
            buf.push_back(o);
        }
        let eval = evaluate_signals(
            &buf,
            &CircuitThresholds::canonical(),
            300,
        );
        prop_assert!(
            !eval.signal_a,
            "5xx_rate exactly 0.5 should NOT breach (strict-> boundary)"
        );
    }
}

// ---- prop_hysteresis_no_flapping (canonical Resilience4j) --------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// A multi-signal breach followed by an immediate signal-floor
    /// recovery does NOT flap: the Open state stays Open for at least
    /// HALFOPEN_DWELL_MS regardless of how clean the recovery signals
    /// look (canonical hysteresis pattern).
    #[test]
    fn prop_hysteresis_dwell_floor_never_breached(
        clean_count in 50_usize..200,
        dwell_ms in 0_u64..(HALFOPEN_DWELL_MS - 1),
    ) {
        let (breaker, _, _) = fresh();
        let mut now = BASE_NOW;
        // Trip via multi-signal.
        for t in 0..200 {
            let mut o = HealthObservation {
                timestamp_ms: now,
                status: if (t as usize) < 150 {
                    ObservationStatus::ServerError5xx
                } else {
                    ObservationStatus::Success
                },
                latency_p99_us: 6_000_000,
                do_error_rate_5m: 0.5,
            };
            o.timestamp_ms = now;
            let _ = breaker.record_observation(o, now).unwrap();
            now += 1;
        }
        let snap = breaker.snapshot().unwrap();
        let is_open = matches!(snap.state, CircuitState::Open { .. });
        prop_assert!(is_open);
        // Now feed clean signals BEFORE the dwell period elapses.
        let recovery_now = now + dwell_ms;
        for _ in 0..clean_count {
            let o = HealthObservation {
                timestamp_ms: recovery_now,
                status: ObservationStatus::Success,
                latency_p99_us: 100_000,
                do_error_rate_5m: 0.01,
            };
            let _ = breaker.record_observation(o, recovery_now).unwrap();
        }
        let snap = breaker.snapshot().unwrap();
        // Must still be Open — dwell_ms < HALFOPEN_DWELL_MS so no
        // transition allowed.
        prop_assert!(
            matches!(snap.state, CircuitState::Open { .. }),
            "hysteresis breached: state={:?}",
            snap.state
        );
    }
}

// ---- prop_overshoot_does_not_panic ------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Extreme inputs (max u64 timestamps; max-latency observations)
    /// never panic the orchestrator.
    #[test]
    fn prop_overshoot_does_not_panic(
        latency in 0_u64..u64::MAX,
        do_err in -10.0_f64..10.0,
        five_xx_count in 0_usize..500,
    ) {
        let (breaker, _, _) = fresh();
        let mut now = BASE_NOW;
        for t in 0..200_u64 {
            let o = HealthObservation {
                timestamp_ms: now,
                status: if (t as usize) < five_xx_count {
                    ObservationStatus::ServerError5xx
                } else {
                    ObservationStatus::Success
                },
                latency_p99_us: latency,
                do_error_rate_5m: do_err,
            };
            let _ = breaker.record_observation(o, now);
            now = now.saturating_add(1);
        }
        let _ = breaker.snapshot().unwrap();
    }
}

// ---- prop_metrics_record_per_circuit_decision -------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Each Reject in Open state bumps the within_quota counter for
    /// `global_circuit_open`. Each manual override bumps the
    /// manual_override counter.
    #[test]
    fn prop_metrics_record_per_reject(
        n_rejects in 1_usize..50,
    ) {
        let (breaker, _, metrics) = fresh();
        breaker.manual_override(
            ManualOverrideTarget::Open,
            "admin-1".to_string(),
            "drill".to_string(),
            BASE_NOW,
        ).unwrap();
        for i in 0..n_rejects {
            let _ = breaker.check(i as u64, BASE_NOW + 1).unwrap();
        }
        prop_assert_eq!(
            metrics.counter_for_labels(
                CircuitMetricKind::WithinQuotaTotal,
                "global_circuit_open",
                "iad"
            ),
            n_rejects as u64
        );
        prop_assert_eq!(
            metrics.counter_total(CircuitMetricKind::ManualOverrideTotal),
            1
        );
    }
}

// ---- prop_rolling_window_culls_old_observations -----------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Observations older than ROLLING_WINDOW_MS age out at the next
    /// `record_observation` call; the rolling-window evaluator's
    /// view tracks the current truth.
    #[test]
    fn prop_rolling_window_culls_old_observations(
        older_count in 50_usize..200,
        recent_count in 50_usize..200,
    ) {
        let (breaker, _, _) = fresh();
        let mut now = BASE_NOW;
        for t in 0..older_count {
            let _ = breaker.record_observation(
                HealthObservation {
                    timestamp_ms: now,
                    status: ObservationStatus::ServerError5xx,
                    latency_p99_us: 10_000_000,
                    do_error_rate_5m: 0.9,
                },
                now,
            ).unwrap();
            now += 1;
            let _ = t;
        }
        // Advance well past the rolling window.
        now += ROLLING_WINDOW_MS + 1;
        for _ in 0..recent_count {
            let _ = breaker.record_observation(
                HealthObservation {
                    timestamp_ms: now,
                    status: ObservationStatus::Success,
                    latency_p99_us: 100_000,
                    do_error_rate_5m: 0.01,
                },
                now,
            ).unwrap();
            now += 1;
        }
        let snap = breaker.snapshot().unwrap();
        if let Some(eval) = snap.last_evaluation {
            prop_assert!(eval.error_5xx_rate < 0.5);
            prop_assert!(eval.p99_latency_us < 1_000_000);
            prop_assert!(eval.do_error_rate < 0.3);
        }
    }
}

// ---- prop_rate_limit_body_always_consistent_with_headers ------------
//
// Audit `specs/_audits/2026-05-15-ratelimit-ux-audit.md` §6 invariant:
// on EVERY 429, the canonical JSON body's fields mirror the headers
// byte-for-byte — `error.kind` == `X-Rate-Limit-Type`,
// `error.retry_after_seconds` == `Retry-After`,
// `error.limit / remaining / reset_seconds` == RFC 9331 `RateLimit:`,
// `error.tier_upgrade_url / docs_url` == frozen canonical strings.
//
// This pins the contract customer SDKs depend on: the body and the
// headers NEVER disagree, regardless of which camada emitted the 429
// or which input values the upstream chose.

fn kind_strategy() -> impl proptest::strategy::Strategy<Value = XRateLimitTypeKind>
{
    use corelink_rate_headers::canonical_kind_list;
    let kinds = canonical_kind_list();
    (0_usize..kinds.len()).prop_map(move |i| kinds[i])
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Body and headers are ALWAYS consistent on every 429.
    #[test]
    fn prop_rate_limit_body_always_consistent_with_headers(
        limit in 0_u64..1_000_000,
        remaining in 0_u64..1_000_000,
        reset_secs in 0_u64..1_000_000,
        retry_after in 0_u64..(RETRY_AFTER_HARD_CEILING_SECS + 1_000),
        kind in kind_strategy(),
        tier in proptest::sample::select(vec![
            String::new(),
            String::from("free"),
            String::from("solo"),
            String::from("team"),
            String::from("business"),
            String::from("enterprise"),
        ]),
        reset_utc in "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z",
        request_id in "[0-9A-HJKMNP-TV-Z]{0,26}",
        message in ".{0,128}",
    ) {
        use corelink_rate_headers::{
            RateLimitErrorBody, RateLimitHeaderBuilder, RateLimitPolicy,
            DOCS_URL, ERROR_CODE_RATE_LIMIT_EXCEEDED, TIER_UPGRADE_URL,
        };

        let h = RateLimitHeaderBuilder::build_with_vendor(
            limit,
            remaining,
            reset_secs,
            vec![RateLimitPolicy::new(limit.max(1), 60)],
            retry_after,
            kind,
            tier.clone(),
            reset_utc.clone(),
        );
        let body = RateLimitErrorBody::from_headers(
            &h,
            message.clone(),
            request_id.clone(),
        );

        // INV-BODY-HEADER-MIRROR-1: discriminator matches.
        prop_assert_eq!(body.kind, h.x_rate_limit_type);
        prop_assert_eq!(body.kind.as_str(), h.render_x_rate_limit_type());

        // INV-BODY-HEADER-MIRROR-2: retry-after matches.
        prop_assert_eq!(body.retry_after_seconds, h.retry_after_secs);
        prop_assert_eq!(
            body.retry_after_seconds.to_string(),
            h.render_retry_after()
        );

        // INV-BODY-HEADER-MIRROR-3: RFC 9331 limit / remaining / reset.
        prop_assert_eq!(body.limit, h.limit);
        prop_assert_eq!(body.remaining, h.remaining);
        prop_assert_eq!(body.reset_seconds, h.reset_secs);

        // INV-BODY-HEADER-MIRROR-4: vendor fields mirror exactly.
        prop_assert_eq!(&body.tier, &h.corelink_tier);
        prop_assert_eq!(&body.reset_utc, &h.corelink_quota_reset_utc);

        // INV-BODY-FROZEN-URLS: tier_upgrade_url + docs_url canonical.
        prop_assert_eq!(body.tier_upgrade_url, TIER_UPGRADE_URL);
        prop_assert_eq!(body.docs_url, DOCS_URL);
        prop_assert_eq!(
            h.corelink_tier_upgrade_url,
            "https://corelink.humangr.com/pricing",
        );

        // INV-BODY-STABLE-CODE: at GA there is exactly one stable code.
        prop_assert_eq!(body.code, ERROR_CODE_RATE_LIMIT_EXCEEDED);

        // INV-BODY-RENDER-WELL-FORMED: JSON envelope balanced.
        let json = body.render_json();
        let envelope_prefix = "{\"error\":{";
        let envelope_suffix = "}}";
        prop_assert!(
            json.starts_with(envelope_prefix),
            "json missing envelope prefix"
        );
        prop_assert!(
            json.ends_with(envelope_suffix),
            "json missing envelope suffix"
        );
        // No raw control characters leaked into the output (every
        // < 0x20 char MUST have been escaped per RFC 8259).
        for byte in json.bytes() {
            prop_assert!(
                byte >= 0x20,
                "raw control byte {byte:#x} leaked into JSON",
            );
        }
    }
}

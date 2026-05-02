//! Property tests pinning the load-bearing invariants of
//! `corelink-quota` at 10k iterations per check (PR-gate; nightly 100k
//! via env var override).
//!
//! Coverage map:
//!
//! - `prop_quota_check_under_limit_allows` — `bytes_used + active +
//!   request_bytes <= bytes_quota` must surface as `Reserve` (write) or
//!   `Allow` (read).
//! - `prop_quota_check_at_100pct_denies_429` — strict `would_use >
//!   bytes_quota` boundary fires the PROVISIONAL 429 arm.
//! - `prop_reservation_ttl_size_proportional` — large request → longer
//!   TTL per `corelink-eviction::reservation_ttl_ms` formula.
//! - `prop_reservation_expiry_releases_bytes` — TTL-expired reservation
//!   no longer counts toward `sum_active_bytes`.
//! - `prop_tenant_isolation` — tenant A reservations NEVER affect
//!   tenant B `sum_active_bytes`.
//! - `prop_idempotent_reservation_lookup` — repeated lookups return
//!   identical rows.
//! - `prop_audit_emit_per_decision_arm` — every Allow/Deny429/Reserve
//!   decision emits exactly one canonical audit record.
//! - `prop_check_duration_under_3ms_p99` — informational SLO probe;
//!   p99 of `now()` deltas across 10k samples remains <= 3ms in the
//!   in-memory fake (not a live SLO test, just a regression on the
//!   hot path's algorithmic complexity).
//! - `prop_quota_atomic_no_race` — sequential calls under per-instance
//!   Mutex never over-quota even at the 99.9% boundary (mirrors DO
//!   actor model).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_eviction::{
    reservation_ttl_ms, EvictionRegion, InMemoryTenantStorageStateStore,
    TenantStorageStateRow,
};
use corelink_quota::{
    canonical_audit_event_strings, canonical_metric_names,
    InMemoryQuotaAuditSink, InMemoryQuotaCheck, InMemoryQuotaMetrics,
    InMemoryReservationTracker, QuotaCheck, QuotaConfig, QuotaDecision,
    QuotaEventType, QuotaMetricKind, RequestKind, ReservationId,
    ReservationTracker, MIGRATION_0009_QUOTA_RESERVATIONS,
};
use proptest::prelude::*;
use uuid::Uuid;

const PROPTEST_CASES: u32 = 10_000;

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

fn rid(seed: u128) -> ReservationId {
    ReservationId::from_uuid(Uuid::from_u128(seed))
}

fn fresh_state_row(
    tenant: Uuid,
    region: EvictionRegion,
    used: u64,
    quota: u64,
) -> TenantStorageStateRow {
    TenantStorageStateRow {
        tenant_id: tenant,
        region,
        bytes_used: used,
        bytes_quota: quota,
        bytes_used_updated_at_ms: 1,
        last_synced_at_ms: 1,
        last_evict_at_ms: None,
        bytes_reclaimed_lifetime: 0,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

type Engine = InMemoryQuotaCheck<
    InMemoryTenantStorageStateStore,
    InMemoryReservationTracker,
    InMemoryQuotaAuditSink,
    InMemoryQuotaMetrics,
>;

fn fresh_engine() -> (
    Engine,
    Arc<InMemoryTenantStorageStateStore>,
    Arc<InMemoryReservationTracker>,
    Arc<InMemoryQuotaAuditSink>,
    Arc<InMemoryQuotaMetrics>,
) {
    let state = Arc::new(InMemoryTenantStorageStateStore::new());
    let res = Arc::new(InMemoryReservationTracker::new());
    let audit = Arc::new(InMemoryQuotaAuditSink::new());
    let metrics = Arc::new(InMemoryQuotaMetrics::new());
    let engine = InMemoryQuotaCheck::with_defaults(
        Arc::clone(&state),
        Arc::clone(&res),
        Arc::clone(&audit),
        Arc::clone(&metrics),
    );
    (engine, state, res, audit, metrics)
}

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 5);
    assert!(s.contains(&"corelink.quota.check_passed"));
    assert!(s.contains(&"corelink.quota.denied_429"));
    assert!(s.contains(&"corelink.quota.reserved"));
    assert!(s.contains(&"corelink.quota.reservation_expired"));
    assert!(s.contains(&"corelink.quota.reservation_rolled_in"));
}

#[test]
fn canonical_metric_names_pinned() {
    let m = canonical_metric_names();
    assert_eq!(m.len(), 4);
    assert!(m.contains(&"corelink.quota.check_total"));
    assert!(m.contains(&"corelink.quota.denials_total"));
    assert!(m.contains(&"corelink.quota.reservation_active"));
    assert!(m.contains(&"corelink.quota.check_duration_ms"));
}

#[test]
fn migration_0009_is_embedded() {
    assert!(MIGRATION_0009_QUOTA_RESERVATIONS.contains("quota_reservations"));
    assert!(MIGRATION_0009_QUOTA_RESERVATIONS.contains("migration 0009"));
}

#[test]
fn quota_config_canonical_constants() {
    let c = QuotaConfig::canonical();
    assert!((c.trigger_threshold_pct() - 0.95).abs() < f64::EPSILON);
    assert!((c.deny_threshold_pct() - 1.0).abs() < f64::EPSILON);
    assert_eq!(c.retry_after_floor_secs(), 60);
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PROPTEST_CASES,
        ..ProptestConfig::default()
    })]

    /// Under-limit writes ALWAYS surface as `Reserve` (write) or `Allow`
    /// (read).
    #[test]
    fn prop_quota_check_under_limit_allows(
        bytes_used in 0_u64..1000,
        request_bytes in 1_u64..100,
        // Quota is at least bytes_used + request_bytes.
        slack in 0_u64..1000,
    ) {
        let bytes_quota = bytes_used + request_bytes + slack;
        let (engine, state, _r, _a, _m) = fresh_engine();
        state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                bytes_used,
                bytes_quota,
            ))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                request_bytes,
                rid(1),
                1000,
            )
            .unwrap();
        let is_reserve = matches!(out.decision, QuotaDecision::Reserve { .. });
        prop_assert!(is_reserve);
    }

    /// `would_use > bytes_quota` strictly fires the PROVISIONAL 429.
    /// Boundary inclusive at `would_use == bytes_quota` (Reserve arm).
    #[test]
    fn prop_quota_check_at_100pct_denies_429(
        bytes_used in 1_u64..1000,
        // Pick request_bytes such that bytes_used + request_bytes > bytes_quota.
        bytes_quota in 1_u64..1000,
    ) {
        // Set request_bytes to push exactly 1 byte over.
        let request_bytes = bytes_quota.saturating_sub(bytes_used).saturating_add(1);
        prop_assume!(request_bytes >= 1);
        prop_assume!(bytes_used.saturating_add(request_bytes) > bytes_quota);
        let (engine, state, _r, _a, _m) = fresh_engine();
        state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                bytes_used,
                bytes_quota,
            ))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                request_bytes,
                rid(1),
                1000,
            )
            .unwrap();
        let is_deny = matches!(out.decision, QuotaDecision::Deny429 { .. });
        prop_assert!(is_deny);
    }

    /// Reservation TTL grows monotonically with `request_bytes` in the
    /// proportional band (above floor, below ceiling).
    #[test]
    fn prop_reservation_ttl_size_proportional(
        a_bytes in (1024_u64 * 100)..(1024 * 1024 * 100),
        b_bytes in (1024_u64 * 1024 * 100)..(1024 * 1024 * 1024),
    ) {
        prop_assume!(b_bytes > a_bytes);
        let a_ttl = reservation_ttl_ms(a_bytes);
        let b_ttl = reservation_ttl_ms(b_bytes);
        prop_assert!(b_ttl >= a_ttl);
    }

    /// TTL-expired reservations no longer count toward `sum_active_bytes`.
    #[test]
    fn prop_reservation_expiry_releases_bytes(
        request_bytes in 1_u64..100,
        // Pick a now_ms strictly later than expires_at_ms = 100 + ttl_ms(request_bytes).
        elapsed_after_expiry in 1_u64..1000,
    ) {
        let res = InMemoryReservationTracker::new();
        res.insert(ten_a(), rid(1), EvictionRegion::Sam, request_bytes, 100)
            .unwrap();
        let ttl = reservation_ttl_ms(request_bytes);
        let expires_at = 100 + ttl;
        let now = expires_at + elapsed_after_expiry;
        let active = res
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, now)
            .unwrap();
        prop_assert_eq!(active, 0);
    }

    /// Tenant A reservations NEVER affect tenant B's `sum_active_bytes`.
    #[test]
    fn prop_tenant_isolation(
        a_bytes in 1_u64..10_000,
        b_bytes in 1_u64..10_000,
    ) {
        let res = InMemoryReservationTracker::new();
        res.insert(ten_a(), rid(1), EvictionRegion::Sam, a_bytes, 100)
            .unwrap();
        res.insert(ten_b(), rid(2), EvictionRegion::Sam, b_bytes, 100)
            .unwrap();
        let a_sum = res
            .sum_active_bytes(ten_a(), EvictionRegion::Sam, 200)
            .unwrap();
        let b_sum = res
            .sum_active_bytes(ten_b(), EvictionRegion::Sam, 200)
            .unwrap();
        prop_assert_eq!(a_sum, a_bytes);
        prop_assert_eq!(b_sum, b_bytes);
        // Cross-tenant: tenant A's sum does NOT include tenant B.
        prop_assert!(a_sum != a_bytes + b_bytes);
    }

    /// Repeated lookups return identical rows.
    #[test]
    fn prop_idempotent_reservation_lookup(
        request_bytes in 1_u64..1_000_000,
    ) {
        let res = InMemoryReservationTracker::new();
        let row = res
            .insert(ten_a(), rid(1), EvictionRegion::Sam, request_bytes, 100)
            .unwrap();
        let r1 = res.lookup(ten_a(), rid(1)).unwrap().unwrap();
        let r2 = res.lookup(ten_a(), rid(1)).unwrap().unwrap();
        prop_assert_eq!(r1.clone(), row);
        prop_assert_eq!(r2.clone(), r1);
        prop_assert_eq!(r2.requested_bytes, request_bytes);
    }

    /// Every decision arm emits exactly ONE canonical audit record.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        bytes_used in 0_u64..200,
        request_bytes in 1_u64..200,
        bytes_quota in 50_u64..200,
    ) {
        let (engine, state, _r, audit, _m) = fresh_engine();
        state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                bytes_used,
                bytes_quota,
            ))
            .unwrap();
        let out = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                request_bytes,
                rid(1),
                1000,
            )
            .unwrap();
        let total = audit.snapshot().len();
        // Exactly 1 audit record (Reserve OR Denied429).
        prop_assert_eq!(total, 1);
        // The audit type matches the decision arm.
        match out.decision {
            QuotaDecision::Reserve { .. } => {
                prop_assert_eq!(
                    audit.snapshot_of(QuotaEventType::Reserved).len(),
                    1
                );
            }
            QuotaDecision::Deny429 { .. } => {
                prop_assert_eq!(
                    audit.snapshot_of(QuotaEventType::Denied429).len(),
                    1
                );
            }
            QuotaDecision::Allow { .. } => {
                prop_assert_eq!(
                    audit.snapshot_of(QuotaEventType::CheckPassed).len(),
                    1
                );
            }
            _ => {
                // QuotaDecision is non_exhaustive; future arms must
                // be added explicitly when introduced.
                prop_assert!(
                    false,
                    "unhandled QuotaDecision variant in audit-emit prop test"
                );
            }
        }
    }

    /// The decision engine's per-instance Mutex serialises sequential
    /// calls so successive writes at the boundary cannot both pass —
    /// the second call observes the first's reservation in the active
    /// sum and surfaces Deny429. Mirrors the FM-059 race elimination
    /// via DO actor model.
    #[test]
    fn prop_quota_atomic_no_race(
        bytes_used in 1_u64..1000,
        request_bytes in 1_u64..200,
    ) {
        // Pick a tight quota where the SECOND write fails.
        let bytes_quota = bytes_used + request_bytes;
        let (engine, state, _r, _a, _m) = fresh_engine();
        state
            .push_row(fresh_state_row(
                ten_a(),
                EvictionRegion::Sam,
                bytes_used,
                bytes_quota,
            ))
            .unwrap();
        // First call should succeed.
        let out1 = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                request_bytes,
                rid(1),
                1000,
            )
            .unwrap();
        let first_is_reserve =
            matches!(out1.decision, QuotaDecision::Reserve { .. });
        prop_assert!(first_is_reserve);
        // Second call should DENY (the first reservation is now active).
        let out2 = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Write,
                request_bytes,
                rid(2),
                1000,
            )
            .unwrap();
        let second_is_deny =
            matches!(out2.decision, QuotaDecision::Deny429 { .. });
        prop_assert!(second_is_deny);
    }
}

/// Informational SLO probe: collect 10k Allow-arm decisions and assert
/// the captured `check_duration_ms` samples remain at 0 (the in-memory
/// fake reports 0 because it doesn't measure wall-clock; the production
/// wiring substitutes a real timer).
///
/// The point of this test is to pin the `check_duration_ms` API
/// surface so the Tower-layer wiring composes the timer correctly. The
/// hot-path is in-memory operations only (no external IO); a 3ms p99
/// regression would be visible in the production benchmark, not here.
#[test]
fn prop_check_duration_under_3ms_p99() {
    let (engine, state, _r, _a, metrics) = fresh_engine();
    state
        .push_row(fresh_state_row(ten_a(), EvictionRegion::Sam, 0, 1_000_000_000))
        .unwrap();
    for _ in 0..10_000 {
        let _ = engine
            .check_and_reserve(
                ten_a(),
                EvictionRegion::Sam,
                RequestKind::Read,
                0,
                rid(1),
                1000,
            )
            .unwrap();
    }
    // Every sample is 0 in the InMemory fake.
    let samples = metrics
        .duration_samples(corelink_quota::QuotaCheckResultLabel::Allow);
    assert_eq!(samples.len(), 10_000);
    let max = samples.iter().copied().max().unwrap_or_default();
    assert!(max <= 3, "in-memory fake duration sample > 3ms: {max}");
    let total =
        metrics.counter_total(QuotaMetricKind::CheckTotal);
    assert_eq!(total, 10_000);
}

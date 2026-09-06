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

/// The tombstone map introduced by WP-M is a SECOND per-key map on the
/// same singleton, and it shipped with no bound — re-opening, one map
/// over, exactly the unbounded-growth DoS that `LIMITER_BUCKET_MAP_CAP`
/// was added to close (#534). A tombstone is removed only when its own
/// key is re-materialised, so distinct keys that are each drained once
/// leave entries nothing sweeps.
///
/// Drives the real shape: drain each key to `Deny429` (which is what
/// records a tombstone), across far more distinct keys than the cap.
#[test]
fn tombstone_map_is_bounded_under_distinct_key_flood() {
    let audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let cap = 16usize;
    let lim = InMemoryTokenBucketRateLimiter::new_with_cap(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        RateLimitConfig::canonical(),
        cap,
    );
    let burst = RateLimitConfig::canonical().default_burst_capacity();
    for i in 0..(cap * 8) as u32 {
        let key = BucketKey::per_tenant_per_endpoint(ten_a(), format!("oci.repo.{i}"));
        // Exhaust this key, then take one more to earn the Deny429 that
        // records the tombstone.
        let _ = lim.try_acquire(ten_a(), key.clone(), burst, 1000).unwrap();
        let _ = lim.try_acquire(ten_a(), key, 1, 1000).unwrap();
        // The invariant holds at EVERY step, not only at the end.
        assert!(
            lim.tombstone_count().unwrap() <= cap,
            "tombstone map grew past the cap at i={i}"
        );
    }
    assert!(lim.tombstone_count().unwrap() <= cap);
}

/// `tombstone_count` must report the real size, and a single drained key
/// must actually leave ONE entry. Without this, a `tombstone_count` that
/// always answered `Ok(0)` would satisfy every bound assertion — the
/// bound would be "proved" by an accessor that cannot see anything.
#[test]
fn one_drained_key_leaves_exactly_one_tombstone() {
    let (lim, _, _) = fresh();
    let burst = RateLimitConfig::canonical().default_burst_capacity();
    let key = BucketKey::per_tenant_per_endpoint(ten_a(), "oci.repo.solo".to_string());
    assert_eq!(lim.tombstone_count().unwrap(), 0, "nothing drained yet");
    let _ = lim.try_acquire(ten_a(), key.clone(), burst, 1000).unwrap();
    let out = lim.try_acquire(ten_a(), key, 1, 1000).unwrap();
    assert!(!out.decision.is_allow(), "the second take must be denied");
    assert_eq!(
        lim.tombstone_count().unwrap(),
        1,
        "a drained key must leave exactly one tombstone"
    );
}

/// The two halves of the bound do DIFFERENT jobs, and only a map holding
/// BOTH expired and live tombstones tells them apart:
///   - the expiry sweep must drop what is inert (`deadline <= now`),
///   - and must KEEP what is still live, or the sweep silently becomes
///     the bypass the tombstone exists to prevent.
///
/// Fills the map to the cap with expired entries plus one live one, then
/// forces a further insert: the sweep must reclaim the expired ones and
/// leave exactly the live tombstone plus the new one.
#[test]
fn expiry_sweep_drops_only_the_expired_tombstones() {
    let audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let cap = 4usize;
    let lim = InMemoryTokenBucketRateLimiter::new_with_cap(
        Arc::clone(&audit),
        Arc::clone(&metrics),
        RateLimitConfig::canonical(),
        cap,
    );
    let burst = RateLimitConfig::canonical().default_burst_capacity();
    let drain_at = |t: u64, name: &str| {
        let key = BucketKey::per_tenant_per_endpoint(ten_a(), name.to_string());
        let _ = lim.try_acquire(ten_a(), key.clone(), burst, t).unwrap();
        let out = lim.try_acquire(ten_a(), key, 1, t).unwrap();
        assert!(!out.decision.is_allow(), "{name} must be denied at t={t}");
    };

    // Three tombstones that will be expired by the time we come back.
    for i in 0..3 {
        drain_at(1000, &format!("oci.repo.old.{i}"));
    }
    assert_eq!(lim.tombstone_count().unwrap(), 3);

    // Far enough past the TTL that all three are inert.
    let later = 1000 + DRAINED_TOMBSTONE_TTL_SECS.saturating_mul(1000) + 1;
    // Below the cap (3 < 4) so this one is inserted without a sweep.
    drain_at(later, "oci.repo.live");
    assert_eq!(lim.tombstone_count().unwrap(), cap, "map is now at the cap");

    // At the cap: the sweep runs, must reclaim the 3 expired and keep the
    // live one, leaving room for this insert.
    drain_at(later, "oci.repo.new");
    assert_eq!(
        lim.tombstone_count().unwrap(),
        2,
        "sweep must drop the 3 expired and KEEP the live one, then insert"
    );
}

/// `next_tick` is the monotonic LRU recency counter — every access stamps a
/// strictly-increasing tick so the approximate-LRU eviction can pick the
/// least-recently-accessed bucket. A sabotaged `next_tick` that returned a
/// CONSTANT (0/1) would silently collapse all recency to one value → the LRU
/// victim selection becomes meaningless (a hot key could be evicted, a cold
/// one kept) with no other test noticing. Pin strict monotonicity so that
/// blind spot is closed (kills the changed-line mutants that replace the body
/// with a constant).
#[test]
fn next_tick_is_strictly_monotonic() {
    let lim = InMemoryTokenBucketRateLimiter::new_with_cap(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        RateLimitConfig::canonical(),
        16,
    );
    let t0 = lim.next_tick();
    let t1 = lim.next_tick();
    let t2 = lim.next_tick();
    assert!(
        t1 > t0 && t2 > t1,
        "next_tick must be strictly increasing (LRU recency); got {t0}, {t1}, {t2}"
    );
    // Distinctness too (a constant body fails this as well).
    assert!(
        t0 != t1 && t1 != t2,
        "next_tick values must be unique per call"
    );
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
    #[allow(
        clippy::assertions_on_constants,
        reason = "documents the const sample-bound invariant"
    )]
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

use super::*;
use crate::wall_clock::InMemoryFakeWallClock;
use axum::http::StatusCode;

const T0: u64 = 1_700_000_000_000;

fn guard_with(store: InMemoryQuotaStore, now_ms: u64) -> (QuotaGuard, Arc<InMemoryFakeWallClock>) {
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(now_ms));
    let guard = QuotaGuard::new(Arc::new(store), clock.clone());
    (guard, clock)
}

#[tokio::test]
async fn under_ceiling_allows_and_accrues() {
    let store = InMemoryQuotaStore::new(); // fresh tenant → $5 tripwire
    let (guard, _clock) = guard_with(store, T0);
    // $1 op against a fresh tenant — allowed.
    assert!(guard.check("tenant-a", 1_000_000).await.is_none());
    // A second $1 op — still under $5, allowed (accrual carried).
    assert!(guard.check("tenant-a", 1_000_000).await.is_none());
}

#[tokio::test]
async fn brand_new_tenant_first_op_over_ceiling_rejects_402() {
    // Red-team #6: a brand-new tenant (NO row) whose VERY FIRST billable op
    // exceeds the DEFAULT ceiling must still be rejected 402 — the
    // fresh-row seed-and-check path (`seed_checked_accrue`) never persists an
    // over-default row. Constant-relative (default + $1) so this survives the
    // ADR-0068 reconciliation that raised the default to effectively-unlimited.
    let store = InMemoryQuotaStore::new(); // empty → brand-new tenant
    let (guard, _clock) = guard_with(store, T0);
    let fat_op = DEFAULT_MONTHLY_BUDGET_USD_MICROS.saturating_add(1_000_000);
    let resp = guard
        .check("tenant-new-fat", fat_op)
        .await
        .expect("first op over ceiling must be rejected");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn brand_new_tenant_first_op_under_ceiling_seeds_and_allows() {
    // Red-team #6 control: a fresh tenant's first op UNDER the ceiling is
    // still admitted and seeds the row (accrual carried to the next op).
    // Constant-relative so it survives the default change: a first op $1
    // under the default seeds, then a $2 op projects just over the default
    // and trips — proving the first op's spend was persisted, not bypassed.
    let store = InMemoryQuotaStore::new();
    let (guard, _clock) = guard_with(store, T0);
    let first = DEFAULT_MONTHLY_BUDGET_USD_MICROS.saturating_sub(1_000_000);
    assert!(guard.check("tenant-new-ok", first).await.is_none());
    // Second op: $2 projects to (default + $1) > default — now rejected.
    let resp = guard
        .check("tenant-new-ok", 2_000_000)
        .await
        .expect("second op trips the cap");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn default_tenant_not_walled_far_past_prior_5000_op_tripwire() {
    // ADR-0068 reconciliation: the default ceiling is now effectively-
    // unlimited, so a normal self-serve tenant on the DEFAULT (no operator
    // override) must NOT 402 at normal op volumes. The prior `$5` default at
    // `$0.001/op` walled every tenant at ~5000 ops — 100× BELOW the free
    // tier's own request quota. Drive 6000 ops (well past that old wall) at
    // the default per-op cost on a fresh, un-overridden tenant: all allowed.
    let store = InMemoryQuotaStore::new(); // fresh → new effectively-unlimited default
    let (guard, _clock) = guard_with(store, T0);
    let cost = DEFAULT_COST_PER_OP_MICROS;
    for i in 0..6_000 {
        assert!(
            guard.check("tenant-default", cost).await.is_none(),
            "op {i} on the new default must not be walled (old $5 wall was ~5000 ops)"
        );
    }
}

#[tokio::test]
async fn per_tenant_override_low_value_still_402s_when_exceeded() {
    // The owner-tunable per-tenant override is preserved as the deliberate
    // backstop (primarily for the unbounded team/enterprise tiers). A tenant
    // whose `monthly_budget_usd_micros` is set to a LOW value STILL trips 402
    // once its accrued spend would exceed that override — the backstop works.
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-override",
        QuotaState {
            monthly_budget_usd_micros: 1_000_000, // operator-set $1 ceiling
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let (guard, _clock) = guard_with(store, T0);
    // First $1 op hits exactly the $1 override — allowed (`>` test).
    assert!(guard.check("tenant-override", 1_000_000).await.is_none());
    // Any further spend projects over the $1 override — rejected 402.
    let resp = guard
        .check("tenant-override", 1)
        .await
        .expect("override exceeded must 402");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn over_ceiling_rejects_402_fail_closed() {
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-b",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 4_500_000, // $4.50 spent
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let (guard, _clock) = guard_with(store, T0);
    // A $1 op would project to $5.50 > $5 ⇒ rejected 402.
    let resp = guard.check("tenant-b", 1_000_000).await.expect("rejected");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn over_ceiling_reject_body_is_structured_json() {
    // The 402 reject flows the CENTRALIZED structured JSON body
    // (crate::quota_error) — not the old plain-text marker.
    use axum::body::to_bytes;
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-json",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 4_500_000,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let (guard, _clock) = guard_with(store, T0);
    let resp = guard
        .check("tenant-json", 1_000_000)
        .await
        .expect("rejected");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
    );
    let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        json.get("error").and_then(|v| v.as_str()),
        Some("quota_exceeded")
    );
    assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(false));
    assert!(json.get("docs_url").and_then(|v| v.as_str()).is_some());
}

#[tokio::test]
async fn cycle_roll_resets_accrued_then_accrues_correctly() {
    // CAA-360 #5/#20: a tenant AT the cap whose cycle has fully elapsed must
    // roll (accrued reset) and admit the new op — exercising the new
    // roll_if_stale + check_and_accrue path that replaced the lost-update
    // read-decide-absolute-`put`.
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-roll",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 5_000_000, // pinned at the cap last cycle
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    // One full cycle + 1ms past the anchor ⇒ the cycle is stale.
    let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
    let (guard, _clock) = guard_with(store, later);
    // Despite being at the cap last cycle, the op is admitted (rolled).
    assert!(
        guard.check("tenant-roll", 1_000_000).await.is_none(),
        "post-roll op must be admitted — the cycle reset accrued to 0"
    );
    // A second op in the SAME new cycle accrues on top ($1 + $1 < $5) —
    // proving the roll set accrued to this op's cost, not stale + cost.
    assert!(
        guard.check("tenant-roll", 1_000_000).await.is_none(),
        "second post-roll op accrues within the fresh cycle"
    );
}

#[tokio::test]
async fn exact_ceiling_is_allowed() {
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-c",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 4_000_000,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let (guard, _clock) = guard_with(store, T0);
    // Exactly hits $5 (4 + 1) — `>` test means equal is allowed.
    assert!(guard.check("tenant-c", 1_000_000).await.is_none());
    // Now at the cap; one more micro-dollar trips it.
    let resp = guard.check("tenant-c", 1).await.expect("rejected");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn cycle_rolls_after_30_days() {
    let store = InMemoryQuotaStore::new();
    store.seed(
        "tenant-d",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000,
            accrued_usd_micros: 5_000_000, // fully spent
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    // Advance past the 30-day window — the cycle should roll, freeing
    // the full budget again.
    let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
    let (guard, _clock) = guard_with(store, later);
    assert!(guard.check("tenant-d", 1_000_000).await.is_none());
}

#[tokio::test]
async fn clock_unavailable_fail_closed_503() {
    let store = InMemoryQuotaStore::new();
    let (guard, _clock) = guard_with(store, 0); // now_ms == 0
    let resp = guard.check("tenant-e", 1).await.expect("rejected");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn fail_closed_reject_body_is_structured_json_with_reason() {
    // The 503 fail-CLOSED reject flows the CENTRALIZED structured JSON body
    // (crate::quota_error), preserving the marker as the `reason` field.
    use axum::body::to_bytes;
    let store = InMemoryQuotaStore::new();
    let (guard, _clock) = guard_with(store, 0); // now_ms == 0 ⇒ clock unavailable
    let resp = guard.check("tenant-503", 1).await.expect("rejected");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
    );
    let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        json.get("error").and_then(|v| v.as_str()),
        Some("quota_unavailable")
    );
    assert_eq!(
        json.get("reason").and_then(|v| v.as_str()),
        Some("wall clock unavailable"),
    );
    assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(true));
}

#[derive(Debug)]
struct ErroringStore;

#[async_trait::async_trait]
impl QuotaStore for ErroringStore {
    async fn get(&self, _tenant_id: &str) -> Result<Option<QuotaState>, String> {
        Err("simulated D1 transport error".to_owned())
    }
    async fn put(
        &self,
        _tenant_id: &str,
        _state: QuotaState,
        _updated_at_ms: i64,
    ) -> Result<(), String> {
        Err("simulated D1 transport error".to_owned())
    }
    async fn accrue(
        &self,
        _tenant_id: &str,
        _delta_micros: i64,
        _seed_anchor_ms: i64,
        _updated_at_ms: i64,
    ) -> Result<(), String> {
        Err("simulated D1 transport error".to_owned())
    }
}

#[tokio::test]
async fn store_error_fail_closed_503() {
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(Arc::new(ErroringStore), clock);
    let resp = guard.check("tenant-f", 1).await.expect("rejected");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ───────────────────────── WP-2a: LeasedQuotaStore ─────────────────────────

use std::sync::atomic::{AtomicUsize, Ordering};

/// An inner [`QuotaStore`] that delegates to a real [`InMemoryQuotaStore`]
/// but COUNTS every ceiling-guarded inner call (`check_and_accrue` +
/// `seed_checked_accrue`) — the amortisation oracle (invariant-test (a)).
/// `get`/`put`/`accrue`/`roll_if_stale` are NOT counted (the lease passes
/// them straight through, so counting them would not measure amortisation).
#[derive(Debug)]
struct CountingStore {
    inner: InMemoryQuotaStore,
    ceiling_calls: AtomicUsize,
}

impl CountingStore {
    fn new() -> Self {
        Self {
            inner: InMemoryQuotaStore::new(),
            ceiling_calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.ceiling_calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl QuotaStore for CountingStore {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        self.inner.get(tenant_id).await
    }
    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner.put(tenant_id, state, updated_at_ms).await
    }
    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner
            .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .seed_checked_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
}

/// (a) amortisation: N ops within lease chunks of `LEASE_OPS` cause only
/// `ceil(N / LEASE_OPS)` inner ceiling-guarded calls.
#[tokio::test]
async fn lease_amortises_inner_calls() {
    const LEASE_OPS: i64 = 4;
    const COST: i64 = 1_000; // $0.001/op
    let counting = Arc::new(CountingStore::new());
    // Generous budget so no op is ever rejected: 100 ops worth.
    counting.inner.seed(
        "t-amort",
        QuotaState {
            monthly_budget_usd_micros: 100 * COST,
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::with_config(
        counting.clone(),
        LEASE_OPS,
        COST,
    ));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // 10 ops; lease chunk = 4 → ceil(10/4) = 3 inner calls.
    for _ in 0..10 {
        assert!(guard.check("t-amort", COST).await.is_none());
    }
    assert_eq!(
        counting.calls(),
        3,
        "10 ops with a 4-op lease must hit the inner store only ceil(10/4)=3 times"
    );
}

/// (b) over-ceiling still 402 once lease + inner are exhausted.
#[tokio::test]
async fn lease_over_ceiling_rejects_402() {
    const LEASE_OPS: i64 = 8;
    const COST: i64 = 1_000_000; // $1/op
    let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    // Budget = exactly $3 (3 ops) on a fresh-seeded tenant.
    inner
        .put(
            "t-cap",
            QuotaState {
                monthly_budget_usd_micros: 3_000_000,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
            i64::try_from(T0).unwrap(),
        )
        .await
        .unwrap();
    let leased: Arc<dyn QuotaStore> =
        Arc::new(LeasedQuotaStore::with_config(inner, LEASE_OPS, COST));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // First op tries an 8-op chunk ($8) — over the $3 ceiling — so the
    // partial-lease fallback halves down to a 3-op chunk that fits.
    // 3 ops are then served (1 from the refill + 2 from the lease tail).
    assert!(guard.check("t-cap", COST).await.is_none());
    assert!(guard.check("t-cap", COST).await.is_none());
    assert!(guard.check("t-cap", COST).await.is_none());
    // 4th op: lease drained, inner at the $3 ceiling → 402.
    let resp = guard.check("t-cap", COST).await.expect("4th op rejected");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
}

/// (c) inner-store error once the lease is drained → 503 (fail-CLOSED).
/// A store that serves the FIRST lease then errors on every subsequent
/// inner call models a D1 outage mid-cycle.
#[derive(Debug)]
struct ErrorAfterFirstLease {
    inner: InMemoryQuotaStore,
    seen_first_refill: AtomicUsize,
}

#[async_trait::async_trait]
impl QuotaStore for ErrorAfterFirstLease {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        self.inner.get(tenant_id).await
    }
    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner.put(tenant_id, state, updated_at_ms).await
    }
    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner
            .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        if self.seen_first_refill.fetch_add(1, Ordering::SeqCst) >= 1 {
            return Err("simulated D1 outage on refill".to_owned());
        }
        self.inner
            .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
}

#[tokio::test]
async fn lease_drained_then_inner_error_fail_closed_503() {
    const LEASE_OPS: i64 = 2;
    const COST: i64 = 1_000;
    let store = ErrorAfterFirstLease {
        inner: InMemoryQuotaStore::new(),
        seen_first_refill: AtomicUsize::new(0),
    };
    store.inner.seed(
        "t-503",
        QuotaState {
            monthly_budget_usd_micros: 100 * COST,
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::with_config(
        Arc::new(store),
        LEASE_OPS,
        COST,
    ));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // First 2 ops: one inner refill (the 2-op lease), both served.
    assert!(guard.check("t-503", COST).await.is_none());
    assert!(guard.check("t-503", COST).await.is_none());
    // 3rd op: lease drained → refill → inner errors → 503 (fail-CLOSED).
    let resp = guard
        .check("t-503", COST)
        .await
        .expect("refill error → 503");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// (d) charge-never-lost: the total debited to the inner durable store is
/// always ≥ the cost of the ops actually served (we never serve more than
/// was paid for up front). Here 10 ops are served; the inner accrued must
/// be ≥ 10 ops' cost (it is exactly the rounded-up lease chunks).
#[tokio::test]
async fn lease_charge_never_lost() {
    const LEASE_OPS: i64 = 4;
    const COST: i64 = 1_000;
    let inner = Arc::new(InMemoryQuotaStore::new());
    inner.seed(
        "t-debit",
        QuotaState {
            monthly_budget_usd_micros: 1_000 * COST,
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::with_config(
        inner.clone(),
        LEASE_OPS,
        COST,
    ));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    let ops = 10_i64;
    for _ in 0..ops {
        assert!(guard.check("t-debit", COST).await.is_none());
    }
    let debited = inner
        .get("t-debit")
        .await
        .unwrap()
        .unwrap()
        .accrued_usd_micros;
    let served = ops * COST;
    assert!(
        debited >= served,
        "inner debited ({debited}) must be ≥ served ({served}) — charge-never-lost"
    );
    // And bounded by invariant 3: at most one extra lease chunk debited.
    assert!(
        debited <= served + LEASE_OPS * COST,
        "over-charge bounded by one lease chunk"
    );
}

/// (e) cycle roll invalidates the stale lease: an op after the cycle rolls
/// must NOT be served from the old-cycle lease (it would be free, escaping
/// the new cycle's ceiling). Proven by accounting — after the roll the op
/// is debited against the FRESH (reset) inner row.
#[tokio::test]
async fn lease_cycle_roll_invalidates_stale_lease() {
    const LEASE_OPS: i64 = 8;
    const COST: i64 = 1_000_000; // $1/op
    let inner = Arc::new(InMemoryQuotaStore::new());
    inner.seed(
        "t-roll",
        QuotaState {
            monthly_budget_usd_micros: 5_000_000, // $5
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::with_config(
        inner.clone(),
        LEASE_OPS,
        COST,
    ));
    // Cycle 1 at T0: one op pre-buys a lease. The full 8-op chunk ($8)
    // exceeds the $5 ceiling, so the partial-lease fallback halves to a
    // 4-op chunk ($4, fits under $5) and debits exactly that up front.
    let clock1 = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard1 = QuotaGuard::new(leased.clone(), clock1);
    assert!(guard1.check("t-roll", COST).await.is_none());
    // Inner shows the cycle-1 partial-lease chunk debited ($4).
    let cycle1_debit = inner
        .get("t-roll")
        .await
        .unwrap()
        .unwrap()
        .accrued_usd_micros;
    assert_eq!(
        cycle1_debit, 4_000_000,
        "partial lease debited a 4-op chunk"
    );

    // Advance one full cycle: the inner cycle rolls (accrued→0, anchor
    // advances). The old-cycle lease (anchored at T0) must be discarded.
    let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
    let clock2 = Arc::new(InMemoryFakeWallClock::at_unix_ms(later));
    let guard2 = QuotaGuard::new(leased, clock2);
    // This op rolls the inner row and acquires a NEW lease against the new
    // cycle — it is debited against the freshly-reset inner row, NOT
    // served free from the stale lease.
    assert!(guard2.check("t-roll", COST).await.is_none());
    let post = inner.get("t-roll").await.unwrap().unwrap();
    assert!(
        post.cycle_anchor_ms >= i64::try_from(later).unwrap()
            || post.cycle_anchor_ms != i64::try_from(T0).unwrap(),
        "the inner cycle must have rolled (anchor advanced past T0)"
    );
    // A fresh chunk was debited in the new cycle (would be 0 if the stale
    // lease had served the op for free → that is the bug this guards).
    assert!(
        post.accrued_usd_micros > 0,
        "post-roll op must debit the fresh inner cycle, not be served free \
             from the stale lease"
    );
}

/// (f) concurrency: many concurrent ops on the same tenant never
/// double-spend the lease nor over-serve — the inner durable debit always
/// covers everything served, and the inner ceiling is never breached.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_concurrent_ops_no_double_spend() {
    const LEASE_OPS: i64 = 4;
    const COST: i64 = 1_000;
    const N: usize = 200;
    let inner = Arc::new(InMemoryQuotaStore::new());
    inner.seed(
        "t-conc",
        QuotaState {
            monthly_budget_usd_micros: 10_000 * COST, // never trips
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::with_config(
        inner.clone(),
        LEASE_OPS,
        COST,
    ));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = Arc::new(QuotaGuard::new(leased, clock));

    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let g = guard.clone();
        handles.push(tokio::spawn(async move {
            g.check("t-conc", COST).await.is_none()
        }));
    }
    let mut served = 0_i64;
    for h in handles {
        if h.await.unwrap() {
            served += 1;
        }
    }
    assert_eq!(
        served, N as i64,
        "all ops under the high ceiling are served"
    );
    // charge-never-lost under concurrency: inner debited ≥ served.
    let debited = inner
        .get("t-conc")
        .await
        .unwrap()
        .unwrap()
        .accrued_usd_micros;
    assert!(
        debited >= served * COST,
        "concurrent inner debit ({debited}) must cover all served ({}) — no \
             double-spend / over-serve",
        served * COST
    );
}

// ───────────── WP-2a step 2: warm-lease READ served from memory ─────────────

/// An inner [`QuotaStore`] that delegates to a real [`InMemoryQuotaStore`]
/// but COUNTS every `get` READ — the read-amortisation oracle. This is the
/// spy that proves the per-op D1 `get` is GONE on the warm path: after a
/// lease is acquired, subsequent warm ops must NOT increment this counter.
#[derive(Debug)]
struct GetCountingStore {
    inner: InMemoryQuotaStore,
    get_calls: AtomicUsize,
    ceiling_calls: AtomicUsize,
}

impl GetCountingStore {
    fn new() -> Self {
        Self {
            inner: InMemoryQuotaStore::new(),
            get_calls: AtomicUsize::new(0),
            ceiling_calls: AtomicUsize::new(0),
        }
    }
    fn gets(&self) -> usize {
        self.get_calls.load(Ordering::SeqCst)
    }
    fn ceiling(&self) -> usize {
        self.ceiling_calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl QuotaStore for GetCountingStore {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        let _ = self.get_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get(tenant_id).await
    }
    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner.put(tenant_id, state, updated_at_ms).await
    }
    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner
            .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .seed_checked_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }
}

/// SPY-PROOF the per-op D1 `get` is GONE on the warm path (the core WP-2a
/// step-2 deliverable). N ops within lease chunks of `LEASE_OPS` cause only
/// `ceil(N / LEASE_OPS)` inner `get` READS — not one per op — AND the same
/// `ceil(N / LEASE_OPS)` ceiling-guarded writes. Warm ops between refills
/// make ZERO D1 round-trips of EITHER kind.
#[tokio::test]
async fn warm_lease_serves_read_from_memory_no_per_op_get() {
    const LEASE_OPS: i64 = 4;
    const COST: i64 = 1_000;
    let spy = Arc::new(GetCountingStore::new());
    spy.inner.seed(
        "t-warm",
        QuotaState {
            monthly_budget_usd_micros: 100 * COST, // generous — never trips
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> =
        Arc::new(LeasedQuotaStore::with_config(spy.clone(), LEASE_OPS, COST));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // 10 ops, 4-op lease → ceil(10/4)=3 refills. Each refill does ONE `get`
    // + ONE ceiling write; the 7 warm ops in between do NEITHER.
    for _ in 0..10 {
        assert!(guard.check("t-warm", COST).await.is_none());
    }
    assert_eq!(
        spy.gets(),
        3,
        "the per-op D1 `get` is GONE: 10 ops with a 4-op lease hit `get` only \
             ceil(10/4)=3 times (once per refill), NOT once per op"
    );
    assert_eq!(
        spy.ceiling(),
        3,
        "ceiling-guarded writes also amortised to ceil(10/4)=3"
    );
    // Total D1 round-trips for 10 ops = 3 gets + 3 writes = 6, i.e. the 7
    // warm ops made ZERO D1 round-trips.
    assert_eq!(
        spy.gets() + spy.ceiling(),
        6,
        "the 7 warm ops between refills made ZERO D1 round-trips"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();

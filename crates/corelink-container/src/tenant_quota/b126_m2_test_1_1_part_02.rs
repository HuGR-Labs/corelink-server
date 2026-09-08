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

/// INVARIANT 2 (never over-SERVE): the warm-read fast path must NOT admit an
/// over-ceiling op. A tenant whose lease is drained AND whose durable row is
/// at the ceiling is refused 402 — the durable atomic stays the sole ceiling
/// authority even with the read served from memory.
#[tokio::test]
async fn warm_read_still_refuses_over_ceiling_402() {
    const LEASE_OPS: i64 = 8;
    const COST: i64 = 1_000_000; // $1/op
    let spy = Arc::new(GetCountingStore::new());
    // Budget exactly $3 → at most 3 ops ever, on a fresh cycle.
    spy.inner.seed(
        "t-cap2",
        QuotaState {
            monthly_budget_usd_micros: 3_000_000,
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    let leased: Arc<dyn QuotaStore> =
        Arc::new(LeasedQuotaStore::with_config(spy.clone(), LEASE_OPS, COST));
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // The $3 ceiling admits exactly 3 ops. The 8-op chunk overshoots $3, so
    // the partial-lease fallback halves (8→4→2) to a 2-op chunk that fits —
    // so a warm op is served between refills, but the lease is small. What
    // this test PROVES is invariant 2: no matter how the read is served, the
    // 4th op is refused 402 (the durable atomic is the sole ceiling
    // authority) AND at least one op was served warm from memory.
    assert!(guard.check("t-cap2", COST).await.is_none()); // op1: refill (get)
    assert!(guard.check("t-cap2", COST).await.is_none()); // op2: warm (no get)
    assert!(guard.check("t-cap2", COST).await.is_none()); // op3: refill (get)
                                                          // 4th op: durable row now at the $3 ceiling → the warm path cannot cover
                                                          // it → durable path → atomic ceiling refuses → 402. The read served from
                                                          // memory did NOT let an over-ceiling op slip through.
    let resp = guard
        .check("t-cap2", COST)
        .await
        .expect("4th op over ceiling");
    assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    // Fewer gets than ops (4 ops, 3 gets): op2 was served warm from memory —
    // proving the read is served from the lease, WITHOUT ever admitting the
    // over-ceiling 4th op (invariant 2 holds with the read served warm).
    assert!(
        spy.gets() < 4,
        "at least one op was served warm from memory — fewer gets ({}) than ops (4)",
        spy.gets()
    );
}

/// INVARIANT 3 (fail-CLOSED): a drained lease + an unreachable inner store
/// refuses 503 — the warm-read path cannot mask an outage, because a drained
/// lease returns `Ok(None)` and the guard hits the (now-erroring) durable
/// path. Reuses [`ErrorAfterFirstLease`].
#[tokio::test]
async fn warm_read_drained_then_inner_error_fail_closed_503() {
    const LEASE_OPS: i64 = 2;
    const COST: i64 = 1_000;
    let store = ErrorAfterFirstLease {
        inner: InMemoryQuotaStore::new(),
        seen_first_refill: AtomicUsize::new(0),
    };
    store.inner.seed(
        "t-503b",
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

    // Op1: no lease → refill (2-op lease). Op2: warm from memory (no inner
    // call, so no error). Op3: lease drained → refill → inner errors → 503.
    assert!(guard.check("t-503b", COST).await.is_none());
    assert!(guard.check("t-503b", COST).await.is_none()); // warm, in-memory
    let resp = guard
        .check("t-503b", COST)
        .await
        .expect("refill error → 503");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// INVARIANT 3b (fail-CLOSED on a poisoned lease lock): if
/// `try_serve_from_lease` itself errors, the guard rejects 503, never
/// fail-open. Driven by a fake whose warm path returns `Err`.
#[derive(Debug)]
struct WarmErrStore;
#[async_trait::async_trait]
impl QuotaStore for WarmErrStore {
    async fn get(&self, _t: &str) -> Result<Option<QuotaState>, String> {
        Ok(None)
    }
    async fn put(&self, _t: &str, _s: QuotaState, _u: i64) -> Result<(), String> {
        Ok(())
    }
    async fn accrue(&self, _t: &str, _d: i64, _a: i64, _u: i64) -> Result<(), String> {
        Ok(())
    }
    async fn try_serve_from_lease(
        &self,
        _tenant_id: &str,
        _cost_micros: i64,
        _now_ms: i64,
    ) -> Result<Option<bool>, String> {
        Err("simulated poisoned lease lock".to_owned())
    }
}

#[tokio::test]
async fn warm_read_lease_lock_error_fail_closed_503() {
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(Arc::new(WarmErrStore), clock);
    let resp = guard
        .check("t-warmerr", 1_000)
        .await
        .expect("warm err → 503");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// INVARIANT 4 (bounded overshoot unchanged): serving the READ from memory
/// debits NO new budget, so the durable over-charge stays bounded by exactly
/// one lease chunk — identical to WP-2a step 1. 10 ops, 4-op lease.
#[tokio::test]
async fn warm_read_overshoot_still_bounded_one_chunk() {
    const LEASE_OPS: i64 = 4;
    const COST: i64 = 1_000;
    let inner = Arc::new(InMemoryQuotaStore::new());
    inner.seed(
        "t-bound",
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
        assert!(guard.check("t-bound", COST).await.is_none());
    }
    let debited = inner
        .get("t-bound")
        .await
        .unwrap()
        .unwrap()
        .accrued_usd_micros;
    let served = ops * COST;
    assert!(debited >= served, "charge-never-lost: debited ≥ served");
    assert!(
        debited <= served + LEASE_OPS * COST,
        "over-charge still bounded by one lease chunk even with the read served warm"
    );
}

/// INVARIANT 5 (cycle correctness within the documented staleness): once
/// `now_ms` crosses the lease's own cycle boundary, the warm path treats the
/// cached cycle as STALE and defers to the durable roll — a rolled ceiling is
/// never admitted warm past the ≤ one-lease window. Proven by accounting: the
/// post-roll op debits the FRESHLY-RESET inner row (would be free from the
/// stale lease if the cycle check were missing).
#[tokio::test]
async fn warm_read_stale_cycle_defers_to_durable_roll() {
    const LEASE_OPS: i64 = 8;
    const COST: i64 = 1_000_000; // $1/op
    let inner = Arc::new(InMemoryQuotaStore::new());
    inner.seed(
        "t-cycle",
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

    // Cycle 1: one op pre-buys a lease (8-op chunk $8 > $5 → partial 4-op
    // $4 chunk fits). The lease has remaining budget banked (3 ops).
    let clock1 = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard1 = QuotaGuard::new(leased.clone(), clock1);
    assert!(guard1.check("t-cycle", COST).await.is_none());
    assert_eq!(
        inner
            .get("t-cycle")
            .await
            .unwrap()
            .unwrap()
            .accrued_usd_micros,
        4_000_000,
        "cycle-1 partial lease debited a 4-op chunk (banked remainder exists)"
    );

    // Advance one full cycle. The stale lease (anchored at T0) still has
    // banked remainder, but its cycle has elapsed at `later`. The warm path
    // MUST defer (cycle_elapsed → Ok(None)) so the durable roll runs and the
    // op debits the fresh cycle — it is NOT served free from the stale lease.
    let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
    let clock2 = Arc::new(InMemoryFakeWallClock::at_unix_ms(later));
    let guard2 = QuotaGuard::new(leased, clock2);
    assert!(guard2.check("t-cycle", COST).await.is_none());
    let post = inner.get("t-cycle").await.unwrap().unwrap();
    assert!(
        post.cycle_anchor_ms != i64::try_from(T0).unwrap(),
        "the inner cycle rolled (anchor advanced past T0)"
    );
    assert!(
        post.accrued_usd_micros > 0,
        "post-roll op debited the fresh inner cycle — the warm path did NOT \
             serve it free from the stale-cycle lease"
    );
}

/// Control: a store WITHOUT a lease (the durable/in-memory store) inherits
/// the `Ok(None)` default, so the read is NOT short-circuited and behaviour
/// is byte-for-byte the pre-WP-2a-step-2 path (one `get` per op).
#[tokio::test]
async fn no_lease_store_still_reads_per_op() {
    let spy = Arc::new(GetCountingStore::new());
    spy.inner.seed(
        "t-nolease",
        QuotaState {
            monthly_budget_usd_micros: 100_000,
            accrued_usd_micros: 0,
            cycle_anchor_ms: i64::try_from(T0).unwrap(),
        },
    );
    // NOTE: no LeasedQuotaStore wrapper — the spy IS the store.
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(spy.clone(), clock);
    for _ in 0..5 {
        assert!(guard.check("t-nolease", 1_000).await.is_none());
    }
    assert_eq!(
        spy.gets(),
        5,
        "a store without a lease inherits the Ok(None) default → one `get` per op"
    );
}

// ---- B-007: near-$-ceiling early-warning sink ------------------------

/// The pure event builder carries ONLY the tenant id + the three integer
/// quota metrics (PII/secret-free), with the canonical dedup-key, action,
/// severity, and source.
#[test]
fn build_near_ceiling_event_is_canonical_and_pii_free() {
    let ev = build_near_ceiling_event("tenant-xyz", 2_000_000, 8_000_000, 1_700_000_000_000);
    assert_eq!(ev.dedup_key, "near-ceiling:tenant-xyz");
    assert_eq!(ev.event_action, PagerDutyEventAction::Trigger);
    assert_eq!(ev.severity, "sev2");
    assert_eq!(ev.source, "tenant-quota");
    // Summary embeds exactly the tenant id + the three metrics.
    assert!(ev.summary.contains("tenant-xyz"));
    assert!(ev.summary.contains("granted_micros=2000000"));
    assert!(ev.summary.contains("full_chunk_micros=8000000"));
    assert!(ev.summary.contains("cycle_anchor_ms=1700000000000"));
    // PII/secret-free: no credential-shaped material anywhere in the event.
    for hay in [
        ev.summary.as_str(),
        ev.dedup_key.as_str(),
        ev.runbook_url.as_str(),
    ] {
        for needle in [
            "sk_", "sk-", "whsec", "Bearer ", "pat_", "ghp_", "password", "secret=",
        ] {
            assert!(
                !hay.contains(needle),
                "near-ceiling event field must be secret-free but contained {needle:?}: {hay}"
            );
        }
    }
}

/// Default-off: with no sink configured (the `with_config` seam / flag
/// unset), `emit_near_ceiling` returns `None` — zero dispatch, behavior
/// byte-identical to the bare `tracing::warn!` path.
#[tokio::test]
async fn near_ceiling_sink_absent_emits_nothing() {
    let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    let store = LeasedQuotaStore::with_config(inner, 8, 1_000_000);
    assert!(
        store
            .emit_near_ceiling("t-none", 2_000_000, 8_000_000, i64::try_from(T0).unwrap())
            .is_none(),
        "no sink ⇒ emit_near_ceiling is a no-op (default-off, inert)"
    );
}

/// Direct emit: with an injected in-memory sink, `emit_near_ceiling`
/// dispatches exactly one event (fire-and-forget; awaited here to observe
/// it deterministically) and the payload matches the pure builder.
#[tokio::test]
async fn near_ceiling_emit_dispatches_once() {
    let dispatcher = InMemoryPagerDutyDispatcher::new();
    let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    let store = LeasedQuotaStore::with_config(inner, 8, 1_000_000)
        .with_near_ceiling_sink(Arc::new(dispatcher.clone()));
    let handle = store
        .emit_near_ceiling("t-emit", 2_000_000, 8_000_000, i64::try_from(T0).unwrap())
        .expect("sink present ⇒ a spawned emit handle");
    handle.await.expect("emit task joins");
    assert_eq!(dispatcher.attempt_count(), 1);
    assert_eq!(dispatcher.open_incident_count(), 1);
    let snap = dispatcher.snapshot_attempts();
    let first = snap.first().expect("one attempt recorded");
    assert_eq!(first.dedup_key, "near-ceiling:t-emit");
    assert_eq!(first.source, "tenant-quota");
}

/// Idempotency: two near-ceiling emits for the SAME tenant collapse to a
/// single open incident (Events API v2 dedup-key), though both attempts
/// are recorded.
#[tokio::test]
async fn near_ceiling_emit_dedup_collapses_incident() {
    let dispatcher = InMemoryPagerDutyDispatcher::new();
    let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    let store = LeasedQuotaStore::with_config(inner, 8, 1_000_000)
        .with_near_ceiling_sink(Arc::new(dispatcher.clone()));
    for _ in 0..2 {
        store
            .emit_near_ceiling("t-dedup", 2_000_000, 8_000_000, i64::try_from(T0).unwrap())
            .expect("sink present")
            .await
            .expect("emit task joins");
    }
    assert_eq!(dispatcher.attempt_count(), 2, "both attempts recorded");
    assert_eq!(
        dispatcher.open_incident_count(),
        1,
        "same tenant collapses to one incident (dedup_key)"
    );
}

/// End-to-end wiring: driving `charge` through the near-ceiling branch (a
/// refill forced to shrink below a full chunk) fires exactly one
/// fire-and-forget dispatch to the injected sink. The hot path returns
/// `Ok(None)` (Allow) without awaiting the dispatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn near_ceiling_charge_path_dispatches() {
    const LEASE_OPS: i64 = 8;
    const COST: i64 = 1_000_000; // $1/op
    let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
    // Budget = $3: the first op's $8 chunk is rejected and the partial-lease
    // fallback shrinks below the full chunk ⇒ the near-ceiling branch fires.
    inner
        .put(
            "t-wire",
            QuotaState {
                monthly_budget_usd_micros: 3_000_000,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
            i64::try_from(T0).unwrap(),
        )
        .await
        .unwrap();
    let dispatcher = InMemoryPagerDutyDispatcher::new();
    let leased: Arc<dyn QuotaStore> = Arc::new(
        LeasedQuotaStore::with_config(inner, LEASE_OPS, COST)
            .with_near_ceiling_sink(Arc::new(dispatcher.clone())),
    );
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
    let guard = QuotaGuard::new(leased, clock);

    // Allow (Ok(None)) returns immediately — the dispatch is fire-and-forget.
    assert!(guard.check("t-wire", COST).await.is_none());

    // Bounded wait for the fire-and-forget dispatch to land.
    for _ in 0..200 {
        if dispatcher.attempt_count() >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        dispatcher.attempt_count(),
        1,
        "the near-ceiling branch in charge() fired exactly one dispatch"
    );
    assert_eq!(dispatcher.open_incident_count(), 1);
    assert_eq!(
        dispatcher
            .snapshot_attempts()
            .first()
            .expect("one attempt recorded")
            .dedup_key,
        "near-ceiling:t-wire"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_1_2_REANCHOR: () = ();

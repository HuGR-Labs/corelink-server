// ---------------------------------------------------------------------
// FlightGroup — the shared-future coalescer that collapses a burst of COLD
// memo misses onto ONE Argon2id, on BOTH 401 arms.
//
// These first four tests pin the coalescer's own contract deterministically
// (an explicit `Notify` gate holds every run open until the whole burst has
// provably joined, so "exactly N runs" is an assertion, not a race). The
// verifier-level tests after them prove the wiring and the security
// properties that wiring has to preserve.
// ---------------------------------------------------------------------

/// Drive `callers` concurrent [`FlightGroup::run`] calls spread over `keys`
/// distinct keys, holding every run open until ALL callers have joined, and
/// return how many times the work actually ran.
///
/// # Why this is deterministic and not a race
///
/// Two properties do the work, and BOTH are needed:
///
/// 1. **The caller runs on a CURRENT-THREAD runtime** (plain `#[tokio::test]`),
///    which is cooperative: a task is never preempted mid-poll. A caller
///    bumps `entered` and then executes `run`'s whole synchronous prologue —
///    the map lock, the join-or-lead decision, and (for a leader) the first
///    poll of the work — with no await in between, so all of that happens in
///    ONE uninterruptible poll. Observing `entered == callers` therefore
///    proves every caller has already joined or led. On a multi-thread
///    runtime it would prove nothing: a caller could be mid-prologue on
///    another core, and releasing the gate there could let the flight resolve
///    before that caller joins, which would inflate the run count.
/// 2. **The gate is LEVEL-triggered** — a `Semaphore` opened at zero permits
///    and then CLOSED, so an `acquire()` that arrives after the close still
///    returns immediately. An edge-triggered `Notify::notify_waiters()` here
///    would be a lost-wakeup hazard: it wakes only whoever is registered at
///    that instant, so any straggler would hang forever.
async fn count_flight_runs(group: Arc<FlightGroup<u32>>, callers: usize, keys: usize) -> usize {
    let runs = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Semaphore::new(0));

    let mut tasks = Vec::with_capacity(callers);
    for i in 0..callers {
        let (group, runs, entered, gate) = (
            Arc::clone(&group),
            Arc::clone(&runs),
            Arc::clone(&entered),
            Arc::clone(&gate),
        );
        let key = format!("key-{}", i % keys);
        tasks.push(tokio::spawn(async move {
            entered.fetch_add(1, Ordering::SeqCst);
            *group
                .run(
                    &key,
                    move || {
                        async move {
                            runs.fetch_add(1, Ordering::SeqCst);
                            // Blocks until the test CLOSES the gate — never
                            // resolves early, so no caller can arrive to find a
                            // finished flight.
                            let _ = gate.acquire().await;
                            7u32
                        }
                        .boxed()
                    },
                    |m| panic!("flight aborted: {m}"),
                )
                .await
        }));
    }

    // Every caller has now joined or led (see (1) above) — release.
    while entered.load(Ordering::SeqCst) < callers {
        tokio::task::yield_now().await;
    }
    gate.close();

    for t in tasks {
        assert_eq!(
            t.await.expect("flight task"),
            7,
            "every joiner sees the run"
        );
    }
    runs.load(Ordering::SeqCst)
}
/// A concurrent burst on ONE key runs the work EXACTLY once — this is the
/// whole mechanism: the other 15 callers pay nothing and consume no permit.
#[tokio::test]
async fn flight_group_collapses_a_concurrent_burst_to_one_run() {
    let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
    let runs = count_flight_runs(Arc::clone(&group), 16, 1).await;
    assert_eq!(runs, 1, "16 concurrent callers on one key must run ONCE");
}

/// …and DISTINCT keys never share a run. This is the bound on the
/// mechanism: coalescing must never merge two different units of work (a
/// different plaintext, or a different stored hash, is a different proof and
/// must pay its own Argon2id).
#[tokio::test]
async fn flight_group_never_merges_distinct_keys() {
    let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
    let runs = count_flight_runs(Arc::clone(&group), 16, 4).await;
    assert_eq!(runs, 4, "16 callers over 4 keys must run exactly 4 times");
}

/// It is NOT a cache: a call that starts AFTER a flight resolved leads a
/// fresh run rather than reusing the finished one. This is what keeps the
/// per-request D1 row (revocation, expiry, scope) authoritative — a burst
/// shares only the Argon2id comparison it was concurrent with.
#[tokio::test]
async fn flight_group_is_not_a_cache_a_later_call_runs_again() {
    let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
    assert_eq!(count_flight_runs(Arc::clone(&group), 4, 1).await, 1);
    // Second, strictly-later burst on the SAME key ⇒ a fresh run.
    assert_eq!(
        count_flight_runs(Arc::clone(&group), 4, 1).await,
        1,
        "a later burst must not be served by the resolved flight"
    );
}

/// The map retains only genuinely in-flight work: every resolved flight is
/// retired by the first awaiter to observe it, so a long-lived container
/// does not accumulate them.
#[tokio::test]
async fn flight_group_retires_every_resolved_flight() {
    let group = Arc::new(FlightGroup::<u32>::new(FLIGHT_GROUP_CAP));
    let _ = count_flight_runs(Arc::clone(&group), 16, 8).await;
    assert_eq!(
        group.len_for_test(),
        0,
        "resolved flights must not be retained"
    );
}

// ---------------------------------------------------------------------
// The cold-burst 503 — the bug this change exists to kill, and the two
// oracles the fix must not open while killing it.
// ---------------------------------------------------------------------

/// The COLD burst: N simultaneous requests bearing the SAME PAT, all missing
/// the memo, must all succeed.
///
/// This is the `cargo -jN` opening burst against a cold container, scaled
/// down: global pool = 1 and per-tenant sub-cap = 1, so the container can
/// run exactly ONE Argon2id at a time. Production admits 4 per tenant and a
/// real build opens with far more than 4 — same shape, same outcome.
///
/// PROVEN RED without the fix (measured, with coalescing disabled): fails
/// with `burst member 0 was SHED (Backend("pat verifier overloaded"))`. With
/// each request running its own Argon2id the burst serialises on that single
/// permit, so a member cannot start until the members ahead of it have
/// finished a full Argon2id — and one Argon2id at the OWASP-2024 cost
/// (`m=64 MiB, t=3, p=4`) far exceeds the 250 ms `ARGON2_PERMIT_WAIT`, so
/// everything behind the first verify times out. With the fix all 8 share
/// ONE Argon2id under ONE permit and all 8 resolve.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cold_burst_of_one_pat_is_not_shed() {
    const BURST: usize = 8;
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
    let (pt_other, tid_other, hash_other, tenant_other) = mint_pat(&key, 92, SCOPE_CACHE_RW);
    let lookup = Arc::new(fake_with_rows(vec![
        (tid, row(&hash, &tenant, "cas:rw")),
        (tid_other, row(&hash_other, &tenant_other, "cas:rw")),
    ]));
    let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
        lookup,
        vec![(*key).clone()],
        1,
        1,
    ));

    let mut tasks = Vec::with_capacity(BURST);
    for _ in 0..BURST {
        let (v, pt) = (Arc::clone(&verifier), pt.clone());
        tasks.push(tokio::spawn(async move { v.verify(&pt).await }));
    }
    for (i, task) in tasks.into_iter().enumerate() {
        match task.await.expect("burst task") {
            Ok(resolved) => assert_eq!(resolved, tenant, "burst member {i}"),
            Err(e) => panic!(
                "burst member {i} was SHED ({e:?}) — {BURST} concurrent copies of ONE \
                     cold PAT must coalesce onto ONE Argon2id, not race for one permit"
            ),
        }
    }

    // CONTROL — without it this test would pass just as happily against a
    // verifier whose pool was never scarce. Hold the ONE global permit and
    // show a cold, un-memoised PAT is still shed: the pool really is 1 and
    // the 250 ms fail-CLOSED really does fire on this verifier.
    let _held = Arc::clone(&verifier.argon2_permits)
        .acquire_owned()
        .await
        .unwrap();
    let err = verifier.verify(&pt_other).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
        "control: with the only permit held, a cold PAT must shed, got {err:?}"
    );
}

/// An ABANDONED flight must still release its Argon2id permit.
///
/// Coalescing introduces a hazard uncoalesced code does not have. A `Shared`
/// future is driven only by whoever polls it, and an unbursted request has
/// exactly ONE awaiter — so a single client disconnect can leave the run
/// with nobody to poll it. The map still holds a clone, so it is not dropped
/// either; it is frozen wherever it was parked. The dangerous park is the
/// per-tenant acquire, because the GLOBAL permit is already held there and
/// the `tokio::time::timeout` guarding it cannot fire without being polled.
/// Uncoalesced, dropping the request future drops the permit by RAII.
///
/// `FlightGroup::run` closes this by SPAWNING the work, so the runtime owns
/// it and it always finishes. This test drives the exact window: it saturates
/// the tenant bucket so the verify parks holding the global permit, waits for
/// that state, then aborts the only awaiter.
///
/// PROVEN RED without the spawn: the permit is never returned and the test
/// fails on its 5 s budget.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_abandoned_flight_still_releases_its_argon2id_permit() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 99, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    // One global permit, one per-tenant permit.
    let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
        lookup,
        vec![(*key).clone()],
        1,
        1,
    ));

    // Hold the tenant's ONLY sub-permit, so a verify takes the global permit
    // and then parks in the per-tenant acquire — the pinning window.
    let sem = verifier
        .per_tenant_semaphore(&tenant)
        .expect("tenant semaphore");
    let _held = Arc::clone(&sem)
        .try_acquire_owned()
        .expect("the only tenant permit");

    let task = {
        let (v, pt) = (Arc::clone(&verifier), pt.clone());
        tokio::spawn(async move { v.verify(&pt).await })
    };

    // Wait for the flight to actually be parked holding the global permit.
    for _ in 0..500 {
        if verifier.argon2_permits.available_permits() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        verifier.argon2_permits.available_permits(),
        0,
        "precondition: the flight must be parked HOLDING the global permit"
    );

    // The only awaiter goes away — an ordinary client disconnect.
    task.abort();

    // The run is owned by the runtime, so its bounded wait still fires and
    // the permit comes back. Budget generously past ARGON2_PERMIT_WAIT.
    for _ in 0..200 {
        if verifier.argon2_permits.available_permits() == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("an abandoned flight pinned the global Argon2id permit — a cancelled request must never strand one");
}

/// The response "shape" a caller can actually observe on the wire, so the
/// two 401 arms can be compared as data rather than by eye.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Observed {
    Ok,
    Unauthorized,
    Overloaded,
}

fn observe(r: &Result<String, VerifyError>) -> Observed {
    match r {
        Ok(_) => Observed::Ok,
        Err(VerifyError::InvalidPat) => Observed::Unauthorized,
        Err(VerifyError::Backend(_)) => Observed::Overloaded,
    }
}

/// Fire `burst` concurrent copies of ONE plaintext at `verifier` and return
/// what each caller observed.
async fn burst_observations(
    verifier: &Arc<PatVerifier>,
    plaintext: &str,
    burst: usize,
) -> Vec<Observed> {
    let mut tasks = Vec::with_capacity(burst);
    for _ in 0..burst {
        let (v, pt) = (Arc::clone(verifier), plaintext.to_owned());
        tasks.push(tokio::spawn(async move { observe(&v.verify(&pt).await) }));
    }
    let mut out = Vec::with_capacity(burst);
    for t in tasks {
        out.push(t.await.expect("burst task"));
    }
    out
}

/// THE ORACLE TEST (the finding that killed the mutex version).
///
/// A burst of N copies of one WRONG-SECRET token must cost the same, and
/// look the same, whether that token's `token_id` exists or not. If only the
/// row-FOUND arm coalesced, the existing-`token_id` burst would serialise on
/// the permits and shed while the unknown-`token_id` burst sailed through —
/// making `token_id` liveness readable straight off the responses, in the
/// concurrency dimension, which is precisely the enumeration oracle the
/// dummy burn exists to close.
///
/// PROVEN RED without the fix (measured, with coalescing disabled): arm A —
/// `token_id` EXISTS, wrong secret, so every request pays its own failing
/// Argon2id — serialises on the single permit and comes back
/// `[Overloaded, Overloaded, Unauthorized, Overloaded, Overloaded,
/// Overloaded, Overloaded, Overloaded]`, i.e. 7 of 8 shed, while arm B —
/// `token_id` ABSENT — comes back `Unauthorized` 8 times out of 8. One burst,
/// one bit: the token_id is live. With the fix both arms are 8/8
/// `Unauthorized`.
include!("adapter_pat_tests_3/part-01.rs");
include!("adapter_pat_tests_3/part-02.rs");

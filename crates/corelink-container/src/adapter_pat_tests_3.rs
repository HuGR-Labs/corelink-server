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
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_concurrent_burst_cannot_reveal_whether_the_token_id_exists() {
        const BURST: usize = 8;
        let key = test_key();
        // Arm A: a valid-HMAC PAT whose token_id IS in D1 — but the stored hash
        // belongs to a different secret, so every request pays a full (failing)
        // Argon2id. This is the "known token_id, wrong secret" 401.
        let (pt_known, tid_known, _hash_known, tenant) = mint_pat(&key, 93, SCOPE_CACHE_RW);
        let (_pt_foreign, _tid_foreign, foreign_hash, _t) = mint_pat(&key, 94, SCOPE_CACHE_RW);
        // Arm B: a valid-HMAC PAT whose token_id is NOT in D1 at all — the
        // "unknown/expired/revoked token_id" 401, served by the dummy burn.
        let (pt_unknown, _tid_unknown, _h, _t2) = mint_pat(&key, 95, SCOPE_CACHE_RW);

        let lookup = Arc::new(fake_with_rows(vec![(
            tid_known,
            row(&foreign_hash, &tenant, "cas:rw"),
        )]));
        // One Argon2id at a time, per-tenant sub-cap 1 — which is ALSO the cap
        // on the shared UNKNOWN_TOKEN_BUCKET, so neither arm is given an
        // advantage the other lacks.
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        let arm_known = burst_observations(&verifier, &pt_known, BURST).await;
        let arm_unknown = burst_observations(&verifier, &pt_unknown, BURST).await;

        assert_eq!(
            arm_known,
            vec![
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
                Observed::Unauthorized,
            ],
            "a burst of one wrong-secret token for a LIVE token_id must be a \
             uniform 401 — any shed here is the enumeration oracle"
        );
        assert_eq!(
            arm_known, arm_unknown,
            "the two 401 arms must be indistinguishable under a concurrent burst"
        );

        // …and a rejected flight must never leave a proof behind: the burst ran
        // a FAILING Argon2id, so the memo must still be empty.
        assert_eq!(
            verifier.secret_match_memo.len_for_test(),
            0,
            "a rejected verify must never populate the memo"
        );
    }

    /// The GLOBAL load-shed answers the SAME status on both 401 arms — pinned
    /// here at ZERO permits, the one saturation an acquire can never win.
    ///
    /// The asymmetry itself (row-FOUND ⇒ `Backend`/503, unknown-`token_id` ⇒
    /// `InvalidPat`/401, an enumeration oracle readable with no timing
    /// measurement at all) was closed on `main` by #1034 —
    /// `shed_is_indistinguishable_between_live_and_unknown_rows` above is its
    /// oracle test. What THIS test adds is that coalescing does not undo it:
    /// the acquire whose failure produces the shed now lives inside the flight
    /// future, and an unresolvable acquire there must still surface as the
    /// identical `Backend` on both arms.
    #[tokio::test]
    async fn both_401_arms_shed_alike_when_the_global_pool_is_exhausted() {
        let key = test_key();
        let (pt_known, tid, hash, tenant) = mint_pat(&key, 96, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 97, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // ZERO global permits ⇒ the acquire can never succeed on either arm.
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 0);

        let known = verifier.verify(&pt_known).await;
        let unknown = verifier.verify(&pt_unknown).await;
        assert_eq!(observe(&known), Observed::Overloaded);
        assert_eq!(
            observe(&unknown),
            Observed::Overloaded,
            "an unknown token_id must shed with the SAME status as a live one — \
             the status must not encode whether the row exists"
        );
    }

    /// …and so does the PER-TENANT tier, which is what
    /// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` (#1034) requires and what this test
    /// pins THROUGH the coalescer.
    ///
    /// `saturated_dummy_burn_bucket_sheds_an_unknown_row_as_backend` above pins
    /// the row-NOT-FOUND half on its own. This pins the PAIR on one verifier, at
    /// the per-tenant tier specifically, because that is the tier the coalescing
    /// rewrote: the acquires now live INSIDE the flight future, so a resolution
    /// that mapped a flight's per-tenant shed back to `InvalidPat` would
    /// re-open the row-existence oracle in the exact place the shed moved to.
    /// Each arm's own bucket is saturated (the shared
    /// [`UNKNOWN_TOKEN_BUCKET`] for the unknown row, the tenant's own bucket for
    /// the live one) with the global pool left ample, so ONLY the per-tenant
    /// tier can reject and both arms must answer identically.
    #[tokio::test]
    async fn both_401_arms_shed_alike_when_the_per_tenant_tier_is_saturated() {
        let key = test_key();
        let (pt_known, tid, hash, tenant) = mint_pat(&key, 98, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 99, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global pool ample (8) so ONLY the per-tenant tier can reject; every
        // per-tenant bucket has a sub-cap of 1.
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 1);

        // Saturate BOTH buckets: the shared synthetic one the dummy burn uses…
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let _burn_held = Arc::clone(&burn_bucket)
            .try_acquire_owned()
            .expect("the only dummy-bucket permit");
        // …and the live row's own tenant bucket.
        let tenant_bucket = verifier
            .per_tenant_semaphore(&tenant)
            .expect("tenant bucket");
        let _tenant_held = Arc::clone(&tenant_bucket)
            .try_acquire_owned()
            .expect("the only tenant permit");

        let known = verifier.verify(&pt_known).await;
        let unknown = verifier.verify(&pt_unknown).await;
        assert_eq!(
            observe(&known),
            Observed::Overloaded,
            "a per-tenant shed on the row-FOUND arm must be a 503, got {known:?}"
        );
        assert_eq!(
            observe(&unknown),
            Observed::Overloaded,
            "a per-tenant shed on the row-NOT-FOUND arm must be the SAME 503 — \
             the status must not encode whether the row exists, got {unknown:?}"
        );
        match (&known, &unknown) {
            (Err(VerifyError::Backend(a)), Err(VerifyError::Backend(b))) => assert_eq!(
                a, b,
                "the two per-tenant shed arms must be byte-identical too"
            ),
            other => panic!("both arms must shed as Backend, got {other:?}"),
        }

        // CONTROL — without it this test would pass against a verifier that
        // 503s everything. Release the buckets and the same two PATs must go
        // back to their ordinary verdicts (resolve / uniform 401).
        drop(_burn_held);
        drop(_tenant_held);
        assert_eq!(observe(&verifier.verify(&pt_known).await), Observed::Ok);
        assert_eq!(
            observe(&verifier.verify(&pt_unknown).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
    }

    /// THE ACCEPTED DIVERGENCE, pinned. `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` is
    /// scoped to saturation of a tier BOTH arms share — the global pool, or a
    /// per-tenant tier where each arm's own bucket is full. There is exactly one
    /// state in between, which neither
    /// `shed_is_indistinguishable_between_live_and_unknown_rows` (it drains the
    /// GLOBAL pool) nor
    /// `both_401_arms_shed_alike_when_the_per_tenant_tier_is_saturated` (it
    /// saturates BOTH per-tenant buckets) reaches:
    ///
    /// > the shared [`UNKNOWN_TOKEN_BUCKET`] is saturated while the live
    /// > tenant's OWN bucket is free and the global pool is ample.
    ///
    /// There the two arms legitimately diverge — the unknown/expired/revoked
    /// `token_id` sheds `Backend("pat verifier overloaded")` (⇒ 503) while a
    /// live PAT routes to its own unsaturated bucket, pays a real Argon2id and
    /// resolves. That is not a regression of the uniformity rule; it is the
    /// direct consequence of the finding-#12 defence that gives the row-NOT-FOUND
    /// path its own capped bucket precisely so a leaked-key flood across bogus
    /// `token_id`s cannot drain the pool real tenants need.
    ///
    /// ⚠️ DO NOT "FIX" THIS INTO UNIFORMITY. It is accepted, and here is why it
    /// is not an exploitable row-existence oracle:
    ///
    ///  - **Reaching either arm requires the PAT signing key.** Both arms sit
    ///    downstream of the HMAC fast-reject (step 1 of
    ///    [`PatVerifier::verify_capability`]); without the key every probe is an
    ///    `InvalidPat` that never touches D1, a permit, or a bucket — see
    ///    `forged_token_does_not_touch_argon2_permits`. Note what this does
    ///    **not** say: the key alone grants no access (a PAT still needs a live
    ///    D1 row whose `pat_hash` Argon2id-verifies the presented secret — step
    ///    3 below, `verify_with_hash_multi`; with no row the pipeline can never
    ///    return `Ok`), and a key-holder **can** mint `token_id.<any secret>`
    ///    with a valid HMAC, so "valid signature, wrong secret" is a state they
    ///    construct at will and a liveness bit **is** useful to them. The key
    ///    requirement raises the bar; it is not what closes the oracle. The
    ///    bullets below are.
    ///  - **The one edge-unauthenticated surface collapses both arms anyway.**
    ///    OCI `/token` (`routes/oci.rs`) maps `InvalidPat` AND
    ///    `Backend(..)` to the same scrubbed `401 authentication failed (ref:…)`
    ///    envelope — pinned by `oci::tests::token_backend_fault_is_opaque_to_unauth_caller`
    ///    — so the divergence is not observable there even with the key. This
    ///    bullet now carries more of the argument than it used to: it is the
    ///    only surface that is reachable unauthenticated, and it is the one that
    ///    collapses the two arms outright.
    ///  - **Every surface that DOES distinguish the two arms is gated.** On
    ///    cargo/brew/npm/pip a shed is `503` + `Retry-After` and `InvalidPat` is
    ///    `401` — divergent — but the Worker's `extractAuth` resolves the `pat`
    ///    row at the edge and 401s `pat_not_found` before forwarding
    ///    (`worker/src/index.ts:1277-1279`), and the verify cache is
    ///    positives-only (≤5 s isolate, `worker/src/lib/pat_verify_cache.ts:153`;
    ///    ≤60 s KV, same file `:100`). So this arm is unreachable there except
    ///    for a `token_id` that was positively cached and then revoked inside
    ///    that window — which required authenticating with the real secret, i.e.
    ///    a liveness the prober already knew.
    ///    `/internal/v1/auth/introspect` distinguishes them too (200
    ///    `{valid:false}` vs 503, `routes/auth_introspect.rs:634-656`,
    ///    deliberately not special-cased) but demands the internal-auth key
    ///    **on top of** the signing key, and without saturation both arms there
    ///    return the same 200.
    ///  - **Timing is NOT covered in this state — do not imply it is.** On a
    ///    shed the burn is skipped (the `try_acquire` above returns before the
    ///    `spawn_blocking`), so the unknown arm answers with no Argon2id at all
    ///    while the live arm pays a full one. Since the bucket acquire on this
    ///    arm is NON-BLOCKING ([`PerTenantGate::try_acquire`], #1046) that
    ///    answer comes back IMMEDIATELY rather than after `ARGON2_PERMIT_WAIT`,
    ///    so the gap is wider than the pre-#1046 order left, not narrower.
    ///    `INV-AUTH-CONSTANT-TIME-COLD-PAD` holds only when the burn actually
    ///    runs; citing it here would be exactly backwards. This residual is
    ///    accepted for the same reason as the status divergence — signing key
    ///    required, no ungated surface exposes it — not because the pad covers
    ///    it.
    ///
    /// What this test defends is the SHAPE of the state, so a future refactor
    /// cannot slide the live arm into the shared bucket (which would turn one
    /// bogus-token flood into a fleet-wide 503) or the unknown arm out of it
    /// (which would restore the drain vector) without turning this red. The
    /// wrong-secret arm (1b) is what makes the pinned bit unambiguous: with the
    /// credential held wrong on BOTH sides, the divergence can only be encoding
    /// row existence.
    #[tokio::test]
    async fn a_saturated_shared_burn_bucket_sheds_only_the_unknown_arm() {
        let key = test_key();
        let (pt_live, tid, hash, tenant) = mint_pat(&key, 100, SCOPE_CACHE_RW);
        let (pt_unknown, _tid2, _h2, _t2) = mint_pat(&key, 101, SCOPE_CACHE_RW);
        // The WRONG-SECRET arm: its token_id IS in D1, but the stored hash
        // belongs to a different secret, so the Argon2id runs and fails. Its row
        // carries the LIVE tenant string on purpose — same per-tenant bucket,
        // same headroom, so the only thing separating it from `pt_unknown` is
        // row existence. (Idiom borrowed from
        // `a_concurrent_burst_cannot_reveal_whether_the_token_id_exists`.)
        let (pt_wrong, tid_wrong, _h3, _t3) = mint_pat(&key, 103, SCOPE_CACHE_RW);
        let (_pt_foreign, _tid4, foreign_hash, _t4) = mint_pat(&key, 104, SCOPE_CACHE_RW);
        let lookup = Arc::new(fake_with_rows(vec![
            (tid, row(&hash, &tenant, "cas:rw")),
            (tid_wrong, row(&foreign_hash, &tenant, "cas:rw")),
        ]));
        // Ample global pool and the per-tenant sub-cap at its PRODUCTION value,
        // so the only thing that can reject is a bucket this test holds itself.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            64,
            ARGON2_PER_TENANT_PERMITS,
        );

        // Saturate ONLY the shared synthetic bucket the dummy burn routes through.
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let held: Vec<_> = (0..ARGON2_PER_TENANT_PERMITS)
            .map(|_| {
                Arc::clone(&burn_bucket)
                    .try_acquire_owned()
                    .expect("burn-bucket permit")
            })
            .collect();
        assert_eq!(burn_bucket.available_permits(), 0, "burn bucket saturated");
        // …and prove the live tenant's OWN bucket is untouched, so an Ok below
        // cannot be explained by the live arm having been given headroom the
        // unknown arm lacked at the GLOBAL tier.
        let tenant_bucket = verifier
            .per_tenant_semaphore(&tenant)
            .expect("tenant bucket");
        assert_eq!(
            tenant_bucket.available_permits(),
            ARGON2_PER_TENANT_PERMITS,
            "the live tenant's own bucket must be FREE — that is the whole point"
        );
        assert!(
            verifier.argon2_permits.available_permits() >= ARGON2_PER_TENANT_PERMITS,
            "the global pool must stay ample so no shed can be attributed to it"
        );

        // Arm 1 — the unknown/revoked token_id sheds on the shared bucket.
        let unknown = verifier.verify(&pt_unknown).await;
        match &unknown {
            Err(VerifyError::Backend(m)) => assert_eq!(
                m, "pat verifier overloaded",
                "the shared-bucket shed must be the ordinary overloaded signal"
            ),
            other => panic!("expected Backend(pat verifier overloaded), got {other:?}"),
        }

        // Arm 1b — THE BIT THIS TEST IS ACTUALLY ABOUT, isolated. Arm 1 vs arm 2
        // pairs "no row" against "row + CORRECT secret", which conflates two
        // different facts: row existence and credential correctness. This arm
        // holds the credential WRONG in both cases and varies only the row, so
        // what the divergence encodes is unambiguous — and it is the state an
        // attacker with the signing key actually drives, since they can mint
        // `token_id.<any secret>` at will but cannot produce a secret matching a
        // hash they have never seen. Its row is live, so it never consults the
        // shared bucket: it routes to the (free) tenant bucket, pays a full
        // failing Argon2id, and answers the uniform 401.
        let wrong = verifier.verify(&pt_wrong).await;
        match &wrong {
            Err(VerifyError::InvalidPat) => {}
            other => panic!("expected InvalidPat for a live row + wrong secret, got {other:?}"),
        }
        assert_ne!(
            observe(&wrong),
            observe(&unknown),
            "this is the ACCEPTED divergence being pinned: with the credential \
             held wrong on both sides, the row-NOT-FOUND arm sheds and the \
             row-FOUND arm 401s. If these ever match, the shape this test \
             defends has changed — re-derive the safety argument above, do not \
             just relax the assertion"
        );

        // Arm 2 — THE HALF THAT MAKES THIS A PIN RATHER THAN A TAUTOLOGY. In the
        // same instant, on the same verifier, a live PAT still resolves: it never
        // consults the shared bucket, so a bogus-token flood cannot 503 a paying
        // tenant.
        let live = verifier.verify(&pt_live).await;
        assert_eq!(
            observe(&live),
            Observed::Ok,
            "a saturated dummy-burn bucket must NOT shed a live PAT, got {live:?}"
        );
        assert_eq!(live.expect("live PAT resolves"), tenant);

        // CONTROL — release the shared bucket and the unknown arm goes back to
        // the uniform 401. Without this the test would pass just as happily
        // against a verifier that 503s every unknown token unconditionally.
        drop(held);
        assert_eq!(
            observe(&verifier.verify(&pt_unknown).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
        // …and with the bucket released the two wrong-credential arms become
        // indistinguishable again, which is what bounds the divergence to the
        // saturated state rather than making it a standing property.
        assert_eq!(
            observe(&verifier.verify(&pt_wrong).await),
            observe(&verifier.verify(&pt_unknown).await),
            "outside saturation the row-FOUND and row-NOT-FOUND wrong-credential \
             arms must be indistinguishable"
        );
    }

    /// THE ACQUIRE-ORDER PIN. `a_saturated_shared_burn_bucket_sheds_only_the_unknown_arm`
    /// above proves the shared bucket sheds the right arm; this proves the shed
    /// is FREE, which is the property finding #12 was actually claiming.
    ///
    /// The bucket caps concurrent dummy BURNS at the sub-cap by construction.
    /// It does NOT, by construction, cap this arm's occupancy of the GLOBAL
    /// pool: with the original global-then-bucket order, a request already
    /// destined to shed first took a global permit and then queued on the full
    /// bucket for the whole `ARGON2_PERMIT_WAIT`, holding pool capacity hostage
    /// for 250 ms in order to burn nothing. At the rate the bucket itself
    /// admits, that pinned essentially the entire pool — the drain the bucket
    /// exists to close, merely displaced one step upstream. Taking the bucket
    /// FIRST, and non-blockingly, is what turns the sub-cap into a real bound on
    /// the arm's global footprint.
    ///
    /// So a status assertion cannot carry this test: both orders shed, and both
    /// shed with the same message. What separates them is whether the pool was
    /// OCCUPIED while they did it, which is measured here directly — first for
    /// one request, then for a flood.
    ///
    /// ⚠️ A note on what this test deliberately does NOT assert, because the
    /// obvious formulation is a coin flip: "a concurrent live PAT still
    /// resolves" does not separate the two orders. `tokio::sync::Semaphore` is
    /// FIFO-fair, so a live request that queues behind the flood waits for
    /// exactly ONE release — and the pre-fix arm holds its permit for exactly
    /// `ARGON2_PERMIT_WAIT`, the same budget the live request is waiting under.
    /// Measured, the pre-fix order let the live PAT through about half the
    /// time. What a shedding request provably costs is CAPACITY, so capacity is
    /// what gets asserted: an outside consumer must be able to take the whole
    /// pool while the flood is in flight.
    ///
    /// PROVEN RED against the pre-fix order (measured, by reverting the arm to
    /// the bounded `acquire` after the global permit): phase 1 reports
    /// `parked the global permit on 30-38 of 40 samples`, and with phase 1
    /// neutralised phase 2 reports `only 0 of 4 global permits were free while
    /// 4 bogus token_ids shed` on 3 runs of 3. Both are green on 3 runs of 3
    /// with the fix.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_shed_on_the_shared_burn_bucket_holds_no_global_permit() {
        const FLOOD: usize = 4;
        let key = test_key();
        let (pt_live, tid, hash, tenant) = mint_pat(&key, 102, SCOPE_CACHE_RW);
        // FLOOD + 1 DISTINCT bogus PATs. Distinct plaintexts ⇒ distinct flight
        // keys ⇒ the coalescer cannot fold them into ONE burn, which is what
        // makes this a flood rather than one request wearing N hats.
        let (pt_probe, _tid_p, _h_p, _t_p) = mint_pat(&key, 200, SCOPE_CACHE_RW);
        let flood_pats: Vec<String> = (0..FLOOD)
            .map(|i| mint_pat(&key, 201 + i as u128, SCOPE_CACHE_RW).0)
            .collect();
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global pool exactly FLOOD, so the flood is *able* to take all of it,
        // and a per-bucket sub-cap of one so a single held permit saturates the
        // shared burn bucket.
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            FLOOD,
            1,
        ));
        let burn_bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let held = Arc::clone(&burn_bucket)
            .try_acquire_owned()
            .expect("the only burn-bucket permit");
        assert_eq!(burn_bucket.available_permits(), 0, "burn bucket saturated");

        // ── PHASE 1: one request, sampled. It meets the saturated bucket and
        // is going to shed. Watch the pool across a 200 ms window — far wider
        // than a correct shed needs, and well inside the 250 ms the pre-fix
        // order would have parked for. The pool must stay whole throughout.
        let probe = {
            let (v, pt) = (Arc::clone(&verifier), pt_probe.clone());
            tokio::spawn(async move { observe(&v.verify(&pt).await) })
        };
        let mut parked = 0usize;
        for _ in 0..40 {
            if verifier.argon2_permits.available_permits() != FLOOD {
                parked += 1;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            probe.await.expect("probe task"),
            Observed::Overloaded,
            "precondition: the probe must actually have SHED on the shared bucket"
        );
        assert_eq!(
            parked, 0,
            "parked the global permit on {parked} of 40 samples — a request \
             destined to shed must never occupy the pool"
        );

        // ── PHASE 2: the flood, and the property stated as capacity. FLOOD
        // distinct bogus token_ids, all shedding on the saturated bucket, are
        // in flight. An outside consumer — standing in for the concurrent live
        // traffic the pool exists to serve — must be able to take EVERY global
        // permit, because a shed holds none.
        let flood: Vec<_> = flood_pats
            .iter()
            .map(|pt| {
                let (v, pt) = (Arc::clone(&verifier), pt.clone());
                tokio::spawn(async move { observe(&v.verify(&pt).await) })
            })
            .collect();
        // Sampled INSIDE the pre-fix park window (250 ms), so the two orders
        // are distinguished by capacity and not by who woke up first.
        tokio::time::sleep(Duration::from_millis(30)).await;
        let grabbed: Vec<_> = (0..FLOOD)
            .filter_map(|_| {
                Arc::clone(&verifier.argon2_permits)
                    .try_acquire_owned()
                    .ok()
            })
            .collect();
        assert_eq!(
            grabbed.len(),
            FLOOD,
            "only {} of {FLOOD} global permits were free while {FLOOD} bogus \
             token_ids shed — requests that ran no Argon2id at all were holding \
             the pool a paying tenant needs",
            grabbed.len()
        );
        drop(grabbed);
        for (i, t) in flood.into_iter().enumerate() {
            assert_eq!(
                t.await.expect("flood task"),
                Observed::Overloaded,
                "flood request {i} must still shed as the uniform overloaded signal"
            );
        }

        // CONTROLS. (a) The pool is genuinely usable, not merely idle: a live
        // PAT resolves with the burn bucket still saturated…
        let live = verifier.verify(&pt_live).await;
        assert_eq!(observe(&live), Observed::Ok, "live PAT must resolve");
        assert_eq!(live.expect("live PAT resolves"), tenant);
        // …and (b) releasing the bucket returns the unknown arm to the uniform
        // 401, so none of the above is a verifier that just 503s everything.
        drop(held);
        assert_eq!(
            observe(&verifier.verify(&pt_probe).await),
            Observed::Unauthorized,
            "outside saturation an unknown row must be the uniform 401, never a 503"
        );
    }

    // ── The co-read statement (PAT_URL_MAP_COREAD_SQL) ────────────────────────

    /// ⛔ The co-read must not be able to weaken revocation or expiry. Its `pat`
    /// arm carries EVERY predicate of the serial [`PAT_LOOKUP_SQL`], so a
    /// revoked or expired row is filtered out by the same SQL either way — the
    /// #1022 guarantee is a property of the statement, and this asserts it as
    /// one rather than trusting the two to stay in sync by eye.
    #[test]
    fn the_coread_pat_arm_keeps_every_serial_filter() {
        for predicate in [
            "token_id = ?1",
            "expires_ms = 0",
            "expires_ms > unixepoch('now', 'subsec') * 1000",
            "revoked_at_ms IS NULL",
        ] {
            assert!(
                PAT_LOOKUP_SQL.contains(predicate),
                "the serial statement lost `{predicate}` — update this test WITH \
                 the security review that removed it"
            );
            assert!(
                PAT_URL_MAP_COREAD_SQL.contains(predicate),
                "the co-read's pat arm is missing `{predicate}`: it would decide \
                 expiry/revocation differently from the serial read, which is \
                 exactly the guarantee #1022 kept the per-request read for"
            );
        }
    }

    /// The map arm is keyed by BOTH columns of the url-map's unique key, and the
    /// two arms are discriminated by an explicit `kind` literal — a compound
    /// `SELECT` has no row order without `ORDER BY`, so positional attribution
    /// would eventually hand a `content_hash` to the PAT verifier.
    #[test]
    fn the_coread_map_arm_is_fully_keyed_and_tagged() {
        assert!(PAT_URL_MAP_COREAD_SQL.contains("namespace = ?2 AND url_hash = ?3"));
        assert!(PAT_URL_MAP_COREAD_SQL.contains("'p' AS kind"));
        assert!(PAT_URL_MAP_COREAD_SQL.contains("'m' AS kind"));
        // Both arms are individually LIMIT-ed via the subquery wrapper (a bare
        // LIMIT in a compound arm binds to the whole compound in SQLite).
        assert_eq!(PAT_URL_MAP_COREAD_SQL.matches("LIMIT 1").count(), 2);
    }

    /// The two statements decode through ONE parser, so a NULL `scope` still
    /// fails CLOSED and `find_only` still needs an exact `1` — on both paths.
    #[test]
    fn column_decoding_is_shared_and_fails_closed() {
        let null = serde_json::Value::Null;
        let one = serde_json::Value::from(1);
        let tenant = serde_json::Value::String("t".to_owned());
        let hash = serde_json::Value::String("h".to_owned());

        let legacy = pat_row_from_columns(Some(&tenant), Some(&hash), Some(&null), None, None)
            .expect("a legacy row is not a backend error");
        assert_eq!(legacy.scope, "", "NULL scope ⇒ \"\" ⇒ fail-CLOSED gate");
        assert!(!legacy.find_only, "absent find_only ⇒ a normal PAT");
        assert!(
            !legacy.runner_job,
            "absent runner_job_ac_key ⇒ a normal PAT (the pre-0086 behaviour)"
        );

        let find_only =
            pat_row_from_columns(Some(&tenant), Some(&hash), Some(&null), Some(&one), None)
                .expect("row decodes");
        assert!(find_only.find_only);

        // 0086: PRESENCE decides, not the value. The launch value is the literal
        // `"*"`; a future value is a BLAKE3 AC key. Both are narrowed, and an
        // explicit SQL NULL (the co-read's url-map arm) is NOT.
        let star = serde_json::Value::String("*".to_owned());
        let ac_key = serde_json::Value::String("a".repeat(64));
        for marker in [&star, &ac_key] {
            let row =
                pat_row_from_columns(Some(&tenant), Some(&hash), Some(&null), None, Some(marker))
                    .expect("row decodes");
            assert!(
                row.runner_job,
                "any non-NULL runner_job_ac_key ⇒ a narrowed runner-job PAT"
            );
        }
        let explicit_null =
            pat_row_from_columns(Some(&tenant), Some(&hash), Some(&null), None, Some(&null))
                .expect("row decodes");
        assert!(
            !explicit_null.runner_job,
            "an explicit JSON null is an ABSENT marker, not a narrowed PAT"
        );

        // A missing REQUIRED column is a backend fault, never a silent default.
        assert_eq!(
            pat_row_from_columns(None, Some(&hash), None, None, None),
            Err("D1 pat: missing `tenant_id` column".to_owned())
        );
        assert_eq!(
            pat_row_from_columns(Some(&tenant), None, None, None, None),
            Err("D1 pat: missing `pat_hash` column".to_owned())
        );
    }

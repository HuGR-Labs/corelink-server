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

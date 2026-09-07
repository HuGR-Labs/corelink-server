    /// The row-FOUND half of the same contract — existing behaviour, pinned
    /// here in the one-permit idiom so a future refactor cannot quietly
    /// re-asymmetrise the pair by changing only this side.
    #[tokio::test]
    async fn saturated_pool_sheds_a_live_row_as_backend() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 92, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Drain BEFORE the first verify so the PAT is never memoised (a warm
        // memo hit consumes no permit and would sail straight through).
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a shed on the row-FOUND arm must be Backend(overloaded), got {err:?}"
        );
    }

    /// THE ORACLE TEST. On ONE saturated verifier, a live-row PAT and an
    /// unknown-row PAT must produce byte-identical errors — same variant, same
    /// message. Anything an attacker could `!=` on here is a row-existence
    /// oracle.
    #[tokio::test]
    async fn shed_is_indistinguishable_between_live_and_unknown_rows() {
        let key = test_key();
        let (pt_live, tid_live, hash_live, tenant_live) = mint_pat(&key, 93, SCOPE_CACHE_RW);
        let (pt_unknown, _tid_u, _hash_u, _tenant_u) = mint_pat(&key, 94, SCOPE_CACHE_RW);
        // Only the FIRST token_id has a row; the second is unknown to D1.
        let lookup = Arc::new(FakeLookup::with_row(
            &tid_live,
            row(&hash_live, &tenant_live, "cas:rw"),
        ));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err_live = verifier.verify(&pt_live).await.unwrap_err();
        let err_unknown = verifier.verify(&pt_unknown).await.unwrap_err();

        match (&err_live, &err_unknown) {
            (VerifyError::Backend(a), VerifyError::Backend(b)) => assert_eq!(
                a, b,
                "the two shed arms must be byte-identical (row-existence oracle)"
            ),
            other => panic!("both arms must shed as Backend, got {other:?}"),
        }
    }

    /// The anti-blanket-conversion control: with the pool NOT saturated, an
    /// unknown row must still be the uniform `InvalidPat` (401). If this ever
    /// flips to `Backend`, the fix stopped being a shed rule and became "the
    /// verifier answers 503 for every unknown token", which would 503 the whole
    /// world on a single bad credential.
    #[tokio::test]
    async fn unsaturated_pool_still_rejects_an_unknown_row_as_invalid_pat() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 95, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        // Permits available ⇒ the dummy burn actually runs ⇒ terminal InvalidPat.
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 2);

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "an unknown row with permits free must stay InvalidPat, got {err:?}"
        );
        assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
    }

    /// The HMAC fast-reject is UPSTREAM of every permit, so a forged token can
    /// never be pushed onto the shed path — it stays `InvalidPat` even with the
    /// pool fully drained. This is what stops an attacker without the signing
    /// key from using saturation to turn 401s into 503s (or from probing the
    /// shed at all).
    #[tokio::test]
    async fn forged_token_stays_invalid_pat_under_saturation() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "a forged token must fast-reject ahead of the shed, got {err:?}"
        );
        assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
    }

    /// With permits available, the hot path still succeeds end-to-end — the
    /// gate is transparent under normal load (no regression to the verify
    /// pipeline).
    #[tokio::test]
    async fn verify_succeeds_when_permits_available() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 2);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    // ------------------------------------------------------------------
    // Finding #1 — per-tenant fairness (concurrency).
    //
    // NOTE ON TOOLING: the ideal tester here is SHUTTLE (deterministic async
    // interleaving for `tokio::sync::Semaphore`), but shuttle is NOT a
    // dependency anywhere in this workspace (no Cargo.lock entry, no usage) and
    // retrofitting it is heavy — it requires swapping `tokio::sync` for its
    // shimmed primitives AND it cannot model the `spawn_blocking` + real
    // Argon2id crypto on the verify path. Per the WP brief, we do NOT half-wire
    // shuttle; the deterministic-stress `#[tokio::test]` below proves the
    // fairness INVARIANT directly. Wiring shuttle (a feature-gated
    // sync-primitive swap on this module) is the owner-aware follow-up.
    // ------------------------------------------------------------------

    /// FAIRNESS INVARIANT (finding #1): a single tenant A that has SATURATED its
    /// per-tenant Argon2id sub-cap must NOT be able to deny tenant B its verify,
    /// even though A could in principle hold many global permits. We prove this
    /// directly by exhausting tenant A's per-tenant semaphore (held open) and
    /// showing a fresh B verify still succeeds while global headroom remains.
    ///
    /// Setup: global cap = 8 (plenty), per-tenant cap = 2. We pre-acquire BOTH
    /// of tenant A's per-tenant permits and hold them — modelling "A is at its
    /// fair share". A third A-verify is then capped at the per-tenant gate
    /// (fail-CLOSED overloaded), while B — a DIFFERENT tenant — sails through on
    /// its own untouched per-tenant semaphore. This is the anti-starvation
    /// property: one tenant's flood is contained to its own bucket.
    #[tokio::test]
    async fn per_tenant_cap_contains_one_tenant_without_starving_another() {
        let key = test_key();
        let (pt_a, tid_a, hash_a, tenant_a) = mint_pat(&key, 80, SCOPE_CACHE_RW);
        let (pt_b, tid_b, hash_b, tenant_b) = mint_pat(&key, 81, SCOPE_CACHE_RW);
        assert_ne!(tenant_a, tenant_b);

        let mut rows = HashMap::new();
        rows.insert(tid_a.clone(), row(&hash_a, &tenant_a, "cas:rw"));
        rows.insert(tid_b.clone(), row(&hash_b, &tenant_b, "cas:rw"));
        let lookup = Arc::new(FakeLookup {
            rows,
            calls: AtomicUsize::new(0),
            backend_err: None,
        });

        // Global cap 8 (ample headroom), per-tenant sub-cap 2.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            2,
        );

        // Saturate tenant A's per-tenant semaphore by HOLDING both of its
        // permits — model A's in-flight flood occupying its whole fair share.
        let sem_a = verifier
            .per_tenant_semaphore(&tenant_a)
            .expect("tenant A semaphore");
        let _hold1 = Arc::clone(&sem_a).try_acquire_owned().expect("permit 1");
        let _hold2 = Arc::clone(&sem_a).try_acquire_owned().expect("permit 2");
        assert_eq!(sem_a.available_permits(), 0, "A at its per-tenant cap");

        // A THIRD verify for tenant A must be denied at the per-tenant gate
        // (fail-CLOSED overloaded) — A cannot exceed its fair share. Global
        // permits remain plentiful, so this is the per-tenant bound at work,
        // NOT the global one.
        let err_a = verifier.verify(&pt_a).await.unwrap_err();
        match err_a {
            VerifyError::Backend(m) => assert!(
                m.contains("overloaded"),
                "A beyond its cap must be overloaded, got {m}"
            ),
            other => panic!("expected Backend(overloaded) for A, got {other:?}"),
        }

        // CRUX: tenant B is NOT starved by A's saturation — B's verify runs on
        // its OWN per-tenant semaphore and global headroom, and SUCCEEDS.
        let resolved_b = verifier.verify(&pt_b).await.expect("B must not be starved");
        assert_eq!(resolved_b, tenant_b);
    }

    /// The GLOBAL bound still holds with the two-tier gate in place: with the
    /// global pool exhausted (0 permits) but a generous per-tenant cap, a verify
    /// is still rejected at the GLOBAL gate (acquired FIRST) — proving the
    /// per-tenant tier did not weaken the OOM/CPU guard, and that the global
    /// permit is acquired before the per-tenant one (consistent order).
    #[tokio::test]
    async fn global_bound_still_holds_under_two_tier_gate() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 82, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Global 0 ⇒ no global permit can ever be had; per-tenant 8 ⇒ the
        // per-tenant tier is wide open, so a rejection here can ONLY be the
        // global gate (which is acquired first).
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            0,
            8,
        );
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => {
                assert!(m.contains("overloaded"), "expected overloaded, got {m}")
            }
            other => panic!("expected Backend(overloaded), got {other:?}"),
        }
        assert_eq!(
            lookup.call_count(),
            1,
            "global gate sits after the D1 lookup"
        );
    }

    /// FAIL-SAFE: a poisoned per-tenant map must fall back to GLOBAL-only
    /// bounding — a bookkeeping fault must NEVER block a legitimate auth. We
    /// poison the lock, then assert a valid verify still succeeds (it proceeds
    /// under the global bound with `Ok(None)` from the per-tenant acquire).
    #[tokio::test]
    async fn poisoned_per_tenant_map_falls_back_to_global_only() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 83, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            2,
        );
        // Poison the per-tenant mutex by panicking while holding the guard.
        let gate = Arc::clone(&verifier.per_tenant);
        let _ = std::thread::spawn(move || {
            let _guard = gate.permits.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        assert!(
            verifier.per_tenant.permits.is_poisoned(),
            "precondition: map must be poisoned"
        );
        // Despite the poison, the legit verify still resolves (fail-safe to
        // global-only bounding — no auth blocked on bookkeeping).
        assert_eq!(
            verifier.verify(&pt).await.expect("fail-safe global-only"),
            tenant
        );
    }

    /// A single tenant flooding distinct PATs (each a DIFFERENT token_id, all
    /// owned by the SAME tenant) is bounded by that tenant's per-tenant cap:
    /// once its sub-cap is held, additional concurrent verifies for the same
    /// tenant are rejected — exactly the abuse vector finding #1 describes
    /// (distinct PATs each forcing a fresh Argon2id), now contained.
    #[tokio::test]
    async fn same_tenant_distinct_pats_share_one_per_tenant_bucket() {
        let key = test_key();
        // Two DISTINCT PATs (distinct token_ids + hashes) for the SAME tenant.
        let (pt1, tid1, hash1, tenant) = mint_pat(&key, 84, SCOPE_CACHE_RW);
        // Mint a second PAT for the same tenant id (84) — different principal
        // offset via the mint counter, so a distinct token_id/hash.
        let tenant_id = TenantId(Uuid::from_u128(84));
        let (plaintext2, pat2) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(99_999)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt2 = plaintext2.into_string();
        let tid2 = pat2.token_id.as_str().to_owned();
        let hash2 = pat2.hash.as_str().to_owned();
        assert_ne!(tid1, tid2, "distinct PATs ⇒ distinct token_ids");

        let mut rows = HashMap::new();
        rows.insert(tid1, row(&hash1, &tenant, "cas:rw"));
        rows.insert(tid2, row(&hash2, &tenant, "cas:rw"));
        let lookup = Arc::new(FakeLookup {
            rows,
            calls: AtomicUsize::new(0),
            backend_err: None,
        });

        // Per-tenant cap 1 so a single held permit saturates the tenant.
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 1);

        // Hold the tenant's single per-tenant permit (model PAT #1 in-flight).
        let sem = verifier.per_tenant_semaphore(&tenant).expect("tenant sem");
        let _hold = Arc::clone(&sem)
            .try_acquire_owned()
            .expect("hold the only permit");

        // A verify for PAT #2 (same tenant, different token_id) is denied at the
        // shared per-tenant bucket — the flood is contained per tenant.
        let err = verifier.verify(&pt2).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "second distinct PAT for the SAME tenant must hit the per-tenant cap, got {err:?}"
        );
        // (pt1 unused beyond minting — it shares the bucket identity with pt2.)
        let _ = pt1;
    }

    /// BOUNDED-MAP INVARIANT (#1/#12 follow-up): the per-tenant semaphore map
    /// must stay bounded under a churn of MANY distinct tenants, while an
    /// actively-contended tenant's bucket is NEVER evicted.
    ///
    /// Setup: map cap = 4 (tiny, so eviction fires fast). We pin tenant
    /// "active" by HOLDING a permit on its bucket (an in-flight verify ⇒ NOT
    /// fully idle ⇒ ineligible for eviction). Then we touch 50 fresh distinct
    /// tenant_ids through the SAME get-or-insert path the verify uses. We assert:
    ///   (a) the map never exceeds the cap (bounded — no unbounded creep), and
    ///   (b) the "active" tenant's exact `Arc<Semaphore>` is still resident and
    ///       identical (same allocation) — contention shields it from eviction.
    #[tokio::test]
    async fn per_tenant_map_is_bounded_but_keeps_contended_tenant() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        // Map cap 4; per-tenant cap 2; global plenty (irrelevant here — we
        // exercise per_tenant_semaphore directly).
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 2)
                .with_map_cap(4);

        // Pin an actively-contended tenant: create its bucket and HOLD one of
        // its permits, so it is NOT fully idle (available < per_tenant_cap) and
        // must never be evicted.
        let active = "active-tenant";
        let active_sem = verifier
            .per_tenant_semaphore(active)
            .expect("active tenant sem");
        let _hold = Arc::clone(&active_sem)
            .try_acquire_owned()
            .expect("hold one active permit");
        assert!(
            active_sem.available_permits() < 2,
            "active tenant has an in-flight permit ⇒ not idle"
        );
        let active_ptr = Arc::as_ptr(&active_sem);

        // Churn 50 distinct fresh tenants through the same path verify uses.
        for i in 0..50 {
            let t = format!("churn-tenant-{i}");
            let _ = verifier.per_tenant_semaphore(&t).expect("churn tenant sem");
            // (a) BOUNDED: the map never exceeds its cap under churn.
            assert!(
                verifier.per_tenant_map_len() <= 4,
                "map must stay bounded (≤ cap) — got {} at i={i}",
                verifier.per_tenant_map_len()
            );
        }

        // (b) The contended tenant survived the entire churn — same key AND the
        // SAME underlying semaphore allocation (never evicted+recreated).
        let still = verifier
            .per_tenant_semaphore(active)
            .expect("active tenant still resident");
        assert_eq!(
            Arc::as_ptr(&still),
            active_ptr,
            "an actively-contended tenant must NOT be evicted (same Arc)"
        );
        assert!(
            still.available_permits() < 2,
            "and its held permit is still accounted (no reset via eviction)"
        );
    }

    /// The shared synthetic dummy-burn bucket ([`UNKNOWN_TOKEN_BUCKET`]) must
    /// NEVER be evicted, even when it is fully idle and the LRU is over capacity
    /// and churning — evicting it would lose the bounded dummy-burn cap.
    #[tokio::test]
    async fn unknown_token_bucket_is_never_evicted() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits_per_tenant(lookup, vec![(*key).clone()], 8, 2)
                .with_map_cap(3);

        // Create the synthetic bucket (idle — no in-flight burn), exactly as the
        // None-row dummy-burn path does, then leave it untouched (LRU-stale).
        let dummy = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket");
        let dummy_ptr = Arc::as_ptr(&dummy);

        // Churn well past the cap. The dummy bucket is fully idle and becomes
        // the least-recently-used entry, yet must be exempt from eviction.
        for i in 0..30 {
            let t = format!("churn-{i}");
            let _ = verifier.per_tenant_semaphore(&t).expect("churn");
            assert!(verifier.per_tenant_map_len() <= 3, "bounded under churn");
        }

        let still = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("dummy bucket must still be resident");
        assert_eq!(
            Arc::as_ptr(&still),
            dummy_ptr,
            "UNKNOWN_TOKEN_BUCKET must never be evicted (same Arc)"
        );
    }

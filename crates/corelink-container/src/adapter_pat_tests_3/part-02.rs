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

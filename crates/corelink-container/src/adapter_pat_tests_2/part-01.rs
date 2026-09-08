    /// The stored PHC hash is part of the memo key, so re-hashing the row makes
    /// the old proof unreachable — the real Argon2id runs again and fails.
    #[tokio::test]
    async fn a_rehashed_row_is_never_served_from_the_memo() {
        let key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&key, 41, SCOPE_CACHE_RW);
        // A hash of a DIFFERENT secret — what a re-mint/rotation would write.
        let (_pt2, _tid2, foreign_hash, _tenant2) = mint_pat(&key, 42, SCOPE_CACHE_RW);
        let lookup = Arc::new(SwitchableLookup::new(
            Ok(Some(row(&hash, &tenant, "cas:rw"))),
            0,
        ));
        let verifier = PatVerifier::new(lookup.clone(), key);

        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

        lookup.set(Ok(Some(row(&foreign_hash, &tenant, "cas:rw"))));
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "the memo must not survive a pat_hash change, got {err:?}"
        );
    }

    /// A PAT whose SECRET is correct but whose SCOPE is rejected must keep
    /// paying full Argon2id on every attempt — it must never be memoised.
    ///
    /// This is the oracle the memo's placement past the step-4 scope gate
    /// closes. If the memo were populated as soon as the Argon2id succeeded, a
    /// live-but-insufficiently-scoped credential would 401 slowly ONCE and then
    /// ~instantly forever, which separates it on latency alone from a dead /
    /// unknown / wrong-secret token — and separates a revoked PAT from a merely
    /// scope-downgraded one. Every rejection must stay indistinguishable.
    ///
    /// Proven the same way as the hit test, in reverse: hold the verifier's only
    /// Argon2id permit, then present the scope-rejected PAT again. If it were
    /// memoised it would skip Argon2id and fall through to the scope gate for a
    /// fast `InvalidPat`; because it is NOT, it must block on the drained pool
    /// and surface the overloaded `Backend` instead.
    #[tokio::test]
    async fn a_scope_rejected_pat_is_never_memoised() {
        let key = test_key();
        // `SCOPE_CACHE_R` mints a genuine PAT; the D1 row then carries a scope
        // string that grants NOTHING, which is what the step-4 gate rejects.
        let (pt, tid, hash, tenant) = mint_pat(&key, 61, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

        // First attempt: real Argon2id runs, scope gate rejects.
        assert!(matches!(
            verifier.verify(&pt).await.unwrap_err(),
            VerifyError::InvalidPat
        ));

        // Drain the pool: no Argon2id can run from here on.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        // Second attempt MUST still try to run Argon2id ⇒ overloaded, not a fast
        // 401. A fast `InvalidPat` here would mean the scope-rejected PAT had
        // been memoised — the oracle.
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a scope-rejected PAT must re-pay Argon2id every request, got {err:?}"
        );
    }

    /// …and a scope-rejected BURST must not memoise either.
    ///
    /// This is the intersection of the scope-gate placement above and cold-miss
    /// coalescing, and it is the one way coalescing could quietly undo that fix:
    /// the flight is where the Argon2id proof lands, so writing the memo there
    /// would be the natural thing to do — and would be exactly the placement
    /// `a_scope_rejected_pat_is_never_memoised` forbids, reintroduced for every
    /// member of a burst at once. The proof stays inside the flight; the memo
    /// write stays outside it, past the gate, per request.
    ///
    /// A burst also cannot straddle the gate: the flight key binds `scope` and
    /// `find_only`, so every joiner reaches its leader's verdict.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_scope_rejected_burst_leaves_the_memo_empty() {
        const BURST: usize = 8;
        let key = test_key();
        // A genuine PAT whose D1 row grants NOTHING — rejected at step 4, after
        // a perfectly successful Argon2id.
        let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = Arc::new(PatVerifier::with_key_set_and_permits_per_tenant(
            lookup,
            vec![(*key).clone()],
            1,
            1,
        ));

        // Coalescing means the burst is admitted rather than shed (that is the
        // point of this PR), so every member reaches the scope gate and every
        // member must be a uniform 401 — never a fast one.
        assert_eq!(
            burst_observations(&verifier, &pt, BURST).await,
            vec![Observed::Unauthorized; BURST]
        );
        assert_eq!(
            verifier.secret_match_memo.len_for_test(),
            0,
            "not one member of a scope-rejected burst may leave a proof behind"
        );

        // And the behavioural check the placement test uses: with the pool
        // drained, the next attempt must still TRY to run Argon2id.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "after a scope-rejected burst the PAT must still re-pay Argon2id, got {err:?}"
        );
    }

    /// A scope DOWNGRADE must invalidate the memo, so a downgraded PAT stays
    /// indistinguishable from a revoked one.
    ///
    /// Without `scope` in the fingerprint, a PAT that verified successfully and
    /// was then downgraded (rather than revoked) would keep hitting the memo for
    /// the rest of the TTL and 401 in ~0 ms, while a REVOKED PAT still pays the
    /// slow dummy burn — telling the holder "downgraded, not revoked" on latency
    /// alone. Both are uniform on `main` and must stay uniform.
    ///
    /// Proven with the drained-permit technique: after the downgrade the verify
    /// must attempt a fresh Argon2id (⇒ `Backend` on an exhausted pool). A fast
    /// `InvalidPat` would mean the stale entry was still being served.
    #[tokio::test]
    async fn a_scope_downgrade_invalidates_the_memo() {
        let key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
        let lookup = Arc::new(SwitchableLookup::new(
            Ok(Some(row(&hash, &tenant, "cas:rw"))),
            0,
        ));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Warm the memo on the fully-scoped PAT.
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

        // Downgrade in D1 to a scope that grants nothing (NOT a revocation).
        lookup.set(Ok(Some(row(&hash, &tenant, ""))));

        // Drain the pool: no Argon2id can run from here on.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a downgraded PAT must miss the memo and re-pay Argon2id, got {err:?}"
        );
    }

    /// The memo key binds EVERY decision field — plaintext, token_id, the stored
    /// PHC hash, `scope` and `find_only` — and is unambiguous across them.
    ///
    /// `scope` / `find_only` / `runner_job` are not incidental: dropping any of
    /// them back out reintroduces the downgrade-vs-revocation latency split that
    /// `a_scope_downgrade_invalidates_the_memo` exists to catch.
    #[test]
    fn secret_match_fingerprint_binds_every_decision_field() {
        let base = secret_match_fingerprint("pt", "tid", "hash", "cas:rw", false, false);
        assert_ne!(
            base,
            secret_match_fingerprint("pt-x", "tid", "hash", "cas:rw", false, false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid-x", "hash", "cas:rw", false, false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash-x", "cas:rw", false, false)
        );
        // scope + find_only are in the key so a DOWNGRADE (as opposed to a
        // revocation) makes the old entry unreachable instead of serving a
        // fast 401 that a revoked PAT would not get.
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash", "cas:r", false, false)
        );
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash", "cas:rw", true, false)
        );
        // `runner_job` (0086) joined them once it started deciding an operation
        // on this plane (the cargo DELETE containment). Same rule, same reason.
        assert_ne!(
            base,
            secret_match_fingerprint("pt", "tid", "hash", "cas:rw", false, true)
        );
        // Length-prefixed ⇒ no concatenation-boundary collision between two
        // different triples that share a flat concatenation.
        assert_ne!(
            secret_match_fingerprint("ab", "c", "d", "e", false, false),
            secret_match_fingerprint("a", "bc", "d", "e", false, false)
        );
        // A digest, never recoverable material. `!contains("pt")` would be
        // VACUOUS here — `p` and `t` are not hex digits, so it holds for any
        // hex string, including `hex::encode(plaintext)`. Assert the properties
        // that actually constrain the output instead.
        assert_eq!(base.len(), 64);
        assert!(base
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(base, hex::encode("pt"), "must not be the plaintext encoded");
    }

    /// The memo is bounded and TTL'd — it can neither grow without limit nor
    /// serve an aged entry.
    #[test]
    fn memo_is_bounded_and_ttl_expired_entries_are_not_hits() {
        let memo = SecretMatchMemo::new(2, Duration::from_secs(300));
        // Space the inserts so "oldest" is unambiguous rather than clock-jitter.
        memo.insert("a".to_owned());
        std::thread::sleep(Duration::from_millis(5));
        memo.insert("b".to_owned());
        std::thread::sleep(Duration::from_millis(5));
        memo.insert("c".to_owned());
        assert!(
            memo.len_for_test() <= 2,
            "the cap must bound the map, got {}",
            memo.len_for_test()
        );
        // Asserting `contains("c")` would be VACUOUS: `insert` places the new
        // key unconditionally AFTER the eviction pass, so it holds even with
        // the whole eviction branch deleted. What eviction actually DECIDES is
        // WHICH pre-existing entry dies — the oldest, never the newer one.
        assert!(!memo.contains("a"), "eviction must drop the OLDEST entry");
        assert!(memo.contains("b"), "eviction must keep the newer survivor");

        let expiring = SecretMatchMemo::new(8, Duration::ZERO);
        expiring.insert("x".to_owned());
        assert!(!expiring.contains("x"), "an expired entry is not a hit");
        assert_eq!(
            expiring.len_for_test(),
            0,
            "and is evicted on the read that observed it expired"
        );
    }

    /// Fake row source: a `token_id → PatRow` map plus a call counter and
    /// an optional forced backend error. Drives the full verification
    /// pipeline with real crypto but no network.
    #[derive(Default)]
    struct FakeLookup {
        rows: HashMap<String, PatRow>,
        calls: AtomicUsize,
        backend_err: Option<String>,
    }

    impl FakeLookup {
        fn with_row(token_id: &str, row: PatRow) -> Self {
            let mut rows = HashMap::new();
            rows.insert(token_id.to_owned(), row);
            Self {
                rows,
                calls: AtomicUsize::new(0),
                backend_err: None,
            }
        }
        fn backend(err: &str) -> Self {
            Self {
                backend_err: Some(err.to_owned()),
                ..Self::default()
            }
        }
        fn empty() -> Self {
            Self::default()
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PatRowLookup for FakeLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.backend_err {
                return Err(e.clone());
            }
            Ok(self.rows.get(token_id).cloned())
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Mint a real PAT and return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(
        key: &PatSigningKey,
        tenant: u128,
        scopes: u64,
    ) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant + 1000)),
            PatScopes::from_u64(scopes),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    fn row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: scope.to_owned(),
            find_only: false,
            runner_job: false,
        }
    }

    /// [`row`] carrying the 0086 runner-job marker.
    fn runner_job_row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
        PatRow {
            runner_job: true,
            ..row(pat_hash, tenant, scope)
        }
    }

    #[tokio::test]
    async fn forged_token_rejected_before_d1() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
    }

    #[tokio::test]
    async fn wrong_signing_key_rejected_before_d1() {
        let mint_key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&mint_key, 7, SCOPE_CACHE_RW);
        let other_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let lookup = Arc::new(FakeLookup::with_row(
            "ignored",
            row(&hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup.clone(), other_key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn valid_pat_with_cache_scope_resolves_tenant() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 42, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::new(lookup.clone(), key);
        let resolved = verifier.verify(&pt).await.unwrap();
        assert_eq!(resolved, tenant);
        assert_eq!(lookup.call_count(), 1);
    }

    #[tokio::test]
    async fn pat_minted_under_prev_key_validates_during_overlap() {
        // Rotation overlap: the verifier's CURRENT key differs from the key
        // the PAT was minted under, but the old key is still in the overlap
        // set — so the PAT must still validate (no instant fleet-wide outage).
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 60, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new (current), old (prev)].
        let verifier =
            PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone(), (*old_key).clone()]);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn pat_rejected_when_minting_key_not_in_overlap_set() {
        // After the overlap window closes (old key dropped), a PAT minted
        // under the now-retired key must be rejected.
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 61, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new] only — old key retired.
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone()]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn empty_key_set_fails_closed() {
        // No key bound ⇒ nothing verifies (fail-closed).
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "empty key set rejects pre-D1");
    }

    #[tokio::test]
    async fn admin_scope_grants_cache() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 43, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "admin")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn unknown_token_id_is_invalid() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 44, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
    }

    #[tokio::test]
    async fn wrong_stored_hash_is_invalid() {
        let key = test_key();
        let (pt, tid, _hash, tenant) = mint_pat(&key, 45, SCOPE_CACHE_RW);
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 46, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(
            &tid,
            row(&other_hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn corrupt_stored_hash_is_backend_error_not_invalid_pat() {
        let key = test_key();
        let (pt, tid, _hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(
            &tid,
            row("not-a-phc-string", &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(message) if message.contains("stored PAT hash invalid")),
            "a corrupt PHC row is infrastructure failure, not a customer credential mismatch: {err:?}"
        );
    }

    #[tokio::test]
    async fn empty_scope_fails_closed() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 47, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn read_only_scope_still_resolves() {
        // The verifier gates on "has cache read" (the port carries no op);
        // a cas:r PAT resolves here — per-op write-deny is the route's job.
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:r")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn find_only_pat_is_rejected_on_the_adapter_plane() {
        // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only` but is
        // narrowed to find-missing at the Worker header. The adapter verifier
        // authorizes package-manager reads (OCI has NO header gate) directly from
        // the D1 scope, so `requires_cache_read("read-only")` would otherwise grant
        // e.g. `docker pull`. The `find_only` marker MUST fail-CLOSE here even
        // though the base scope reads. (Same possession proof as a normal PAT.)
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 52, SCOPE_CACHE_R);
        let find_only_row = PatRow {
            tenant_id: tenant.clone(),
            pat_hash: hash,
            scope: "read-only".to_owned(),
            find_only: true,
            runner_job: false,
        };
        let lookup = Arc::new(FakeLookup::with_row(&tid, find_only_row));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "a find-only PAT must be rejected on every adapter surface; got {err:?}"
        );
    }

    #[tokio::test]
    async fn revoked_row_is_invalid_pat() {
        // Soft revocation (migration 0063) is enforced INSIDE the lookup
        // SQL (`AND revoked_at_ms IS NULL`), so a revoked row surfaces to
        // the pipeline exactly like an absent one: `lookup → None`. Model
        // that here (the FakeLookup map simply does not contain the
        // revoked row) and assert the uniform InvalidPat — a revoked PAT
        // must be indistinguishable from an unknown one on the wire.
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 50, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "revocation is decided at D1");
    }

    #[test]
    fn lookup_sql_filters_revoked_and_expired_rows() {
        // The production D1 query must carry BOTH SQL-side liveness
        // filters — losing either one silently re-admits dead tokens.
        assert!(
            PAT_LOOKUP_SQL.contains("AND revoked_at_ms IS NULL"),
            "lookup SQL must exclude soft-revoked rows (migration 0063)"
        );
        assert!(
            PAT_LOOKUP_SQL.contains("expires_ms = 0 OR expires_ms >"),
            "lookup SQL must keep the expiry filter"
        );
        assert!(PAT_LOOKUP_SQL.contains("WHERE token_id = ?1"));
    }

    #[tokio::test]
    async fn backend_error_is_not_invalid_pat() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 49, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::backend("d1 unreachable"));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => assert!(m.contains("d1 unreachable")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    /// Finding #2: the Argon2id concurrency gate bounds work — when every
    /// permit is held, a hot-path verify (valid HMAC + real D1 row, so it
    /// reaches the Argon2id stage) must give up after the bounded wait and
    /// surface `Backend("…overloaded…")` rather than piling on another
    /// 64-MiB Argon2id allocation. We model "all permits held" by building a
    /// verifier with ZERO permits, so the acquire can never succeed and the
    /// timeout path is exercised deterministically.
    #[tokio::test]
    async fn exhausted_argon2_permits_yields_backend_overloaded() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 70, SCOPE_CACHE_RW);
        // A live row so the pipeline gets PAST the HMAC fast-reject and the D1
        // lookup, all the way to the permit acquire that guards Argon2id.
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => {
                assert!(m.contains("overloaded"), "expected overloaded, got {m}")
            }
            other => panic!("expected Backend(overloaded), got {other:?}"),
        }
        // The D1 row WAS consulted (we are past the fast-reject) — the gate
        // sits AFTER the lookup, on the expensive stage only.
        assert_eq!(lookup.call_count(), 1);
    }

    /// The permit gate must NOT consume a permit for a forged token: the HMAC
    /// fast-reject fires first, so even with zero permits a forged token is
    /// still a plain `InvalidPat` (an attacker without the key cannot drive the
    /// verifier into the overloaded path).
    #[tokio::test]
    async fn forged_token_does_not_touch_argon2_permits() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, VerifyError::InvalidPat),
            "forged token must fast-reject before the permit gate"
        );
        assert_eq!(lookup.call_count(), 0);
    }

    // ------------------------------------------------------------------
    // INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM — the load shed is symmetric
    // across D1 row existence.
    //
    // The shed used to be asymmetric: the row-FOUND arm shed with
    // `Backend` (⇒ 503) and the row-NOT-FOUND dummy-burn arm shed with
    // `InvalidPat` (⇒ 401). An attacker who can saturate the (small,
    // slow) Argon2id pool could therefore read the status code as a
    // ROW-EXISTENCE oracle — strictly more than the latency signal the
    // dummy burn exists to hide, because the burn is skipped on a shed
    // anyway. These tests pin BOTH halves of the contract:
    //
    //   under saturation  ⇒ every outcome is Backend("…overloaded…")
    //   outside saturation ⇒ every rejection is InvalidPat
    //
    // The idiom is #1022's: build the verifier with exactly ONE permit,
    // HOLD it, and let the acquire time out deterministically. A control
    // arm is mandatory — without it the test would pass just as happily
    // if the permit had never actually been held.
    // ------------------------------------------------------------------

    /// THE NEW CASE. A saturated pool + an unknown/absent D1 row must shed as
    /// `Backend`, not `InvalidPat`. This is the arm that used to leak row
    /// existence through the status code.
    #[tokio::test]
    async fn saturated_pool_sheds_an_unknown_row_as_backend() {
        let key = test_key();
        // A genuinely-minted PAT (so it clears the HMAC fast-reject) whose
        // token_id has NO row — the dummy-burn arm.
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 90, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier =
            PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

        // Drain the one and only permit and keep it drained.
        let _held = Arc::clone(&verifier.argon2_permits)
            .acquire_owned()
            .await
            .unwrap();

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
        );
        assert_eq!(lookup.call_count(), 1, "the shed sits AFTER the D1 lookup");
    }

    /// The per-tenant sub-cap arm of the same shed. The dummy burn is routed
    /// through ONE shared synthetic bucket; saturating THAT bucket (with global
    /// headroom to spare) must also shed as `Backend`, never `InvalidPat`.
    #[tokio::test]
    async fn saturated_dummy_burn_bucket_sheds_an_unknown_row_as_backend() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        // Global 8 (ample) so a rejection can ONLY come from the per-tenant
        // tier; synthetic bucket sub-cap 1 so holding one permit saturates it.
        let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
            lookup.clone(),
            vec![(*key).clone()],
            8,
            1,
        );
        let bucket = verifier
            .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
            .expect("synthetic bucket semaphore");
        let _held = Arc::clone(&bucket)
            .try_acquire_owned()
            .expect("bucket permit");
        assert_eq!(bucket.available_permits(), 0, "burn bucket saturated");

        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(
            matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
            "a sub-cap shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
        );
    }

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

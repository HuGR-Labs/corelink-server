// ── SingleFlightPatLookup (cold-hydrate PAT read-herd) ─────────────────────

/// A lookup fake that counts inner calls, delays (to force burst overlap),
/// and whose returned result is SWAPPABLE (to simulate revocation between
/// reads — proving the single-flight is NOT a cache).
struct SwitchableLookup {
    calls: Arc<AtomicUsize>,
    delay_ms: u64,
    result: Mutex<Result<Option<PatRow>, String>>,
}
impl SwitchableLookup {
    fn new(result: Result<Option<PatRow>, String>, delay_ms: u64) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms,
            result: Mutex::new(result),
        }
    }
    fn set(&self, r: Result<Option<PatRow>, String>) {
        *self.result.lock().unwrap() = r;
    }
}
#[async_trait]
impl PatRowLookup for SwitchableLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        self.result.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn single_flight_coalesces_a_concurrent_burst_to_one_read() {
    let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 30));
    let calls = Arc::clone(&inner.calls);
    let sf = Arc::new(SingleFlightPatLookup::new(inner));
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..24 {
        let s = Arc::clone(&sf);
        set.spawn(async move { s.lookup("tok-hot").await.unwrap() });
    }
    while let Some(r) = set.join_next().await {
        assert_eq!(r.unwrap(), Some(row("h", "t", "cas:rw")));
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a parallel burst does ONE inner D1 read"
    );
}

#[tokio::test]
async fn single_flight_is_not_a_cache_sequential_reads_are_fresh() {
    let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
    let calls = Arc::clone(&inner.calls);
    let sf = SingleFlightPatLookup::new(inner);
    let _ = sf.lookup("tok").await.unwrap();
    let _ = sf.lookup("tok").await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "sequential reads are NOT cached"
    );
}

#[tokio::test]
async fn single_flight_revocation_is_immediate() {
    // A resolved flight is never reused: after a row is returned, swapping the
    // inner to None (revocation) is visible on the very next read.
    let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 0));
    let calls = Arc::clone(&inner.calls);
    let inner_for_swap = Arc::clone(&inner);
    let sf = SingleFlightPatLookup::new(inner);
    assert_eq!(
        sf.lookup("tok").await.unwrap(),
        Some(row("h", "t", "cas:rw"))
    );
    inner_for_swap.set(Ok(None)); // revoke
    assert_eq!(
        sf.lookup("tok").await.unwrap(),
        None,
        "revocation is immediate (no cache)"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn single_flight_distinct_token_ids_do_not_coalesce() {
    let inner = Arc::new(SwitchableLookup::new(Ok(Some(row("h", "t", "cas:rw"))), 20));
    let calls = Arc::clone(&inner.calls);
    let sf = Arc::new(SingleFlightPatLookup::new(inner));
    let (a, b) = (Arc::clone(&sf), Arc::clone(&sf));
    let h1 = tokio::spawn(async move { a.lookup("tok-A").await.unwrap() });
    let h2 = tokio::spawn(async move { b.lookup("tok-B").await.unwrap() });
    let _ = h1.await.unwrap();
    let _ = h2.await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "different token_ids each read"
    );
}

#[tokio::test]
async fn single_flight_backend_error_is_not_cached() {
    let inner = Arc::new(SwitchableLookup::new(Err("D1 down".to_owned()), 0));
    let calls = Arc::clone(&inner.calls);
    let inner_for_swap = Arc::clone(&inner);
    let sf = SingleFlightPatLookup::new(inner);
    assert!(sf.lookup("tok").await.is_err());
    inner_for_swap.set(Ok(Some(row("h", "t", "cas:rw"))));
    assert_eq!(
        sf.lookup("tok").await.unwrap(),
        Some(row("h", "t", "cas:rw")),
        "error not cached"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// ⚠️ WIRING GUARD. Everything above builds `SingleFlightPatLookup` by hand,
/// so none of it notices if PRODUCTION stops using it. This one goes through
/// [`PatVerifier::from_parts`] — the sole stack assembly `from_env` defers to
/// — and asserts the wrapper's EFFECT rather than its presence, so it fails
/// if the wrapper is removed AND if it is neutered into a pass-through.
///
/// # What this pins, and what it does NOT
///
/// Pinned: the production assembly coalesces concurrent D1 `pat` reads of one
/// `token_id` into a single inner read, driven through the real
/// `verify` pipeline (HMAC → D1 → Argon2id → scope), not through a
/// hand-built lookup.
///
/// NOT pinned: that `from_env` calls `from_parts`. That is four lines of env
/// decoding plus one call, all visible in review, and testing it would need
/// process-env mutation this repo deliberately avoids. Saying so explicitly
/// is the point — a doc claim wider than the mechanism is what let the #1055
/// co-read defect through review ("the published key is the cell's key **by
/// construction**").
///
/// Why the count is the assertion: coalescing is invisible in the RESULT —
/// both callers get the same row either way. Only the inner call count
/// distinguishes "one read shared" from "two reads raced", which is exactly
/// the property #1055's co-read correctness is written against.
///
/// # Why a SEQUENTIAL third verify is also asserted
///
/// "concurrent pair ⇒ 1 inner read" on its own does NOT say the stack
/// coalesces — a genuine CACHE at this seam would satisfy it too, while
/// silently breaking `INV-PAT-REVOKE-PROPAGATION` (a revoked PAT would keep
/// working for the cache's TTL). That distinction IS tested
/// (`single_flight_is_not_a_cache_sequential_reads_are_fresh`,
/// `single_flight_revocation_is_immediate`) — but only against a
/// hand-built wrapper, i.e. inside the exact blind spot this test exists to
/// close. So the third verify runs AFTER the pair has resolved and must
/// produce a SECOND inner read: at the production assembly, freshness per
/// request is pinned alongside coalescing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_production_stack_coalesces_concurrent_reads_of_one_token() {
    let key = test_key();
    let (pt, _tid, hash, tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
    // 30 ms in the inner read so the second verify genuinely JOINS the first
    // one's flight instead of arriving after it resolved (a resolved flight
    // is deliberately never reused — see `single_flight_is_not_a_cache…`).
    let inner = Arc::new(SwitchableLookup::new(
        Ok(Some(row(&hash, &tenant, "cas:rw"))),
        30,
    ));
    let calls = Arc::clone(&inner.calls);
    let verifier = Arc::new(PatVerifier::from_parts(inner, vec![(*key).clone()]));

    let (a, b) = (Arc::clone(&verifier), Arc::clone(&verifier));
    let (pt_a, pt_b) = (pt.clone(), pt.clone());
    let h1 = tokio::spawn(async move { a.verify(&pt_a).await });
    let h2 = tokio::spawn(async move { b.verify(&pt_b).await });
    let (r1, r2) = (h1.await.unwrap(), h2.await.unwrap());

    // Both must have gone all the way through — otherwise a count of 1 could
    // just mean one of them was rejected before it ever reached D1.
    assert_eq!(r1.expect("first verify must succeed"), tenant);
    assert_eq!(r2.expect("second verify must succeed"), tenant);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the production PAT lookup stack must coalesce a concurrent burst of \
             one token_id into ONE D1 read — 2 means `from_parts` no longer \
             wraps the row source in SingleFlightPatLookup (or the wrapper stopped \
             coalescing)"
    );

    // …and it is a single-FLIGHT, not a cache: a later request for the same
    // token re-reads D1, which is what keeps revocation immediate.
    assert_eq!(
        verifier
            .verify(&pt)
            .await
            .expect("third verify must succeed"),
        tenant
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the production PAT lookup stack must NOT cache — a verify issued \
             after the flight resolved must pay its own D1 read. Still 1 means \
             something at this seam is serving a retained row, which would defeat \
             INV-PAT-REVOKE-PROPAGATION"
    );
}

// ── The co-read row must land in the cell whose key fetched it ─────────

/// A `PatRowLookup` fake shaped exactly like [`D1HttpClient::lookup`] on the
/// co-read path: read the hint, make ONE round trip, publish the url-map arm
/// it fetched, return the `pat` arm.
///
/// The `yield_now` in the middle is the round trip, and it is the whole
/// point: [`SingleFlightPatLookup`] shares an **unspawned** future, so the
/// task that resumes it after an await point is whichever caller happens to
/// poll next — a *joiner*, not necessarily the leader that read the hint.
struct CoReadingLookup {
    calls: Arc<AtomicUsize>,
    /// `(namespace, url_hash) -> content_hash`.
    map: HashMap<(String, String), String>,
    row: Option<PatRow>,
}

#[async_trait]
impl PatRowLookup for CoReadingLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Capture the destination cell WITH the key, before the await —
        // exactly as `D1HttpClient::lookup` does.
        let Some((cell, namespace, url_hash)) = crate::d1_coread::hint() else {
            return Ok(self.row.clone());
        };
        // ── the D1 round trip ──
        tokio::task::yield_now().await;
        let fetched = self.map.get(&(namespace, url_hash)).cloned();
        cell.publish(fetched);
        Ok(self.row.clone())
    }
}

/// ⚠️ REGRESSION GUARD. Two concurrent cargo reads on the SAME runner PAT
/// but DIFFERENT objects — the exact hot path this PR optimises — coalesce
/// into one `SingleFlightPatLookup` flight. The single co-read is keyed by
/// the LEADER's url_hash, so the row it brings back belongs to the leader
/// and to nobody else. A joiner must be served NOTHING (and fall back to its
/// own correctly-keyed read), never the leader's `content_hash` under its
/// own key.
#[tokio::test]
async fn a_joined_single_flight_never_publishes_into_the_joiners_cell() {
    let key_a = "a".repeat(64);
    let key_b = "b".repeat(64);
    let mut map = HashMap::new();
    let _ = map.insert(("t".to_owned(), key_a.clone()), "content-hash-A".to_owned());
    let _ = map.insert(("t".to_owned(), key_b.clone()), "content-hash-B".to_owned());
    let inner = Arc::new(CoReadingLookup {
        calls: Arc::new(AtomicUsize::new(0)),
        map,
        row: Some(row("h", "t", "cas:rw")),
    });
    let calls = Arc::clone(&inner.calls);
    let sf = Arc::new(SingleFlightPatLookup::new(inner));

    // Each request gets its OWN co-read scope, as the cargo layer gives it.
    // `futures::future::join` polls in declaration order, so A leads the
    // flight and B joins it and drives it past the round trip.
    let (a_sf, b_sf) = (Arc::clone(&sf), Arc::clone(&sf));
    let (a_key, b_key) = (key_a.clone(), key_b.clone());
    let (served_a, served_b) = futures::future::join(
        crate::d1_coread::scope("t", a_key.clone(), async move {
            let _ = a_sf.lookup("tok-shared").await.unwrap();
            crate::d1_coread::take("t", &a_key)
        }),
        crate::d1_coread::scope("t", b_key.clone(), async move {
            let _ = b_sf.lookup("tok-shared").await.unwrap();
            crate::d1_coread::take("t", &b_key)
        }),
    )
    .await;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the two requests must actually have coalesced — otherwise this test \
             proves nothing about the joiner"
    );
    // Serve the CONTENT HASH, not a count: this is what the moat hands to
    // the CAS read, and the integrity check downstream only ties bytes to
    // hash — never key to hash — so a mis-keyed row is served silently.
    assert_ne!(
        served_b,
        Some(Some("content-hash-A".to_owned())),
        "the joiner was served the LEADER's row under its own url_hash — \
             cross-object content substitution"
    );
    assert_eq!(
        served_b, None,
        "the joiner's key was never fetched, so it must get no prefetch and \
             fall back to its own read"
    );
    assert_eq!(
        served_a,
        Some(Some("content-hash-A".to_owned())),
        "the leader still keeps the co-read it paid for"
    );
}

// ---------------------------------------------------------------------
// SecretMatchMemo — the Argon2id memo that lifts the adapter plane's
// ~3 req/s throughput ceiling. The tests below are written to prove the
// two claims that make it safe, not merely that it is fast:
//   (1) a memo HIT genuinely skips Argon2id, and
//   (2) it caches NO authorization state — revocation, scope changes and
//       re-hashes all still take effect on the very next request.
// ---------------------------------------------------------------------

/// A `FakeLookup` holding several rows (the single-row `with_row` cannot
/// serve the control arm of the permit-exhaustion test, which needs a
/// SECOND, un-memoised PAT to resolve).
fn fake_with_rows(pairs: Vec<(String, PatRow)>) -> FakeLookup {
    FakeLookup {
        rows: pairs.into_iter().collect(),
        calls: AtomicUsize::new(0),
        backend_err: None,
    }
}

/// A memo HIT must not run Argon2id.
///
/// Proven behaviourally rather than by counting: we HOLD the verifier's one
/// and only Argon2id permit, which makes any real Argon2id impossible (the
/// acquire times out after `ARGON2_PERMIT_WAIT` into `Backend`). A verify
/// that still SUCCEEDS under that condition provably never entered the
/// Argon2id path. The second half is the control — without it, the test
/// would pass just as happily if the permit had never actually been held.
#[tokio::test]
async fn memo_hit_skips_argon2id_proven_by_holding_the_only_permit() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 21, SCOPE_CACHE_RW);
    let (pt_other, tid_other, hash_other, tenant_other) = mint_pat(&key, 22, SCOPE_CACHE_RW);
    let lookup = Arc::new(fake_with_rows(vec![
        (tid, row(&hash, &tenant, "cas:rw")),
        (tid_other, row(&hash_other, &tenant_other, "cas:rw")),
    ]));
    // ONE global permit, so holding it drains the pool completely.
    let verifier = PatVerifier::with_key_set_and_permits(lookup, vec![(*key).clone()], 1);

    // Cold: runs the real Argon2id and memoises the match.
    assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);

    // Drain the pool and keep it drained for the rest of the test.
    let _held = Arc::clone(&verifier.argon2_permits)
        .acquire_owned()
        .await
        .unwrap();

    // Warm: succeeds with ZERO permits available ⇒ no Argon2id ran.
    assert_eq!(
        verifier.verify(&pt).await.unwrap(),
        tenant,
        "a memoised PAT must verify without an Argon2id permit"
    );

    // CONTROL: a different, un-memoised PAT MUST fail-closed on the very
    // same drained pool — which is what proves the pool really was empty.
    let err = verifier.verify(&pt_other).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
        "un-memoised PAT must still need (and fail to get) a permit, got {err:?}"
    );
}

/// The memo holds no authorization state: revocation and scope changes in
/// D1 take effect on the NEXT request, with no TTL window. This is the
/// property that makes the adapter plane strictly stronger than the native
/// plane's ≤5 s `native_pat_gate::VERIFY_CACHE_TTL`.
#[tokio::test]
async fn memo_caches_no_authorization_state() {
    let key = test_key();
    let (pt, _tid, hash, tenant) = mint_pat(&key, 31, SCOPE_CACHE_RW);
    let lookup = Arc::new(SwitchableLookup::new(
        Ok(Some(row(&hash, &tenant, "cas:rw"))),
        0,
    ));
    let verifier = PatVerifier::new(lookup.clone(), key);

    // Warm the memo on a read-write PAT.
    let (_t, can_write) = verifier.verify_capability(&pt).await.unwrap();
    assert!(can_write);

    // Scope downgraded in D1 → the write bit must drop immediately.
    lookup.set(Ok(Some(row(&hash, &tenant, "cas:r"))));
    let (_t, can_write) = verifier.verify_capability(&pt).await.unwrap();
    assert!(
        !can_write,
        "a scope downgrade must apply on the next request (scope is never memoised)"
    );

    // Revoked in D1 (the SQL filter makes a revoked row indistinguishable
    // from an absent one) → rejected immediately, NOT after a TTL.
    lookup.set(Ok(None));
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::InvalidPat),
        "revocation must be immediate with a warm memo, got {err:?}"
    );
}

/// The 0086 marker reaches this plane at all — the gap AUDIT-2026-08-23
/// F-1 found — and it NARROWS rather than rejects: a runner-job PAT still
/// verifies and still carries write, because the runner fabric's own sccache
/// dogfood depends on it. Only `routes/cargo.rs` acts on the flag.
#[tokio::test]
async fn a_runner_job_pat_verifies_and_surfaces_its_marker() {
    let key = test_key();
    let (pt, _tid, hash, tenant) = mint_pat(&key, 71, SCOPE_CACHE_RW);
    let lookup = Arc::new(SwitchableLookup::new(
        Ok(Some(runner_job_row(&hash, &tenant, "cas:rw"))),
        0,
    ));
    let verifier = PatVerifier::new(lookup.clone(), key);

    let (t, can_write, runner_job) = verifier.verify_capability_full(&pt).await.unwrap();
    assert_eq!(t, tenant);
    assert!(can_write, "a runner-job PAT still writes the cache");
    assert!(runner_job, "the 0086 marker must reach the adapter plane");

    // The narrow entry point is unchanged for the five adapters that have
    // no narrowable operation.
    let (t2, w2) = verifier.verify_capability(&pt).await.unwrap();
    assert_eq!((t2, w2), (tenant.clone(), true));

    // Same PAT, marker cleared in D1 → the next request sees a normal PAT.
    // (The marker is in the memo key, so the old entry is unreachable.)
    lookup.set(Ok(Some(row(&hash, &tenant, "cas:rw"))));
    let (_t, _w, runner_job) = verifier.verify_capability_full(&pt).await.unwrap();
    assert!(
        !runner_job,
        "clearing the marker must apply on the next request, never after a TTL"
    );
}

include!("adapter_pat_tests_2/part-01.rs");

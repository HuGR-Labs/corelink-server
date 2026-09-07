/// `0` cap = genuine unlimited — keeps every PUT under the cap so the test
/// exercises the byte-DELTA math, not the over-cap gate. Seeded via the
/// server-trusted `x-corelink-storage-quota-bytes` header.
const UNLIMITED_QUOTA_HEADER: &str = "0";

/// Build a route state wired with a real `ByteAccountant` over an in-memory
/// `ByteStore`, so PUTs flow through the byte-delta reconciliation. Returns
/// the state and the store handle for `bytes_used` assertions.
fn fixture_with_byte_accounting() -> (
    TurboRouteState,
    Arc<crate::byte_accounting::testing::InMemoryByteStore>,
) {
    let store = Arc::new(crate::byte_accounting::testing::InMemoryByteStore::new());
    let dyn_store: Arc<dyn crate::byte_accounting::ByteStore> = store.clone();
    let acc = Arc::new(crate::byte_accounting::ByteAccountant::new(
        dyn_store,
        TEST_BYTES_REGION.to_owned(),
    ));
    let mut state = build_handlers();
    state.bytes = Some(acc);
    (state, store)
}

/// Issue a PUT of `body` to `hash` and return the status. Carries the
/// unlimited quota header so the fresh `tenant_storage_state` row seeds
/// (else 503).
async fn put_artifact_status(app: &Router, hash: &str, body: Vec<u8>) -> StatusCode {
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v8/artifacts/{hash}?teamId=team_x"))
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .header(
            crate::byte_accounting::STORAGE_QUOTA_HEADER,
            UNLIMITED_QUOTA_HEADER,
        )
        .body(Body::from(body))
        .expect("request");
    app.clone().oneshot(req).await.expect("oneshot").status()
}

/// Issue a PUT of `body` to `hash` and assert 200 (a fresh insert). Panics
/// on any other status — use for the first write of a key.
async fn put_artifact(app: &Router, hash: &str, body: Vec<u8>) {
    assert_eq!(
        put_artifact_status(app, hash, body).await,
        StatusCode::OK,
        "PUT {hash} must 200"
    );
}

/// B-024 create-only: a fresh insert is charged in full, and a SECOND PUT to
/// the same key is REFUSED with 409 — the bytes are never overwritten, so
/// `bytes_used` cannot change on a re-PUT. This supersedes the old
/// overwrite-byte-delta reconciliation (rt34): with overwrites refused there
/// is no grow/shrink delta to account, and the "PUT 1B then PUT 100MiB under
/// the same key" free-storage exploit shape simply cannot be issued.
#[tokio::test]
async fn put_is_create_only_second_put_refused_409() {
    let (state, store) = fixture_with_byte_accounting();
    let app = router(state);
    let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

    // 1) Fresh insert of 1 byte → charge full (used = 1).
    put_artifact(&app, "k", vec![b'a']).await;
    assert_eq!(used(), 1, "fresh insert charges the full new bytes");

    // 2) Any second PUT to the same key — same size, GROW, or SHRINK — is
    //    refused create-only (409). The stored bytes and the charge are
    //    unchanged: the reservation the route accrued up front is released
    //    on the refusal, so `used` stays exactly the first insert's size.
    assert_eq!(
        put_artifact_status(&app, "k", vec![0u8; 100]).await,
        StatusCode::CONFLICT,
        "a GROW re-PUT (the old free-storage exploit shape) must be refused 409"
    );
    assert_eq!(used(), 1, "a refused overwrite must not change bytes_used");

    assert_eq!(
        put_artifact_status(&app, "k", vec![b'b']).await,
        StatusCode::CONFLICT,
        "a same-size re-PUT must be refused 409"
    );
    assert_eq!(used(), 1, "still the first insert's size after a refusal");
}

/// Distinct keys are each charged independently, and a re-PUT of an existing
/// key is refused create-only (409) — proving the refusal targets the exact
/// stored object, not the whole tenant, and never disturbs another key.
#[tokio::test]
async fn put_distinct_keys_accumulate_independently() {
    let (state, store) = fixture_with_byte_accounting();
    let app = router(state);
    let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

    put_artifact(&app, "k1", vec![0u8; 10]).await;
    assert_eq!(used(), 10);
    // Fresh second key: full add.
    put_artifact(&app, "k2", vec![0u8; 25]).await;
    assert_eq!(used(), 35, "two fresh keys sum");
    // Re-PUT k1: refused create-only, total unchanged, k2 untouched.
    assert_eq!(
        put_artifact_status(&app, "k1", vec![1u8; 10]).await,
        StatusCode::CONFLICT,
        "a re-PUT of an existing key is refused 409"
    );
    assert_eq!(
        used(),
        35,
        "a refused re-PUT of one key leaves the total (and the other key) intact"
    );
}

// ── finding #2: the cap is enforced PRE-BUFFER (FromRequestParts) ─────────

/// The KILLING finding-#2 test: a 5th concurrent PUT is rejected 429 BEFORE
/// its body is buffered into the heap.
///
/// The `PutConcurrencyGuard` is a `FromRequestParts` extractor declared
/// AHEAD of `body: Bytes`; axum runs every `FromRequestParts` extractor
/// before the single body extractor. We prove the ordering deterministically:
/// with the tenant AT the cap, this PUT carries a body LARGER than the route's
/// 100 MiB `DefaultBodyLimit`. If the body were buffered first (the OLD,
/// in-handler guard), axum's body-limit layer would reject the oversized body
/// (413/400). Because the concurrency extractor runs FIRST, we get **429**
/// instead — and the oversized body is never read (the `Body` is streamed
/// lazily; the extractor short-circuits before any of it is consumed, so the
/// test stays cheap).
#[tokio::test]
async fn fifth_concurrent_put_rejected_429_before_body_buffering() {
    let state = fixture();
    {
        let mut g = state.put_inflight.lock().unwrap();
        // Tenant already AT the limit (4 in-flight).
        g.insert(TEST_AUTH_TENANT.to_owned(), TURBO_PUT_CONCURRENCY_LIMIT);
    }
    let app = router(state);
    // A body strictly larger than the route's 100 MiB DefaultBodyLimit. If the
    // body extractor ran before the concurrency guard, the body-limit layer
    // would reject the oversized body; the pre-buffer guard makes it 429
    // without reading the body.
    let oversized = Body::from(vec![0u8; TURBO_BODY_LIMIT_BYTES + 1]);
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/h_oversized?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(oversized)
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "the 5th concurrent PUT must be rejected 429 by the FromRequestParts \
             guard BEFORE the body is buffered (else the oversized body would be \
             rejected by the body-limit layer instead)"
    );
}

// ── C1: per-object write serialization (no double-release / no underflow) ──

/// B-024 create-only under concurrency: with the key already present, N
/// CONCURRENT PUTs to the SAME key are ALL refused 409 and the primed bytes
/// are never mutated. The per-(tenant, team, hash) write lock serializes the
/// probe→refuse decision, so no interleaving lets an overwrite slip through
/// (the old double-release race required an overwrite to happen at all;
/// create-only removes the overwrite, so the race is gone by construction).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_same_key_puts_all_refused_create_only() {
    let (state, store) = fixture_with_byte_accounting();
    let app = router(state);
    let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

    // Prime the key with a large object: bytes_used = 1000.
    put_artifact(&app, "shared", vec![0u8; 1000]).await;
    assert_eq!(used(), 1000, "primed large object");

    // Fire N concurrent PUTs to the SAME, already-present key. N <=
    // TURBO_PUT_CONCURRENCY_LIMIT (4) so the per-tenant PutConcurrencyGuard
    // never 429s one of them (which would muddy the create-only assertion).
    // Every one targets an existing key, so every one must be refused 409.
    const N: usize = 4;
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::PUT)
                .uri("/v8/artifacts/shared?teamId=team_x")
                .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
                .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
                .header(
                    crate::byte_accounting::STORAGE_QUOTA_HEADER,
                    UNLIMITED_QUOTA_HEADER,
                )
                .body(Body::from(vec![b'a' + (i as u8 % 26)]))
                .expect("request");
            app.oneshot(req).await.expect("oneshot").status()
        }));
    }
    for h in handles {
        assert_eq!(
            h.await.expect("join"),
            StatusCode::CONFLICT,
            "every concurrent re-PUT of an existing key must be refused 409"
        );
    }

    // The primed 1000-byte object is untouched: no overwrite happened, so
    // bytes_used stays exactly 1000 — there is no prior to release and no
    // double-release race to trip.
    assert_eq!(
        used(),
        1000,
        "refused overwrites must leave the primed object and its charge intact"
    );
}

/// B-024 — create-only under concurrent PUTs of a FRESH key: exactly ONE of N
/// concurrent PUTs wins (probe-absent → write); the rest observe the winner
/// and 409, and a GET returns the winner's bytes intact.
///
/// HONESTY NOTE — this is a happy-path concurrency SMOKE test, NOT a
/// regression guard for the per-(tenant, team, hash) write lock. That lock is
/// what makes "exactly one winner" hold in prod, where the probe (R2 GET) and
/// write (R2 PUT) are slow network calls with a wide interleaving window. In
/// this in-RAM harness the window collapses: probe+write is a pair of fast
/// synchronous HashMap ops, and empirically this test still reports one winner
/// with the write lock neutered (measured: lock removed → still 1×200). Making
/// it deterministically fail without the lock is not achievable at the unit
/// level — any mechanism that FORCES two tasks to be between probe and write
/// simultaneously (a barrier) DEADLOCKS a correct lock, and anything softer
/// (a slow probe) the scheduler does not reliably interleave. The lock's
/// serialization is therefore covered by code inspection (cold-review H1: the
/// route holds `_write_lock` across `handler.put`) and by the design's prod
/// concurrency probe, not by this test. This test still earns its place: it
/// proves the create-only path does not corrupt, 500, or mis-account under
/// concurrent fresh-key load.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fresh_key_puts_one_wins_rest_409_smoke() {
    let (state, store) = fixture_with_byte_accounting();
    let app = router(state);
    let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

    // N concurrent PUTs of an ABSENT key, each a DISTINCT 1-byte payload so
    // the winner is identifiable. N <= TURBO_PUT_CONCURRENCY_LIMIT (4) so the
    // per-tenant PutConcurrencyGuard never 429s one of them.
    const N: usize = 4;
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::PUT)
                .uri("/v8/artifacts/fresh_shared?teamId=team_x")
                .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
                .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
                .header(
                    crate::byte_accounting::STORAGE_QUOTA_HEADER,
                    UNLIMITED_QUOTA_HEADER,
                )
                .body(Body::from(vec![b'a' + (i as u8)]))
                .expect("request");
            (i as u8, app.oneshot(req).await.expect("oneshot").status())
        }));
    }
    let mut winner: Option<u8> = None;
    let mut oks = 0usize;
    let mut conflicts = 0usize;
    for h in handles {
        let (i, status) = h.await.expect("join");
        match status {
            StatusCode::OK => {
                oks += 1;
                winner = Some(i);
            }
            StatusCode::CONFLICT => conflicts += 1,
            other => panic!("unexpected status {other} for concurrent fresh-key PUT"),
        }
    }
    assert_eq!(
        oks, 1,
        "exactly ONE concurrent fresh-key PUT may win — more than one means the \
             per-object write lock did not serialize the racers (overwrite window open)"
    );
    assert_eq!(conflicts, N - 1, "every non-winner must be refused 409");
    assert_eq!(
        used(),
        1,
        "one 1-byte object stored; the N-1 refused reservations were released"
    );

    // The stored bytes are the winner's, intact — a GET returns exactly the
    // payload of the PUT that won, not a torn/overwritten object.
    let winner = winner.expect("one winner");
    let get = Request::builder()
        .method(Method::GET)
        .uri("/v8/artifacts/fresh_shared?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::empty())
        .expect("get request");
    let get_resp = app.oneshot(get).await.expect("get oneshot");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let got = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        got.as_ref(),
        &[b'a' + winner][..],
        "the stored object must be exactly the winning PUT's bytes"
    );
}

/// Distinct keys are NOT serialized against each other (the per-object lock
/// only serializes the SAME object): concurrent PUTs to different keys both
/// land and accumulate independently.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_distinct_key_puts_stay_concurrent_and_sum() {
    let (state, store) = fixture_with_byte_accounting();
    let app = router(state);
    let used = || store.used(TEST_AUTH_TENANT, TEST_BYTES_REGION);

    let mut handles = Vec::new();
    for i in 0..6u32 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::PUT)
                .uri(format!("/v8/artifacts/key{i}?teamId=team_x"))
                .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
                .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
                .header(
                    crate::byte_accounting::STORAGE_QUOTA_HEADER,
                    UNLIMITED_QUOTA_HEADER,
                )
                .body(Body::from(vec![0u8; 10]))
                .expect("request");
            app.oneshot(req).await.expect("oneshot").status()
        }));
    }
    for h in handles {
        assert_eq!(h.await.expect("join"), StatusCode::OK);
    }
    assert_eq!(used(), 60, "six fresh distinct 10-byte keys sum to 60");
}

/// The shard index is a pure deterministic function of the lock identity
/// tuple and always lands in `[0, TURBO_WRITE_LOCK_SHARDS)`; distinct
/// identities generally map to distinct shards (sanity, not a guarantee).
#[test]
fn write_lock_shard_in_range_and_deterministic() {
    let a = write_lock_shard("tenantA", "team1", "hash1");
    let b = write_lock_shard("tenantA", "team1", "hash1");
    assert_eq!(a, b, "shard must be deterministic for a fixed identity");
    for (t, team, h) in [
        ("t", "x", "h"),
        ("tenant-uuid", "default", "deadbeef"),
        ("", "", ""),
    ] {
        assert!(
            write_lock_shard(t, team, h) < TURBO_WRITE_LOCK_SHARDS,
            "shard must be in range"
        );
    }
    // Domain separation: ("a","b",_) must not collapse to ("ab","",_).
    assert_ne!(
        write_lock_shard("a", "b", "h"),
        write_lock_shard("ab", "", "h"),
        "domain-separated identity components must not alias"
    );
}

// ── C4: events route has its OWN small body cap (413 over it) ──────────────

/// An events POST OVER the small `EVENTS_BODY_LIMIT_BYTES` (64 KiB) cap is
/// rejected (413) — the route must NOT inherit the 100 MiB artifact limit,
/// so a telemetry body cannot OOM the container. A normal small telemetry
/// POST still 200s.
#[tokio::test]
async fn events_over_small_cap_rejected_normal_still_200() {
    let app = test_router();

    // Over the 64 KiB events cap (but WELL under the 100 MiB artifact cap,
    // so a 200 here would prove the route wrongly inherited the big limit).
    let oversized = vec![0u8; EVENTS_BODY_LIMIT_BYTES + 1];
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header("content-type", "application/json")
        .body(Body::from(oversized))
        .expect("request");
    let resp = app.clone().oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "events body over the 64 KiB cap must be rejected 413, NOT accepted \
             under the 100 MiB artifact limit"
    );

    // A normal small telemetry POST still succeeds.
    let small = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"sessionId":"abc","source":"LOCAL"}"#))
        .expect("request");
    let resp = app.oneshot(small).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK, "small telemetry still 200s");
}

/// A 100 MiB artifact PUT is NOT rejected by the events cap (proves the
/// small cap is route-LOCAL to `/events`, not applied to artifact PUTs).
/// We assert the artifact route still accepts a body LARGER than the events
/// cap (a 1 MiB body — far above 64 KiB, far below 100 MiB).
#[tokio::test]
async fn artifact_put_not_constrained_by_events_cap() {
    let app = test_router();
    let body = vec![0u8; EVENTS_BODY_LIMIT_BYTES * 4]; // 256 KiB > events cap
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/v8/artifacts/bigart?teamId=team_x")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header(crate::scope::SCOPE_HEADER, TEST_SCOPE_RW)
        .body(Body::from(body))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "an artifact PUT above the events cap (but under 100 MiB) must NOT be \
             413'd — the small cap is route-local to /events"
    );
}

// ── rt-nuclear cycle-2 #8: /events slow-body read timeout ──────────────────

/// #8 (slowloris on the events pool): a `POST /v8/artifacts/events` whose
/// body never finishes streaming must NOT pin the held events permit + slot
/// forever — the server-side `EVENTS_BODY_READ_TIMEOUT` aborts it with 408,
/// which returns the handler and RAII-releases the permit + the per-tenant
/// slot. We assert (a) the request resolves to 408 well within a bound far
/// shorter than "forever", and (b) the per-tenant slot is released after
/// (so a stalled body did not leak a slot).
#[tokio::test]
async fn events_slow_body_times_out_408_and_releases_slot() {
    let state = fixture();
    let app = router(state.clone());

    // A body that yields ONE chunk then never completes (a slowloris that
    // dribbles a byte and stalls) — the canonical slow-body attack.
    let stalled = Body::from_stream(async_stream::stream! {
        yield Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"{"));
        // Never produce EOF: pend forever. The handler's read timeout, not
        // the stream, must end the request.
        futures::future::pending::<()>().await;
        // Unreachable, but satisfies the stream's item-type inference.
        yield Ok(axum::body::Bytes::new());
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header("content-type", "application/json")
        .body(stalled)
        .expect("request");

    // Bound the whole call by a deadline a bit longer than the handler's read
    // timeout: if the handler did NOT enforce its own timeout this outer
    // bound would fire (test would hang/err here), proving the regression.
    let resp = tokio::time::timeout(
        EVENTS_BODY_READ_TIMEOUT + Duration::from_secs(3),
        app.oneshot(req),
    )
    .await
    .expect("handler must self-abort the slow body — it must NOT hang forever")
    .expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::REQUEST_TIMEOUT,
        "a stalled events body must be aborted 408 by the server read timeout"
    );

    // The per-tenant slot must be released (back to 0 ⇒ entry removed) — a
    // timed-out request must not leak its concurrency slot.
    let g = state.events_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        None,
        "the events concurrency slot must be released after a 408 timeout"
    );
}

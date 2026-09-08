// ── rt-nuclear cycle-2 #9: per-tenant /events concurrency cap ───────────────

#[test]
fn events_inflight_counter_is_shared_across_clones() {
    // `TurboRouteState::clone` shares the `Arc<Mutex<..>>` — axum clones the
    // state per request, so every `/events` invocation must see the SAME
    // per-tenant counter. Mirrors `put_inflight_counter_is_shared_across_clones`.
    let state = build_handlers();
    let state2 = state.clone();
    {
        let mut g = state.events_inflight.lock().unwrap();
        g.insert(TEST_AUTH_TENANT.to_owned(), 2);
    }
    let g = state2.events_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        Some(&2),
        "cloned state must share the same events inflight counter"
    );
}

/// #9 (per-tenant fairness): one tenant AT its `/events` concurrency cap is
/// rejected 429 — while a DIFFERENT tenant (at 0 in-flight) is still served.
/// Proves the cap is PER-TENANT, so one tenant cannot monopolise the events
/// pool and starve others. Mirrors the PUT plane's `put_at_limit_returns_429`.
#[tokio::test]
async fn events_per_tenant_cap_429s_hog_but_other_tenant_still_served() {
    const OTHER_TENANT: &str = "22222222-2222-2222-2222-222222222222";
    let state = fixture();
    // Pre-seed the noisy tenant AT its cap (simulating EVENTS_CONCURRENCY_LIMIT
    // already-in-flight events POSTs for that tenant).
    {
        let mut g = state.events_inflight.lock().unwrap();
        g.insert(TEST_AUTH_TENANT.to_owned(), EVENTS_CONCURRENCY_LIMIT);
    }
    let app = router(state.clone());

    // The hog's next events POST is rejected 429 (BEFORE the body is read).
    let hog = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"sessionId":"x"}"#))
        .expect("request");
    let resp = app.clone().oneshot(hog).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a tenant at its per-tenant events cap must be 429'd"
    );

    // A DIFFERENT tenant (0 in-flight) is still served — the cap did not
    // close the whole pool, only the hog's share.
    let other = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", OTHER_TENANT)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"sessionId":"y"}"#))
        .expect("request");
    let resp = app.oneshot(other).await.expect("oneshot");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "another tenant under its cap must still be served — fairness, not a \
             global lockout"
    );
}

/// A normal events POST releases its per-tenant slot on completion (the
/// counter returns to 0 ⇒ entry removed, not leaked). Mirrors
/// `put_below_limit_succeeds_and_decrements_counter`.
#[tokio::test]
async fn events_below_cap_succeeds_and_releases_slot() {
    let state = fixture();
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v8/artifacts/events")
        .header("x-corelink-tenant-id", TEST_AUTH_TENANT)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"sessionId":"ok"}"#))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let g = state.events_inflight.lock().unwrap();
    assert_eq!(
        g.get(TEST_AUTH_TENANT),
        None,
        "the events concurrency slot must be released after a successful POST"
    );
}

// ── C5: process-wide PUT budget ────────────────────────────────────────────

/// The process-wide budget is a TRUE singleton (every call to
/// `global_turbo_put_budget()` returns the SAME `Arc<Semaphore>`), so all
/// tenants/routers in the process share one budget. (We assert identity
/// only — NOT live `available_permits`, which concurrent PUT tests mutate.)
#[test]
fn global_budget_is_process_wide_singleton() {
    let s1 = global_turbo_put_budget();
    let s2 = global_turbo_put_budget();
    assert!(
        Arc::ptr_eq(&s1, &s2),
        "global budget must be a process-wide singleton shared by all tenants"
    );
}

/// Unit test of the permit-accounting SEMANTICS on a fresh semaphore sized
/// exactly like the global budget — isolated from the live singleton so it
/// cannot flake against concurrent PUT tests. Proves: exactly
/// `GLOBAL_TURBO_PUT_PERMITS` permits are acquirable; one more times out
/// within `GLOBAL_PUT_PERMIT_WAIT` (the extractor's 503 path); dropping the
/// held permits restores the budget.
#[tokio::test]
async fn global_budget_permit_accounting_saturates_then_restores() {
    let sem = Arc::new(Semaphore::new(GLOBAL_TURBO_PUT_PERMITS));
    let mut held = Vec::new();
    for _ in 0..GLOBAL_TURBO_PUT_PERMITS {
        held.push(
            Arc::clone(&sem)
                .acquire_owned()
                .await
                .expect("permit available within the configured count"),
        );
    }
    assert_eq!(sem.available_permits(), 0, "budget fully drained");
    // One more acquire must time out (saturated) — exactly the path the
    // `GlobalPutBudgetGuard` extractor maps to 503.
    let timed_out = tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, Arc::clone(&sem).acquire_owned())
        .await
        .is_err();
    assert!(
        timed_out,
        "beyond the permit count, acquire must time out → extractor returns 503"
    );
    drop(held);
    assert_eq!(
        sem.available_permits(),
        GLOBAL_TURBO_PUT_PERMITS,
        "permits restored on drop (RAII release)"
    );
}

// ── WP-I.3: turbo_v8 routes reuse the canonical sentinel set ──────────
//
// The pre-fix code carried three local copies of the sentinel list, and
// they were missing `_oci` and `_public` (the two sentinels the
// canonical `auth_tenant::is_reserved_sentinel` helper includes).
// We pin the helper's coverage here at the surface level so a
// future drift (e.g. dropping a sentinel from auth_tenant) does
// not silently re-introduce the bypass.

#[test]
fn rejects_oci_and_public_sentinels_via_canonical_helper() {
    // The exact set of sentinels the helper recognises (mirror of
    // the literal in `auth_tenant::SENTINELS`). Listed here so a
    // copy/paste of the test into another surface is intentional.
    for sentinel in [
        "",
        "_anonymous",
        "_unknown",
        "_system",
        "_pending",
        "_oci",
        "_public",
    ] {
        assert!(
            crate::auth_tenant::is_reserved_sentinel(sentinel),
            "expected the canonical helper to reject sentinel={sentinel:?}"
        );
    }
    // And the negative: a normal-looking tenant id is NOT a sentinel.
    assert!(!crate::auth_tenant::is_reserved_sentinel("acme-co"));
    assert!(!crate::auth_tenant::is_reserved_sentinel(
        "11111111-1111-1111-1111-111111111111"
    ));
}

#[allow(dead_code)]
const B126_M2_TEST_1_2_REANCHOR: () = ();

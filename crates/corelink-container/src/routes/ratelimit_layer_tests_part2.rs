#[tokio::test]
async fn tier_resolver_absent_leaves_team_default() {
    // No resolver → team default burst (200) preserved (back-compat).
    let clock = fixed_clock(6_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));
    // 200 in-budget requests at the same instant all allowed; the 201st
    // denies (identical to the no-resolver baseline test above).
    for _ in 0..DEFAULT_TENANT_BURST {
        let app = app(state.clone(), hits.clone());
        let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let app = app(state.clone(), hits.clone());
    let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn tier_resolver_unknown_tier_falls_back_to_team_default() {
    // An unresolvable tier (None) leaves the bucket on the team default —
    // fail-SAFE on availability (never over-throttle an unclassified tenant).
    let clock = fixed_clock(7_000_000);
    let resolver: Arc<dyn TenantTierResolver> = Arc::new(FakeTierResolver { label: None });
    let state = RateLimitLayerState::with_tier_resolver(clock, resolver);
    let hits = Arc::new(AtomicUsize::new(0));
    for _ in 0..DEFAULT_TENANT_BURST {
        let app = app(state.clone(), hits.clone());
        let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let app = app(state.clone(), hits.clone());
    let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn seed_persisted_bucket_survives_into_limiter() {
    // F-017 durability seam: a seeded bucket is materialised so a restart
    // reload preserves available_tokens instead of resetting to full burst.
    let state = RateLimitLayerState::new();
    let tenant = Uuid::from_u128(0xfeed);
    state
        .seed_persisted_bucket(tenant, 3.0, 200, 100.0, 1_000)
        .unwrap();
    let snap = state
        .limiter
        .snapshot_bucket(&BucketKey::per_tenant(tenant))
        .unwrap()
        .unwrap();
    assert_eq!(snap.available_tokens, 3.0);
    assert_eq!(snap.burst_capacity, 200);
}

// ---- WP-I.1: planned entries are NOT permanent ----------------------
//
// The pre-fix `HashSet<Uuid>` lifetime set broke two production
// shapes. We pin BOTH here.

/// Resolver returns `None` (transient D1 outage / unknown tier) on
/// the FIRST call. The tenant must NOT be marked planned — the
/// next call must re-attempt the resolver. The pre-fix code
/// inserted the tenant into the set on `None` (silently pinning
/// the customer to the team-default ladder for the container
/// lifetime). We assert via a calls counter that the second
/// request reaches the resolver.
#[tokio::test]
async fn transient_resolver_failure_does_not_pin_planned() {
    use std::sync::atomic::{AtomicU32, Ordering as AOrd};

    /// Resolver that returns `None` for the first call, then a label.
    /// Call count is exposed for the test assertion.
    #[derive(Debug)]
    struct FlakyResolver {
        none_until: AtomicU32,
        calls: Arc<AtomicU32>,
    }
    #[async_trait::async_trait]
    impl TenantTierResolver for FlakyResolver {
        async fn resolve_tier_label(&self, _tenant_id: &str) -> Option<String> {
            self.calls.fetch_add(1, AOrd::SeqCst);
            let prev = self.none_until.load(AOrd::SeqCst);
            if prev > 0 {
                self.none_until.store(prev - 1, AOrd::SeqCst);
            }
            if prev > 0 {
                None
            } else {
                Some("team".to_owned())
            }
        }
    }

    let clock = fixed_clock(7_000_000);
    let calls = Arc::new(AtomicU32::new(0));
    let resolver: Arc<dyn TenantTierResolver> = Arc::new(FlakyResolver {
        none_until: AtomicU32::new(1),
        calls: Arc::clone(&calls),
    });
    let state = RateLimitLayerState::with_tier_resolver(clock, resolver);

    // First call: resolver returns None (transient D1 outage).
    // Bucket stays on team default. No planned entry inserted.
    let app1 = app(state.clone(), Arc::new(AtomicUsize::new(0)));
    let _ = app1.oneshot(req(Some(TENANT_A))).await.unwrap();
    let calls_after_first = calls.load(AOrd::SeqCst);
    assert_eq!(calls_after_first, 1, "first request must call the resolver");

    // Second call: resolver returns Some("team"). It IS called again,
    // proving the first call's failure did NOT pin the planned entry.
    // (Pre-fix code: `g.insert(tenant)` even on None -> second call would
    // short-circuit on `if g.contains(...) return` and the calls counter
    // would still be 1.)
    let app2 = app(state.clone(), Arc::new(AtomicUsize::new(0)));
    let _ = app2.oneshot(req(Some(TENANT_A))).await.unwrap();
    let calls_after_second = calls.load(AOrd::SeqCst);
    assert!(
            calls_after_second > calls_after_first,
            "the resolver was NOT re-called after a None result -- the              planned entry was inserted on failure, the WP-I.1 fix is not              working"
        );
}

/// TTL expiry: a planned entry expires after `PLANNED_ENTRY_TTL_MS`
/// and the next call re-resolves. The pre-fix `HashSet` had no TTL
/// at all, so a tier upgrade was invisible to the limiter until
/// container restart.
#[tokio::test]
async fn planned_entry_expires_after_ttl() {
    use crate::wall_clock::InMemoryFakeWallClock;
    use std::sync::atomic::AtomicU32;

    #[derive(Debug)]
    struct AlwaysOkResolver {
        calls: Arc<AtomicU32>,
    }
    #[async_trait::async_trait]
    impl TenantTierResolver for AlwaysOkResolver {
        async fn resolve_tier_label(&self, _tenant_id: &str) -> Option<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Some("team".to_owned())
        }
    }

    let base_ms: u64 = 7_000_000;
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(base_ms));
    let calls = Arc::new(AtomicU32::new(0));
    let resolver: Arc<dyn TenantTierResolver> = Arc::new(AlwaysOkResolver {
        calls: Arc::clone(&calls),
    });
    let state =
        RateLimitLayerState::with_tier_resolver(Arc::clone(&clock) as Arc<dyn WallClock>, resolver);

    // First request at t=base_ms: resolver is called, tenant is
    // marked planned with deadline = base_ms + 5 min.
    let app1 = app(state.clone(), Arc::new(AtomicUsize::new(0)));
    let _ = app1.oneshot(req(Some(TENANT_A))).await.unwrap();
    let calls_after_first = calls.load(Ordering::SeqCst);
    assert!(
        calls_after_first >= 1,
        "resolver was not called on first request"
    );

    // Subsequent requests within the TTL window: NO new resolver
    // calls (the planned entry is alive).
    for _ in 0..5 {
        let a = app(state.clone(), Arc::new(AtomicUsize::new(0)));
        let _ = a.oneshot(req(Some(TENANT_A))).await.unwrap();
    }
    assert_eq!(
            calls.load(Ordering::SeqCst),
            calls_after_first,
            "resolver was re-called within the TTL window — the planned              entry is being ignored or the TTL is too short"
        );

    // Advance the fake clock past the 5-min TTL.
    clock.advance(std::time::Duration::from_millis(5 * 60 * 1000 + 1));

    // Next request: the planned entry has expired, the resolver
    // MUST be called again.
    let app3 = app(state.clone(), Arc::new(AtomicUsize::new(0)));
    let _ = app3.oneshot(req(Some(TENANT_A))).await.unwrap();
    let calls_after_expiry = calls.load(Ordering::SeqCst);
    assert!(
            calls_after_expiry > calls_after_first,
            "resolver was NOT re-called after the TTL expired — the              planned entry is permanent, the WP-I.1 fix is not working"
        );
}

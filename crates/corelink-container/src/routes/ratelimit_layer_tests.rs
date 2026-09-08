#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use super::*;
use crate::wall_clock::InMemoryFakeWallClock;
use axum::{
    body::Body,
    http::{Request as HttpRequest, StatusCode},
    routing::get,
    Router,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt; // for `.oneshot()`

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";

/// A wall clock pinned at a fixed unix-ms instant (so refill never tops
/// the bucket between requests within a single test).
fn fixed_clock(unix_ms: u64) -> Arc<dyn WallClock> {
    Arc::new(InMemoryFakeWallClock::at_unix_ms(unix_ms))
}

fn app(state: RateLimitLayerState, hits: Arc<AtomicUsize>) -> Router {
    Router::new()
        .route(
            "/v1/cas/{tenant}/{hash}",
            get(move || {
                let h = hits.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            state,
            rate_limit_layer,
        ))
}

fn req(tenant: Option<&str>) -> HttpRequest<Body> {
    let mut b = HttpRequest::builder().uri("/v1/cas/t/abc");
    if let Some(t) = tenant {
        b = b.header(TENANT_HEADER, t);
    }
    b.body(Body::empty()).unwrap()
}

/// An app mounting OCI-shaped routes so the layer's path-based OCI gate
/// (F-016) is exercised end-to-end. `/token` is mounted alongside `/v2/*`
/// because the unauthenticated-scope partition is keyed there.
fn oci_app(state: RateLimitLayerState, hits: Arc<AtomicUsize>) -> Router {
    let token_hits = hits.clone();
    Router::new()
        .route(
            "/v2/{*rest}",
            get(move || {
                let h = hits.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            }),
        )
        .route(
            "/token",
            get(move || {
                let h = token_hits.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            state,
            rate_limit_layer,
        ))
}

/// An OCI request carries NO tenant header (the Worker deletes it) — exactly
/// the F-016 condition that fail-OPENED the per-tenant gate.
fn oci_req(uri: &str) -> HttpRequest<Body> {
    HttpRequest::builder().uri(uri).body(Body::empty()).unwrap()
}

/// An OCI request as the Worker actually forwards it: no tenant header, but
/// the server-trusted `x-corelink-client-ip` set from `cf-connecting-ip`.
fn oci_req_from_ip(uri: &str, ip: &str) -> HttpRequest<Body> {
    HttpRequest::builder()
        .uri(uri)
        .header(CLIENT_IP_HEADER, ip)
        .body(Body::empty())
        .unwrap()
}

/// Drain an OCI bucket to exhaustion by replaying `build` `OCI_REPO_BURST`
/// times against the shared `state`.
async fn drain_oci_bucket(
    state: &RateLimitLayerState,
    hits: &Arc<AtomicUsize>,
    build: impl Fn() -> HttpRequest<Body>,
) {
    for _ in 0..OCI_REPO_BURST {
        let app = oci_app(state.clone(), hits.clone());
        let _ = app.oneshot(build()).await.unwrap();
    }
}

/// A fake tier resolver returning a fixed label (F-017 wiring test).
#[derive(Debug)]
struct FakeTierResolver {
    label: Option<String>,
}

#[async_trait]
impl TenantTierResolver for FakeTierResolver {
    async fn resolve_tier_label(&self, _tenant_id: &str) -> Option<String> {
        self.label.clone()
    }
}

#[test]
fn tenant_key_uuid_is_stable_and_distinct() {
    // Same input → same key.
    assert_eq!(tenant_key_uuid("acme"), tenant_key_uuid("acme"));
    // Distinct inputs → distinct keys.
    assert_ne!(tenant_key_uuid("acme"), tenant_key_uuid("globex"));
    // A real UUID maps through unchanged.
    let u = Uuid::from_u128(0xdead_beef);
    assert_eq!(tenant_key_uuid(&u.to_string()), u);
}

#[tokio::test]
async fn within_rate_allows() {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = app(RateLimitLayerState::new(), hits.clone());
    let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn absent_tenant_passes_through() {
    // Sentinel/absent tenant is NOT billable data-plane traffic → pass
    // through (handler's own AuthTenant fail-closes downstream).
    let hits = Arc::new(AtomicUsize::new(0));
    let app = app(RateLimitLayerState::new(), hits.clone());
    let resp = app.oneshot(req(None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn sentinel_tenant_passes_through() {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = app(RateLimitLayerState::new(), hits.clone());
    let resp = app.oneshot(req(Some("_anonymous"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn over_burst_is_429_with_retry_after() {
    // Pin the clock so refill never tops the bucket up between requests
    // (all requests share `now_ms`). Burst = 200 → the 201st request in the
    // same instant must be denied.
    let clock = fixed_clock(1_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    let mut last = StatusCode::OK;
    for _ in 0..(DEFAULT_TENANT_BURST + 1) {
        let app = app(state.clone(), hits.clone());
        let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
        last = resp.status();
        if last == StatusCode::TOO_MANY_REQUESTS {
            assert!(
                resp.headers().get(RETRY_AFTER).is_some(),
                "429 must carry Retry-After"
            );
            break;
        }
    }
    assert_eq!(
        last,
        StatusCode::TOO_MANY_REQUESTS,
        "exceeding the per-tenant burst must 429"
    );
}

#[tokio::test]
async fn distinct_tenants_have_independent_buckets() {
    // Drain tenant A to a 429, then confirm tenant B still gets through on
    // the SAME shared limiter instance (INV-AVAIL-ISOLATION).
    let clock = fixed_clock(2_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    for _ in 0..(DEFAULT_TENANT_BURST + 1) {
        let app = app(state.clone(), hits.clone());
        let _ = app.oneshot(req(Some(TENANT_A))).await.unwrap();
    }
    // Tenant B (different key) — fresh bucket → allow.
    let app = app(state.clone(), hits.clone());
    let resp = app
        .oneshot(req(Some("22222222-2222-2222-2222-222222222222")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---- F-016: OCI velocity gate ----------------------------------------

#[test]
fn oci_repo_scope_parses_canonical_paths() {
    assert_eq!(oci_repo_scope("/token").as_deref(), Some("_token"));
    assert_eq!(
        oci_repo_scope("/token?scope=repository:alpine:pull").as_deref(),
        Some("_token")
    );
    assert_eq!(oci_repo_scope("/v2").as_deref(), Some("_v2root"));
    assert_eq!(oci_repo_scope("/v2/").as_deref(), Some("_v2root"));
    assert_eq!(oci_repo_scope("/v2/_catalog").as_deref(), Some("_v2root"));
    assert_eq!(
        oci_repo_scope("/v2/alpine/blobs/sha256:abc").as_deref(),
        Some("alpine")
    );
    assert_eq!(
        oci_repo_scope("/v2/library/alpine/manifests/latest").as_deref(),
        Some("library/alpine")
    );
    assert_eq!(
        oci_repo_scope("/v2/library/alpine/tags/list").as_deref(),
        Some("library/alpine")
    );
    // Non-OCI paths → None (gate not engaged; per-tenant path handles them).
    assert_eq!(oci_repo_scope("/v1/cas/t/abc"), None);
    assert_eq!(oci_repo_scope("/v2foo/bar"), None);
    assert_eq!(oci_repo_scope("/health"), None);
}

#[test]
fn distinct_oci_repos_get_distinct_buckets() {
    let (_, a) = oci_bucket_key("alpine", "203.0.113.1");
    let (_, b) = oci_bucket_key("nginx", "203.0.113.1");
    assert_ne!(a, b);
    // Same repo → same key (stable).
    let (_, a2) = oci_bucket_key("alpine", "203.0.113.1");
    assert_eq!(a, a2);
}

#[test]
fn credential_gated_repo_scopes_ignore_the_client_ip() {
    // Per-repo scopes are already credential-gated (HMAC Bearer minted at
    // /token); their keying is UNCHANGED by this fix — the per-repo ceiling
    // is the isolation property they exist for.
    let (_, a) = oci_bucket_key("alpine", "203.0.113.1");
    let (_, b) = oci_bucket_key("alpine", "198.51.100.7");
    let (_, c) = oci_bucket_key("alpine", "");
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn unauthenticated_oci_scopes_partition_by_client_ip() {
    // The live defect: `_token` (and the equally credential-free `/v2`
    // version-check root) keyed on the scope literal alone, so every caller
    // on earth shared ONE 50 rps bucket.
    for scope in OCI_UNAUTH_SCOPES {
        let (_, a) = oci_bucket_key(scope, "203.0.113.1");
        let (_, b) = oci_bucket_key(scope, "198.51.100.7");
        assert_ne!(a, b, "{scope}: distinct client IPs must not share a bucket");
        // Same IP → same bucket (the limit still binds per source).
        let (_, a2) = oci_bucket_key(scope, "203.0.113.1");
        assert_eq!(a, a2, "{scope}: same client IP must share one bucket");
    }
    // The two unauthenticated scopes never alias each other.
    let (_, tok) = oci_bucket_key("_token", "203.0.113.1");
    let (_, root) = oci_bucket_key("_v2root", "203.0.113.1");
    assert_ne!(tok, root);
}

#[test]
fn absent_or_empty_client_ip_shares_one_disjoint_partition() {
    // Absent (`""`, what index.ts writes when cf-connecting-ip is missing)
    // and whitespace-only collapse to the SAME dedicated `_no_ip` bucket —
    // bounded (not fail-OPEN) yet disjoint from every real per-IP bucket, so
    // draining it can never 429 a request that carries a trusted IP.
    let (_, empty) = oci_bucket_key("_token", "");
    let (_, blank) = oci_bucket_key("_token", "   ");
    assert_eq!(empty, blank);
    let (_, real) = oci_bucket_key("_token", "203.0.113.1");
    assert_ne!(empty, real);
}

#[tokio::test]
async fn oci_flood_is_429_despite_absent_tenant_header() {
    // The exact F-016 condition: NO tenant header, OCI path. The prior
    // code fail-OPENED here (unbounded). Now the per-repo gate must 429
    // once the OCI repo burst is exhausted.
    let clock = fixed_clock(3_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    let mut last = StatusCode::OK;
    for _ in 0..(OCI_REPO_BURST + 1) {
        let app = oci_app(state.clone(), hits.clone());
        let resp = app
            .oneshot(oci_req("/v2/alpine/manifests/latest"))
            .await
            .unwrap();
        last = resp.status();
        if last == StatusCode::TOO_MANY_REQUESTS {
            assert!(
                resp.headers().get(RETRY_AFTER).is_some(),
                "OCI 429 must carry Retry-After"
            );
            break;
        }
    }
    assert_eq!(
        last,
        StatusCode::TOO_MANY_REQUESTS,
        "flooding one OCI repo (no tenant header) must 429 — F-016"
    );
}

#[tokio::test]
async fn oci_distinct_repos_have_independent_velocity_buckets() {
    // Draining repo `alpine` must NOT starve repo `nginx` (per-repo
    // isolation on the shared _oci pool).
    let clock = fixed_clock(4_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    for _ in 0..(OCI_REPO_BURST + 1) {
        let app = oci_app(state.clone(), hits.clone());
        let _ = app
            .oneshot(oci_req("/v2/alpine/manifests/latest"))
            .await
            .unwrap();
    }
    // Different repo — fresh bucket → allow.
    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req("/v2/nginx/manifests/latest"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn oci_token_flood_from_one_ip_does_not_429_another_ip() {
    // The live defect, end-to-end: one host draining /token used to 429
    // `docker login` for EVERY OCI user. Burn IP A's whole burst, then a
    // request from IP B must still be served.
    let clock = fixed_clock(8_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    drain_oci_bucket(&state, &hits, || oci_req_from_ip("/token", "203.0.113.1")).await;

    // Same IP, one more request → its own bucket is empty → 429.
    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req_from_ip("/token", "203.0.113.1"))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "the flooding source must still be shed"
    );

    // Different IP → fresh bucket → allowed.
    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req_from_ip("/token", "198.51.100.7"))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a flood from one host must not 429 /token for everyone else"
    );
}

#[tokio::test]
async fn oci_v2_root_flood_from_one_ip_does_not_429_another_ip() {
    // `/v2/_catalog` maps to the `_v2root` scope, which is reachable with no
    // credential at all (the OCI version-check ping every client sends
    // first) — same unauthenticated shape as /token, same partition.
    let clock = fixed_clock(9_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    drain_oci_bucket(&state, &hits, || {
        oci_req_from_ip("/v2/_catalog", "203.0.113.1")
    })
    .await;

    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req_from_ip("/v2/_catalog", "203.0.113.1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req_from_ip("/v2/_catalog", "198.51.100.7"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn oci_token_flood_without_client_ip_is_bounded_and_isolated() {
    // Header absent entirely (a request that never traversed the Worker):
    // it must still be SHED once the `_no_ip` partition is drained (not
    // fail-OPEN), and draining it must NOT lock out a real IP-carrying user
    // (not fail-CLOSED onto the shared population).
    let clock = fixed_clock(10_000_000);
    let state = RateLimitLayerState::with_clock(clock);
    let hits = Arc::new(AtomicUsize::new(0));

    drain_oci_bucket(&state, &hits, || oci_req("/token")).await;

    let app = oci_app(state.clone(), hits.clone());
    let resp = app.oneshot(oci_req("/token")).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "no-IP traffic must still be bounded, never fail-OPEN"
    );

    // An empty header value (what index.ts writes when cf-connecting-ip is
    // missing) lands in the SAME `_no_ip` partition.
    let app = oci_app(state.clone(), hits.clone());
    let resp = app.oneshot(oci_req_from_ip("/token", "")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // ...and a genuine client IP is untouched by that drain.
    let app = oci_app(state.clone(), hits.clone());
    let resp = app
        .oneshot(oci_req_from_ip("/token", "203.0.113.1"))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the _no_ip partition must be disjoint from every real per-IP bucket"
    );
}

// ---- F-017: per-tenant tier ladder -----------------------------------

#[tokio::test]
async fn tier_resolver_tightens_free_tenant_below_team_default() {
    // A `free` tenant (burst 50) must 429 well before the team default
    // burst (200) once the ladder is applied — proving the tier ladder is
    // actually enforced, not the flat hardcoded default.
    let clock = fixed_clock(5_000_000);
    let resolver: Arc<dyn TenantTierResolver> = Arc::new(FakeTierResolver {
        label: Some("free".to_string()),
    });
    let state = RateLimitLayerState::with_tier_resolver(clock, resolver);
    let hits = Arc::new(AtomicUsize::new(0));

    let mut denied_at = None;
    for i in 1..=(DEFAULT_TENANT_BURST) {
        let app = app(state.clone(), hits.clone());
        let resp = app.oneshot(req(Some(TENANT_A))).await.unwrap();
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            denied_at = Some(i);
            break;
        }
    }
    let denied_at = denied_at.expect("a free tenant must 429 before the team default burst");
    // Free burst is 50 (corelink_ratelimit::FREE_BURST). The deny must
    // happen at/around the free burst, far below the team default 200.
    assert!(
        denied_at <= corelink_ratelimit::FREE_BURST + 1,
        "free tenant denied at {denied_at}, expected ≤ {} (ladder not applied?)",
        corelink_ratelimit::FREE_BURST + 1
    );
    assert!(
        denied_at < DEFAULT_TENANT_BURST,
        "free tenant should be tighter than the team default"
    );
}

include!("ratelimit_layer_tests_part2.rs");

//! In-app per-tenant request-rate limiting for the data-plane routers
//! (audit #14/#16 closure).
//!
//! # Why this exists
//!
//! Before this layer the metered data plane (CAS/AC, Bazel REAPI, Turbo,
//! sccache, the cache adapters) had NO in-app request-RATE limiting — only a
//! storage-BYTE quota gate and a repo-invisible Cloudflare zone WAF rule. A
//! single authenticated PAT could therefore hammer the shared container with
//! unbounded request volume (DoS / noisy-neighbour) as long as it stayed under
//! its byte quota. This layer wires the already-built `corelink-ratelimit`
//! token-bucket engine in front of every data-plane handler, keyed on the
//! trusted, edge-injected tenant id.
//!
//! # Where it sits
//!
//! Wired as ONE `.layer(...)` line at the end of
//! [`crate::routes::build_with_factory`], so it covers exactly the composed
//! data-plane router. The `/_health` readiness probe and the `/_internal/*`
//! routes are merged in `main.rs` AFTER `build_with_factory` returns, so they
//! are intentionally OUTSIDE this layer (the DO's readiness probe must never be
//! rate-limited, and the internal surfaces carry their own shared-secret gate).
//!
//! # Tenant keying
//!
//! The bucket is keyed on the DO-injected `x-corelink-tenant-id` header — the
//! ONLY trustworthy tenant source inside the container (the Worker resolves it
//! from the PAT and strips any client-supplied value; see
//! [`crate::auth_tenant`]). `corelink-ratelimit` keys its buckets on a `Uuid`;
//! the container tenant header is an opaque string (a real UUID in production,
//! but the cas/ac handlers treat it as opaque). We therefore derive a STABLE
//! 128-bit key from the raw tenant string ([`tenant_key_uuid`]) so distinct
//! tenants land in distinct buckets without requiring the header to parse as a
//! UUID (no behavioural change / no new rejection for non-UUID tenants).
//!
//! # Fail-OPEN posture
//!
//! Requests with NO authenticated tenant header (sentinels / absent) are passed
//! through untouched — they are not billable data-plane traffic and the
//! handlers fail-closed on their own ([`crate::auth_tenant::AuthTenant`]). On
//! the limiter's OWN internal error (mutex poison, audit/metrics sink fault) we
//! fail-OPEN (allow) but log at `warn` — availability of the paid data plane is
//! prioritised over a perfectly-enforced cap, matching the crate's documented
//! "limiter backend fault ⇒ availability" intent. An over-limit decision is
//! rejected with HTTP 429 + `Retry-After` (RFC 6585 §4).

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use corelink_ratelimit::{
    BucketKey, InMemoryTokenBucketRateLimiter, InMemoryRateLimitAuditSink,
    InMemoryRateLimitMetrics, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use uuid::Uuid;

use crate::wall_clock::{self, WallClock};

/// The trusted, edge-injected tenant header (see [`crate::auth_tenant`]).
const TENANT_HEADER: &str = "x-corelink-tenant-id";

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors [`crate::auth_tenant`]'s sentinel list: such traffic is passed
/// through (not billable data-plane traffic; the handlers fail-closed).
const SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending", ""];

/// Default sustained per-tenant request rate (tokens / second).
///
/// 100 req/s sustained is comfortably above any interactive client's steady
/// state yet low enough to blunt a single-PAT flood against the shared
/// container. Documented const (not a magic number) so the cap is auditable in
/// one place.
pub const DEFAULT_TENANT_REQ_PER_SEC: u32 = 100;

/// Default per-tenant burst capacity (tokens). A 200-token bucket absorbs a
/// 2-second burst at the sustained rate (e.g. a parallel `bazel` /
/// `turbo`/`cargo` fan-out kicking off many cache lookups at once) before the
/// token bucket starts shedding with 429s.
pub const DEFAULT_TENANT_BURST: u32 = 200;

/// Per-request cost charged against the bucket (one token per HTTP request).
const COST_PER_REQUEST: u32 = 1;

/// Shared rate-limit layer state: the per-tenant token-bucket limiter plus the
/// wall clock that anchors the bucket refill instant.
///
/// Held behind `Arc`s so the single limiter instance (and therefore the single
/// per-tenant bucket map) is shared across every data-plane route, exactly like
/// the production CF Durable Object singleton it stands in for.
#[derive(Clone)]
pub struct RateLimitLayerState {
    limiter: Arc<
        InMemoryTokenBucketRateLimiter<InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics>,
    >,
    clock: Arc<dyn WallClock>,
}

impl std::fmt::Debug for RateLimitLayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitLayerState")
            .finish_non_exhaustive()
    }
}

impl RateLimitLayerState {
    /// Build the production layer state with the canonical per-tenant cap
    /// ([`DEFAULT_TENANT_REQ_PER_SEC`] / [`DEFAULT_TENANT_BURST`]) and the
    /// system wall clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(wall_clock::default_wall_clock())
    }

    /// Build with an explicit wall clock (test wiring injects a deterministic
    /// fake; production uses [`wall_clock::default_wall_clock`]).
    #[must_use]
    pub fn with_clock(clock: Arc<dyn WallClock>) -> Self {
        // `with_overrides` only returns `None` on a self-inconsistent config
        // (zero burst, inverted Retry-After bounds); our constants are
        // statically valid, so fall back to the crate's canonical config if a
        // future edit ever breaks that invariant rather than panicking.
        let config = RateLimitConfig::with_overrides(
            DEFAULT_TENANT_REQ_PER_SEC,
            DEFAULT_TENANT_BURST,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap_or_else(RateLimitConfig::canonical);
        let limiter = InMemoryTokenBucketRateLimiter::new(
            Arc::new(InMemoryRateLimitAuditSink::new()),
            Arc::new(InMemoryRateLimitMetrics::new()),
            config,
        );
        Self {
            limiter: Arc::new(limiter),
            clock,
        }
    }
}

impl Default for RateLimitLayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed namespace bytes for [`tenant_key_uuid`] — a private constant so the
/// derivation is stable across restarts (the in-memory bucket map is
/// per-process, but a stable mapping keeps test reasoning + future durable
/// mirrors deterministic).
const TENANT_NS: [u8; 16] = *b"corelink-rl-tnt!";

/// Derive a STABLE 128-bit bucket key from the raw (opaque) tenant string.
///
/// `corelink-ratelimit` keys on a `Uuid`; the container tenant header is an
/// opaque string (a real UUID in prod, but not guaranteed to parse — the cas/ac
/// handlers treat it as opaque). A real UUID maps through unchanged; any other
/// string is folded into 16 bytes via FNV-1a-128 over a fixed namespace so
/// distinct tenants land in distinct buckets with overwhelming probability and
/// the same tenant always lands in the same bucket. Pure + dependency-free (no
/// extra `uuid` feature needed).
#[must_use]
pub fn tenant_key_uuid(raw_tenant: &str) -> Uuid {
    if let Ok(parsed) = Uuid::parse_str(raw_tenant.trim()) {
        return parsed;
    }
    // FNV-1a-128 over (namespace || tenant bytes).
    const FNV_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = FNV_OFFSET;
    for &b in TENANT_NS.iter().chain(raw_tenant.as_bytes()) {
        hash ^= u128::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    Uuid::from_u128(hash)
}

/// Axum middleware: per-tenant request-rate token-bucket gate.
///
/// Wired as `.layer(axum::middleware::from_fn_with_state(state, rate_limit_layer))`.
/// Reads the trusted tenant header, charges one token against the tenant's
/// bucket, and either forwards (`Allow`) or rejects with 429 + `Retry-After`
/// (`Deny429`). Fail-OPEN on an absent tenant (sentinel / non-data-plane
/// traffic) and on the limiter's own internal fault (logged).
pub async fn rate_limit_layer(
    State(state): State<RateLimitLayerState>,
    req: Request,
    next: Next,
) -> Response {
    let raw_tenant = req
        .headers()
        .get(TENANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");

    // No authenticated tenant ⇒ not billable data-plane traffic. Pass through
    // (the handler's own AuthTenant extractor fail-CLOSES). Keying a shared
    // bucket on the empty/sentinel tenant would also wrongly couple unrelated
    // anonymous probes into one bucket.
    if raw_tenant.is_empty() || SENTINELS.contains(&raw_tenant) {
        return next.run(req).await;
    }

    let tenant = tenant_key_uuid(raw_tenant);
    let bucket_key = BucketKey::per_tenant(tenant);
    let now_ms = state.clock.now_ms();

    match state
        .limiter
        .try_acquire(tenant, bucket_key, COST_PER_REQUEST, now_ms)
    {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => next.run(req).await,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                tracing::warn!(
                    retry_after_secs,
                    "rate_limit: per-tenant request rate exceeded (429)"
                );
                too_many_requests(retry_after_secs)
            }
            // `RateLimitDecision` is `#[non_exhaustive]`; any future non-Allow
            // arm fail-CLOSES to 429 (a new deny-shaped arm should not silently
            // become an allow).
            _ => too_many_requests(corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS),
        },
        // Limiter's OWN internal fault (mutex poison, sink error) ⇒ fail-OPEN
        // for availability, but log so it is visible. The paid data plane stays
        // up; the bucket fault is an operational signal, not a client error.
        Err(err) => {
            tracing::warn!(error = %err, "rate_limit: limiter internal error; failing OPEN");
            next.run(req).await
        }
    }
}

/// Build the uniform 429 response with a clamped `Retry-After` header.
fn too_many_requests(retry_after_secs: u64) -> Response {
    let body = format!(
        "{{\"error\":\"rate_limited\",\"message\":\"per-tenant request rate exceeded; \
         retry after {retry_after_secs}s\"}}"
    );
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        [("content-type", "application/json")],
        body,
    )
        .into_response();
    if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        resp.headers_mut().insert(RETRY_AFTER, val);
    }
    resp
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        routing::get,
        Router,
    };
    use crate::wall_clock::InMemoryFakeWallClock;
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
                "/v1/cas/:tenant/:hash",
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
}

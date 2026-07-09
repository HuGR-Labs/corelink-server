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
//! # Bounded production sinks (F-022)
//!
//! The limiter is wired with `corelink_ratelimit::NoOpRateLimitAuditSink` /
//! `NoOpRateLimitMetrics` — bounded (O(1) memory, zero per-request allocation)
//! production sinks. The crate also ships `InMemoryRateLimitAuditSink` /
//! `InMemoryRateLimitMetrics`, but those are TEST CAPTURE sinks that push every
//! decision onto unbounded `Vec`/`HashMap`s; wiring them here (the prior bug)
//! leaks heap on ordinary in-budget traffic and self-OOMs the data-plane
//! container. The full prod composition (`OutboxAuditSink` + `MultiplexAuditSink`
//! → D1 `audit_outbox` + SIEM) lands with the live-DO/D1 wiring (WI-S08-006);
//! until then the bounded NoOp sinks are the correct posture (rate-limit audit
//! is informational — no SEV-1 arm).
//!
//! # Per-tenant tier ladder (F-017)
//!
//! When `routes.rs` constructs the state via [`RateLimitLayerState::with_tier_resolver`]
//! with a D1-backed [`TenantTierResolver`], the first request from each tenant
//! loads its billing tier and applies the canonical RPS ladder
//! ([`corelink_ratelimit::refill_rate_for_tier`]) to its bucket via
//! `update_plan` — so paid tiers actually get more headroom (and free/solo are
//! tightened) instead of every tenant sharing the team default. Without a
//! resolver ([`RateLimitLayerState::new`]) every tenant stays on the team
//! default (prior behaviour). **Residual** (out of this file's scope): bucket
//! durability across container restart needs the D1 `ratelimit_buckets` mirror
//! (migration 0010) reloaded at start via [`RateLimitLayerState::seed_persisted_bucket`]
//! — a `routes.rs`/`main.rs` seam.
//!
//! # OCI velocity gate (F-016)
//!
//! The Worker forwards the OCI plane (`/v2/*` + `/token`) with the tenant header
//! DELETED, so the per-tenant gate fail-OPENS for every OCI request — leaving
//! the shared `_oci` pool with no per-second limit (an unauthenticated
//! `/v2/`+`/token` flood can starve all OCI tenants). This layer keys a
//! SEPARATE, tighter limiter on the OCI repo/realm parsed from the path
//! ([`oci_repo_scope`]) so a single repo/realm's req/s is bounded. **Residual**:
//! a true per-IP edge limit needs `cf-connecting-ip` forwarded on the OCI arm
//! (the Worker does not set it there today) and/or an edge per-IP WAF rule on
//! `corelink-oci.humangr.com` — both are Worker/infra, off-repo. The
//! container-side per-repo velocity gate closes the no-limit-at-all hole.
//!
//! # Fail-OPEN posture
//!
//! Sentinel/absent-tenant requests that are NOT on the OCI plane are passed
//! through untouched — not billable data-plane traffic; the handlers fail-closed
//! on their own ([`crate::auth_tenant::AuthTenant`]). On the limiter's OWN
//! internal error (mutex poison, sink fault) we fail-OPEN (allow) but log at
//! `warn` — availability of the paid data plane is prioritised over a
//! perfectly-enforced cap, matching the crate's documented "limiter backend
//! fault ⇒ availability" intent. An over-limit decision is rejected with HTTP
//! 429 + `Retry-After` (RFC 6585 §4).

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use corelink_ratelimit::{
    refill_rate_for_tier, tier_for_billing_label, BucketKey, InMemoryTokenBucketRateLimiter,
    KeyDimension, NoOpRateLimitAuditSink, NoOpRateLimitMetrics, RateLimitConfig,
    RateLimitDecision, RateLimiter,
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

// --- OCI velocity gate (F-016) ----------------------------------------------

/// Synthetic namespace bytes for the OCI velocity gate's per-repo buckets.
///
/// The OCI plane (`/v2/*` + `/token`) is forwarded by the Worker with the
/// `x-corelink-tenant-id` header DELETED (the tenant is derived from the OCI
/// Bearer/PAT inside the container, AFTER the limiter runs), so the per-tenant
/// gate above fail-OPENS for every OCI request — leaving the shared `_oci` pool
/// with NO per-second velocity limit (F-016: unauth-internet DoS reachable via
/// `/v2/`+`/token`). We cannot key OCI on a verified tenant pre-auth, so this
/// gate keys a SEPARATE limiter on a value the limiter CAN see before the
/// container resolves the bearer: the OCI **repository name** parsed from the
/// request path (`/v2/<repo>/...`), with `/token` and the `/v2/` root mapped to
/// fixed synthetic scopes. This bounds any single repo/realm's req/s against the
/// shared pool. It is NOT a per-IP edge limit (the OCI forward arm does not
/// carry `cf-connecting-ip`, and the edge per-IP WAF rule is infra, off-repo) —
/// see the module residual note.
const OCI_NS: [u8; 16] = *b"corelink-rl-oci!";

/// Sustained per-OCI-repo request rate (tokens / second). Tighter than the
/// per-tenant default because this gate guards the SHARED `_oci` pool reachable
/// **unauthenticated**, so the safe per-key budget is small while still
/// comfortably above any single `docker pull/push` repo's interactive rate.
const OCI_REPO_REQ_PER_SEC: u32 = 50;

/// Per-OCI-repo burst capacity (tokens) — a 4× burst window absorbs a layered
/// `docker push`/`pull` fan-out (many concurrent blob/manifest ops on one repo)
/// before shedding with 429s.
const OCI_REPO_BURST: u32 = 200;

// --- Per-tenant tier ladder (F-017) -----------------------------------------

/// Resolves a tenant's billing tier so the per-tenant token-bucket ladder
/// (Solo→Enterprise RPS) is actually enforced instead of every tenant sharing
/// the hardcoded team-default ([`DEFAULT_TENANT_REQ_PER_SEC`]).
///
/// Implemented container-side (NOT in `corelink-ratelimit`, which is
/// wasm32-clean and tokio-free) over D1 — mirroring [`crate::oci_cap`]'s
/// `tier_selections(active) → tenant.tier → free` lookup order. The layer
/// calls [`Self::resolve_tier_label`] the first time it sees a tenant and feeds
/// the result through [`corelink_ratelimit::tier_for_billing_label`] →
/// [`corelink_ratelimit::refill_rate_for_tier`] → `update_plan`, so the bucket
/// adopts the tenant's real ladder rung.
#[async_trait]
pub trait TenantTierResolver: std::fmt::Debug + Send + Sync {
    /// Resolve the billing-tier wire label (e.g. `"solo"`, `"pro"`) for
    /// `tenant_id` (canonical UUID text). Returns `None` when the tier cannot
    /// be confirmed (D1 error / no row); the caller then leaves the bucket on
    /// the team-default config — fail-SAFE on availability (never over-throttle
    /// a tenant we cannot classify), matching `tier_for_billing_label`'s own
    /// unknown→Team posture.
    async fn resolve_tier_label(&self, tenant_id: &str) -> Option<String>;
}

/// Shared rate-limit layer state: the per-tenant token-bucket limiter plus the
/// wall clock that anchors the bucket refill instant.
///
/// Held behind `Arc`s so the single limiter instance (and therefore the single
/// per-tenant bucket map) is shared across every data-plane route, exactly like
/// the production CF Durable Object singleton it stands in for.
#[derive(Clone)]
pub struct RateLimitLayerState {
    /// Per-tenant token bucket (camada 1). Keyed on the verified tenant header.
    ///
    /// F-022: wired with the BOUNDED production sinks
    /// ([`NoOpRateLimitAuditSink`] / [`NoOpRateLimitMetrics`]) — O(1) memory,
    /// no unbounded `Vec`/label-cardinality growth per request. The previous
    /// wiring used the crate's TEST capture sinks (`InMemory*`), which push
    /// onto unbounded buffers on every (Allowed) request and self-OOM the
    /// data-plane container under honest sustained load.
    limiter: Arc<InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>>,
    /// Per-OCI-repo velocity bucket (F-016). A SEPARATE limiter (own tighter
    /// config) keyed on the OCI repo/realm parsed from the path, because the
    /// OCI plane reaches this layer with the tenant header deleted and so
    /// fail-OPENS the per-tenant gate.
    oci_limiter: Arc<InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>>,
    clock: Arc<dyn WallClock>,
    /// Per-tenant tier resolver (F-017). `None` in dev/CI or until `routes.rs`
    /// wires a D1-backed impl — then every tenant stays on the team-default
    /// config (current behaviour). When present, the first request from a
    /// tenant loads its tier and calls `update_plan` so the RPS ladder applies.
    tier_resolver: Option<Arc<dyn TenantTierResolver>>,
    /// Tenants whose tier has already been resolved + applied to the bucket
    /// (so we resolve a tenant's tier at most once per container lifetime,
    /// keeping the hot path D1-free after first touch).
    planned: Arc<Mutex<HashSet<Uuid>>>,
}

impl std::fmt::Debug for RateLimitLayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitLayerState")
            .field("tier_resolver", &self.tier_resolver.is_some())
            .finish_non_exhaustive()
    }
}

impl RateLimitLayerState {
    /// Build the production layer state with the canonical per-tenant cap
    /// ([`DEFAULT_TENANT_REQ_PER_SEC`] / [`DEFAULT_TENANT_BURST`]) and the
    /// system wall clock. No tier resolver (every tenant on the team default);
    /// `routes.rs` should prefer [`Self::with_tier_resolver`] to enforce the
    /// per-tier RPS ladder (F-017).
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(wall_clock::default_wall_clock())
    }

    /// Build with an explicit wall clock (test wiring injects a deterministic
    /// fake; production uses [`wall_clock::default_wall_clock`]).
    #[must_use]
    pub fn with_clock(clock: Arc<dyn WallClock>) -> Self {
        Self::build(clock, None)
    }

    /// Build the production layer state with a D1-backed per-tenant tier
    /// resolver (F-017). The first request from each tenant loads its billing
    /// tier and applies the canonical RPS ladder
    /// ([`corelink_ratelimit::refill_rate_for_tier`]) to its bucket via
    /// `update_plan`, so paid tiers actually get more headroom (and free/solo
    /// are tightened) instead of every tenant sharing the team default.
    ///
    /// Residual (durability across restart): the in-process bucket map is
    /// ephemeral, so a container cold-start still re-resolves tiers lazily on
    /// first touch. Persisting/reloading buckets across restart needs the D1
    /// `ratelimit_buckets` mirror (migration 0010) wired through
    /// [`Self::seed_persisted_bucket`] at container start — a `routes.rs`/
    /// `main.rs` seam (out of this file's scope); see the module residual note.
    #[must_use]
    pub fn with_tier_resolver(
        clock: Arc<dyn WallClock>,
        tier_resolver: Arc<dyn TenantTierResolver>,
    ) -> Self {
        Self::build(clock, Some(tier_resolver))
    }

    fn build(clock: Arc<dyn WallClock>, tier_resolver: Option<Arc<dyn TenantTierResolver>>) -> Self {
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
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            config,
        );
        // F-016: the OCI velocity limiter has its own, tighter config keyed
        // per-repo. Same NoOp bounded sinks (F-022).
        let oci_config = RateLimitConfig::with_overrides(
            OCI_REPO_REQ_PER_SEC,
            OCI_REPO_BURST,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap_or_else(RateLimitConfig::canonical);
        let oci_limiter = InMemoryTokenBucketRateLimiter::new(
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            oci_config,
        );
        Self {
            limiter: Arc::new(limiter),
            oci_limiter: Arc::new(oci_limiter),
            clock,
            tier_resolver,
            planned: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Seed a per-tenant bucket from the durable D1 `ratelimit_buckets` mirror
    /// (migration 0010) at container start, so buckets survive a restart
    /// instead of resetting to full burst (F-017 durability half).
    ///
    /// `routes.rs`/`main.rs` calls this once per persisted row before the
    /// router serves traffic. Kept here (not in the hot path) so the wiring is
    /// a single explicit seam; absence simply means the bucket materialises
    /// lazily on first `try_acquire` (today's behaviour).
    ///
    /// # Errors
    ///
    /// Propagates [`corelink_ratelimit::RateLimitError`] on limiter mutex
    /// poisoning (the only failure mode of the underlying seed).
    pub fn seed_persisted_bucket(
        &self,
        tenant: Uuid,
        available_tokens: f64,
        burst_capacity: u32,
        refill_rate_per_sec: f64,
        last_refill_at_ms: u64,
    ) -> Result<(), corelink_ratelimit::RateLimitError> {
        let state = corelink_ratelimit::TokenBucketState::from_persisted(
            available_tokens,
            burst_capacity,
            refill_rate_per_sec,
            last_refill_at_ms,
        );
        self.limiter
            .seed_bucket(BucketKey::per_tenant(tenant), state)
    }

    /// Resolve + apply a tenant's tier to its bucket exactly once (F-017).
    /// Called from the middleware before the first charge against a new
    /// tenant's bucket. After this returns, the bucket reflects the tenant's
    /// real RPS ladder rung. A resolver absence / `None` tier leaves the bucket
    /// on the team default (fail-safe on availability).
    async fn ensure_tier_applied(&self, raw_tenant: &str, tenant: Uuid, now_ms: u64) {
        let Some(resolver) = self.tier_resolver.as_ref() else {
            return;
        };
        // Fast path: already planned this tenant — skip the D1 hop. A poisoned
        // set (Err) is treated as not-yet-planned (re-resolve is idempotent);
        // we never block the request on a poisoned guard.
        if let Ok(g) = self.planned.lock() {
            if g.contains(&tenant) {
                return;
            }
        }
        let label = resolver.resolve_tier_label(raw_tenant).await;
        let tier = match label {
            Some(l) => tier_for_billing_label(&l),
            // Unknown tier: leave the bucket on the team default but STILL
            // mark planned so we don't re-hit D1 every request for a tenant
            // whose tier is (currently) unresolvable.
            None => {
                if let Ok(mut g) = self.planned.lock() {
                    g.insert(tenant);
                }
                return;
            }
        };
        let (rps, burst) = refill_rate_for_tier(tier);
        // `update_plan` materialises the bucket if absent and snaps available
        // tokens down on a shrink; a freshly-materialised bucket starts at the
        // new (tier) burst. Failure (mutex poison) is logged, not fatal — the
        // request proceeds on whatever bucket exists.
        if let Err(err) = self.limiter.update_plan(
            tenant,
            KeyDimension::PerTenant,
            "",
            rps,
            burst,
            now_ms,
        ) {
            tracing::warn!(error = %err, "rate_limit: tier update_plan failed; bucket left on default");
            return;
        }
        if let Ok(mut g) = self.planned.lock() {
            g.insert(tenant);
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

    // No verified tenant ⇒ either non-billable traffic OR the OCI plane (the
    // Worker deletes the tenant header on `/v2/*` + `/token`; the tenant is
    // resolved from the OCI Bearer AFTER this layer). F-016: instead of an
    // unconditional fail-OPEN, apply the per-OCI-repo velocity gate on OCI
    // paths so an unauthenticated `/v2/`+`/token` flood cannot starve the
    // shared `_oci` pool. Genuinely non-OCI sentinel/absent traffic still
    // passes through (the handler's own AuthTenant fail-CLOSES).
    if raw_tenant.is_empty() || SENTINELS.contains(&raw_tenant) {
        if let Some(scope) = oci_repo_scope(req.uri().path()) {
            return run_oci_velocity_gate(&state, &scope, req, next).await;
        }
        return next.run(req).await;
    }

    let tenant = tenant_key_uuid(raw_tenant);
    let now_ms = state.clock.now_ms();
    // F-017: apply the tenant's real tier ladder before charging the bucket
    // (no-op when no resolver is wired or the tier is unresolvable).
    state.ensure_tier_applied(raw_tenant, tenant, now_ms).await;
    let bucket_key = BucketKey::per_tenant(tenant);

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

/// Parse the OCI velocity-gate scope key from a request path, or `None` if the
/// path is not on the OCI plane (`/v2/*` + `/token`).
///
/// - `/token`               → `Some("_token")`
/// - `/v2`, `/v2/`, `/v2/_catalog` → `Some("_v2root")`
/// - `/v2/<repo>/...`       → `Some("<repo>")` (the OCI repository name — the
///   first path segment after `/v2/`; everything before `/blobs|/manifests|...`)
///
/// Keying on the repo name gives per-repo isolation (one hot/abused repo can't
/// starve the rest) plus a global per-repo ceiling on the shared `_oci` pool.
/// The repo name is taken verbatim (OCI repo names are `[a-z0-9._/-]+`); it is
/// only ever used as an opaque bucket scope string, never interpreted.
#[must_use]
fn oci_repo_scope(path: &str) -> Option<String> {
    if path == "/token" || path.starts_with("/token?") || path.starts_with("/token/") {
        return Some("_token".to_string());
    }
    // The OCI registry root is exactly `/v2`; everything else is `/v2/<rest>`
    // (a bare `/v2foo` is NOT an OCI path and must NOT match — it would
    // otherwise fail-open back to a non-OCI path, which is acceptable, but we
    // keep the prefix exact for clarity).
    if path == "/v2" {
        return Some("_v2root".to_string());
    }
    let rest = path.strip_prefix("/v2/")?;
    if rest.is_empty() || rest == "_catalog" {
        return Some("_v2root".to_string());
    }
    // The OCI repo name is everything up to the first OCI verb segment
    // (`/blobs`, `/manifests`, `/tags`, `/referrers`) or `/`. Repo names CAN
    // contain `/` (e.g. `library/alpine`), so split on the known verbs.
    for verb in ["/blobs", "/manifests", "/tags", "/referrers"] {
        if let Some(idx) = rest.find(verb) {
            let repo = &rest[..idx];
            if !repo.is_empty() {
                return Some(repo.to_string());
            }
        }
    }
    // No recognised verb — fall back to the leading segment so a malformed /
    // probe path still keys a bounded bucket rather than fail-opening.
    let lead = rest.split('/').next().unwrap_or(rest);
    Some(if lead.is_empty() {
        "_v2root".to_string()
    } else {
        lead.to_string()
    })
}

/// Derive a STABLE per-OCI-repo bucket key under the synthetic OCI namespace
/// ([`OCI_NS`]). All OCI buckets share one synthetic tenant id (the namespace
/// UUID) and isolate by repo via the `per_tenant_per_endpoint` scope.
#[must_use]
fn oci_bucket_key(scope: &str) -> (Uuid, BucketKey) {
    let realm = Uuid::from_bytes(OCI_NS);
    (realm, BucketKey::per_tenant_per_endpoint(realm, scope))
}

/// Run the per-OCI-repo velocity gate (F-016). Fail-OPEN on the limiter's own
/// internal fault (availability), mirroring the per-tenant path.
async fn run_oci_velocity_gate(
    state: &RateLimitLayerState,
    scope: &str,
    req: Request,
    next: Next,
) -> Response {
    let (realm, bucket_key) = oci_bucket_key(scope);
    let now_ms = state.clock.now_ms();
    match state
        .oci_limiter
        .try_acquire(realm, bucket_key, COST_PER_REQUEST, now_ms)
    {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => next.run(req).await,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                tracing::warn!(
                    retry_after_secs,
                    oci_scope = scope,
                    "rate_limit: OCI per-repo request rate exceeded (429)"
                );
                too_many_requests(retry_after_secs)
            }
            _ => too_many_requests(corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS),
        },
        Err(err) => {
            tracing::warn!(error = %err, "rate_limit: OCI limiter internal error; failing OPEN");
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

    /// An app mounting an OCI-shaped route so the layer's path-based OCI gate
    /// (F-016) is exercised end-to-end.
    fn oci_app(state: RateLimitLayerState, hits: Arc<AtomicUsize>) -> Router {
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
            .layer(axum::middleware::from_fn_with_state(state, rate_limit_layer))
    }

    /// An OCI request carries NO tenant header (the Worker deletes it) — exactly
    /// the F-016 condition that fail-OPENED the per-tenant gate.
    fn oci_req(uri: &str) -> HttpRequest<Body> {
        HttpRequest::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap()
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
        let (_, a) = oci_bucket_key("alpine");
        let (_, b) = oci_bucket_key("nginx");
        assert_ne!(a, b);
        // Same repo → same key (stable).
        let (_, a2) = oci_bucket_key("alpine");
        assert_eq!(a, a2);
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
        let resolver: Arc<dyn TenantTierResolver> =
            Arc::new(FakeTierResolver { label: None });
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
}

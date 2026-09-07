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
//! ([`oci_repo_scope`]) so a single repo/realm's req/s is bounded.
//!
//! # Per-source partition of the UNAUTHENTICATED OCI scopes
//!
//! Two of those scopes are reachable with NO credential of any kind: `/token`
//! (the OCI Basic→Bearer exchange) and the `/v2` version-check root — every
//! `docker login` / `pull` / `push` starts by hitting BOTH, and this layer
//! charges the bucket BEFORE `next.run(req)`, so a garbage `Authorization`
//! header (or none at all) still spends a token. Keying those two on the scope
//! literal alone therefore gave the whole internet ONE
//! [`OCI_REPO_REQ_PER_SEC`]-req/s bucket each: a single host could 429 OCI token
//! exchange for every legitimate user. They are now additionally partitioned by
//! the server-trusted client IP ([`CLIENT_IP_HEADER`] — set by the Worker from
//! `cf-connecting-ip`, unspoofable because `stripClientTrustHeaders` deletes any
//! client-supplied copy before the Worker sets its own). Credential-gated
//! per-repo scopes keep their existing per-repo keying (that is the isolation
//! property they are there for).
//!
//! **Partition ≠ elimination.** A distributed attacker who rotates source IPs
//! still gets a fresh source partition; a separately managed edge per-IP/ASN
//! policy may add another outer layer. That operational distinction does not
//! reopen DD-016: the data plane has its own bounded, executable deny gate and
//! no longer treats a dashboard-only WAF rule as its sole request-rate defense.
//! The deployed WAF inventory is deliberately not asserted here as application
//! evidence; this module is the source of truth for the in-container control.
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

use std::collections::HashMap;
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
    KeyDimension, NoOpRateLimitAuditSink, NoOpRateLimitMetrics, RateLimitConfig, RateLimitDecision,
    RateLimiter,
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
/// shared pool.
///
/// For the two synthetic scopes that carry NO credential ([`OCI_UNAUTH_SCOPES`])
/// the key additionally folds in the server-trusted client IP — see
/// [`oci_bucket_key`] and the module's per-source-partition note. This is NOT an
/// edge cap: it runs in-container, after the request has already been paid for
/// at the edge.
const OCI_NS: [u8; 16] = *b"corelink-rl-oci!";

/// The server-trusted client IP header the Worker injects on the OCI forward
/// arm (`worker/src/index.ts`, `oci_v2`/`oci_token` route kinds): it is set from
/// `cf-connecting-ip` AFTER `stripClientTrustHeaders` has deleted any
/// client-supplied copy, so a client cannot spoof or remove it. Same header the
/// signup and public-attestation rate limits key on.
const CLIENT_IP_HEADER: &str = "x-corelink-client-ip";

/// The OCI velocity-gate scopes reachable with NO credential of any kind, whose
/// buckets are partitioned per client IP (see the module note).
///
/// These are the two synthetic literals minted by [`oci_repo_scope`], never a
/// real repository name: the OCI Distribution grammar requires a repo path
/// component to START with an alphanumeric (`[a-z0-9]+(?:[._-]+[a-z0-9]+)*`), so
/// no real repo can be spelled `_token` / `_v2root` and collide with them.
const OCI_UNAUTH_SCOPES: &[&str] = &["_token", "_v2root"];

/// Bucket partition used for an unauthenticated OCI scope when the trusted
/// client IP is absent or empty — see [`oci_bucket_key`] for the rationale.
const NO_CLIENT_IP_PARTITION: &str = "_no_ip";

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
    ///
    /// WP-I.1: the previous `HashSet<Uuid>` was a pure LIFETIME set — once
    /// inserted, the entry never expired. That broke TWO production cases:
    ///   1. a tenant whose `D1` lookup initially failed (e.g. transient
    ///      D1 outage during the container's first request) was
    ///      permanently stuck on the team-default RPS ladder — a real
    ///      customer being silently throttled even after the outage
    ///      cleared. The fix: a failure (resolver `None` / D1 fault) is
    ///      treated as retryable — the entry is NOT inserted.
    ///   2. a tier upgrade (a Solo customer pays for Pro mid-window) was
    ///      invisible to the limiter — `update_plan` would only run on
    ///      the FIRST ever call from that tenant, and any subsequent call
    ///      hits the fast-path. The fix: entries carry a wall-clock
    ///      `expires_at_ms` and are evicted lazily on read.
    planned: Arc<Mutex<HashMap<Uuid, u64>>>,
}

/// TTL (ms) for a successful tier-resolution entry. Short enough that a
/// tenant upgrading their plan sees the new RPS ladder within one window
/// (5 min) without requiring a container restart, long enough that the
/// hot path stays D1-free under normal load. Re-anchored from the
/// briefing's 5 min default.
const PLANNED_ENTRY_TTL_MS: u64 = 5 * 60 * 1000;

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

    fn build(
        clock: Arc<dyn WallClock>,
        tier_resolver: Option<Arc<dyn TenantTierResolver>>,
    ) -> Self {
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
            planned: Arc::new(Mutex::new(HashMap::new())),
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
        // WP-I.1: a `planned` entry is only valid for `PLANNED_ENTRY_TTL_MS`
        // from its insertion. After expiry the entry is treated as
        // absent (lazy eviction on read) — a tier upgrade is observed
        // within one window, and a failure-then-success path recovers
        // without a container restart.
        if let Ok(mut g) = self.planned.lock() {
            if let Some(deadline) = g.get(&tenant).copied() {
                if deadline > now_ms {
                    return;
                }
                g.remove(&tenant);
            }
        }
        let label = resolver.resolve_tier_label(raw_tenant).await;
        let tier = match label {
            Some(l) => tier_for_billing_label(&l),
            // WP-I.1: a transient resolution failure MUST NOT mark the
            // tenant as planned. The next call (after D1 recovers, after
            // the tier-label lands in the next resolver pass, or after
            // the cache TTL expires) will retry the lookup. A real
            // customer was previously pinned to the team-default ladder
            // for the lifetime of the container — silent throttle.
            None => return,
        };
        let (rps, burst) = refill_rate_for_tier(tier);
        // `update_plan` materialises the bucket if absent and snaps available
        // tokens down on a shrink; a freshly-materialised bucket starts at the
        // new (tier) burst. Failure (mutex poison) is logged, not fatal — the
        // request proceeds on whatever bucket exists.
        if let Err(err) =
            self.limiter
                .update_plan(tenant, KeyDimension::PerTenant, "", rps, burst, now_ms)
        {
            tracing::warn!(error = %err, "rate_limit: tier update_plan failed; bucket left on default");
            return;
        }
        // Insert under TTL — the NEXT call within the window skips D1.
        if let Ok(mut g) = self.planned.lock() {
            g.insert(tenant, now_ms.saturating_add(PLANNED_ENTRY_TTL_MS));
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
    // `ortier`: the one D1 round trip in here (`resolve_tier_label`) is timed
    // the same way `opat`/`oquota`/`ostore` already are — wrap the single
    // awaited future, no restructuring of `ensure_tier_applied` itself. It is
    // called directly on this task (no `tokio::spawn`), so the ambient
    // task-local ledger is visible without a captured handle.
    crate::origin_timing::timed(
        crate::origin_timing::Phase::Tier,
        state.ensure_tier_applied(raw_tenant, tenant, now_ms),
    )
    .await;
    let bucket_key = BucketKey::per_tenant(tenant);

    let admission = {
        // `oratelimit`: keep the synchronous token-bucket decision out of
        // `oother` without wrapping the handler future or any named I/O phase.
        let _scope =
            crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::RateLimit);
        state
            .limiter
            .try_acquire(tenant, bucket_key, COST_PER_REQUEST, now_ms)
    };

    match admission {
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
/// The two synthetic scopes ([`OCI_UNAUTH_SCOPES`]) are credential-free and are
/// further partitioned per client IP in [`oci_bucket_key`].
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
///
/// For a credential-gated per-repo scope the key is the repo name verbatim
/// (unchanged: per-repo isolation is exactly the property that scope wants).
/// For an UNAUTHENTICATED scope ([`OCI_UNAUTH_SCOPES`] — `/token` and the `/v2`
/// version-check root) the trusted client IP is folded in, so one abusive source
/// can no longer drain the single global bucket that every `docker login` /
/// `pull` / `push` on the platform has to draw from. `|` and `=` cannot appear
/// in an OCI repo name, so the composed scope string can never alias a repo key.
///
/// # Absent / empty `x-corelink-client-ip`
///
/// `index.ts` writes `""` when `cf-connecting-ip` is missing, and a request that
/// never traversed the Worker carries no header at all. Those requests go into
/// ONE dedicated [`NO_CLIENT_IP_PARTITION`] bucket per unauthenticated scope —
/// deliberately NOT fail-OPEN (that would restore the unbounded flood this fix
/// exists to close) and deliberately NOT fail-CLOSED (a 429 on a missing header
/// would lock out real users on any future forward path that forgets to set it).
///
/// Collapsing them together is safe here specifically because the partition is
/// DISJOINT from every real per-IP bucket: draining `_no_ip` cannot 429 any
/// request that carries a trusted IP, and every request arriving through the
/// sanctioned Worker path carries one (the Worker is the header's sole setter
/// and sets it unconditionally on the OCI arm). An attacker cannot move itself
/// into `_no_ip` by stripping the header, because `stripClientTrustHeaders`
/// deletes the client's copy and the Worker then sets its own from
/// `cf-connecting-ip`; reaching `_no_ip` at all requires bypassing the Worker,
/// at which point the only traffic it can starve is other Worker-bypassing
/// traffic. The bucket map is capped with approximate-LRU eviction upstream, so
/// per-IP keys cannot grow it without bound.
#[must_use]
fn oci_bucket_key(scope: &str, client_ip: &str) -> (Uuid, BucketKey) {
    let realm = Uuid::from_bytes(OCI_NS);
    let scope_key = if OCI_UNAUTH_SCOPES.contains(&scope) {
        let ip = client_ip.trim();
        let partition = if ip.is_empty() {
            NO_CLIENT_IP_PARTITION
        } else {
            ip
        };
        format!("{scope}|ip={partition}")
    } else {
        scope.to_owned()
    };
    (realm, BucketKey::per_tenant_per_endpoint(realm, scope_key))
}

/// Read the server-trusted client IP ([`CLIENT_IP_HEADER`]) off a request.
/// Absent / non-UTF-8 / whitespace-only all collapse to `""`, which
/// [`oci_bucket_key`] maps to the [`NO_CLIENT_IP_PARTITION`] bucket.
fn trusted_client_ip(req: &Request) -> &str {
    req.headers()
        .get(CLIENT_IP_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("")
}

/// Run the per-OCI-repo velocity gate (F-016). Fail-OPEN on the limiter's own
/// internal fault (availability), mirroring the per-tenant path.
async fn run_oci_velocity_gate(
    state: &RateLimitLayerState,
    scope: &str,
    req: Request,
    next: Next,
) -> Response {
    let (realm, bucket_key) = oci_bucket_key(scope, trusted_client_ip(&req));
    let now_ms = state.clock.now_ms();
    let admission = {
        // The OCI credential-free branch has the same bounded admission
        // decision as the tenant branch above; keep both populations closed.
        let _scope =
            crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::RateLimit);
        state
            .oci_limiter
            .try_acquire(realm, bucket_key, COST_PER_REQUEST, now_ms)
    };

    match admission {
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
#[path = "ratelimit_layer_tests.rs"]
mod tests;

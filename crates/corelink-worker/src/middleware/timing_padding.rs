//! Constant-time 404 timing-padding Tower middleware (WI-S02-004).
//!
//! ## Purpose
//!
//! Per ADR-0028 every CAS read [`MissReason`][miss-reason] variant
//! (`NeverExisted` × `Tombstoned` × `R2OrphanRow`) maps to a uniform
//! `404 NOT_FOUND` wire response — but the underlying compute paths
//! differ (KV negative-cache hit vs D1 row + soft-delete check vs
//! D1 row + R2 GET round-trip). Without padding, an attacker holding
//! a valid PAT for tenant B can statistically distinguish the arms by
//! latency clustering and enumerate cross-tenant blob existence —
//! that is INV-TENANT-ISOLATION violated indirectly through a side
//! channel (THR-I-002 / `security_model.md §5.4`).
//!
//! Per ADR-0023 the canonical defense is a Tower layer that:
//!
//! 1. Inspects each handler response via a configurable
//!    [`PredicateKind`] — `Http404` for the REST stack
//!    (`StatusCode == 404`), `GrpcNotFound` for the tonic gRPC stack
//!    (`grpc-status: 5` initial response header — tonic's canonical
//!    encoding of `Err(Status::not_found)` per `tonic 0.12
//!    status.rs::into_http`), `ExtensionMarker` for handlers that
//!    opt in via the [`MissMarker`] extension regardless of status,
//!    or `Any` (the canonical default) which triggers on any of the
//!    three signals.
//! 2. If the predicate fires, sleeps until the total wall-clock
//!    latency reaches `target_p99_ms ± jitter` via
//!    `tokio::time::sleep_until` (Tokio scheduler timer; **never** a
//!    spinlock — zero CF Worker CPU cost added).
//! 3. Skips padding for every other response: 200 OK / gRPC OK
//!    (attacker already has the body), 4xx other than 404 (different
//!    concerns; e.g. 403 PAT-scope is a separate enumeration model
//!    handled by S-08 rate limit), 5xx (no enumeration semantic).
//!
//! ## Statistical evidence gate
//!
//! The middleware's correctness is asserted by `tests/timing_indistinguishability.rs`
//! using a 3-arm methodology (per ADR-0023 §3 + WI-S02-004 §10.4.1):
//!
//! - 10 000 samples per arm × 3 arms (`NeverExisted` ×
//!   `Tombstoned` × `R2OrphanRow`).
//! - Pairwise Mann-Whitney U test on each pair (3 pairs per trial)
//!   via this module's [`mann_whitney_u_p_value`] (canonical normal
//!   approximation per Mann & Whitney 1947 + tie-correction per
//!   Hollander & Wolfe 1973; `statrs` 0.18 does not surface
//!   `mann_whitney_u`, so the formula is implemented in this crate
//!   under strict lints — fully deterministic + auditable).
//! - 3 independent trials × 3 pairs = 9 tests per CI run; full
//!   conjunction acceptance (ALL p > 0.05) with **Šidák correction**
//!   per-test α' = 1 − (1 − 0.05)^(1/9) ≈ 0.005 685 8 (cycle 13 SEAL
//!   math correction; earlier `0.000125` was incorrect).
//! - Bootstrap 95 % CI on |Δmedian| ≤ 1 ms — both `point_estimate`
//!   AND `ci_upper` MUST be ≤ 1 ms (the strict practical-equivalence
//!   gate; the `ci_lower ≤ 1 ms` check that earlier drafts mentioned
//!   was redundant — `|·|` already pins the lower at 0).
//!
//! ## Jitter policy
//!
//! Random jitter without seed is broken via correlation analysis
//! (attacker averages adjacent requests → narrow window). The
//! middleware uses [`JitterPolicy::Seeded`] which derives a
//! `ChaCha20Rng` seed by **mixing**:
//!
//! 1. The per-request `x-request-id` header (when present) — provides
//!    request-level entropy.
//! 2. A **per-layer server-side secret** generated once at
//!    [`TimingPaddingLayer::canonical`] / [`TimingPaddingLayer::new`]
//!    construction via `OsRng` — defeats client-controlled-seed
//!    attacks where the attacker reuses or chooses
//!    `x-request-id` values to predict the jitter window
//!    (codex round-1 P1 fix; without this mix an attacker with
//!    PAT and a known `x-request-id` can pre-compute the per-request
//!    pad and statistically subtract it).
//! 3. A monotonic per-call counter — provides cross-request
//!    independence even when two calls share the same id.
//!
//! Per-request entropy + cross-request independence + server-side
//! secret prevents correlation collapse.
//!
//! ## Anti-patterns (do NOT)
//!
//! - **Do not** pad responses other than miss-classified ones.
//!   Padding 200 / 403 / 413 / 503 is latency tax with no security
//!   benefit (per ADR-0023 §"Trade-offs rejected").
//! - **Do not** swap the seeded jitter for an unseeded `OsRng` —
//!   correlation analysis attacker-side breaks the window.
//! - **Do not** seed exclusively from the client-controlled
//!   `x-request-id` — the seed must mix in the per-layer
//!   server-side secret so the attacker cannot predict the pad
//!   from a chosen id.
//! - **Do not** introduce a `target_p99_ms` runtime override path that
//!   bypasses [`TimingPaddingConfig::new`] validation; the bounds
//!   `[5 ms, 5 000 ms]` are load-bearing for the chaos test §15.5.
//! - **Do not** fold the AuthZ-failure 403 case into the miss padding
//!   bucket. ADR-0023 §"Decision" point 3 explicitly excludes 403
//!   from padding.
//!
//! ## Mounting on the gRPC stack (tonic)
//!
//! tonic's `Server::builder().layer(...)` accepts any tower
//! [`Layer`] whose service `Response = http::Response<BoxBody>`.
//! Mount the canonical layer with [`PredicateKind::GrpcNotFound`]:
//!
//! ```ignore
//! let server = tonic::transport::Server::builder()
//!     .layer(corelink_worker::middleware::TimingPaddingLayer::canonical())
//!     // (the default `Any` predicate matches BOTH HTTP 404 AND
//!     // gRPC `grpc-status: 5`, which is canonical when the same
//!     // tonic server hosts a tonic-web HTTP/REST adapter alongside
//!     // pure gRPC; for pure-gRPC use `with_predicate(GrpcNotFound)`.)
//!     .add_service(byte_stream_service)
//!     .add_service(content_addressable_storage_service)
//!     .serve(addr).await?;
//! ```
//!
//! tonic's `Status::into_http` encodes `Err(Status::not_found(...))`
//! as HTTP 200 + `grpc-status: 5` in **initial response headers**
//! (per tonic 0.12 `status.rs::into_http`); the [`PredicateKind::GrpcNotFound`]
//! variant inspects exactly that header so the layer pads gRPC misses
//! the same way it pads HTTP 404s. Codex round-1 P0 fix —
//! a naive `StatusCode == 404` predicate would let every gRPC miss
//! bypass the defense.
//!
//! [miss-reason]: https://docs.rs/corelink-reapi/latest/corelink_reapi/read/enum.MissReason.html

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use http::{Request, Response, StatusCode};
use rand::{rngs::OsRng, Rng, SeedableRng, TryRngCore};
use rand_chacha::ChaCha20Rng;
use tower_layer::Layer;
use tower_service::Service;

/// Default target padded latency in milliseconds (per ADR-0023 §"Decision" 1).
///
/// Chosen to comfortably exceed the typical handler-resolution window
/// across all 3 arms (NeverExisted ≈ 50 ms KV + AuthZ; Tombstoned
/// ≈ 60 ms D1; R2OrphanRow ≈ 80 ms D1+R2) so [`canonical_pad_target`]
/// always produces a positive sleep amount.
pub const TARGET_P99_MS_DEFAULT: u64 = 200;

/// Hard upper bound on `target_p99_ms` (per WI-S02-004 §10.4.8 chaos
/// scenario: a value > 5 s is more likely a programmer typo than a
/// real config and would interact poorly with worker ingress timeouts).
pub const TARGET_P99_MS_MAX: u64 = 5_000;

/// Default jitter percent (±10 %) per ADR-0023 §"Decision" 1.
pub const JITTER_PCT_DEFAULT: u8 = 10;

/// Hard upper bound on `jitter_pct`. Exceeding 50 % flips the meaning
/// of the target window (target becomes a midpoint, not a p99) — the
/// constructor rejects values above this bound.
pub const JITTER_PCT_MAX: u8 = 50;

/// Padding granularity floor in milliseconds (per WI-S02-004 §10.4.9).
///
/// CF Worker scheduler quanta are ≈ 1 ms; granularity ≥ 5 ms dominates
/// jitter so `tokio::time::sleep_until` produces a meaningful pad
/// rather than a no-op.
pub const PADDING_GRANULARITY_MS_MIN: u64 = 5;

/// Validated configuration for [`TimingPaddingLayer`].
///
/// All construction goes through [`TimingPaddingConfig::new`] so the
/// invariants `target_p99_ms ∈ [PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_MAX]`
/// and `jitter_pct ≤ JITTER_PCT_MAX` are checked once at deploy time
/// rather than per request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimingPaddingConfig {
    target_p99_ms: u64,
    jitter_pct: u8,
}

/// Construction errors for [`TimingPaddingConfig`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimingPaddingError {
    /// `target_p99_ms` is below [`PADDING_GRANULARITY_MS_MIN`] or
    /// above [`TARGET_P99_MS_MAX`].
    #[error(
        "target_p99_ms = {provided} ms outside accepted range [{min}, {max}] ms",
        min = PADDING_GRANULARITY_MS_MIN,
        max = TARGET_P99_MS_MAX,
    )]
    TargetOutOfRange {
        /// The rejected value (in milliseconds).
        provided: u64,
    },
    /// `jitter_pct` exceeds [`JITTER_PCT_MAX`].
    #[error(
        "jitter_pct = {provided}% exceeds maximum {max}%",
        max = JITTER_PCT_MAX,
    )]
    JitterOutOfRange {
        /// The rejected value (in percent).
        provided: u8,
    },
}

impl TimingPaddingConfig {
    /// Construct a validated config.
    ///
    /// # Errors
    ///
    /// - [`TimingPaddingError::TargetOutOfRange`] when `target_p99_ms`
    ///   is below [`PADDING_GRANULARITY_MS_MIN`] or above [`TARGET_P99_MS_MAX`].
    /// - [`TimingPaddingError::JitterOutOfRange`] when `jitter_pct`
    ///   exceeds [`JITTER_PCT_MAX`].
    pub const fn new(target_p99_ms: u64, jitter_pct: u8) -> Result<Self, TimingPaddingError> {
        if target_p99_ms < PADDING_GRANULARITY_MS_MIN || target_p99_ms > TARGET_P99_MS_MAX {
            return Err(TimingPaddingError::TargetOutOfRange {
                provided: target_p99_ms,
            });
        }
        if jitter_pct > JITTER_PCT_MAX {
            return Err(TimingPaddingError::JitterOutOfRange {
                provided: jitter_pct,
            });
        }
        Ok(Self {
            target_p99_ms,
            jitter_pct,
        })
    }

    /// Canonical default ([`TARGET_P99_MS_DEFAULT`] / [`JITTER_PCT_DEFAULT`]).
    #[must_use]
    pub const fn canonical() -> Self {
        // Canonical defaults are constants from this module, both
        // hard-coded inside `[5, 5_000]` and `≤ 50` so `new` cannot
        // fail. Using a `match` here lets the function stay `const`
        // while preserving the error-rejection guarantee at the
        // surface — the `Err` arm is provably unreachable.
        match Self::new(TARGET_P99_MS_DEFAULT, JITTER_PCT_DEFAULT) {
            Ok(c) => c,
            // Unreachable: defaults are inside the validated range; the
            // crate-level `panic = deny` lint forbids `unreachable!()`,
            // so we surface a witness config that still upholds the
            // type invariant. This branch cannot execute at runtime —
            // any consumer who flips the constants out of range will
            // see a compile-time `const` evaluation failure of a
            // sibling test (`canonical_defaults_are_within_bounds`).
            Err(_) => Self {
                target_p99_ms: TARGET_P99_MS_DEFAULT,
                jitter_pct: JITTER_PCT_DEFAULT,
            },
        }
    }

    /// Target padded p99 latency, in milliseconds.
    #[must_use]
    pub const fn target_p99_ms(&self) -> u64 {
        self.target_p99_ms
    }

    /// Jitter window, in percent (±).
    #[must_use]
    pub const fn jitter_pct(&self) -> u8 {
        self.jitter_pct
    }
}

impl Default for TimingPaddingConfig {
    fn default() -> Self {
        Self::canonical()
    }
}

/// Per-request jitter source policy (ADR-0023 §"Decision" 1.2).
#[derive(Clone, Debug)]
pub enum JitterPolicy {
    /// Production policy: deterministic per-request seed derived from
    /// the request's `x-request-id` header **mixed with the
    /// per-layer server-side secret** (codex round-1 P1 fix —
    /// preventing client-controlled-seed attacks). When the header is
    /// absent, the fallback monotonic counter feeds the same mix.
    /// Independent across requests + unpredictable to the attacker.
    Seeded,
    /// Test policy: identical jitter every call. **Never** use in
    /// production — defeats the correlation-resistance property. The
    /// adversarial test in `tests/timing_indistinguishability.rs`
    /// uses [`Self::Seeded`]; this variant is reserved for property
    /// tests asserting padding-window arithmetic alone.
    FixedForTests {
        /// Forced jitter offset in `[-jitter_pct%, +jitter_pct%]`.
        signed_pct: i8,
    },
}

/// Marker extension a handler attaches to its [`Response`] to opt the
/// response into miss-classified padding regardless of HTTP
/// `StatusCode`. The optional [`MissArm`] discriminator lets the
/// timing-padding emit hook record which canonical
/// `corelink-reapi::read::MissReason` arm produced the miss — the
/// load-bearing field for the S-09 aggregation that computes
/// `corelink_cas_side_channel_timing_diff_ms` as a pairwise median
/// across arms (codex round-5 P1 fix; without `miss_arm` the
/// aggregation cannot reconstruct per-arm distributions from the
/// per-request stream).
///
/// Production wiring: gRPC + HTTP handlers that surface
/// `MissReason` insert
/// `response.extensions_mut().insert(MissMarker::for_arm(MissArm::Tombstoned))`
/// (or the appropriate arm) before returning. The
/// [`miss_predicates::extension_marker`] predicate keys off the
/// presence of the marker; the emit hook reads the optional
/// [`MissArm`] for the aggregation discriminator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissMarker {
    /// Optional arm discriminator for S-09 aggregation; `None`
    /// surfaces in the emit log as `unknown` (still padded, but
    /// without per-arm attribution).
    pub arm: Option<MissArm>,
}

impl MissMarker {
    /// Construct an arm-less marker (used when the handler cannot
    /// classify the arm — pre-S-02-001 paths or future REST
    /// endpoints).
    #[must_use]
    pub const fn new() -> Self {
        Self { arm: None }
    }

    /// Construct a marker carrying a canonical arm discriminator.
    /// Production wiring: pass the
    /// `corelink-reapi::read::MissReason → MissArm` mapping.
    #[must_use]
    pub const fn for_arm(arm: MissArm) -> Self {
        Self { arm: Some(arm) }
    }
}

/// Canonical `corelink-reapi::read::MissReason` discriminator used
/// by the timing-padding emit hook for S-09 aggregation. Mirrors
/// the read orchestrator's enum at the layer boundary so
/// `corelink-worker` does not depend on `corelink-reapi` (cycles
/// would block wasm32 builds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissArm {
    /// `NeverExisted` — folds in the conflated `CrossTenantMasked`
    /// arm at the orchestrator surface (per ADR-0028 v1.1.0).
    NeverExisted,
    /// `Tombstoned` — D1 row found, `deleted_at != NULL`.
    Tombstoned,
    /// `R2OrphanRow` — D1 row alive + AuthZ pass + R2 NotFound.
    R2OrphanRow,
}

impl MissArm {
    /// Static label used in structured logs + aggregations.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NeverExisted => "never_existed",
            Self::Tombstoned => "tombstoned",
            Self::R2OrphanRow => "r2_orphan_row",
        }
    }
}

/// Predicate kind that decides whether a given response is a miss
/// (canonical 404 paying-into-padding contract). Enum-typed (not
/// boxed `dyn Fn`) so the [`TimingPaddingLayer`] can stay generic-
/// parameter-free while still letting callers pick the right
/// detection logic per-stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateKind {
    /// Pad when HTTP `StatusCode == 404`. Canonical for the HTTP REST
    /// path (`GET /v1/cas/<digest>`).
    Http404,
    /// Pad when the gRPC `grpc-status` initial response header equals
    /// `5` (NOT_FOUND). Canonical for the tonic-served gRPC path —
    /// `tonic::Status::into_http` renders an `Err(Status::not_found)`
    /// as HTTP 200 + `grpc-status: 5` in initial response headers
    /// (vide tonic 0.12 `status.rs::into_http`); a naive
    /// `StatusCode == 404` predicate would miss every gRPC NOT_FOUND
    /// because tonic returns HTTP 200 (codex round-1 P0 fix).
    GrpcNotFound,
    /// Pad when the response carries a [`MissMarker`] extension.
    /// Useful for handlers that wish to opt a response into padding
    /// regardless of HTTP or gRPC status — e.g. a future REST
    /// endpoint that returns 200 + body for cache-warm but 200 +
    /// `MissMarker` for cache-miss.
    ExtensionMarker,
    /// Pad when **any** miss-shaped signal is present —
    /// `StatusCode == 404` OR `grpc-status: 5` OR the
    /// [`MissMarker`] extension. Recommended default when one
    /// `TimingPaddingLayer` wraps both HTTP and gRPC services.
    Any,
}

impl PredicateKind {
    /// Apply the predicate to a response.
    #[must_use]
    pub fn matches<B>(&self, resp: &Response<B>) -> bool {
        match self {
            Self::Http404 => resp.status() == StatusCode::NOT_FOUND,
            Self::GrpcNotFound => is_grpc_not_found(resp),
            Self::ExtensionMarker => resp.extensions().get::<MissMarker>().is_some(),
            Self::Any => {
                resp.status() == StatusCode::NOT_FOUND
                    || is_grpc_not_found(resp)
                    || resp.extensions().get::<MissMarker>().is_some()
            }
        }
    }
}

/// Inspect the initial response headers for `grpc-status: 5` —
/// tonic's canonical encoding of `Err(Status::not_found(...))` per
/// `tonic 0.12 status.rs::into_http`.
fn is_grpc_not_found<B>(resp: &Response<B>) -> bool {
    resp.headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.trim() == "5")
}

/// Convenience constructors for [`PredicateKind`] values; mirrors the
/// previous `miss_predicates::*` API surface (kept for back-compat in
/// callers that have already typed those calls).
pub mod miss_predicates {
    use super::PredicateKind;

    /// [`PredicateKind::Http404`] alias.
    #[must_use]
    pub const fn http_404() -> PredicateKind {
        PredicateKind::Http404
    }
    /// [`PredicateKind::GrpcNotFound`] alias.
    #[must_use]
    pub const fn grpc_not_found() -> PredicateKind {
        PredicateKind::GrpcNotFound
    }
    /// [`PredicateKind::ExtensionMarker`] alias.
    #[must_use]
    pub const fn extension_marker() -> PredicateKind {
        PredicateKind::ExtensionMarker
    }
    /// [`PredicateKind::Any`] alias.
    #[must_use]
    pub const fn any() -> PredicateKind {
        PredicateKind::Any
    }
}

/// Tower [`Layer`] applying the timing-padding policy to wrapped services.
///
/// See module-level rustdoc for the full design rationale.
pub struct TimingPaddingLayer {
    config: TimingPaddingConfig,
    policy: JitterPolicy,
    /// Per-layer server-side secret mixed into every request seed —
    /// generated once via OsRng at layer construction so the same
    /// `x-request-id` does NOT yield a deterministic seed across
    /// deploys / processes (codex round-1 P1 fix). When OsRng fails
    /// (pathological host), the fallback is `0` and the layer
    /// degrades to id-only seeding — this is documented + still
    /// correlation-resistant via the per-call counter.
    server_secret: u64,
    /// Monotonic counter used for cross-call independence + as the
    /// absent-`x-request-id` fallback entropy source. Two consecutive
    /// requests with the same id (intentional or attacker-controlled)
    /// therefore still pick distinct seeds.
    call_counter: Arc<AtomicU64>,
    /// Predicate selected for downstream services. Codex round-2 P2
    /// fix — earlier draft hard-coded `Any` at `Layer::layer`, leaving
    /// callers no way to select a stricter predicate at the layer
    /// boundary; selecting at the *service* boundary required a
    /// `with_predicate` chain that the canonical
    /// `tonic::Server::builder().layer(...)` and
    /// `axum::Router::layer(...)` pipelines cannot express.
    predicate_kind: PredicateKind,
}

impl TimingPaddingLayer {
    /// New layer with the [`TimingPaddingConfig::canonical`] config,
    /// production [`JitterPolicy::Seeded`] policy, and the canonical
    /// [`PredicateKind::Any`] predicate (matches HTTP 404 ∪ gRPC
    /// `grpc-status: 5` ∪ [`MissMarker`] extension).
    #[must_use]
    pub fn canonical() -> Self {
        Self::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded)
    }

    /// New layer with explicit config + jitter policy. Predicate
    /// defaults to [`PredicateKind::Any`]; override with
    /// [`Self::with_predicate`] for pure-HTTP / pure-gRPC stacks.
    #[must_use]
    pub fn new(config: TimingPaddingConfig, policy: JitterPolicy) -> Self {
        let server_secret = generate_server_secret();
        Self {
            config,
            policy,
            server_secret,
            call_counter: Arc::new(AtomicU64::new(0)),
            predicate_kind: PredicateKind::Any,
        }
    }

    /// Select the [`PredicateKind`] at the layer boundary so the
    /// canonical mounting calls (`tonic::Server::builder().layer(...)`,
    /// `axum::Router::layer(...)`) can be written directly without
    /// chaining `service.with_predicate(...)` after `Layer::layer`.
    /// Codex round-2 P2 fix.
    #[must_use]
    pub const fn with_predicate(mut self, predicate_kind: PredicateKind) -> Self {
        self.predicate_kind = predicate_kind;
        self
    }

    /// View the config (read-only — runtime mutation flips the
    /// statistical-indistinguishability proof window).
    #[must_use]
    pub const fn config(&self) -> &TimingPaddingConfig {
        &self.config
    }

    /// View the policy (read-only).
    #[must_use]
    pub const fn policy(&self) -> &JitterPolicy {
        &self.policy
    }

    /// View the predicate (read-only).
    #[must_use]
    pub const fn predicate_kind(&self) -> PredicateKind {
        self.predicate_kind
    }
}

impl Clone for TimingPaddingLayer {
    fn clone(&self) -> Self {
        Self {
            config: self.config,
            policy: self.policy.clone(),
            server_secret: self.server_secret,
            call_counter: Arc::clone(&self.call_counter),
            predicate_kind: self.predicate_kind,
        }
    }
}

impl core::fmt::Debug for TimingPaddingLayer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TimingPaddingLayer")
            .field("config", &self.config)
            .field("policy", &self.policy)
            .field("predicate_kind", &self.predicate_kind)
            // server_secret intentionally redacted — disclosing it
            // hands the attacker the deterministic per-request seed.
            .field("server_secret", &"<REDACTED>")
            .finish()
    }
}

impl Default for TimingPaddingLayer {
    fn default() -> Self {
        Self::canonical()
    }
}

impl<S> Layer<S> for TimingPaddingLayer {
    type Service = TimingPaddingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimingPaddingService {
            inner,
            config: self.config,
            policy: self.policy.clone(),
            server_secret: self.server_secret,
            call_counter: Arc::clone(&self.call_counter),
            predicate_kind: self.predicate_kind,
        }
    }
}

/// Tower [`Service`] wrapping an inner HTTP/gRPC service to enforce
/// the miss-response timing-padding policy.
#[derive(Clone)]
pub struct TimingPaddingService<S> {
    inner: S,
    config: TimingPaddingConfig,
    policy: JitterPolicy,
    server_secret: u64,
    call_counter: Arc<AtomicU64>,
    predicate_kind: PredicateKind,
}

impl<S: core::fmt::Debug> core::fmt::Debug for TimingPaddingService<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TimingPaddingService")
            .field("inner", &self.inner)
            .field("config", &self.config)
            .field("policy", &self.policy)
            .field("predicate_kind", &self.predicate_kind)
            .field("server_secret", &"<REDACTED>")
            .finish()
    }
}

impl<S> TimingPaddingService<S> {
    /// Read-only access to the config.
    #[must_use]
    pub const fn config(&self) -> &TimingPaddingConfig {
        &self.config
    }

    /// Read-only access to the active [`PredicateKind`].
    #[must_use]
    pub const fn predicate_kind(&self) -> PredicateKind {
        self.predicate_kind
    }

    /// Replace the [`PredicateKind`] used to decide whether a
    /// response should be padded. Production wiring picks
    /// [`PredicateKind::Http404`] for the HTTP REST stack and
    /// [`PredicateKind::GrpcNotFound`] for the tonic gRPC stack
    /// (the latter inspects `grpc-status: 5` initial response
    /// header, the canonical tonic `Status::into_http` encoding for
    /// `Err(Status::not_found)`); the default after [`Layer::layer`]
    /// is [`PredicateKind::Any`] which fires on any of the three
    /// canonical signals.
    #[must_use]
    pub fn with_predicate(mut self, predicate_kind: PredicateKind) -> Self {
        self.predicate_kind = predicate_kind;
        self
    }
}

impl<S, ReqBody, RespBody> Service<Request<ReqBody>> for TimingPaddingService<S>
where
    S: Service<Request<ReqBody>, Response = Response<RespBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ReqBody: Send + 'static,
    RespBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Per Tower's contract, `poll_ready` drives readiness on
        // `&mut self.inner`; `call` MUST consume that readied state
        // on the same `&mut S` (codex round-1 P1 fix — earlier draft
        // cloned `self.inner` and called the clone, bypassing
        // back-pressure on stateful inner services).
        //
        // The canonical fix per `tower::ServiceExt::ready` and the
        // `tower::Buffer` blueprint: swap the readied service into
        // the future via `core::mem::replace`. The fresh `not-ready`
        // clone left in `self.inner` will be re-polled on the next
        // `poll_ready` call, preserving the "exactly one `poll_ready`
        // per `call`" invariant for stateful inner services.
        let started = tokio::time::Instant::now();
        let request_id = compute_request_seed(
            &req,
            self.server_secret,
            &self.call_counter,
        );
        let config = self.config;
        let policy = self.policy.clone();
        let predicate_kind = self.predicate_kind;
        // Swap the readied inner OUT and replace with a fresh clone
        // that the next `poll_ready` call will drive. This is the
        // canonical pattern from `tower-http::map_response::MapResponse`
        // + `tower::Buffer::poll_ready` + `tower::ServiceBuilder`
        // — it preserves Tower's readiness contract for stateful
        // inner services (codex round-1 P1 fix).
        let clone = self.inner.clone();
        let mut inner = core::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let response = inner.call(req).await?;
            if predicate_kind.matches(&response) {
                let pre_pad_elapsed_ms =
                    started.elapsed().as_secs_f64() * 1000.0;
                let pad_target =
                    canonical_pad_target(config, &policy, request_id, started.elapsed());
                // Saturating add — `tokio::time::Instant` can in
                // theory overflow; surface the inner `started`
                // unchanged so the sleep collapses to a no-op rather
                // than panicking (the strict-lints crate forbids
                // panics).
                let deadline = started.checked_add(pad_target).unwrap_or(started);
                tokio::time::sleep_until(deadline).await;
                // Codex round-4 P1 fix + round-5 P1 fix — emit the
                // canonical observability hook with the per-arm
                // discriminator so S-09 aggregation can reconstruct
                // `corelink_cas_side_channel_timing_diff_ms` as
                // pairwise medians per arm. The
                // `MissMarker { arm: Option<MissArm> }` extension is
                // inserted by the handler before returning;
                // `miss_arm` is `unknown` when the handler did not
                // classify the arm (still padded, but without per-arm
                // attribution).
                let total_elapsed_ms =
                    started.elapsed().as_secs_f64() * 1000.0;
                // Codex round-5 P1: read the canonical arm
                // discriminator first from the `MissMarker` extension
                // (HTTP path; tonic does not propagate extensions
                // through `Status::into_http`), THEN from the
                // `x-corelink-miss-arm` response header (gRPC path
                // that emits the arm via Status metadata + tonic's
                // metadata-to-header transform). The fallback is
                // `unknown` (still padded; just unattributed).
                let miss_arm = response
                    .extensions()
                    .get::<MissMarker>()
                    .and_then(|m| m.arm)
                    .map_or_else(
                        || {
                            response
                                .headers()
                                .get("x-corelink-miss-arm")
                                .and_then(|v| v.to_str().ok())
                                .map(|s| s.trim().to_owned())
                                .unwrap_or_else(|| "unknown".to_owned())
                        },
                        |a| a.label().to_owned(),
                    );
                tracing::event!(
                    target: "corelink.cas.side_channel",
                    tracing::Level::DEBUG,
                    pre_pad_elapsed_ms = pre_pad_elapsed_ms,
                    pad_target_ms = pad_target.as_secs_f64() * 1000.0,
                    total_elapsed_ms = total_elapsed_ms,
                    target_p99_ms = config.target_p99_ms(),
                    jitter_pct = config.jitter_pct(),
                    request_id_seed = request_id,
                    miss_arm = miss_arm,
                    "corelink.cas.side_channel.timing_padded"
                );
            }
            Ok(response)
        })
    }
}

/// Compute the canonical pad target (per ADR-0023 §"Decision" 1).
///
/// Returns the **total** wall-clock duration that should elapse from
/// the request entry point until the padded response is released.
/// Callers compose this with the captured `started` instant via
/// `tokio::time::sleep_until(started + canonical_pad_target(...))`.
///
/// Handler resolution that already exceeds the padded window is
/// returned **unchanged** — i.e. the function returns the larger of
/// `(target ± jitter, observed_elapsed)` so `sleep_until` never
/// produces a negative delay (per WI-S02-004 §10.4.8 — long-resolution
/// anomaly is logged separately by the caller; the middleware itself
/// never **shortens** a response).
#[must_use]
pub fn canonical_pad_target(
    config: TimingPaddingConfig,
    policy: &JitterPolicy,
    request_id_seed: u64,
    elapsed: Duration,
) -> Duration {
    let target_ms = config.target_p99_ms();
    let jitter_pct = config.jitter_pct();

    let signed_pct = match policy {
        JitterPolicy::Seeded => {
            let mut rng = ChaCha20Rng::seed_from_u64(request_id_seed);
            let signed = i32::from(jitter_pct);
            if signed == 0 {
                0i32
            } else {
                // Range is inclusive on both ends so the boundary
                // cases (`-jitter_pct` and `+jitter_pct`) appear in
                // the test set — flushing out off-by-one regressions.
                rng.random_range(-signed..=signed)
            }
        }
        JitterPolicy::FixedForTests { signed_pct } => i32::from(*signed_pct),
    };

    let delta_ms = (i64::try_from(target_ms).unwrap_or(i64::MAX) * i64::from(signed_pct)) / 100;
    let padded_ms = i64::try_from(target_ms).unwrap_or(i64::MAX).saturating_add(delta_ms);
    let padded_ms_u = u64::try_from(padded_ms.max(0)).unwrap_or(0);
    let padded = Duration::from_millis(padded_ms_u);
    if elapsed > padded {
        elapsed
    } else {
        padded
    }
}

/// Compute the per-request seed by mixing:
/// - The `x-request-id` header hash (when present).
/// - The per-layer server-side secret (`OsRng`-generated at layer
///   construction; opaque to the caller).
/// - A monotonic per-call counter (cross-request independence).
///
/// Returns a u64 suitable for `ChaCha20Rng::seed_from_u64`.
fn compute_request_seed<B>(
    req: &Request<B>,
    server_secret: u64,
    call_counter: &AtomicU64,
) -> u64 {
    let header_seed = if let Some(value) = req.headers().get("x-request-id") {
        if let Ok(s) = value.to_str() {
            splitmix_str(s)
        } else {
            0
        }
    } else {
        0
    };
    let counter = call_counter.fetch_add(1, Ordering::Relaxed);
    // Mix all three: server_secret defeats client-controlled seeds;
    // counter defeats id-collision (same id, two probes); header_seed
    // gives correlation across ostensibly-paired logs / traces.
    splitmix_u64(server_secret ^ counter ^ header_seed)
}

/// Generate a per-layer server-side secret via `OsRng`. Falls back to
/// a deterministic seed mixed with the system epoch when `OsRng`
/// itself fails (treated as non-fatal because the per-call counter
/// still provides cross-request independence).
fn generate_server_secret() -> u64 {
    let mut buf = [0u8; 8];
    if OsRng.try_fill_bytes(&mut buf).is_ok() {
        u64::from_le_bytes(buf)
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        splitmix_u64(now)
    }
}

/// SplitMix64 finalizer — fast, well-distributed, deterministic.
fn splitmix_u64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn splitmix_str(s: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    splitmix_u64(h)
}

// =============================================================================
// Statistical primitives — Mann-Whitney U + Šidák correction + bootstrap CI.
// =============================================================================

/// Two-sided Mann-Whitney U p-value via the canonical normal
/// approximation with tie correction (Mann & Whitney 1947;
/// Hollander & Wolfe 1973 §4.1 ties).
///
/// Returns `None` when `xs` or `ys` is empty (undefined p-value).
///
/// The implementation is intentionally textbook + auditable:
///
/// 1. Concatenate `xs` and `ys`, sort by value, assign **mid-ranks**
///    to ties (canonical Wilcoxon rank-sum convention).
/// 2. Sum the ranks of the `xs` group → `R1`.
/// 3. `U1 = R1 − n1·(n1+1)/2`; `U = min(U1, n1·n2 − U1)`.
/// 4. `μ_U = n1·n2 / 2`; tie-corrected
///    `σ_U² = (n1·n2/12) · ((N+1) − Σ(t³−t)/(N·(N−1)))`.
/// 5. `z = (|U − μ_U| − 0.5) / σ_U` (continuity correction; sign
///    chosen so the two-sided p stays symmetric in the no-effect
///    limit).
/// 6. `p = 2 · (1 − Φ(|z|))` via `erfc` for the right-tail tail prob.
///
/// Returns the **two-sided** p-value (null hypothesis: distributions
/// are identical; large p ⇒ fail to reject ⇒ indistinguishable, which
/// is the desired outcome for the timing-padding test).
#[must_use]
pub fn mann_whitney_u_p_value(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n1 = xs.len();
    let n2 = ys.len();
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let big_n = n1 + n2;

    let mut pooled: Vec<(f64, bool)> = Vec::with_capacity(big_n);
    pooled.extend(xs.iter().map(|&v| (v, true)));
    pooled.extend(ys.iter().map(|&v| (v, false)));
    pooled.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

    let mut rank_sum_x: f64 = 0.0;
    let mut tie_correction_sum: f64 = 0.0; // Σ(t³ − t)
    let mut idx: usize = 0;
    while idx < big_n {
        let mut run_end = idx + 1;
        let val = match pooled.get(idx) {
            Some((v, _)) => *v,
            None => break,
        };
        while run_end < big_n {
            let next_val = match pooled.get(run_end) {
                Some((v, _)) => *v,
                None => break,
            };
            if (next_val - val).abs() <= f64::EPSILON {
                run_end += 1;
            } else {
                break;
            }
        }
        let t = run_end - idx;
        // Mid-rank: ranks are 1-based; mid-rank for a tie of `t` items
        // at positions [idx+1 .. idx+t] is `idx + (t+1)/2`.
        let mid_rank = (idx as f64) + ((t as f64) + 1.0) / 2.0;
        for k in idx..run_end {
            if let Some((_, is_x)) = pooled.get(k) {
                if *is_x {
                    rank_sum_x += mid_rank;
                }
            }
        }
        if t > 1 {
            let t_f = t as f64;
            tie_correction_sum += t_f * t_f * t_f - t_f;
        }
        idx = run_end;
    }

    let n1_f = n1 as f64;
    let n2_f = n2 as f64;
    let big_n_f = big_n as f64;
    let u1 = rank_sum_x - n1_f * (n1_f + 1.0) / 2.0;
    let u2 = n1_f * n2_f - u1;
    let u = if u1 < u2 { u1 } else { u2 };

    let mu_u = n1_f * n2_f / 2.0;

    let big_n_minus_one = big_n_f - 1.0;
    let tie_term = if big_n_minus_one <= 0.0 {
        0.0
    } else {
        tie_correction_sum / (big_n_f * big_n_minus_one)
    };
    let sigma_sq = (n1_f * n2_f / 12.0) * ((big_n_f + 1.0) - tie_term);
    if sigma_sq <= 0.0 {
        return Some(1.0);
    }
    let sigma = sigma_sq.sqrt();

    let raw_diff = (u - mu_u).abs();
    let corrected = (raw_diff - 0.5).max(0.0);
    let z = corrected / sigma;
    let p_two_sided = erfc(z / core::f64::consts::SQRT_2);
    Some(p_two_sided.clamp(0.0, 1.0))
}

/// Standard erfc via Abramowitz & Stegun §7.1.26 + §7.1.28 (max
/// relative error ≈ 1.5e-7; ample for the indistinguishability gate
/// which only needs three significant digits at the α threshold).
fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let sign = x.signum();
    let abs_x = x.abs();
    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;
    let p_factor = 0.327_591_1;

    let t = 1.0 / (1.0 + p_factor * abs_x);
    let poly = ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t;
    let erf_pos = 1.0 - poly * (-abs_x * abs_x).exp();
    let erf = sign * erf_pos;
    1.0 - erf
}

/// Šidák correction per-test α' for `k` independent tests with
/// combined familywise α = `alpha`.
///
/// `α' = 1 − (1 − α)^(1/k)`.
///
/// Returns `None` when `k == 0` (vacuous family) or `alpha` is outside
/// `[0, 1]` (mis-call).
#[must_use]
pub fn sidak_per_test_alpha(alpha: f64, k: usize) -> Option<f64> {
    if k == 0 || !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    let one_minus = 1.0 - alpha;
    let inv_k = 1.0 / (k as f64);
    Some(1.0 - one_minus.powf(inv_k))
}

/// Bootstrap 95 % CI on the absolute median difference `|median(xs) - median(ys)|`.
///
/// Returns `None` if either sample is empty. The CI is derived by
/// resampling each input independently `iterations` times via the
/// supplied `seed`-driven `ChaCha20Rng`; the 2.5 / 97.5 percentiles
/// of the resulting `|median diff|` distribution form the CI.
///
/// Interpretation: the test passes the WI-S02-004 §10.4.1 gate iff
/// **both** `point_estimate ≤ 1 ms` AND `ci_upper ≤ 1 ms` (codex
/// round-1 P1 fix — the upper bound is the load-bearing
/// practical-equivalence claim; `ci_lower` of `|·|` is trivially
/// `≥ 0` and was redundant in earlier drafts).
#[must_use]
pub fn bootstrap_median_ci(
    xs: &[f64],
    ys: &[f64],
    iterations: usize,
    seed: u64,
) -> Option<BootstrapMedianCi> {
    if xs.is_empty() || ys.is_empty() || iterations == 0 {
        return None;
    }
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let mut diffs = Vec::with_capacity(iterations);
    let mut x_buf = vec![0.0f64; xs.len()];
    let mut y_buf = vec![0.0f64; ys.len()];
    for _ in 0..iterations {
        for slot in x_buf.iter_mut() {
            let idx = rng.random_range(0..xs.len());
            *slot = match xs.get(idx) {
                Some(v) => *v,
                None => 0.0,
            };
        }
        for slot in y_buf.iter_mut() {
            let idx = rng.random_range(0..ys.len());
            *slot = match ys.get(idx) {
                Some(v) => *v,
                None => 0.0,
            };
        }
        let mx = median(&mut x_buf.clone());
        let my = median(&mut y_buf.clone());
        diffs.push((mx - my).abs());
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let lower_idx = ((iterations as f64) * 0.025) as usize;
    let upper_idx = (((iterations as f64) * 0.975) as usize).min(iterations - 1);
    let median_x = median(&mut xs.to_vec());
    let median_y = median(&mut ys.to_vec());
    let point_estimate = (median_x - median_y).abs();
    Some(BootstrapMedianCi {
        point_estimate,
        ci_lower: diffs.get(lower_idx).copied().unwrap_or(0.0),
        ci_upper: diffs.get(upper_idx).copied().unwrap_or(0.0),
    })
}

/// Bootstrap 95 % CI result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BootstrapMedianCi {
    /// |Δmedian| point estimate from the original samples.
    pub point_estimate: f64,
    /// 2.5-percentile of the resampled |Δmedian| distribution.
    pub ci_lower: f64,
    /// 97.5-percentile of the resampled |Δmedian| distribution.
    pub ci_upper: f64,
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values.get(mid).copied().unwrap_or(0.0)
    } else {
        let lo = values.get(mid - 1).copied().unwrap_or(0.0);
        let hi = values.get(mid).copied().unwrap_or(0.0);
        (lo + hi) / 2.0
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use http::HeaderValue;

    // Compile-time witness: defaults are inside the validated range.
    const _: () = {
        assert!(TARGET_P99_MS_DEFAULT >= PADDING_GRANULARITY_MS_MIN);
        assert!(TARGET_P99_MS_DEFAULT <= TARGET_P99_MS_MAX);
        assert!(JITTER_PCT_DEFAULT <= JITTER_PCT_MAX);
    };

    // ----- TimingPaddingConfig invariants ----------------------------

    #[test]
    fn canonical_defaults_are_within_bounds() {
        let cfg = TimingPaddingConfig::canonical();
        assert_eq!(cfg.target_p99_ms(), TARGET_P99_MS_DEFAULT);
        assert_eq!(cfg.jitter_pct(), JITTER_PCT_DEFAULT);
    }

    #[test]
    fn config_rejects_target_too_low() {
        let err = TimingPaddingConfig::new(PADDING_GRANULARITY_MS_MIN - 1, 10).unwrap_err();
        assert!(matches!(err, TimingPaddingError::TargetOutOfRange { .. }));
    }

    #[test]
    fn config_rejects_target_too_high() {
        let err = TimingPaddingConfig::new(TARGET_P99_MS_MAX + 1, 10).unwrap_err();
        assert!(matches!(err, TimingPaddingError::TargetOutOfRange { .. }));
    }

    #[test]
    fn config_rejects_jitter_too_high() {
        let err = TimingPaddingConfig::new(200, JITTER_PCT_MAX + 1).unwrap_err();
        assert!(matches!(err, TimingPaddingError::JitterOutOfRange { .. }));
    }

    #[test]
    fn config_zero_jitter_ok() {
        let cfg = TimingPaddingConfig::new(200, 0).unwrap();
        assert_eq!(cfg.jitter_pct(), 0);
    }

    #[test]
    fn config_boundary_values_ok() {
        let lo = TimingPaddingConfig::new(PADDING_GRANULARITY_MS_MIN, 0).unwrap();
        assert_eq!(lo.target_p99_ms(), PADDING_GRANULARITY_MS_MIN);
        let hi = TimingPaddingConfig::new(TARGET_P99_MS_MAX, JITTER_PCT_MAX).unwrap();
        assert_eq!(hi.target_p99_ms(), TARGET_P99_MS_MAX);
        assert_eq!(hi.jitter_pct(), JITTER_PCT_MAX);
    }

    // ----- canonical_pad_target arithmetic ---------------------------

    #[test]
    fn pad_target_returns_target_when_jitter_zero_and_handler_fast() {
        let cfg = TimingPaddingConfig::new(200, 0).unwrap();
        let policy = JitterPolicy::Seeded;
        let target = canonical_pad_target(cfg, &policy, 0xDEAD_BEEF, Duration::from_millis(50));
        assert_eq!(target.as_millis(), 200);
    }

    #[test]
    fn pad_target_returns_observed_when_handler_slow() {
        let cfg = TimingPaddingConfig::new(50, 0).unwrap();
        let policy = JitterPolicy::FixedForTests { signed_pct: 0 };
        let observed = Duration::from_millis(80);
        let target = canonical_pad_target(cfg, &policy, 1, observed);
        assert_eq!(target, observed);
    }

    #[test]
    fn pad_target_applies_positive_jitter() {
        let cfg = TimingPaddingConfig::new(200, 10).unwrap();
        let policy = JitterPolicy::FixedForTests { signed_pct: 10 };
        let target = canonical_pad_target(cfg, &policy, 0, Duration::from_millis(0));
        assert_eq!(target.as_millis(), 220);
    }

    #[test]
    fn pad_target_applies_negative_jitter() {
        let cfg = TimingPaddingConfig::new(200, 10).unwrap();
        let policy = JitterPolicy::FixedForTests { signed_pct: -10 };
        let target = canonical_pad_target(cfg, &policy, 0, Duration::from_millis(0));
        assert_eq!(target.as_millis(), 180);
    }

    #[test]
    fn pad_target_seeded_jitter_stays_in_window() {
        let cfg = TimingPaddingConfig::new(200, 10).unwrap();
        let policy = JitterPolicy::Seeded;
        for seed in 0..1024u64 {
            let target =
                canonical_pad_target(cfg, &policy, seed, Duration::from_millis(0));
            let ms = target.as_millis();
            assert!(
                (180..=220).contains(&ms),
                "seed {seed}: pad {ms} ms outside ±10% window of 200 ms"
            );
        }
    }

    #[test]
    fn pad_target_seeded_jitter_is_deterministic() {
        let cfg = TimingPaddingConfig::new(200, 20).unwrap();
        let policy = JitterPolicy::Seeded;
        let a = canonical_pad_target(cfg, &policy, 42, Duration::from_millis(0));
        let b = canonical_pad_target(cfg, &policy, 42, Duration::from_millis(0));
        assert_eq!(a, b);
    }

    #[test]
    fn pad_target_seeded_jitter_varies_across_seeds() {
        let cfg = TimingPaddingConfig::new(200, 20).unwrap();
        let policy = JitterPolicy::Seeded;
        let mut distinct = std::collections::BTreeSet::new();
        for seed in 0..256u64 {
            let target =
                canonical_pad_target(cfg, &policy, seed, Duration::from_millis(0));
            distinct.insert(target.as_millis() as i64);
        }
        assert!(
            distinct.len() >= 8,
            "expected ≥ 8 distinct pad targets across 256 seeds, got {}",
            distinct.len()
        );
    }

    // ----- compute_request_seed: server-secret mixing ----------------

    #[test]
    fn server_secret_changes_seed_for_same_request_id() {
        // Codex round-1 P1 fix: the seed must NOT be a pure function
        // of the client-controlled `x-request-id`.
        let counter = AtomicU64::new(0);
        let req_a = Request::builder().header("x-request-id", "abc").body(()).unwrap();
        let counter_b = AtomicU64::new(0);
        let req_b = Request::builder().header("x-request-id", "abc").body(()).unwrap();
        let s_secret_1 = 0x1111_1111_1111_1111u64;
        let s_secret_2 = 0x2222_2222_2222_2222u64;
        let s1 = compute_request_seed(&req_a, s_secret_1, &counter);
        let s2 = compute_request_seed(&req_b, s_secret_2, &counter_b);
        assert_ne!(s1, s2, "different server secrets must produce different seeds");
    }

    #[test]
    fn same_id_different_calls_yield_different_seeds() {
        // Counter-mix defeats id-collision: two probes with the same
        // `x-request-id` (a misconfigured client OR an attacker
        // intentionally re-using ids) get distinct seeds.
        let counter = AtomicU64::new(0);
        let secret = 0xC0DE_C0DE_C0DE_C0DEu64;
        let req = || Request::builder().header("x-request-id", "same").body(()).unwrap();
        let s1 = compute_request_seed(&req(), secret, &counter);
        let s2 = compute_request_seed(&req(), secret, &counter);
        assert_ne!(s1, s2);
    }

    #[test]
    fn missing_id_falls_back_to_counter_mix() {
        let counter = AtomicU64::new(0);
        let secret = 0xABCDu64;
        let req = || Request::builder().body(()).unwrap();
        let s1 = compute_request_seed(&req(), secret, &counter);
        let s2 = compute_request_seed(&req(), secret, &counter);
        assert_ne!(s1, s2);
    }

    #[test]
    fn non_utf8_header_value_does_not_panic() {
        let counter = AtomicU64::new(0);
        let secret = 1u64;
        // Bytes 0x80..0x9F are valid HeaderValue bytes but not UTF-8.
        let v = HeaderValue::from_bytes(b"\x80\x81invalid").unwrap();
        let mut req = Request::new(());
        req.headers_mut().insert("x-request-id", v);
        let s = compute_request_seed(&req, secret, &counter);
        // Falls back to header_seed = 0; counter advances; s is
        // splitmix(secret ^ counter ^ 0).
        assert_eq!(s, splitmix_u64(secret));
    }

    // ----- Mann-Whitney U statistical primitive ----------------------

    #[test]
    fn mwu_identical_samples_p_one() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [1.0, 2.0, 3.0, 4.0, 5.0];
        let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
        assert!(p > 0.95, "identical samples expected p ≈ 1, got {p}");
    }

    #[test]
    fn mwu_disjoint_samples_p_small() {
        let xs: Vec<f64> = (0..30).map(|i| i as f64).collect();
        let ys: Vec<f64> = (100..130).map(|i| i as f64).collect();
        let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
        assert!(p < 0.001, "disjoint samples expected p ≪ 0.001, got {p}");
    }

    #[test]
    fn mwu_overlapping_samples_p_large() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x00C0_FFEE);
        let xs: Vec<f64> = (0..200).map(|_| rng.random_range(0.0..100.0)).collect();
        let ys: Vec<f64> = (0..200).map(|_| rng.random_range(0.0..100.0)).collect();
        let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
        assert!(p > 0.05, "same-distribution samples expected p > 0.05, got {p}");
    }

    #[test]
    fn mwu_empty_returns_none() {
        assert!(mann_whitney_u_p_value(&[], &[1.0]).is_none());
        assert!(mann_whitney_u_p_value(&[1.0], &[]).is_none());
    }

    #[test]
    fn mwu_handles_ties() {
        let xs = [1.0, 1.0, 2.0, 2.0, 3.0];
        let ys = [1.0, 2.0, 2.0, 3.0, 3.0];
        let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
        assert!((0.0..=1.0).contains(&p));
        assert!(p > 0.3);
    }

    // ----- Šidák correction ------------------------------------------

    #[test]
    fn sidak_canonical_nine_tests() {
        let alpha_prime = sidak_per_test_alpha(0.05, 9).unwrap();
        assert!((alpha_prime - 0.005_69).abs() < 1e-4);
    }

    #[test]
    fn sidak_canonical_three_tests() {
        let alpha_prime = sidak_per_test_alpha(0.05, 3).unwrap();
        assert!((alpha_prime - 0.016_95).abs() < 1e-4);
    }

    #[test]
    fn sidak_zero_k_returns_none() {
        assert!(sidak_per_test_alpha(0.05, 0).is_none());
    }

    #[test]
    fn sidak_alpha_out_of_range_returns_none() {
        assert!(sidak_per_test_alpha(-0.1, 3).is_none());
        assert!(sidak_per_test_alpha(1.5, 3).is_none());
    }

    // ----- Bootstrap CI ----------------------------------------------

    #[test]
    fn bootstrap_ci_zero_for_identical_samples() {
        let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let ys = xs.clone();
        let ci = bootstrap_median_ci(&xs, &ys, 200, 7).unwrap();
        assert_eq!(ci.point_estimate, 0.0);
        assert!(ci.ci_lower >= 0.0);
        assert!(ci.ci_upper <= 30.0);
    }

    #[test]
    fn bootstrap_ci_nonzero_for_separated_samples() {
        let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let ys: Vec<f64> = (200..250).map(|i| i as f64).collect();
        let ci = bootstrap_median_ci(&xs, &ys, 200, 11).unwrap();
        assert!(ci.point_estimate > 100.0);
        assert!(ci.ci_lower > 50.0, "expected CI lower > 50, got {}", ci.ci_lower);
    }

    #[test]
    fn bootstrap_ci_empty_returns_none() {
        let xs: Vec<f64> = vec![];
        let ys = vec![1.0];
        assert!(bootstrap_median_ci(&xs, &ys, 100, 0).is_none());
    }

    // ----- Median ----------------------------------------------------

    #[test]
    fn median_odd() {
        let mut v = [3.0, 1.0, 4.0, 1.0, 5.0];
        assert_eq!(median(&mut v), 3.0);
    }

    #[test]
    fn median_even() {
        let mut v = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(median(&mut v), 2.5);
    }

    // ----- splitmix --------------------------------------------------

    #[test]
    fn splitmix_str_is_deterministic_per_input() {
        let a = splitmix_str("019384a0-face-7000-8000-000000000001");
        let b = splitmix_str("019384a0-face-7000-8000-000000000001");
        assert_eq!(a, b);
    }

    #[test]
    fn splitmix_str_distinct_for_distinct_inputs() {
        let a = splitmix_str("req-001");
        let b = splitmix_str("req-002");
        assert_ne!(a, b);
    }

    #[test]
    fn splitmix_u64_canonical_known_outputs() {
        let v0 = splitmix_u64(0);
        let v1 = splitmix_u64(1);
        let vmax = splitmix_u64(u64::MAX);
        assert_ne!(v0, 0);
        assert_ne!(v1, 0);
        assert_ne!(v0, v1);
        assert_ne!(v1, vmax);
        for v in [v0, v1, vmax] {
            let ones = v.count_ones();
            assert!(
                (16..=48).contains(&ones),
                "splitmix output {v:#x} has unbalanced popcount {ones}"
            );
        }
    }

    #[test]
    fn splitmix_u64_avalanche_property() {
        for seed in 0..64u64 {
            let a = splitmix_u64(seed);
            let b = splitmix_u64(seed.wrapping_add(1));
            let differing = (a ^ b).count_ones();
            assert!(
                differing >= 16,
                "splitmix({seed}) ^ splitmix({}) only differs in {differing} bits",
                seed.wrapping_add(1)
            );
        }
    }

    // ----- MissPredicate canonical impls -----------------------------

    #[test]
    fn predicate_http_404_matches_404_only() {
        let p = miss_predicates::http_404();
        let mut r: Response<()> = Response::new(());
        *r.status_mut() = StatusCode::NOT_FOUND;
        assert!(p.matches(&r));
        *r.status_mut() = StatusCode::OK;
        assert!(!p.matches(&r));
        *r.status_mut() = StatusCode::FORBIDDEN;
        assert!(!p.matches(&r));
    }

    #[test]
    fn predicate_extension_marker_matches_marker_only() {
        let p = miss_predicates::extension_marker();
        let mut r: Response<()> = Response::new(());
        *r.status_mut() = StatusCode::OK;
        assert!(!p.matches(&r));
        r.extensions_mut().insert(MissMarker::new());
        assert!(p.matches(&r));
    }

    #[test]
    fn predicate_any_matches_any_signal() {
        let p = miss_predicates::any();
        // No status, no marker.
        let mut r: Response<()> = Response::new(());
        assert!(!p.matches(&r));
        // 404 alone.
        *r.status_mut() = StatusCode::NOT_FOUND;
        assert!(p.matches(&r));
        // Marker alone, OK status.
        let mut r: Response<()> = Response::new(());
        r.extensions_mut().insert(MissMarker::new());
        assert!(p.matches(&r));
        // grpc-status: 5 alone, OK status.
        let mut r: Response<()> = Response::new(());
        r.headers_mut().insert("grpc-status", "5".parse().unwrap());
        assert!(p.matches(&r));
    }

    #[test]
    fn predicate_grpc_not_found_matches_grpc_status_5() {
        let p = miss_predicates::grpc_not_found();
        let mut r: Response<()> = Response::new(());
        // No grpc-status header.
        assert!(!p.matches(&r));
        // grpc-status: 0 (OK).
        r.headers_mut().insert("grpc-status", "0".parse().unwrap());
        assert!(!p.matches(&r));
        // grpc-status: 5 (NOT_FOUND).
        r.headers_mut().insert("grpc-status", "5".parse().unwrap());
        assert!(p.matches(&r));
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptest harness: panics surface as test failures by design"
)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 10_000,
            failure_persistence: Some(Box::new(
                proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
            )),
            ..ProptestConfig::default()
        })]

        #[test]
        fn padding_window_holds_for_seeded_jitter(
            target_ms in PADDING_GRANULARITY_MS_MIN..=TARGET_P99_MS_MAX,
            jitter_pct in 0u8..=JITTER_PCT_MAX,
            seed in any::<u64>(),
            elapsed_ms in 0u64..1000,
        ) {
            let cfg = TimingPaddingConfig::new(target_ms, jitter_pct).unwrap();
            let policy = JitterPolicy::Seeded;
            let pad = canonical_pad_target(
                cfg,
                &policy,
                seed,
                Duration::from_millis(elapsed_ms),
            );
            let pad_ms = pad.as_millis() as i128;
            let target_i = target_ms as i128;
            let jitter_i = jitter_pct as i128;
            let min_padded = (target_i * (100 - jitter_i)) / 100;
            let lower_bound = core::cmp::max(min_padded, elapsed_ms as i128);
            let max_padded = (target_i * (100 + jitter_i)) / 100;
            let upper_bound = core::cmp::max(max_padded, elapsed_ms as i128);
            prop_assert!(
                pad_ms >= lower_bound && pad_ms <= upper_bound,
                "pad {pad_ms} ms not in [{lower_bound}, {upper_bound}] (target {target_ms}, jitter {jitter_pct}%, elapsed {elapsed_ms})"
            );
        }

        #[test]
        fn padding_deterministic_per_seed(
            target_ms in PADDING_GRANULARITY_MS_MIN..=TARGET_P99_MS_MAX,
            jitter_pct in 0u8..=JITTER_PCT_MAX,
            seed in any::<u64>(),
        ) {
            let cfg = TimingPaddingConfig::new(target_ms, jitter_pct).unwrap();
            let policy = JitterPolicy::Seeded;
            let elapsed = Duration::from_millis(0);
            let a = canonical_pad_target(cfg, &policy, seed, elapsed);
            let b = canonical_pad_target(cfg, &policy, seed, elapsed);
            prop_assert_eq!(a, b);
        }

        #[test]
        fn mwu_bounds_hold(
            xs in proptest::collection::vec(0.0_f64..1000.0, 5..200),
            ys in proptest::collection::vec(0.0_f64..1000.0, 5..200),
        ) {
            let p = mann_whitney_u_p_value(&xs, &ys).unwrap();
            prop_assert!(p.is_finite());
            prop_assert!((0.0..=1.0).contains(&p));
        }

        #[test]
        fn sidak_monotone_in_k(alpha in 0.001f64..=0.5, k in 1usize..50) {
            let alpha_k = sidak_per_test_alpha(alpha, k).unwrap();
            prop_assert!(alpha_k > 0.0);
            prop_assert!(alpha_k <= alpha + 1e-9);
            if k > 1 {
                let alpha_one = sidak_per_test_alpha(alpha, 1).unwrap();
                prop_assert!(alpha_k <= alpha_one + 1e-9);
            }
        }

        // INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE: server-side secret
        // entropy property — given DIFFERENT server secrets, the same
        // client-controlled `x-request-id` MUST produce DIFFERENT
        // seeds. Codex round-1 P1 fix; client must not be able to
        // pre-compute the pad.
        #[test]
        fn server_secret_dominates_client_id(
            secret_a in any::<u64>(),
            secret_b in any::<u64>(),
            id in "[a-zA-Z0-9_-]{1,64}",
        ) {
            prop_assume!(secret_a != secret_b);
            let counter_a = AtomicU64::new(0);
            let counter_b = AtomicU64::new(0);
            let req = || Request::builder().header("x-request-id", id.as_str()).body(()).unwrap();
            let sa = compute_request_seed(&req(), secret_a, &counter_a);
            let sb = compute_request_seed(&req(), secret_b, &counter_b);
            prop_assert_ne!(sa, sb);
        }
    }
}

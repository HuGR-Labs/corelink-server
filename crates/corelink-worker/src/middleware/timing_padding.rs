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
//! [`tower_layer::Layer`] whose service `Response = http::Response<BoxBody>`.
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
//! ## Internal layout (wave-33 stage 2.PRE-A.3)
//!
//! The original 1601-LOC monolith is decomposed into the following
//! submodules + tests + proptests. Every public name is re-exported
//! through this parent so consumers continue to import via
//! `middleware::timing_padding::*` (and through the
//! `middleware::*` umbrella).
//!
//! - [`config`] — constants + `TimingPaddingConfig` + `TimingPaddingError`.
//! - [`policy`] — `JitterPolicy` + `MissMarker` + `MissArm`.
//! - [`predicate`] — `PredicateKind` + `miss_predicates::*` aliases.
//! - [`padding`] — `canonical_pad_target` + per-request seed mixing
//!   + splitmix primitives + server-secret generator.
//! - [`stats`] — Mann-Whitney U + Šidák + bootstrap CI + median.
//! - [`layer`] — `TimingPaddingLayer` (Tower `Layer`).
//! - [`service`] — `TimingPaddingService` (Tower `Service` with the
//!   per-request padding loop).
//!
//! [miss-reason]: https://docs.rs/corelink-reapi/latest/corelink_reapi/read/enum.MissReason.html

pub mod config;
pub mod layer;
pub mod padding;
pub mod policy;
pub mod predicate;
pub mod service;
pub mod stats;

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split
// `middleware::timing_padding::*` public surface verbatim.
// ---------------------------------------------------------------------------

pub use config::{
    TimingPaddingConfig, TimingPaddingError, JITTER_PCT_DEFAULT, JITTER_PCT_MAX,
    PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_DEFAULT, TARGET_P99_MS_MAX,
};
pub use layer::TimingPaddingLayer;
pub use padding::canonical_pad_target;
pub use policy::{JitterPolicy, MissArm, MissMarker};
pub use predicate::{miss_predicates, PredicateKind};
pub use service::TimingPaddingService;
pub use stats::{
    bootstrap_median_ci, mann_whitney_u_p_value, sidak_per_test_alpha, BootstrapMedianCi,
};

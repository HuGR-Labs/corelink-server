//! Cross-cutting Tower middleware (host-server feature: `tower-middleware`).
//!
//! S-02 ships [`timing_padding`] (WI-S02-004) — the constant-time 404
//! [`MissReason`][miss-reason] parity defense that makes the read seam
//! statistically indistinguishable across `NeverExisted` /
//! `Tombstoned` / `R2OrphanRow` arms (per ADR-0028 + ADR-0023). The
//! layer is [`http::Response`]-typed so it composes cleanly with both
//! `axum::Router` (HTTP REST) and `tonic::transport::Server` (gRPC
//! over HTTP/2). The canonical
//! [`PredicateKind`][timing_padding::PredicateKind] enum captures
//! all four detection shapes:
//!
//! - `Http404` — HTTP `StatusCode == 404` (REST stack).
//! - `GrpcNotFound` — `grpc-status: 5` initial response header
//!   (tonic's canonical encoding of `Err(Status::not_found)` per
//!   `tonic 0.12 status.rs::into_http`; codex round-1 P0 fix).
//! - `ExtensionMarker` — handler-emitted
//!   [`MissMarker`][timing_padding::MissMarker] extension; useful
//!   for handlers that wish to opt a response into padding regardless
//!   of HTTP or gRPC status.
//! - `Any` (canonical default) — any of the three signals.
//!
//! [miss-reason]: https://docs.rs/corelink-reapi/latest/corelink_reapi/read/enum.MissReason.html

pub mod timing_padding;

pub use timing_padding::{
    bootstrap_median_ci, canonical_pad_target, mann_whitney_u_p_value, miss_predicates,
    sidak_per_test_alpha, BootstrapMedianCi, JitterPolicy, MissArm, MissMarker, PredicateKind,
    TimingPaddingConfig, TimingPaddingError, TimingPaddingLayer, TimingPaddingService,
    JITTER_PCT_DEFAULT, JITTER_PCT_MAX, PADDING_GRANULARITY_MS_MIN, TARGET_P99_MS_DEFAULT,
    TARGET_P99_MS_MAX,
};

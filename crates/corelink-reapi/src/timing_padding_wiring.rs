//! Canonical wiring helpers for the WI-S02-004 [`TimingPaddingLayer`]
//! across the HTTP REST surface (`cas_get_router`) and the gRPC
//! tonic Server stack.
//!
//! ## Why a dedicated wiring module
//!
//! tonic's `Server::builder().layer(L)` accepts any tower
//! [`tower_layer::Layer`]; mounting the padding layer there is the
//! canonical way to ensure EVERY gRPC service hosted on that server
//! flows through the constant-time 404 defense — including the gRPC
//! happy-path `Capabilities::GetCapabilities` (whose responses are
//! NOT padded — predicates exclude OK), the cold-cache miss arms of
//! `BatchReadBlobs::r2_orphan_row`, and any future `cache:r` /
//! `cache:find-missing` scope-failures that surface as gRPC
//! `NOT_FOUND`.
//!
//! ## Why two helpers
//!
//! - [`canonical_http_padding_layer`] fixes the predicate to
//!   [`PredicateKind::Any`] so the HTTP REST stack catches both
//!   pure-`StatusCode == 404` responses AND any handler that opts in
//!   via [`MissMarker`][corelink_worker::middleware::MissMarker]
//!   (defense-in-depth: a future map_response that rewrites 404 →
//!   200 + body still triggers padding).
//! - [`canonical_grpc_padding_layer`] fixes the predicate to
//!   [`PredicateKind::GrpcNotFound`] — the tonic-encoded
//!   `Err(Status::not_found)` lands as HTTP 200 + `grpc-status: 5`
//!   in initial response headers (vide tonic 0.12
//!   `status.rs::into_http`). The HTTP-404-only predicate would miss
//!   it; the request-extension miss marker (`MissMarker` in the
//!   request-handling layer) does not survive the
//!   `tonic::Status → http::Response` transform; [`PredicateKind::GrpcNotFound`]
//!   is the only canonical detection. Codex round-1 P0 + round-2
//!   P0 fix.

use corelink_auth::middleware::{
    JitterPolicy, PredicateKind, TimingPaddingConfig, TimingPaddingLayer,
};

/// Type re-export — callers usually import `MissPaddingLayer` rather
/// than the longer `corelink_worker::middleware::TimingPaddingLayer`.
pub type MissPaddingLayer = TimingPaddingLayer;

/// Canonical layer for the HTTP REST stack (`cas_get_router`).
///
/// Predicate: [`PredicateKind::Any`] — pads HTTP 404 responses AND
/// any response with the [`MissMarker`][corelink_worker::middleware::MissMarker]
/// extension AND any `grpc-status: 5` (defense-in-depth: lets the
/// same builder host a tonic-web adapter alongside REST without an
/// extra layer).
#[must_use]
pub fn canonical_http_padding_layer() -> TimingPaddingLayer {
    TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded)
        .with_predicate(PredicateKind::Any)
}

/// Canonical layer for the gRPC stack (mount at
/// `tonic::Server::builder().layer(...)`).
///
/// Predicate: [`PredicateKind::Any`] — pads `grpc-status: 5` shape
/// that `tonic::Status::into_http` emits for `Err(Status::not_found)`
/// AND any response that carries the
/// [`MissMarker`][corelink_worker::middleware::MissMarker] extension.
/// The extension path covers the canonical `BatchReadBlobs` /
/// `FindMissingBlobs` "all-slots-miss" shape (codex round-6 P1 fix —
/// the per-blob misses inside batch handlers complete as gRPC OK at
/// the envelope level; without `MissMarker` insertion + `Any`
/// predicate, single-digest probes via batch endpoints would bypass
/// the layer).
///
/// Production wiring (in apps/server when reapi gRPC services are
/// integrated):
///
/// ```ignore
/// use tonic::transport::Server;
/// use corelink_reapi::canonical_grpc_padding_layer;
///
/// Server::builder()
///     .layer(canonical_grpc_padding_layer())
///     .add_service(byte_stream_service)
///     .add_service(content_addressable_storage_service)
///     .add_service(capabilities_service)
///     .serve(addr)
///     .await?;
/// ```
#[must_use]
pub fn canonical_grpc_padding_layer() -> TimingPaddingLayer {
    TimingPaddingLayer::new(TimingPaddingConfig::canonical(), JitterPolicy::Seeded)
        .with_predicate(PredicateKind::Any)
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
    use corelink_auth::middleware::PredicateKind;

    #[test]
    fn canonical_http_layer_uses_any_predicate() {
        let l = canonical_http_padding_layer();
        assert_eq!(l.predicate_kind(), PredicateKind::Any);
    }

    #[test]
    fn canonical_grpc_layer_uses_any_predicate() {
        let l = canonical_grpc_padding_layer();
        assert_eq!(l.predicate_kind(), PredicateKind::Any);
    }
}

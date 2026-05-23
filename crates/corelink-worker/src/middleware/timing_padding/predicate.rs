//! [`PredicateKind`] — miss-detection predicate enum + the
//! `miss_predicates::*` constructor aliases.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

use http::{Response, StatusCode};

use super::policy::MissMarker;

/// Predicate kind that decides whether a given response is a miss
/// (canonical 404 paying-into-padding contract). Enum-typed (not
/// boxed `dyn Fn`) so the [`super::layer::TimingPaddingLayer`] can
/// stay generic-parameter-free while still letting callers pick the
/// right detection logic per-stack.
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
pub(super) fn is_grpc_not_found<B>(resp: &Response<B>) -> bool {
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

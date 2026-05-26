//! Top-level error type for the brew adapter.
//!
//! Mirrors the dispatch packet (`specs/_proposals/adapters/brew.md` §5)
//! variant set 1:1. `#[non_exhaustive]` so future variants (e.g. quota
//! exhaustion, region-pin violation) can land without breaking callers.

use thiserror::Error;

/// All recoverable failure modes surfaced by [`crate::run_brew_adapter`]
/// and the underlying request pipeline.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BrewAdapterError {
    /// `TcpListener::bind` (or the equivalent axum bind step) failed.
    /// Typically: port already in use, insufficient privileges, or the
    /// supplied [`std::net::SocketAddr`] is unroutable on the host.
    #[error("bind: {0}")]
    Bind(std::io::Error),

    /// Authentication failed. The `String` carries a non-secret reason
    /// (e.g. `"missing Authorization header"`, `"invalid PAT"`); the
    /// PAT plaintext is NEVER included. Routes map this to HTTP 401.
    #[error("auth: {0}")]
    Auth(String),

    /// Per-tenant CAS read / write failed. The `String` is a structured
    /// reason from the [`crate::ports::CasStore`] implementor. Routes
    /// map this to HTTP 502 (we treat CAS unavailability as a gateway
    /// dependency failure, not an auth or input issue).
    #[error("cas: {0}")]
    Cas(String),

    /// Upstream bottle host (default `https://ghcr.io`) returned an
    /// error, timed out, or produced a malformed body. Routes map this
    /// to HTTP 502.
    #[error("upstream: {0}")]
    Upstream(String),

    /// Upstream Content-Length (or streamed byte total) exceeded the
    /// configured [`crate::BrewAdapterConfig::bottle_size_limit_bytes`].
    /// Routes map this to HTTP 413.
    #[error("bottle exceeds limit: {0} bytes")]
    BottleOversized(u64),

    /// Audit emit failed (the chokepoint trait returned an error). Per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`, the CAS mutation MUST NOT
    /// proceed. Routes map this to HTTP 503.
    #[error("audit: {0}")]
    Audit(String),
}

//! Top-level error type for the cargo adapter.
//!
//! Mirrors the dispatch packet (`specs/_proposals/adapters/cargo.md` §5)
//! variant set 1:1. `#[non_exhaustive]` so future variants (e.g. quota
//! exhaustion, region-pin violation) can land without breaking callers.

use thiserror::Error;

/// All recoverable failure modes surfaced by [`crate::run_cargo_adapter`]
/// and the underlying request pipeline.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CargoAdapterError {
    /// `TcpListener::bind` (or equivalent axum bind step) failed.
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
    /// reason from the [`crate::cargo::ports::CasStore`] implementor. Routes
    /// map this to HTTP 502.
    #[error("cas: {0}")]
    Cas(String),

    /// Audit emit failed (the chokepoint trait returned an error). Per
    /// the audit-fail-CLOSED contract, the CAS mutation MUST NOT proceed.
    /// Routes map this to HTTP 503.
    #[error("audit: {0}")]
    Audit(String),

    /// PUT body exceeded the configured body size limit.
    /// Routes map this to HTTP 413.
    #[error("body exceeds limit: {0} bytes")]
    BodyOversized(u64),
}

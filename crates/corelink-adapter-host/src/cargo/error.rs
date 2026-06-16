//! Top-level error type for the cargo adapter.
//!
//! Mirrors the dispatch packet (`specs/_proposals/adapters/cargo.md` §5)
//! variant set 1:1. `#[non_exhaustive]` so future variants (e.g. quota
//! exhaustion, region-pin violation) can land without breaking callers.

use thiserror::Error;

/// All recoverable failure modes surfaced by [`crate::cargo::run_cargo_adapter`]
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

impl CargoAdapterError {
    /// True for variants whose inner string carries INTERNAL backend detail
    /// (D1 / Cloudflare API errors, possibly SQL; R2 storage topology; the
    /// derived per-tenant R2 prefix) that MUST NOT reach the client
    /// (Cluster E — A27 / A29). Their wire message is replaced with an
    /// opaque, reference-only string; the real detail is logged server-side.
    #[must_use]
    pub const fn leaks_internal_detail(&self) -> bool {
        matches!(self, Self::Bind(_) | Self::Auth(_) | Self::Cas(_) | Self::Audit(_))
    }

    /// The CLIENT-FACING response body for this error (Cluster E).
    ///
    /// Backend-fault variants collapse to an opaque, class-keyed string plus a
    /// `ref` correlation id (joined to the server-side `tracing::error!` that
    /// carries the real detail). `BodyOversized` keeps its actionable, non-
    /// leaking message.
    #[must_use]
    pub fn client_message(&self, request_id: &str) -> String {
        if self.leaks_internal_detail() {
            let class = match self {
                Self::Auth(_) => "authentication failed",
                _ => "internal error",
            };
            format!("{class} (ref: {request_id})")
        } else {
            self.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_fault_client_message_is_opaque_and_ref_tagged() {
        let raw = "D1 HTTP 500: no such table: pat in SELECT ... FROM pat";
        for err in [
            CargoAdapterError::Auth(format!("backend: {raw}")),
            CargoAdapterError::Cas(raw.into()),
            CargoAdapterError::Audit(raw.into()),
        ] {
            assert!(err.leaks_internal_detail(), "{err:?} must be flagged leaky");
            let msg = err.client_message("RIDc");
            assert!(!msg.contains("D1"), "leaked D1: {msg}");
            assert!(!msg.contains("SELECT"), "leaked SQL: {msg}");
            assert!(!msg.contains("FROM pat"), "leaked SQL: {msg}");
            assert!(msg.contains("RIDc"), "missing correlation ref: {msg}");
        }
    }

    #[test]
    fn oversized_keeps_actionable_message() {
        let e = CargoAdapterError::BodyOversized(4096);
        assert!(!e.leaks_internal_detail());
        assert!(e.client_message("RID").contains("4096"));
    }
}

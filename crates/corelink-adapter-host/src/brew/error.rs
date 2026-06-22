//! Top-level error type for the brew adapter.
//!
//! Mirrors the dispatch packet (`specs/_proposals/adapters/brew.md` §5)
//! variant set 1:1. `#[non_exhaustive]` so future variants (e.g. quota
//! exhaustion, region-pin violation) can land without breaking callers.

use thiserror::Error;

/// All recoverable failure modes surfaced by [`crate::brew::run_brew_adapter`]
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
    /// reason from the [`crate::brew::ports::CasStore`] implementor. Routes
    /// map this to HTTP 502 (we treat CAS unavailability as a gateway
    /// dependency failure, not an auth or input issue).
    #[error("cas: {0}")]
    Cas(String),

    /// Upstream bottle host (default `https://ghcr.io`) returned an
    /// error, timed out, or produced a malformed body. Routes map this
    /// to HTTP 502.
    #[error("upstream: {0}")]
    Upstream(String),

    /// The requested bottle path is outside the allowed Homebrew repo
    /// namespace (`homebrew/core`, `homebrew/cask`). The brew adapter is a
    /// read-through cache for PUBLIC Homebrew bottles only — it MUST NOT be
    /// usable as an unrestricted authenticated ghcr.io proxy that fetches
    /// attacker-chosen content into the shared `_public` namespace (F-005).
    /// Routes map this to HTTP 403. The `String` is the offending (canonical)
    /// path and is client-safe (it is the caller's own request path).
    #[error("forbidden repo path: {0}")]
    ForbiddenRepoPath(String),

    /// Upstream Content-Length (or streamed byte total) exceeded the
    /// configured [`crate::brew::BrewAdapterConfig::bottle_size_limit_bytes`].
    /// Routes map this to HTTP 413.
    #[error("bottle exceeds limit: {0} bytes")]
    BottleOversized(u64),

    /// Audit emit failed (the chokepoint trait returned an error). Per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`, the CAS mutation MUST NOT
    /// proceed. Routes map this to HTTP 503.
    #[error("audit: {0}")]
    Audit(String),
}

impl BrewAdapterError {
    /// True for variants whose inner string carries INTERNAL backend detail
    /// (D1 / Cloudflare API errors, possibly SQL; R2 storage topology; the
    /// derived per-tenant R2 prefix; upstream host/transport detail) that
    /// MUST NOT reach the client (Cluster E — A27 / A29).
    #[must_use]
    pub const fn leaks_internal_detail(&self) -> bool {
        matches!(
            self,
            Self::Bind(_) | Self::Auth(_) | Self::Cas(_) | Self::Upstream(_) | Self::Audit(_)
        )
    }

    /// The CLIENT-FACING response body for this error (Cluster E).
    ///
    /// Backend-fault variants collapse to an opaque, class-keyed string plus a
    /// `ref` correlation id (joined to the server-side `tracing::error!` that
    /// carries the real detail). `BottleOversized` keeps its actionable,
    /// non-leaking message.
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
        let raw = "R2 GetObject failed: bucket corelink-cas key tenants/abc/...";
        for err in [
            BrewAdapterError::Auth(format!("backend: {raw}")),
            BrewAdapterError::Cas(raw.into()),
            BrewAdapterError::Upstream(raw.into()),
            BrewAdapterError::Audit(raw.into()),
        ] {
            assert!(err.leaks_internal_detail(), "{err:?} must be flagged leaky");
            let msg = err.client_message("RIDb");
            assert!(!msg.contains("R2"), "leaked R2 topology: {msg}");
            assert!(!msg.contains("bucket"), "leaked bucket: {msg}");
            assert!(!msg.contains("tenants/"), "leaked per-tenant prefix: {msg}");
            assert!(msg.contains("RIDb"), "missing correlation ref: {msg}");
        }
    }

    #[test]
    fn oversized_keeps_actionable_message() {
        let e = BrewAdapterError::BottleOversized(9001);
        assert!(!e.leaks_internal_detail());
        assert!(e.client_message("RID").contains("9001"));
    }
}

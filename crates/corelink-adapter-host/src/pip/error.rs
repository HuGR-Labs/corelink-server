//! Terminal error enum for the pip adapter.
//!
//! Every public function in the adapter that can fail surfaces a
//! [`PipAdapterError`] (or wraps it). The variants intentionally map
//! 1:1 onto the HTTP responses the axum layer produces so that
//! response shaping in [`crate::pip::server`] is a single `match` (no
//! conversion gymnastics, no swallowing of failure context).

use thiserror::Error;

/// Errors produced by the pip adapter.
///
/// `#[non_exhaustive]` so future variants (e.g. per-tenant rate-limit
/// rejection, malformed `Accept` header) can be added without
/// breaking downstream consumers that match on this enum.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PipAdapterError {
    /// The HTTP listener could not bind to the configured address.
    ///
    /// Surfaces only at boot from [`crate::pip::server::run_pip_adapter`].
    #[error("bind: {0}")]
    Bind(std::io::Error),

    /// PAT verification failed (missing header, malformed token, or
    /// tenant resolver rejected). Maps to `401 Unauthorized` at the
    /// HTTP layer.
    #[error("auth: {0}")]
    Auth(String),

    /// CAS read / write failure (transport, encoding, or store-level).
    /// Maps to `503 Service Unavailable`.
    #[error("cas: {0}")]
    Cas(String),

    /// Upstream PyPI request failed (DNS, TLS, 5xx, or non-2xx
    /// status). Maps to `502 Bad Gateway`.
    #[error("upstream: {0}")]
    Upstream(String),

    /// The downloaded wheel/sdist bytes' SHA256 did not match the
    /// `#sha256=` fragment from the upstream index URL. Adapter
    /// REJECTS the store and emits `pip.wheel.integrity_mismatch`
    /// audit. Maps to `502 Bad Gateway` (the upstream gave us
    /// corrupt bytes).
    #[error("integrity mismatch: expected sha256 {expected}, got {actual}")]
    IntegrityMismatch {
        /// Hex-encoded SHA256 from the upstream URL `#sha256=` fragment.
        expected: String,
        /// Hex-encoded SHA256 of the downloaded bytes.
        actual: String,
    },

    /// Wheel/sdist exceeded the configured
    /// [`crate::pip::config::PipAdapterConfig::wheel_size_limit_bytes`]
    /// limit. Maps to `413 Payload Too Large`.
    #[error("wheel exceeds limit: {0} bytes")]
    WheelOversized(u64),

    /// Audit emit failed; per the audit-fail-CLOSED contract the
    /// state-mutating CAS / KV write is rolled back and a 503 is
    /// returned to the client.
    #[error("audit: {0}")]
    Audit(String),

    /// Index JSON parse failed or upstream returned malformed payload
    /// (defends against the §8 adversarial row 5 "index injection"
    /// attack — fail-CLOSED). Maps to `502 Bad Gateway`.
    #[error("index parse: {0}")]
    IndexParse(String),

    /// KV (tenant-scoped index cache) read / write failure. Maps to
    /// `503 Service Unavailable`.
    #[error("kv: {0}")]
    Kv(String),
}

impl PipAdapterError {
    /// True for variants whose inner string carries INTERNAL backend detail
    /// (D1 / Cloudflare API errors, possibly SQL; R2 storage topology; the
    /// derived per-tenant R2 prefix; upstream host/transport detail) that
    /// MUST NOT reach the client (Cluster E — A27 / A29).
    #[must_use]
    pub const fn leaks_internal_detail(&self) -> bool {
        matches!(
            self,
            Self::Bind(_)
                | Self::Auth(_)
                | Self::Cas(_)
                | Self::Upstream(_)
                | Self::IndexParse(_)
                | Self::Kv(_)
                | Self::Audit(_)
        )
    }

    /// The CLIENT-FACING response body for this error (Cluster E).
    ///
    /// Backend-fault variants collapse to an opaque, class-keyed string plus a
    /// `ref` correlation id (joined to the server-side `tracing::error!` that
    /// carries the real detail). `IntegrityMismatch` (hashes) and
    /// `WheelOversized` keep their actionable, non-leaking messages.
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

    /// Map an error variant to the HTTP status code the axum layer
    /// emits. Pure-logic helper extracted here so unit tests can pin
    /// the mapping table independently of the router wiring.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Bind(_) => 500,
            Self::Auth(_) => 401,
            Self::Cas(_) | Self::Kv(_) | Self::Audit(_) => 503,
            Self::Upstream(_) | Self::IntegrityMismatch { .. } | Self::IndexParse(_) => 502,
            Self::WheelOversized(_) => 413,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn status_code_table_matches_contract() {
        assert_eq!(PipAdapterError::Auth("x".into()).status_code(), 401);
        assert_eq!(PipAdapterError::Cas("x".into()).status_code(), 503);
        assert_eq!(PipAdapterError::Kv("x".into()).status_code(), 503);
        assert_eq!(PipAdapterError::Audit("x".into()).status_code(), 503);
        assert_eq!(PipAdapterError::Upstream("x".into()).status_code(), 502);
        assert_eq!(PipAdapterError::IndexParse("x".into()).status_code(), 502);
        assert_eq!(
            PipAdapterError::IntegrityMismatch {
                expected: "a".into(),
                actual: "b".into()
            }
            .status_code(),
            502
        );
        assert_eq!(PipAdapterError::WheelOversized(9001).status_code(), 413);
    }

    #[test]
    fn display_includes_context() {
        let err = PipAdapterError::IntegrityMismatch {
            expected: "abc".into(),
            actual: "def".into(),
        };
        let rendered = format!("{err}");
        assert!(rendered.contains("abc"));
        assert!(rendered.contains("def"));
    }

    // ── Cluster E (A27/A29): client_message scrubs internal backend detail ────

    #[test]
    fn backend_fault_client_message_is_opaque_and_ref_tagged() {
        let raw = "D1 HTTP 500: no such table: pat in SELECT ... FROM pat";
        for err in [
            PipAdapterError::Auth(format!("backend: {raw}")),
            PipAdapterError::Cas(raw.into()),
            PipAdapterError::Kv(raw.into()),
            PipAdapterError::Upstream(raw.into()),
            PipAdapterError::IndexParse(raw.into()),
            PipAdapterError::Audit(raw.into()),
        ] {
            assert!(err.leaks_internal_detail(), "{err:?} must be flagged leaky");
            let msg = err.client_message("RID9");
            assert!(!msg.contains("D1"), "leaked D1: {msg}");
            assert!(!msg.contains("SELECT"), "leaked SQL: {msg}");
            assert!(!msg.contains("FROM pat"), "leaked SQL: {msg}");
            assert!(msg.contains("RID9"), "missing correlation ref: {msg}");
        }
    }

    #[test]
    fn safe_variants_keep_actionable_message() {
        let im = PipAdapterError::IntegrityMismatch {
            expected: "aaa".into(),
            actual: "bbb".into(),
        };
        assert!(!im.leaks_internal_detail());
        let msg = im.client_message("RID");
        assert!(msg.contains("aaa") && msg.contains("bbb"));
    }
}

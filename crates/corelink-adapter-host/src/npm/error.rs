//! Terminal error enum for the npm adapter.
//!
//! Every public function that can fail surfaces a [`NpmAdapterError`].
//! Variants map 1:1 onto HTTP responses so that response shaping in
//! [`crate::npm::server`] is a single `match`.

use thiserror::Error;

/// Errors produced by the npm adapter.
///
/// `#[non_exhaustive]` so future variants can be added without
/// breaking downstream consumers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NpmAdapterError {
    /// The HTTP listener could not bind to the configured address.
    #[error("bind: {0}")]
    Bind(std::io::Error),

    /// PAT verification failed (missing, malformed, or tenant resolver
    /// rejected). Maps to `401 Unauthorized`.
    #[error("auth: {0}")]
    Auth(String),

    /// The PAT verifier SHED this request under load — it never reached a
    /// verdict on the credential, so this is emphatically NOT an auth
    /// failure. Maps to `503 Service Unavailable` +
    /// `Retry-After: `[`crate::overload::SHED_RETRY_AFTER_SECS`].
    ///
    /// Distinct from [`Self::Auth`] because collapsing the two told a client
    /// holding a perfectly valid PAT that its credential was invalid, and
    /// because the split must stay uniform across D1 row existence
    /// (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`).
    #[error("verifier overloaded: {0}")]
    VerifierOverloaded(String),

    /// CAS read / write failure. Maps to `503 Service Unavailable`.
    #[error("cas: {0}")]
    Cas(String),

    /// KV (tenant-scoped metadata cache) read / write failure.
    /// Maps to `503 Service Unavailable`.
    #[error("kv: {0}")]
    Kv(String),

    /// Upstream registry.npmjs.org request failed. Maps to
    /// `502 Bad Gateway`.
    #[error("upstream: {0}")]
    Upstream(String),

    /// Audit emit failed; per the audit-fail-CLOSED contract the
    /// state-mutating CAS / KV write is rolled back and a 503 is
    /// returned to the client.
    #[error("audit: {0}")]
    Audit(String),

    /// Tarball exceeded the configured size limit. Maps to
    /// `413 Payload Too Large`.
    #[error("tarball exceeds limit: {0} bytes")]
    TarballOversized(u64),

    /// The downloaded tarball bytes' SHA256 did not match the
    /// `dist.shasum` field from npm metadata. Adapter REJECTS the
    /// store and emits `npm.tarball.integrity_mismatch.v1` audit.
    /// Maps to `502 Bad Gateway`.
    #[error("integrity mismatch: expected {expected}, got {actual}")]
    IntegrityMismatch {
        /// Hex-encoded SHA1 / SHA512 from npm metadata `dist.shasum`.
        expected: String,
        /// Hex-encoded digest of the downloaded bytes.
        actual: String,
    },

    /// Metadata JSON from upstream is malformed or could not be
    /// parsed. Maps to `502 Bad Gateway`.
    #[error("metadata parse: {0}")]
    MetadataParse(String),

    /// Upstream metadata's canonical `name` does not match the requested
    /// (normalized) package — a case/trim/encoding alias that would poison the
    /// SHARED `_public` metadata namespace cross-tenant (rt-nuclear #7). The
    /// adapter REJECTS the cache store (fail-CLOSED), mirroring the tarball
    /// `IntegrityMismatch` contract. Maps to `502 Bad Gateway`. Both names are
    /// public package identifiers, so the message is non-leaking.
    #[error("metadata name mismatch: requested {requested}, upstream returned {fetched}")]
    MetadataNameMismatch {
        /// The requested package name, normalized.
        requested: String,
        /// The canonical `name` the upstream metadata carried (normalized).
        fetched: String,
    },
}

impl NpmAdapterError {
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
                | Self::VerifierOverloaded(_)
                | Self::Cas(_)
                | Self::Kv(_)
                | Self::Upstream(_)
                | Self::Audit(_)
                | Self::MetadataParse(_)
        )
    }

    /// `Retry-After` (seconds) this error must carry, if any.
    ///
    /// Only the load-shed variant is retryable on a bounded horizon; every
    /// other 503 here (`Cas` / `Kv` / `Audit`) is a fail-CLOSED fault with no
    /// useful back-off to advertise.
    #[must_use]
    pub const fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::VerifierOverloaded(_) => Some(crate::overload::SHED_RETRY_AFTER_SECS),
            _ => None,
        }
    }

    /// The CLIENT-FACING response body for this error (Cluster E).
    ///
    /// Backend-fault variants collapse to an opaque, class-keyed string plus a
    /// `ref` correlation id (joined to the server-side `tracing::error!` that
    /// carries the real detail). `IntegrityMismatch` (hashes) and
    /// `TarballOversized` keep their actionable, non-leaking messages.
    #[must_use]
    pub fn client_message(&self, request_id: &str) -> String {
        if self.leaks_internal_detail() {
            let class = match self {
                Self::Auth(_) => "authentication failed",
                // A shed is NOT an auth verdict — say so, without leaking
                // which internal pool shed (the message is class-only).
                Self::VerifierOverloaded(_) => "authentication service overloaded; retry",
                _ => "internal error",
            };
            format!("{class} (ref: {request_id})")
        } else {
            self.to_string()
        }
    }

    /// Map an error variant to the HTTP status code the axum layer
    /// emits.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Bind(_) => 500,
            Self::Auth(_) => 401,
            Self::VerifierOverloaded(_) | Self::Cas(_) | Self::Kv(_) | Self::Audit(_) => 503,
            Self::Upstream(_)
            | Self::IntegrityMismatch { .. }
            | Self::MetadataParse(_)
            | Self::MetadataNameMismatch { .. } => 502,
            Self::TarballOversized(_) => 413,
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
        assert_eq!(NpmAdapterError::Auth("x".into()).status_code(), 401);
        // A load shed is NOT an auth verdict — 503, and the ONLY variant here
        // that advertises a back-off (INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM).
        let shed = NpmAdapterError::VerifierOverloaded("x".into());
        assert_eq!(shed.status_code(), 503);
        assert_eq!(shed.retry_after_secs(), Some(1));
        assert_eq!(NpmAdapterError::Cas("x".into()).retry_after_secs(), None);
        assert_eq!(NpmAdapterError::Auth("x".into()).retry_after_secs(), None);
        assert_eq!(NpmAdapterError::Cas("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Kv("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Audit("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Upstream("x".into()).status_code(), 502);
        assert_eq!(
            NpmAdapterError::MetadataParse("x".into()).status_code(),
            502
        );
        assert_eq!(
            NpmAdapterError::IntegrityMismatch {
                expected: "a".into(),
                actual: "b".into(),
            }
            .status_code(),
            502
        );
        assert_eq!(NpmAdapterError::TarballOversized(9001).status_code(), 413);
    }

    #[test]
    fn display_integrity_includes_both_digests() {
        let err = NpmAdapterError::IntegrityMismatch {
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
            NpmAdapterError::Auth(format!("backend: {raw}")),
            NpmAdapterError::Cas(raw.into()),
            NpmAdapterError::Kv(raw.into()),
            NpmAdapterError::Upstream(raw.into()),
            NpmAdapterError::Audit(raw.into()),
            NpmAdapterError::MetadataParse(raw.into()),
        ] {
            assert!(err.leaks_internal_detail(), "{err:?} must be flagged leaky");
            let msg = err.client_message("RIDn");
            assert!(!msg.contains("D1"), "leaked D1: {msg}");
            assert!(!msg.contains("SELECT"), "leaked SQL: {msg}");
            assert!(!msg.contains("FROM pat"), "leaked SQL: {msg}");
            assert!(msg.contains("RIDn"), "missing correlation ref: {msg}");
        }
    }

    #[test]
    fn safe_variants_keep_actionable_message() {
        let im = NpmAdapterError::IntegrityMismatch {
            expected: "aaa".into(),
            actual: "bbb".into(),
        };
        assert!(!im.leaks_internal_detail());
        let msg = im.client_message("RID");
        assert!(msg.contains("aaa") && msg.contains("bbb"));
    }
}

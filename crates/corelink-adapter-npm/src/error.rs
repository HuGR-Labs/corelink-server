//! Terminal error enum for the npm adapter.
//!
//! Every public function that can fail surfaces a [`NpmAdapterError`].
//! Variants map 1:1 onto HTTP responses so that response shaping in
//! [`crate::server`] is a single `match`.

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
}

impl NpmAdapterError {
    /// Map an error variant to the HTTP status code the axum layer
    /// emits.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Bind(_) => 500,
            Self::Auth(_) => 401,
            Self::Cas(_) | Self::Kv(_) | Self::Audit(_) => 503,
            Self::Upstream(_)
            | Self::IntegrityMismatch { .. }
            | Self::MetadataParse(_) => 502,
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
        assert_eq!(NpmAdapterError::Cas("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Kv("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Audit("x".into()).status_code(), 503);
        assert_eq!(NpmAdapterError::Upstream("x".into()).status_code(), 502);
        assert_eq!(NpmAdapterError::MetadataParse("x".into()).status_code(), 502);
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
}

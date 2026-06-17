//! `BazelBridgeError` taxonomy.

use thiserror::Error;

/// All errors that the Bazel bridge surfaces to callers.
///
/// The variant set is `#[non_exhaustive]` per the CoreLink charter §9.3;
/// callers MUST use a wildcard arm.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BazelBridgeError {
    /// The requested blob does not exist in the CAS for the given tenant.
    ///
    /// HTTP 404 equivalent.
    #[error("blob not found: tenant={tenant} hash={hash}")]
    NotFound {
        /// Tenant the lookup was scoped to.
        tenant: String,
        /// Content hash requested.
        hash: String,
    },

    /// The `Digest` in the REAPI URI or request body could not be parsed
    /// or failed validation. Covers:
    /// - hash is not exactly 64 lowercase hex characters
    /// - `size_bytes` is not a valid `u64`
    /// - `size_bytes` exceeds 4 GiB
    ///
    /// HTTP 400 equivalent.
    #[error("invalid REAPI digest: {reason}")]
    InvalidDigest {
        /// Human-readable explanation of what failed validation.
        reason: String,
    },

    /// The `size_bytes` in the REAPI Digest does not match the actual byte
    /// length of the uploaded blob.
    ///
    /// HTTP 400 equivalent.
    #[error("size mismatch: digest_size={digest_size} actual={actual}")]
    SizeMismatch {
        /// The `size_bytes` value from the REAPI Digest.
        digest_size: u64,
        /// The actual byte length of the payload.
        actual: u64,
    },

    /// The uploaded bytes do not hash (SHA-256, the REAPI v2 content-
    /// addressing function) to the client-supplied digest. The REAPI write
    /// boundary verifies this BEFORE delegating to the shared handler (a
    /// clean early rejection + defense-in-depth; the durable gate re-verifies
    /// the SHA-256 keyspace independently). Distinct from
    /// [`Self::SizeMismatch`] (byte length) and [`Self::InvalidDigest`]
    /// (malformed digest).
    ///
    /// HTTP 422 equivalent.
    #[error("digest mismatch: {reason}")]
    DigestMismatch {
        /// Human-readable explanation (expected vs actual SHA-256).
        reason: String,
    },

    /// Caller attempted to access another tenant's objects. Fail-CLOSED.
    ///
    /// HTTP 403 equivalent.
    #[error("cross-tenant access denied: caller={caller} requested={requested}")]
    CrossTenantDenied {
        /// Tenant the caller authenticated as.
        caller: String,
        /// Tenant extracted from the REAPI URI instance path.
        requested: String,
    },

    /// Audit emit failed before a mutation could be applied. Fail-CLOSED:
    /// the state is unchanged.
    ///
    /// HTTP 503 equivalent.
    #[error("audit emit failed: {0}")]
    AuditFailed(String),

    /// A `findMissingBlobs` request exceeded the maximum digest count cap
    /// ([`crate::FIND_MISSING_BLOB_CAP`]).
    ///
    /// HTTP 413 equivalent.
    #[error("findMissingBlobs batch too large: requested={requested} cap={cap}")]
    BatchTooLarge {
        /// Number of digests in the request.
        requested: usize,
        /// The server-enforced cap.
        cap: usize,
    },

    /// Internal error (lock poisoning, unexpected handler state, etc.).
    ///
    /// HTTP 500 equivalent.
    #[error("internal bazel-bridge error: {0}")]
    Internal(String),
}

impl BazelBridgeError {
    /// Suggested HTTP status code for this error variant, as an integer.
    /// Callers mapping to an HTTP response layer use this.
    #[must_use]
    pub fn http_status(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 404,
            Self::InvalidDigest { .. } | Self::SizeMismatch { .. } => 400,
            Self::DigestMismatch { .. } => 422,
            Self::CrossTenantDenied { .. } => 403,
            Self::AuditFailed(_) => 503,
            Self::BatchTooLarge { .. } => 413,
            Self::Internal(_) => 500,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn http_status_not_found() {
        let e = BazelBridgeError::NotFound {
            tenant: "t".into(),
            hash: "h".into(),
        };
        assert_eq!(e.http_status(), 404);
    }

    #[test]
    fn http_status_invalid_digest() {
        let e = BazelBridgeError::InvalidDigest {
            reason: "bad hex".into(),
        };
        assert_eq!(e.http_status(), 400);
    }

    #[test]
    fn http_status_size_mismatch() {
        let e = BazelBridgeError::SizeMismatch {
            digest_size: 10,
            actual: 5,
        };
        assert_eq!(e.http_status(), 400);
    }

    #[test]
    fn http_status_cross_tenant() {
        let e = BazelBridgeError::CrossTenantDenied {
            caller: "a".into(),
            requested: "b".into(),
        };
        assert_eq!(e.http_status(), 403);
    }

    #[test]
    fn http_status_audit_failed() {
        let e = BazelBridgeError::AuditFailed("sink down".into());
        assert_eq!(e.http_status(), 503);
    }

    #[test]
    fn http_status_batch_too_large() {
        let e = BazelBridgeError::BatchTooLarge {
            requested: 5000,
            cap: 4096,
        };
        assert_eq!(e.http_status(), 413);
    }

    #[test]
    fn http_status_internal() {
        let e = BazelBridgeError::Internal("lock poisoned".into());
        assert_eq!(e.http_status(), 500);
    }

    #[test]
    fn display_includes_key_fields() {
        let e = BazelBridgeError::SizeMismatch {
            digest_size: 100,
            actual: 50,
        };
        let s = e.to_string();
        assert!(s.contains("100"));
        assert!(s.contains("50"));
    }
}

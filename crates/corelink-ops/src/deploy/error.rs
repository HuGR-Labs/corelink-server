//! Error taxonomy for the deploy verifier.
//!
//! Every error variant maps to an HTTP status code per §23 API contract and
//! to a Prometheus label in `corelink_supply_deploy_blocked_total`.
//!
//! The enum is `#[non_exhaustive]` — adding a new variant is a non-breaking
//! change; removing one is a major version bump.

use thiserror::Error;

/// All failure modes from the deploy verify gate.
///
/// Variants are **fail-CLOSED**: any error blocks the deploy and triggers
/// audit emit.  `AuditEmitFailed` additionally fires SEV-1 (audit gap =
/// compliance violation per CTRL-AUDIT-002).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeployVerifyError {
    /// Cosign signature missing from OCI registry or cryptographically invalid.
    ///
    /// HTTP 401 `deploy_verify_failed`.
    #[error("Cosign signature missing or invalid: {0}")]
    SignatureInvalid(String),

    /// Rekor inclusion proof not found for the given image.
    ///
    /// Enforces INV-SUPPLY-PROVENANCE-IN-REKOR (HIGH).  No grace period; no
    /// "Rekor down" bypass (intentional per §19 waiver policy).
    ///
    /// HTTP 401 `deploy_verify_failed`.
    #[error("Rekor inclusion proof missing for image {image}")]
    RekorMissing {
        /// OCI image reference that was looked up.
        image: String,
    },

    /// Fulcio certificate chain not anchored to the sigstore Fulcio root.
    ///
    /// HTTP 401 `deploy_verify_failed`.
    #[error("Fulcio chain invalid: {0}")]
    FulcioChainInvalid(String),

    /// Cosign SAN URI does not match the expected identity pattern.
    ///
    /// Catches fork attacks (WI-S12-003 §2 threat 4).
    ///
    /// HTTP 401 `deploy_verify_failed`.
    #[error("identity mismatch (got {got}, expected pattern {expected})")]
    IdentityMismatch {
        /// Actual SAN URI from the Fulcio certificate.
        got: String,
        /// Expected identity regex pattern.
        expected: String,
    },

    /// OCI registry fetch failed (e.g. ghcr.io 5xx).
    ///
    /// HTTP 503 `oci_unavailable`.
    #[error("OCI registry fetch failed: {0}")]
    OciFetchFailed(String),

    /// Cloudflare API propagation failed (e.g. CF 5xx).
    ///
    /// HTTP 502 `cf_api_unavailable`.
    #[error("Cloudflare API propagation failed: {0}")]
    CfApiFailed(String),

    /// Audit emit failed — **fail-CLOSED**: deploy rejected, SEV-1 fired.
    ///
    /// HTTP 500 `audit_emit_failed`.
    #[error("audit emit failed (fail-CLOSED): {0}")]
    AuditEmitFailed(String),

    /// Image digest binding mismatch — TOCTOU replay attack detected.
    ///
    /// Signature was for a different image digest than the one resolved from
    /// the OCI registry.  HTTP 401 `deploy_verify_failed`.
    #[error(
        "image digest mismatch: signature bound to {signed_digest}, resolved {resolved_digest}"
    )]
    DigestMismatch {
        /// Digest embedded in the Cosign signature.
        signed_digest: String,
        /// Digest resolved from the OCI registry at verify time.
        resolved_digest: String,
    },

    /// Webhook HMAC authentication failed.
    ///
    /// HTTP 401 `webhook_auth_failed`.
    #[error("webhook HMAC authentication failed")]
    WebhookAuthFailed,

    /// Webhook rate limit exceeded (≥ 10 deploys/hour/source).
    ///
    /// HTTP 429 `rate_limit_exceeded`.
    #[error("webhook rate limit exceeded")]
    RateLimitExceeded,
}

impl DeployVerifyError {
    /// HTTP status code for this error (per §23 API contract).
    pub fn http_status(&self) -> u16 {
        match self {
            Self::SignatureInvalid(_)
            | Self::RekorMissing { .. }
            | Self::FulcioChainInvalid(_)
            | Self::IdentityMismatch { .. }
            | Self::DigestMismatch { .. }
            | Self::WebhookAuthFailed => 401,
            Self::OciFetchFailed(_) => 503,
            Self::CfApiFailed(_) => 502,
            Self::AuditEmitFailed(_) => 500,
            Self::RateLimitExceeded => 429,
        }
    }

    /// Prometheus label for `corelink_supply_deploy_blocked_total{reason}`.
    pub fn blocked_reason_label(&self) -> &'static str {
        match self {
            Self::SignatureInvalid(_) | Self::WebhookAuthFailed => "unsigned",
            Self::RekorMissing { .. } => "rekor_missing",
            Self::FulcioChainInvalid(_) => "fulcio_invalid",
            Self::IdentityMismatch { .. } => "identity_mismatch",
            Self::OciFetchFailed(_) => "oci_fetch_failed",
            Self::CfApiFailed(_) => "cf_api_failed",
            Self::AuditEmitFailed(_) => "audit_emit_failed",
            Self::DigestMismatch { .. } => "digest_mismatch",
            Self::RateLimitExceeded => "rate_limit_exceeded",
        }
    }

    /// Returns `true` if this error mandates an SEV-1 alert (audit gap).
    pub fn is_sev1(&self) -> bool {
        matches!(self, Self::AuditEmitFailed(_))
    }

    /// Returns `true` if this error mandates an SEV-2 alert (blocked deploy).
    pub fn is_sev2(&self) -> bool {
        matches!(
            self,
            Self::SignatureInvalid(_)
                | Self::RekorMissing { .. }
                | Self::FulcioChainInvalid(_)
                | Self::IdentityMismatch { .. }
                | Self::DigestMismatch { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_status_mapping_correct() {
        assert_eq!(
            DeployVerifyError::SignatureInvalid("bad".to_string()).http_status(),
            401
        );
        assert_eq!(
            DeployVerifyError::RekorMissing {
                image: "img".to_string()
            }
            .http_status(),
            401
        );
        assert_eq!(
            DeployVerifyError::OciFetchFailed("net".to_string()).http_status(),
            503
        );
        assert_eq!(
            DeployVerifyError::CfApiFailed("cf".to_string()).http_status(),
            502
        );
        assert_eq!(
            DeployVerifyError::AuditEmitFailed("sink".to_string()).http_status(),
            500
        );
        assert_eq!(DeployVerifyError::RateLimitExceeded.http_status(), 429);
    }

    #[test]
    fn sev_classification_correct() {
        assert!(DeployVerifyError::AuditEmitFailed("x".to_string()).is_sev1());
        assert!(!DeployVerifyError::SignatureInvalid("x".to_string()).is_sev1());
        assert!(DeployVerifyError::SignatureInvalid("x".to_string()).is_sev2());
        assert!(DeployVerifyError::RekorMissing {
            image: "i".to_string()
        }
        .is_sev2());
        assert!(!DeployVerifyError::AuditEmitFailed("x".to_string()).is_sev2());
    }

    #[test]
    fn blocked_reason_labels_non_empty() {
        let errors: Vec<DeployVerifyError> = vec![
            DeployVerifyError::SignatureInvalid("x".to_string()),
            DeployVerifyError::RekorMissing {
                image: "i".to_string(),
            },
            DeployVerifyError::FulcioChainInvalid("f".to_string()),
            DeployVerifyError::IdentityMismatch {
                got: "g".to_string(),
                expected: "e".to_string(),
            },
            DeployVerifyError::OciFetchFailed("o".to_string()),
            DeployVerifyError::CfApiFailed("c".to_string()),
            DeployVerifyError::AuditEmitFailed("a".to_string()),
            DeployVerifyError::DigestMismatch {
                signed_digest: "s".to_string(),
                resolved_digest: "r".to_string(),
            },
            DeployVerifyError::WebhookAuthFailed,
            DeployVerifyError::RateLimitExceeded,
        ];
        for e in &errors {
            assert!(
                !e.blocked_reason_label().is_empty(),
                "empty label for {e:?}"
            );
        }
    }
}

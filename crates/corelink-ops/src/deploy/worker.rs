//! Cloudflare Worker entry-point helpers.
//!
//! This module provides the HMAC-SHA256 webhook authentication logic and the
//! HTTP request/response mapping for the `POST /webhook/deploy` endpoint.
//!
//! # Authentication
//!
//! Every incoming webhook must carry an `X-Deploy-Signature` header containing
//! the HMAC-SHA256 hex digest of the raw request body, keyed with the shared
//! `WEBHOOK_SECRET` (rotation quarterly via CTRL-AUTH-014).
//!
//! Timing-safe comparison uses `subtle::ConstantTimeEq` to prevent timing-side-
//! channel attacks.
//!
//! # Rate limiting
//!
//! The entry-point enforces ≤ 10 deploys per hour per source IP (S-08 stub
//! passthrough; full rate-limit integration deferred to later sprint).  Excess
//! requests return HTTP 429 + [`super::error::DeployVerifyError::RateLimitExceeded`].
//!
//! # Wire schema
//!
//! See `specs/_schemas/webhook-v1.json` for the JSON schema.  The CF Worker
//! receives `CfDeployWebhook` deserialized from the request body.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tracing::warn;

use super::error::DeployVerifyError;

type HmacSha256 = Hmac<Sha256>;

// ── HMAC webhook authentication ───────────────────────────────────────────

/// Verify the HMAC-SHA256 signature on an incoming deploy webhook.
///
/// The header value must be the hex-encoded HMAC-SHA256 of the raw request
/// body using `secret` as the key.
///
/// Uses `subtle::ConstantTimeEq` for timing-safe comparison.
///
/// # Errors
///
/// Returns [`DeployVerifyError::WebhookAuthFailed`] if the signature is absent,
/// malformed, or does not match.
///
/// # Example
///
/// ```rust
/// use corelink_ops::deploy::worker::verify_webhook_hmac;
///
/// let secret = b"super-secret-webhook-key";
/// let body = b"{\"release_tag\":\"v0.1.0\"}";
///
/// // Compute expected signature with the same key
/// use hmac::{Hmac, Mac};
/// use sha2::Sha256;
///
/// type HmacSha256 = Hmac<Sha256>;
/// let mut mac = HmacSha256::new_from_slice(secret).unwrap();
/// mac.update(body);
/// let sig = hex::encode(mac.finalize().into_bytes());
///
/// let result = verify_webhook_hmac(body, &sig, secret);
/// assert!(result.is_ok());
/// ```
pub fn verify_webhook_hmac(
    body: &[u8],
    received_hex_sig: &str,
    secret: &[u8],
) -> Result<(), DeployVerifyError> {
    // Decode the received hex signature
    let received_bytes = hex::decode(received_hex_sig).map_err(|_| {
        warn!("webhook HMAC: received signature is not valid hex");
        DeployVerifyError::WebhookAuthFailed
    })?;

    // Compute expected HMAC
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|e| {
        warn!("webhook HMAC: invalid key length: {e}");
        DeployVerifyError::WebhookAuthFailed
    })?;
    mac.update(body);
    let expected_bytes = mac.finalize().into_bytes();

    // Timing-safe comparison
    if expected_bytes.ct_eq(received_bytes.as_slice()).into() {
        Ok(())
    } else {
        warn!("webhook HMAC: signature mismatch");
        Err(DeployVerifyError::WebhookAuthFailed)
    }
}

/// Map a [`DeployVerifyError`] to an HTTP status code and JSON error body.
///
/// Returns `(status_code, json_body)`.  Matches the API contract from
/// WI-S12-003 §23.
pub fn error_to_http_response(err: &DeployVerifyError) -> (u16, String) {
    let status = err.http_status();
    let code = match err {
        DeployVerifyError::SignatureInvalid(_)
        | DeployVerifyError::RekorMissing { .. }
        | DeployVerifyError::FulcioChainInvalid(_)
        | DeployVerifyError::IdentityMismatch { .. }
        | DeployVerifyError::DigestMismatch { .. } => "deploy_verify_failed",
        DeployVerifyError::WebhookAuthFailed => "webhook_auth_failed",
        DeployVerifyError::OciFetchFailed(_) => "oci_unavailable",
        DeployVerifyError::CfApiFailed(_) => "cf_api_unavailable",
        DeployVerifyError::AuditEmitFailed(_) => "audit_emit_failed",
        DeployVerifyError::RateLimitExceeded => "rate_limit_exceeded",
    };
    let body = serde_json::json!({
        "error": code,
        "message": err.to_string(),
    })
    .to_string();
    (status, body)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;

    fn make_valid_sig(body: &[u8], secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("valid key");
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn valid_hmac_passes() {
        let secret = b"test-secret";
        let body = b"payload";
        let sig = make_valid_sig(body, secret);
        assert!(verify_webhook_hmac(body, &sig, secret).is_ok());
    }

    #[test]
    fn wrong_secret_fails() {
        let body = b"payload";
        let sig = make_valid_sig(body, b"correct-secret");
        let result = verify_webhook_hmac(body, &sig, b"wrong-secret");
        assert!(matches!(result, Err(DeployVerifyError::WebhookAuthFailed)));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = b"secret";
        let original = b"original body";
        let sig = make_valid_sig(original, secret);
        let tampered = b"tampered body";
        let result = verify_webhook_hmac(tampered, &sig, secret);
        assert!(matches!(result, Err(DeployVerifyError::WebhookAuthFailed)));
    }

    #[test]
    fn invalid_hex_signature_fails() {
        let result = verify_webhook_hmac(b"body", "not-hex!!!", b"secret");
        assert!(matches!(result, Err(DeployVerifyError::WebhookAuthFailed)));
    }

    #[test]
    fn error_to_http_response_mapping() {
        let (status, _) = error_to_http_response(&DeployVerifyError::SignatureInvalid("x".into()));
        assert_eq!(status, 401);

        let (status, _) = error_to_http_response(&DeployVerifyError::OciFetchFailed("x".into()));
        assert_eq!(status, 503);

        let (status, _) = error_to_http_response(&DeployVerifyError::AuditEmitFailed("x".into()));
        assert_eq!(status, 500);

        let (status, _) = error_to_http_response(&DeployVerifyError::RateLimitExceeded);
        assert_eq!(status, 429);
    }
}

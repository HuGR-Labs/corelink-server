//! HMAC-SHA256 `X-Hub-Signature-256` verification for DT webhook events.
//!
//! Uses constant-time comparison via the [`subtle`] crate to prevent timing
//! side-channel attacks.
//!
//! # Invariant HMAC_REQUIRED
//!
//! Every incoming webhook payload MUST carry a valid `X-Hub-Signature-256`
//! header. Any payload without a valid signature MUST be rejected with HTTP 401
//! and `DtWebhookError::HmacInvalid`.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::types::DtWebhookError;

type HmacSha256 = Hmac<Sha256>;

/// Verify the `X-Hub-Signature-256` header value against the raw request body.
///
/// `header_value` is the full header value string, e.g.
/// `"sha256=abcdef1234..."`.
///
/// # Errors
///
/// Returns [`DtWebhookError::HmacInvalid`] if:
/// - The header does not start with `"sha256="`.
/// - The hex string cannot be decoded.
/// - The computed HMAC does not match (constant-time comparison).
pub fn verify_signature(
    secret: &[u8],
    body: &[u8],
    header_value: &str,
) -> Result<(), DtWebhookError> {
    let hex_sig = header_value
        .strip_prefix("sha256=")
        .ok_or(DtWebhookError::HmacInvalid)?;

    let expected = hex::decode(hex_sig).map_err(|_| DtWebhookError::HmacInvalid)?;

    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| DtWebhookError::HmacInvalid)?;
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    // Constant-time comparison — prevents timing oracle attacks.
    if computed.as_slice().ct_eq(expected.as_slice()).into() {
        Ok(())
    } else {
        Err(DtWebhookError::HmacInvalid)
    }
}

/// Produce a `sha256=<hex>` `X-Hub-Signature-256` header value for `body`.
///
/// Used in tests and mock injection scenarios to generate valid signatures.
pub fn sign(secret: &[u8], body: &[u8]) -> Result<String, DtWebhookError> {
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| DtWebhookError::HmacInvalid)?;
    mac.update(body);
    let result = mac.finalize().into_bytes();
    Ok(format!("sha256={}", hex::encode(result)))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sign_verify() {
        let secret = b"super-secret-rotation-quarterly";
        let body = b"{\"event_type\":\"NEW_VULNERABILITY\"}";
        let sig = sign(secret, body).expect("sign must succeed");
        verify_signature(secret, body, &sig).expect("valid signature must verify");
    }

    #[test]
    fn tampered_body_rejected() {
        let secret = b"super-secret-rotation-quarterly";
        let body = b"original body";
        let sig = sign(secret, body).expect("sign");
        let tampered = b"tampered body";
        let result = verify_signature(secret, tampered, &sig);
        assert!(matches!(result, Err(DtWebhookError::HmacInvalid)));
    }

    #[test]
    fn wrong_secret_rejected() {
        let body = b"payload";
        let sig = sign(b"correct-secret", body).expect("sign");
        let result = verify_signature(b"wrong-secret", body, &sig);
        assert!(matches!(result, Err(DtWebhookError::HmacInvalid)));
    }

    #[test]
    fn missing_prefix_rejected() {
        let result = verify_signature(b"key", b"body", "abcdef");
        assert!(matches!(result, Err(DtWebhookError::HmacInvalid)));
    }

    #[test]
    fn invalid_hex_rejected() {
        let result = verify_signature(b"key", b"body", "sha256=zzzz");
        assert!(matches!(result, Err(DtWebhookError::HmacInvalid)));
    }
}

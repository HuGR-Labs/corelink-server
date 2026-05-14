//! Pure-function Stripe webhook signature verification.
//!
//! Implements the canonical Stripe spec:
//! `Stripe-Signature: t=<unix_ts>,v1=<hex_sig>[,v0=<hex_sig>]`
//!
//! - signed payload = `"{t}.{payload}"`
//! - sig = HMAC-SHA256(signed_payload, secret), hex-encoded
//! - replay window: 5 min (300s) default per spec, configurable via
//!   `tolerance_seconds` argument.
//!
//! Constant-time comparison via [`subtle::ConstantTimeEq`].

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::WebhookVerifyError;

type HmacSha256 = Hmac<Sha256>;

/// Canonical Stripe replay-window tolerance: 5 minutes (300s).
pub const DEFAULT_TOLERANCE_SECONDS: u64 = 300;

/// Verify a Stripe webhook signature.
///
/// # Arguments
///
/// - `payload` — exact request body bytes (BEFORE any deserialization).
/// - `signature_header` — value of `Stripe-Signature` header.
/// - `secret` — webhook signing secret (`whsec_...`).
/// - `now_seconds` — current unix timestamp (seconds). Caller-injected
///   for determinism.
/// - `tolerance_seconds` — max acceptable `|now - t|`. Use
///   [`DEFAULT_TOLERANCE_SECONDS`] for spec-compliant 5 min.
///
/// # Errors
///
/// Returns [`WebhookVerifyError`] on malformed header / replay /
/// future-dated / signature mismatch / hex decode failure. Caller MUST
/// fail-CLOSED (return 401 to Stripe).
pub fn verify_webhook_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &[u8],
    now_seconds: u64,
    tolerance_seconds: u64,
) -> Result<(), WebhookVerifyError> {
    let (ts_seconds, sig_hex_list) = parse_signature_header(signature_header)?;

    // Replay window check (i64 math for symmetric skew).
    let now_i = i64::try_from(now_seconds).unwrap_or(i64::MAX);
    let ts_i = i64::try_from(ts_seconds).unwrap_or(i64::MAX);
    let tol_i = i64::try_from(tolerance_seconds).unwrap_or(i64::MAX);
    let skew = now_i.saturating_sub(ts_i);
    if skew > tol_i {
        return Err(WebhookVerifyError::ReplayWindowExceeded {
            skew_seconds: skew,
        });
    }
    if skew < -tol_i {
        return Err(WebhookVerifyError::FutureDated {
            skew_seconds: skew,
        });
    }

    let expected = compute_signature(secret, ts_seconds, payload);
    let expected_bytes =
        hex::decode(&expected).map_err(|e| WebhookVerifyError::HexDecode(e.to_string()))?;

    // Stripe may attach multiple v1 signatures during key rotation;
    // any match wins.
    for sig_hex in &sig_hex_list {
        let provided = match hex::decode(sig_hex) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if provided.len() == expected_bytes.len()
            && bool::from(provided.ct_eq(&expected_bytes))
        {
            return Ok(());
        }
    }
    Err(WebhookVerifyError::SignatureMismatch)
}

/// Compute HMAC-SHA256 hex signature for `{ts}.{payload}` with `secret`.
#[must_use]
pub fn compute_signature(secret: &[u8], timestamp_seconds: u64, payload: &[u8]) -> String {
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return String::new(), // unreachable in practice
    };
    mac.update(timestamp_seconds.to_string().as_bytes());
    mac.update(b".");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Parse `Stripe-Signature: t=<n>,v1=<hex>[,v1=<hex>...]` into
/// `(timestamp, list_of_v1_signatures)`.
fn parse_signature_header(header: &str) -> Result<(u64, Vec<String>), WebhookVerifyError> {
    let mut ts: Option<u64> = None;
    let mut v1s: Vec<String> = Vec::new();
    for part in header.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("t=") {
            ts = rest.parse::<u64>().ok();
        } else if let Some(rest) = part.strip_prefix("v1=") {
            v1s.push(rest.to_string());
        }
    }
    let ts = ts.ok_or_else(|| WebhookVerifyError::MalformedHeader("missing t=".to_string()))?;
    if v1s.is_empty() {
        return Err(WebhookVerifyError::MalformedHeader(
            "missing v1=".to_string(),
        ));
    }
    Ok((ts, v1s))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn make_header(secret: &[u8], ts: u64, payload: &[u8]) -> String {
        let sig = compute_signature(secret, ts, payload);
        format!("t={ts},v1={sig}")
    }

    #[test]
    fn roundtrip_ok() {
        let secret = b"whsec_unit";
        let payload = b"{\"event\":\"x\"}";
        let ts = 1_700_000_000_u64;
        let header = make_header(secret, ts, payload);
        verify_webhook_signature(payload, &header, secret, ts, DEFAULT_TOLERANCE_SECONDS).unwrap();
    }

    #[test]
    fn replay_window_exceeded() {
        let secret = b"whsec_unit";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let header = make_header(secret, ts, payload);
        // now = ts + 10min (600s > 300s tol).
        let err = verify_webhook_signature(
            payload,
            &header,
            secret,
            ts + 600,
            DEFAULT_TOLERANCE_SECONDS,
        )
        .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::ReplayWindowExceeded { .. }));
    }

    #[test]
    fn future_dated_rejected() {
        let secret = b"whsec_unit";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let header = make_header(secret, ts, payload);
        // now = ts - 600s.
        let err = verify_webhook_signature(
            payload,
            &header,
            secret,
            ts - 600,
            DEFAULT_TOLERANCE_SECONDS,
        )
        .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::FutureDated { .. }));
    }

    #[test]
    fn tampered_payload_rejected() {
        let secret = b"whsec_unit";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let header = make_header(secret, ts, payload);
        let tampered = b"{\"evil\":true}";
        let err =
            verify_webhook_signature(tampered, &header, secret, ts, DEFAULT_TOLERANCE_SECONDS)
                .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::SignatureMismatch));
    }

    #[test]
    fn wrong_secret_rejected() {
        let secret = b"whsec_unit";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let header = make_header(secret, ts, payload);
        let err = verify_webhook_signature(
            payload,
            &header,
            b"whsec_other",
            ts,
            DEFAULT_TOLERANCE_SECONDS,
        )
        .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::SignatureMismatch));
    }

    #[test]
    fn missing_t_rejected() {
        let err =
            verify_webhook_signature(b"{}", "v1=abc", b"s", 100, DEFAULT_TOLERANCE_SECONDS)
                .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::MalformedHeader(_)));
    }

    #[test]
    fn missing_v1_rejected() {
        let err =
            verify_webhook_signature(b"{}", "t=100", b"s", 100, DEFAULT_TOLERANCE_SECONDS)
                .unwrap_err();
        assert!(matches!(err, WebhookVerifyError::MalformedHeader(_)));
    }

    #[test]
    fn multiple_v1_one_matches() {
        let secret = b"whsec_unit";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let good = compute_signature(secret, ts, payload);
        let header = format!("t={ts},v1=0000deadbeef,v1={good}");
        verify_webhook_signature(payload, &header, secret, ts, DEFAULT_TOLERANCE_SECONDS).unwrap();
    }
}

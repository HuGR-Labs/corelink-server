//! Stripe `Stripe-Signature` HMAC-SHA256 verification primitive.
//!
//! ## Canonical formula (per Stripe webhook signature spec)
//!
//! Stripe webhook deliveries carry the canonical
//! `Stripe-Signature: t=<unix_seconds>,v1=<hex_signature>` header. The
//! receiver verifies the payload integrity per the canonical Stripe spec
//! (<https://stripe.com/docs/webhooks/signatures>):
//!
//! 1. Parse the comma-separated header into `t=<ts>` and `v1=<sig>`
//!    fields. Multiple `v1=` fields are allowed (key rotation); ANY
//!    matching v1 verifies. Unknown scheme versions are tolerated
//!    (forward-compat) but never accepted as proof.
//! 2. Compose the signed payload: `signed_payload = "<ts>.<raw_body>"`
//!    (where `<ts>` is the literal Unix-seconds string and `<raw_body>`
//!    is the unparsed JSON byte stream).
//! 3. Compute `expected = HMAC-SHA256(webhook_secret, signed_payload)`.
//! 4. Constant-time compare `expected` against each `v1=` candidate
//!    using [`subtle::ConstantTimeEq`]. The constant-time discipline
//!    prevents an adversary from learning the secret via timing-attack
//!    side-channels.
//! 5. Reject if `(now - ts) > 300s` (canonical 5-minute skew window
//!    per Stripe spec; defends against signature capture + delayed
//!    replay attacks).
//!
//! ## Why `subtle::ConstantTimeEq` (NOT `==`)
//!
//! The standard library's `==` for `&[u8]` short-circuits on the first
//! byte mismatch — an adversary measuring the time delta between
//! `verify(prefix_match)` and `verify(no_prefix_match)` could
//! incrementally recover bytes of the expected MAC. Constant-time
//! comparison removes the timing oracle by always reading every byte
//! before returning. The `subtle` crate is the canonical Rust crate for
//! constant-time primitives + is already a workspace dependency.
//!
//! ## Why 5-minute skew (not 1-minute, not 1-hour)
//!
//! Per Stripe's canonical webhook signature documentation: 5 minutes is
//! the recommended tolerance window. Shorter windows reject legitimate
//! webhooks during clock drift (NTP sync delays / leap seconds);
//! longer windows widen the replay-attack horizon. WI-S10-003 §6.1.5
//! pins the canonical `300_000` ms (5 min × 60 s × 1000 ms/s) boundary
//! per the sprint contract §15 R-007 mitigation row.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::StripeError;

/// Canonical 5-min replay tolerance window in milliseconds (Stripe
/// webhook signature spec recommendation).
pub const REPLAY_WINDOW_MS: u64 = 300_000;

/// Parsed [`StripeSignatureHeader`] yields the timestamp + every `v1=`
/// signature candidate. Multiple `v1=` fields are allowed for key
/// rotation; the verifier accepts the request if ANY candidate
/// constant-time matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripeSignatureHeader {
    /// Stripe-side wall-clock instant (Unix epoch SECONDS) parsed from
    /// the `t=` field. Conversion to ms is done by the caller for
    /// arithmetic against `now_ms`.
    pub timestamp_seconds: u64,
    /// One or more `v1=<hex>` candidate signatures. Key rotation may
    /// produce multiple candidates; the verifier accepts ANY constant-
    /// time match.
    pub v1_signatures: Vec<[u8; 32]>,
}

impl StripeSignatureHeader {
    /// Parse a canonical `Stripe-Signature` header value.
    ///
    /// Accepted form: `t=<unix_seconds>,v1=<hex>[,v1=<hex>…]` with
    /// optional unknown scheme fields tolerated (forward-compat).
    /// Whitespace between fields is tolerated (the canonical Stripe
    /// header has none, but the spec permits LWS).
    ///
    /// # Errors
    ///
    /// - [`StripeError::SignatureRejected`] when the header is empty,
    ///   missing the `t=` field, missing every `v1=` field, contains a
    ///   non-numeric timestamp, or contains a malformed v1 hex.
    pub fn parse(header: &str) -> Result<Self, StripeError> {
        if header.is_empty() {
            return Err(StripeError::SignatureRejected(
                "empty Stripe-Signature header".to_string(),
            ));
        }
        let mut timestamp_seconds: Option<u64> = None;
        let mut v1_signatures: Vec<[u8; 32]> = Vec::new();
        for part in header.split(',') {
            let part = part.trim();
            if let Some(t_value) = part.strip_prefix("t=") {
                let parsed: u64 = t_value.parse().map_err(|e| {
                    StripeError::SignatureRejected(format!(
                        "Stripe-Signature t= field is not a u64: {e}"
                    ))
                })?;
                timestamp_seconds = Some(parsed);
            } else if let Some(sig_hex) = part.strip_prefix("v1=") {
                let bytes = hex::decode(sig_hex).map_err(|e| {
                    StripeError::SignatureRejected(format!(
                        "Stripe-Signature v1= hex decode failed: {e}"
                    ))
                })?;
                if bytes.len() != 32 {
                    return Err(StripeError::SignatureRejected(format!(
                        "Stripe-Signature v1= expected 32 bytes; got {}",
                        bytes.len()
                    )));
                }
                let mut sig = [0u8; 32];
                sig.copy_from_slice(&bytes);
                v1_signatures.push(sig);
            }
            // Unknown scheme fields tolerated for forward-compat (Stripe
            // may add v2/v3 schemes in the future). The verifier rejects
            // the whole header if NO recognized field landed.
        }
        let timestamp_seconds = timestamp_seconds.ok_or_else(|| {
            StripeError::SignatureRejected(
                "Stripe-Signature missing t= field".to_string(),
            )
        })?;
        if v1_signatures.is_empty() {
            return Err(StripeError::SignatureRejected(
                "Stripe-Signature missing every v1= field".to_string(),
            ));
        }
        Ok(Self {
            timestamp_seconds,
            v1_signatures,
        })
    }
}

/// Compute the canonical Stripe HMAC-SHA256 over the
/// `<timestamp_seconds>.<payload>` signed-payload form. Returns the
/// 32-byte tag.
///
/// # Errors
///
/// - [`StripeError::Internal`] when the underlying HMAC primitive
///   rejects the secret (`hmac::Hmac::new_from_slice` accepts arbitrary
///   key length per RFC 2104; this arm is structurally unreachable for
///   well-formed input but the defensive guard keeps the trait surface
///   typed).
pub fn compute_signature(
    webhook_secret: &[u8],
    timestamp_seconds: u64,
    payload: &[u8],
) -> Result<[u8; 32], StripeError> {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(webhook_secret).map_err(|e| {
        StripeError::Internal(format!("HMAC key length rejected: {e}"))
    })?;
    let ts_str = timestamp_seconds.to_string();
    mac.update(ts_str.as_bytes());
    mac.update(b".");
    mac.update(payload);
    let tag = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&tag);
    Ok(out)
}

/// Verify a Stripe-Signature header against the raw webhook payload +
/// shared secret + receiver wall-clock instant.
///
/// Returns `Ok(())` on success; on rejection returns one of
/// `StripeError::SignatureRejected` (HMAC mismatch / malformed header)
/// or `StripeError::SignatureSkewRejected` (timestamp delta exceeds
/// canonical 5-min tolerance).
///
/// ## 5-min skew tolerance (canonical Stripe webhook spec)
///
/// The verifier rejects if `delta_ms > REPLAY_WINDOW_MS` where
/// `delta_ms = |now_ms - signature_ts_ms|`. Stripe's documentation pins
/// the canonical 5-min tolerance against signature capture + delayed
/// replay; WI-S10-003 §6.1.5 preserves the spec exactly.
///
/// ## Constant-time comparison rationale
///
/// The verifier uses [`subtle::ConstantTimeEq::ct_eq`] for the
/// `expected ?== claimed` check. A naive `==` comparison short-circuits
/// on the first byte mismatch — an adversary timing the receiver could
/// recover bytes of the expected MAC byte-by-byte (well-known
/// timing-attack technique against authentication codes). The canonical
/// `subtle` crate guarantees the comparison reads every byte before
/// returning, removing the timing oracle.
///
/// # Errors
///
/// - [`StripeError::SignatureRejected`] when the header is malformed OR
///   no `v1=` candidate constant-time matches the recomputed HMAC tag.
/// - [`StripeError::SignatureSkewRejected`] when the timestamp delta
///   exceeds the canonical 5-min replay window.
/// - [`StripeError::Internal`] if the HMAC primitive rejects the secret
///   (structurally unreachable for well-formed input).
pub fn verify_stripe_signature(
    header: &str,
    payload: &[u8],
    webhook_secret: &[u8],
    now_ms: u64,
) -> Result<(), StripeError> {
    let parsed = StripeSignatureHeader::parse(header)?;

    // Recompute the canonical HMAC tag over `<ts>.<payload>`.
    let expected = compute_signature(webhook_secret, parsed.timestamp_seconds, payload)?;

    // Constant-time match against EVERY v1= candidate (key rotation
    // tolerance). A single ct_eq match accepts.
    let mut any_match: u8 = 0;
    for candidate in &parsed.v1_signatures {
        let eq = expected.ct_eq(candidate);
        any_match |= eq.unwrap_u8();
    }
    if any_match == 0 {
        return Err(StripeError::SignatureRejected(
            "no v1= candidate matches the recomputed HMAC-SHA256 tag (constant-time compare via subtle::ConstantTimeEq)".to_string(),
        ));
    }

    // 5-min skew window check (per Stripe spec). Convert seconds → ms
    // for arithmetic against `now_ms`. Saturating ops guard against
    // u64 overflow at cosmic timescales (signature_ts_ms × 1000 ≤
    // u64::MAX for any signature_ts_ms ≤ ~5.8e8 years from epoch).
    let signature_ts_ms = parsed.timestamp_seconds.saturating_mul(1000);
    let delta_ms = if now_ms >= signature_ts_ms {
        now_ms.saturating_sub(signature_ts_ms)
    } else {
        signature_ts_ms.saturating_sub(now_ms)
    };
    if delta_ms > REPLAY_WINDOW_MS {
        return Err(StripeError::SignatureSkewRejected {
            now_ms,
            signature_ts_ms,
            delta_ms,
        });
    }

    Ok(())
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

    const TEST_SECRET: &[u8] = b"whsec_test_secret_canonical";

    fn build_valid_header(payload: &[u8], ts_seconds: u64) -> String {
        let tag = compute_signature(TEST_SECRET, ts_seconds, payload).unwrap();
        format!("t={},v1={}", ts_seconds, hex::encode(tag))
    }

    #[test]
    fn replay_window_constant_pinned_to_5min() {
        assert_eq!(REPLAY_WINDOW_MS, 5 * 60 * 1000);
    }

    #[test]
    fn parse_rejects_empty_header() {
        let err = StripeSignatureHeader::parse("").unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn parse_rejects_missing_t() {
        let err = StripeSignatureHeader::parse("v1=ab").unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn parse_rejects_missing_v1() {
        let err = StripeSignatureHeader::parse("t=12345").unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn parse_rejects_non_numeric_t() {
        let err = StripeSignatureHeader::parse("t=oops,v1=ab").unwrap_err();
        let s = format!("{err}");
        assert!(s.contains("not a u64"));
    }

    #[test]
    fn parse_rejects_malformed_v1_hex() {
        let err = StripeSignatureHeader::parse("t=1,v1=zz").unwrap_err();
        let s = format!("{err}");
        assert!(s.contains("hex decode"));
    }

    #[test]
    fn parse_rejects_v1_wrong_length() {
        let err = StripeSignatureHeader::parse("t=1,v1=ab").unwrap_err();
        let s = format!("{err}");
        assert!(s.contains("32 bytes"));
    }

    #[test]
    fn parse_accepts_canonical_form() {
        let h = format!("t=1700000000,v1={}", "ab".repeat(32));
        let p = StripeSignatureHeader::parse(&h).unwrap();
        assert_eq!(p.timestamp_seconds, 1_700_000_000);
        assert_eq!(p.v1_signatures.len(), 1);
    }

    #[test]
    fn parse_accepts_multiple_v1_for_key_rotation() {
        let h = format!("t=1,v1={},v1={}", "ab".repeat(32), "cd".repeat(32));
        let p = StripeSignatureHeader::parse(&h).unwrap();
        assert_eq!(p.v1_signatures.len(), 2);
    }

    #[test]
    fn parse_tolerates_unknown_scheme_fields() {
        let h = format!("t=1,v0=legacy,v1={},v2=future", "ab".repeat(32));
        let p = StripeSignatureHeader::parse(&h).unwrap();
        assert_eq!(p.v1_signatures.len(), 1);
    }

    #[test]
    fn compute_signature_deterministic() {
        let payload = b"{\"id\":\"evt_1\"}";
        let s1 = compute_signature(TEST_SECRET, 1_700_000_000, payload).unwrap();
        let s2 = compute_signature(TEST_SECRET, 1_700_000_000, payload).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn compute_signature_diverges_on_payload_change() {
        let s1 = compute_signature(TEST_SECRET, 1, b"a").unwrap();
        let s2 = compute_signature(TEST_SECRET, 1, b"b").unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn compute_signature_diverges_on_timestamp_change() {
        let s1 = compute_signature(TEST_SECRET, 1, b"a").unwrap();
        let s2 = compute_signature(TEST_SECRET, 2, b"a").unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn compute_signature_diverges_on_secret_change() {
        let s1 = compute_signature(b"secret_a", 1, b"a").unwrap();
        let s2 = compute_signature(b"secret_b", 1, b"a").unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn verify_accepts_fresh_canonical_signature() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts);
        let now_ms = ts.saturating_mul(1000);
        let result = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms);
        assert!(result.is_ok(), "expected Ok; got {result:?}");
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts);
        let tampered = b"{\"id\":\"evt_evil\"}";
        let now_ms = ts.saturating_mul(1000);
        let err = verify_stripe_signature(&header, tampered, TEST_SECRET, now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn verify_rejects_tampered_signature() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let mut tag = compute_signature(TEST_SECRET, ts, payload).unwrap();
        tag[0] ^= 0xFF;
        let header = format!("t={},v1={}", ts, hex::encode(tag));
        let now_ms = ts.saturating_mul(1000);
        let err = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts);
        let now_ms = ts.saturating_mul(1000);
        let err = verify_stripe_signature(&header, payload, b"wrong_secret", now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }

    #[test]
    fn verify_accepts_at_exact_5min_boundary() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts);
        // now exactly 5 min after ts (boundary inclusive).
        let now_ms = ts.saturating_mul(1000).saturating_add(REPLAY_WINDOW_MS);
        let result = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms);
        assert!(result.is_ok(), "5min boundary should be accepted; got {result:?}");
    }

    #[test]
    fn verify_rejects_just_past_5min_boundary() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts);
        // now = ts + 5min + 1ms.
        let now_ms = ts
            .saturating_mul(1000)
            .saturating_add(REPLAY_WINDOW_MS)
            .saturating_add(1);
        let err = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureSkewRejected { .. }));
    }

    #[test]
    fn verify_rejects_future_timestamp_past_5min() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        // signature_ts is far in the future (clock drift attack).
        let ts = 1_700_000_000_u64.saturating_add(REPLAY_WINDOW_MS / 1000 + 1);
        let header = build_valid_header(payload, ts);
        let now_ms = 1_700_000_000_u64.saturating_mul(1000);
        let err = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureSkewRejected { .. }));
    }

    #[test]
    fn verify_accepts_future_timestamp_within_5min() {
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts_seconds = 1_700_000_000_u64.saturating_add(60); // 1 minute future
        let header = build_valid_header(payload, ts_seconds);
        let now_ms = 1_700_000_000_u64.saturating_mul(1000);
        let result = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms);
        assert!(result.is_ok(), "1min future drift should be accepted; got {result:?}");
    }

    #[test]
    fn verify_with_multiple_v1_accepts_any_match() {
        let payload = b"payload";
        let ts = 1_700_000_000_u64;
        let valid_tag = compute_signature(TEST_SECRET, ts, payload).unwrap();
        let header = format!(
            "t={ts},v1={},v1={}",
            "ab".repeat(32),                  // bogus
            hex::encode(valid_tag)            // canonical
        );
        let now_ms = ts.saturating_mul(1000);
        let result = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_with_only_bogus_v1_rejects() {
        let payload = b"payload";
        let ts = 1_700_000_000_u64;
        let header = format!("t={ts},v1={},v1={}", "ab".repeat(32), "cd".repeat(32));
        let now_ms = ts.saturating_mul(1000);
        let err = verify_stripe_signature(&header, payload, TEST_SECRET, now_ms).unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
    }
}

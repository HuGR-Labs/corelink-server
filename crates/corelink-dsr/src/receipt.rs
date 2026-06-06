//! JWT receipt issuer trait + in-memory deterministic fake.
//!
//! ## Why JWT (RS256) for the receipt
//!
//! Per WI-S11-001 §1: the canonical receipt is a JWT compact-form
//! token signed RS256 with claims `request_id` (UUIDv7) +
//! `data_subject_id` (pseudonymous tenant id) + `request_kind` +
//! `submitted_at_ms` + `expires_at_ms` (90d cap per
//! [`crate::event::RECEIPT_EXPIRY_DAYS`]).
//!
//! The customer stores this token as **proof of submission**
//! verifiable post-facto. Production wiring at WI-S11-008 binds the
//! real RS256 sign / verify path via `jsonwebtoken = 9.3` (reused from
//! `corelink-clerk` S-03 inheritance) + a KMS-backed key rotation
//! (HKDF info=`corelink/v1/dsr-receipt` per security_model.md §7.2 +
//! key_management.md §2). The trait surface here ships a
//! deterministic in-memory fake so the orchestrator can pin every
//! load-bearing invariant the production binding relies on
//! (signature roundtrip, expiration cap, claim integrity, tenant
//! isolation).
//!
//! ## In-memory fake construction
//!
//! The fake emits a 3-segment compact form (header.claims.signature)
//! with:
//!
//! - `header` = base64url-no-pad of canonical
//!   `{"alg":"RS256","typ":"JWT","kid":"<key_id>"}`.
//! - `claims` = base64url-no-pad of canonical JSON claims.
//! - `signature` = base64url-no-pad of `derive_tag(header || "." ||
//!   claims, key_bytes)` where `derive_tag` is a deterministic
//!   in-memory mixing function (NOT cryptographically secure; the
//!   real wiring at WI-S11-008 substitutes RS256). The mixing
//!   function uses the canonical FNV-1a 64-bit primer per
//!   `derive_tag` so the in-memory fake is byte-deterministic across
//!   runs.
//!
//! The verify path at [`InMemoryJwtReceiptIssuer::verify`] re-derives
//! the canonical tag + constant-time-compares against the inbound
//! signature segment; a mismatch surfaces as
//! [`crate::error::DsrReceiptError::SignatureInvalid`]. Expiration is
//! checked separately against the canonical `exp` claim.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DsrReceiptError;
use crate::event::{DsrJurisdiction, DsrRequest, DsrRequestKind, RECEIPT_EXPIRY_DAYS};

/// Canonical JWT signing algorithm. Pinned for cross-component
/// regression tests + dashboard widget grouping.
pub const RECEIPT_ALG_RS256: &str = "RS256";

/// Canonical JWT issuer claim. Pinned for the canonical
/// `iss` claim per WI-S11-001 §1.
pub const RECEIPT_ISSUER: &str = "corelink.humangr.com/privacy";

/// JWT receipt token (compact form: `header.claims.signature`). The
/// trait surface treats the bytes as opaque; the verify surface
/// re-parses + verifies via [`JwtReceiptIssuer::verify`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JwtReceiptToken {
    /// The canonical compact-form bytes (3-segment base64url-no-pad).
    bytes: String,
}

impl JwtReceiptToken {
    /// Construct a synthetic JWT receipt token from a deterministic
    /// test tag. Production wiring at WI-S11-008 NEVER constructs via
    /// this path — the real surface emits via the canonical
    /// `jsonwebtoken::encode` call.
    #[must_use]
    pub fn synthetic_for_test(tag: impl Into<String>) -> Self {
        Self { bytes: tag.into() }
    }

    /// Borrow the canonical compact-form bytes.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.bytes
    }

    /// Whether this token's bytes are non-empty.
    #[must_use]
    pub fn is_non_empty(&self) -> bool {
        !self.bytes.is_empty()
    }
}

impl core::fmt::Display for JwtReceiptToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.bytes)
    }
}

/// Canonical JWT receipt claims (the canonical `claims` segment of
/// the compact-form receipt). Per WI-S11-001 §1:
///
/// - `iss` = canonical [`RECEIPT_ISSUER`] (`corelink.humangr.com/privacy`).
/// - `request_id` = canonical UUIDv7 (the idempotency ledger key).
/// - `data_subject_id` = canonical UUIDv7 (PAT principal post-authn).
/// - `request_kind` = canonical 6-arm enum.
/// - `jurisdiction` = canonical 3-arm enum.
/// - `submitted_at_ms` = wall-clock instant of the canonical
///   submission.
/// - `expires_at_ms` = `submitted + 90d` per
///   [`RECEIPT_EXPIRY_DAYS`] cap (anti-replay).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsrReceipt {
    /// Canonical issuer claim (`iss`).
    pub iss: String,
    /// Canonical UUIDv7 request id.
    pub request_id: Uuid,
    /// Canonical UUIDv7 data subject id (PAT principal post-authn).
    /// Per CTRL-PRIV-014 the audit boundary stores
    /// `sha256(subject_id || tenant_salt)` not the raw id; the JWT
    /// receipt itself carries the raw UUID because the customer is
    /// the legitimate holder of their own id.
    pub data_subject_id: Uuid,
    /// Canonical 6-arm request kind taxonomy.
    pub request_kind: DsrRequestKind,
    /// Canonical 3-arm jurisdiction taxonomy.
    pub jurisdiction: DsrJurisdiction,
    /// Wall-clock instant of the canonical submission (Unix epoch ms).
    pub submitted_at_ms: u64,
    /// JWT exp claim (= submitted + 90d per [`RECEIPT_EXPIRY_DAYS`]).
    pub expires_at_ms: u64,
}

impl DsrReceipt {
    /// Construct a fresh receipt with the canonical 90d expiration
    /// cap. Per WI-S11-001 §1 anti-replay invariant.
    #[must_use]
    pub fn new(request: &DsrRequest) -> Self {
        let expires_at_ms = request
            .submitted_at_ms
            .saturating_add((RECEIPT_EXPIRY_DAYS as u64) * 86_400_000);
        Self {
            iss: RECEIPT_ISSUER.to_string(),
            request_id: request.request_id,
            data_subject_id: request.data_subject_id,
            request_kind: request.request_kind,
            jurisdiction: request.jurisdiction,
            submitted_at_ms: request.submitted_at_ms,
            expires_at_ms,
        }
    }

    /// Whether this receipt has expired at `now_ms`.
    #[must_use]
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

/// JWT receipt issuer trait. Production wiring at WI-S11-008 composes
/// the canonical `jsonwebtoken = 9.3` RS256 sign / verify path
/// (reused from `corelink-clerk` S-03 inheritance) + KMS-backed key
/// rotation (HKDF info=`corelink/v1/dsr-receipt`).
pub trait JwtReceiptIssuer: Send + Sync + core::fmt::Debug {
    /// Sign the canonical receipt claims producing the compact-form
    /// JWT token. The trait surface enforces:
    ///
    /// - The canonical `expires_at_ms` claim equals
    ///   `submitted_at_ms + 90d` per [`RECEIPT_EXPIRY_DAYS`].
    /// - The canonical `iss` claim equals [`RECEIPT_ISSUER`].
    ///
    /// # Errors
    ///
    /// Returns [`DsrReceiptError::Signing`] when the underlying key
    /// is unavailable / KMS fetch failed / rotation in progress.
    fn issue(&self, receipt: &DsrReceipt) -> Result<JwtReceiptToken, DsrReceiptError>;

    /// Verify the canonical compact-form JWT token. Returns the
    /// parsed claims on success; an expired token surfaces as
    /// [`DsrReceiptError::Expired`]; a tampered signature surfaces as
    /// [`DsrReceiptError::SignatureInvalid`]; a malformed token
    /// surfaces as [`DsrReceiptError::Malformed`].
    ///
    /// The customer SDK calls this verify path post-facto to confirm
    /// their proof of submission is still valid + the canonical
    /// `request_id` / `submitted_at_ms` / `expires_at_ms` are
    /// untampered.
    ///
    /// # Errors
    ///
    /// Returns [`DsrReceiptError::SignatureInvalid`] /
    /// [`DsrReceiptError::Expired`] /
    /// [`DsrReceiptError::Malformed`] per the canonical contract.
    fn verify(&self, token: &JwtReceiptToken, now_ms: u64) -> Result<DsrReceipt, DsrReceiptError>;
}

/// Deterministic in-memory JWT receipt issuer. Pins the canonical
/// JWT roundtrip contract (sign + verify produces the same claims
/// byte-for-byte) so the orchestrator's idempotency arm exercises the
/// same fail-CLOSED envelope discipline as the production wiring.
#[derive(Clone, Debug)]
pub struct InMemoryJwtReceiptIssuer {
    key_id: String,
    key_bytes: std::sync::Arc<Vec<u8>>,
    issued: std::sync::Arc<Mutex<u64>>,
}

impl Default for InMemoryJwtReceiptIssuer {
    fn default() -> Self {
        Self::new("dsr-receipt-key-v1", b"corelink-dsr-receipt-key-v1-fake")
    }
}

impl InMemoryJwtReceiptIssuer {
    /// Construct a fresh in-memory issuer keyed by `(key_id,
    /// key_bytes)`. The bytes are mixed into the canonical
    /// signature derivation so two distinct issuers produce mutually-
    /// invalid tokens (cross-key reject pinned by the canonical
    /// `prop_jwt_receipt_verifies_post_facto` property test).
    #[must_use]
    pub fn new(key_id: impl Into<String>, key_bytes: &[u8]) -> Self {
        Self {
            key_id: key_id.into(),
            key_bytes: std::sync::Arc::new(key_bytes.to_vec()),
            issued: std::sync::Arc::new(Mutex::new(0)),
        }
    }

    /// Number of receipts issued by this issuer.
    #[must_use]
    pub fn issued_count(&self) -> u64 {
        match self.issued.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        }
    }

    /// Borrow the canonical key id.
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    fn header_segment(&self) -> String {
        let header = format!(
            r#"{{"alg":"{RECEIPT_ALG_RS256}","typ":"JWT","kid":"{}"}}"#,
            self.key_id
        );
        b64url_encode(header.as_bytes())
    }

    fn claims_segment(receipt: &DsrReceipt) -> Result<String, DsrReceiptError> {
        let bytes = serde_json::to_vec(receipt)
            .map_err(|e| DsrReceiptError::Signing(format!("claims serialize: {e}")))?;
        Ok(b64url_encode(&bytes))
    }

    fn signature_segment(&self, signing_input: &str) -> String {
        let tag = derive_tag(signing_input.as_bytes(), &self.key_bytes);
        b64url_encode(&tag)
    }
}

impl JwtReceiptIssuer for InMemoryJwtReceiptIssuer {
    fn issue(&self, receipt: &DsrReceipt) -> Result<JwtReceiptToken, DsrReceiptError> {
        // Anti-replay: enforce the canonical 90d expiration cap at
        // sign time. A receipt with exp ≠ submitted + 90d is a
        // contract violation (rejected at the issuer boundary).
        let expected_exp = receipt
            .submitted_at_ms
            .saturating_add((RECEIPT_EXPIRY_DAYS as u64) * 86_400_000);
        if receipt.expires_at_ms != expected_exp {
            return Err(DsrReceiptError::Signing(format!(
                "exp claim must equal submitted + {RECEIPT_EXPIRY_DAYS}d (anti-replay cap)"
            )));
        }
        if receipt.iss != RECEIPT_ISSUER {
            return Err(DsrReceiptError::Signing(format!(
                "iss claim must equal canonical {RECEIPT_ISSUER}"
            )));
        }

        let header = self.header_segment();
        let claims = Self::claims_segment(receipt)?;
        let signing_input = format!("{header}.{claims}");
        let signature = self.signature_segment(&signing_input);
        let token = format!("{signing_input}.{signature}");

        match self.issued.lock() {
            Ok(mut g) => {
                *g = g.saturating_add(1);
            }
            Err(p) => {
                // counter is purely observational (not load-bearing);
                // a poisoned mutex is recovered via into_inner so the
                // canonical issuance still proceeds.
                let mut g = p.into_inner();
                *g = g.saturating_add(1);
            }
        }

        Ok(JwtReceiptToken { bytes: token })
    }

    fn verify(&self, token: &JwtReceiptToken, now_ms: u64) -> Result<DsrReceipt, DsrReceiptError> {
        let s = token.as_str();
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(DsrReceiptError::Malformed(format!(
                "expected 3 segments; got {}",
                parts.len()
            )));
        }
        let header_b64 = parts.first().copied().unwrap_or("");
        let claims_b64 = parts.get(1).copied().unwrap_or("");
        let signature_b64 = parts.get(2).copied().unwrap_or("");

        // Recompute the canonical signature over header.claims and
        // constant-time-compare against the inbound segment.
        let signing_input = format!("{header_b64}.{claims_b64}");
        let expected_sig = self.signature_segment(&signing_input);
        if !constant_time_eq(expected_sig.as_bytes(), signature_b64.as_bytes()) {
            return Err(DsrReceiptError::SignatureInvalid);
        }

        let claims_bytes = b64url_decode(claims_b64)
            .map_err(|e| DsrReceiptError::Malformed(format!("claims base64url: {e}")))?;
        let receipt: DsrReceipt = serde_json::from_slice(&claims_bytes)
            .map_err(|e| DsrReceiptError::Malformed(format!("claims json: {e}")))?;

        // Expiration check (anti-replay).
        if receipt.is_expired(now_ms) {
            return Err(DsrReceiptError::Expired);
        }
        Ok(receipt)
    }
}

/// Deterministic 16-byte tag derivation. Mixes the canonical
/// `signing_input` bytes with `key_bytes` via FNV-1a 64-bit (twice;
/// disjoint primers) so cross-key tokens are mutually-invalid by
/// construction. NOT cryptographically secure; production wiring at
/// WI-S11-008 substitutes RS256 via `jsonwebtoken = 9.3`.
///
/// The function is byte-deterministic across runs (same input →
/// same tag) so the canonical `prop_jwt_receipt_verifies_post_facto`
/// property test pins the roundtrip without RNG dependence.
fn derive_tag(signing_input: &[u8], key_bytes: &[u8]) -> Vec<u8> {
    const FNV_OFFSET_A: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_OFFSET_B: u64 = 0x14d2_a85e_e3a3_4d83;
    const FNV_PRIME: u64 = 0x100_0000_01b3;

    let mut a: u64 = FNV_OFFSET_A;
    let mut b: u64 = FNV_OFFSET_B;
    for byte in signing_input {
        a ^= u64::from(*byte);
        a = a.wrapping_mul(FNV_PRIME);
        b ^= u64::from(*byte).wrapping_shl(3);
        b = b.wrapping_mul(FNV_PRIME);
    }
    for byte in key_bytes {
        a ^= u64::from(*byte).wrapping_shl(5);
        a = a.wrapping_mul(FNV_PRIME);
        b ^= u64::from(*byte);
        b = b.wrapping_mul(FNV_PRIME);
    }
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&a.to_be_bytes());
    out.extend_from_slice(&b.to_be_bytes());
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (xa, xb) in a.iter().zip(b.iter()) {
        diff |= xa ^ xb;
    }
    diff == 0
}

// Minimal base64url-no-pad encoder/decoder (RFC 4648 §5). Pinned
// inline so the trait surface has zero dep beyond uuid + serde +
// thiserror; production wiring at WI-S11-008 substitutes the
// audited `base64 = 0.22` workspace dep.
const B64URL_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

// 6-bit canonical lookup; the input is always masked with `& 0x3f`
// at the call site so the table read is structurally in-bounds.
// `.get(...).copied().unwrap_or(b'A')` avoids the `indexing_slicing`
// clippy lint while preserving the canonical "in-bounds by mask"
// invariant — the fallback `b'A'` is unreachable because the mask
// caps the value at 63 < 64 = table length.
fn b64url_lookup(masked_six_bit: u32) -> char {
    char::from(
        B64URL_TABLE
            .get((masked_six_bit & 0x3f) as usize)
            .copied()
            .unwrap_or(b'A'),
    )
}

fn b64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().saturating_mul(4) / 3 + 4);
    let chunks = input.chunks_exact(3);
    let remainder = chunks.remainder().to_vec();
    for chunk in chunks {
        let a = chunk.first().copied().unwrap_or(0);
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        let n: u32 = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        out.push(b64url_lookup(n >> 18));
        out.push(b64url_lookup(n >> 12));
        out.push(b64url_lookup(n >> 6));
        out.push(b64url_lookup(n));
    }
    match remainder.len() {
        1 => {
            let a = remainder.first().copied().unwrap_or(0);
            let n: u32 = u32::from(a) << 16;
            out.push(b64url_lookup(n >> 18));
            out.push(b64url_lookup(n >> 12));
        }
        2 => {
            let a = remainder.first().copied().unwrap_or(0);
            let b = remainder.get(1).copied().unwrap_or(0);
            let n: u32 = (u32::from(a) << 16) | (u32::from(b) << 8);
            out.push(b64url_lookup(n >> 18));
            out.push(b64url_lookup(n >> 12));
            out.push(b64url_lookup(n >> 6));
        }
        _ => {}
    }
    out
}

fn b64url_decode(input: &str) -> Result<Vec<u8>, String> {
    fn decode_char(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'-' => Ok(62),
            b'_' => Ok(63),
            _ => Err(format!("invalid base64url char: {c}")),
        }
    }
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = Vec::with_capacity(len * 3 / 4);
    let mut i = 0;
    while i + 4 <= len {
        let c0 = decode_char(bytes.get(i).copied().unwrap_or(b'A'))?;
        let c1 = decode_char(bytes.get(i + 1).copied().unwrap_or(b'A'))?;
        let c2 = decode_char(bytes.get(i + 2).copied().unwrap_or(b'A'))?;
        let c3 = decode_char(bytes.get(i + 3).copied().unwrap_or(b'A'))?;
        let n: u32 =
            (u32::from(c0) << 18) | (u32::from(c1) << 12) | (u32::from(c2) << 6) | u32::from(c3);
        out.push(((n >> 16) & 0xff) as u8);
        out.push(((n >> 8) & 0xff) as u8);
        out.push((n & 0xff) as u8);
        i += 4;
    }
    let remaining = len - i;
    match remaining {
        0 => {}
        2 => {
            let c0 = decode_char(bytes.get(i).copied().unwrap_or(b'A'))?;
            let c1 = decode_char(bytes.get(i + 1).copied().unwrap_or(b'A'))?;
            let n: u32 = (u32::from(c0) << 18) | (u32::from(c1) << 12);
            out.push(((n >> 16) & 0xff) as u8);
        }
        3 => {
            let c0 = decode_char(bytes.get(i).copied().unwrap_or(b'A'))?;
            let c1 = decode_char(bytes.get(i + 1).copied().unwrap_or(b'A'))?;
            let c2 = decode_char(bytes.get(i + 2).copied().unwrap_or(b'A'))?;
            let n: u32 = (u32::from(c0) << 18) | (u32::from(c1) << 12) | (u32::from(c2) << 6);
            out.push(((n >> 16) & 0xff) as u8);
            out.push(((n >> 8) & 0xff) as u8);
        }
        _ => return Err(format!("invalid base64url length remainder: {remaining}")),
    }
    Ok(out)
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

    fn fresh_request() -> DsrRequest {
        DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Erasure,
            DsrJurisdiction::Lgpd,
            1_000_000_000_000,
        )
    }

    #[test]
    fn dsr_receipt_new_caps_at_90d() {
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        assert_eq!(
            receipt.expires_at_ms,
            r.submitted_at_ms + (RECEIPT_EXPIRY_DAYS as u64) * 86_400_000
        );
        assert_eq!(receipt.iss, RECEIPT_ISSUER);
    }

    #[test]
    fn dsr_receipt_is_expired_pin() {
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        assert!(!receipt.is_expired(receipt.expires_at_ms - 1));
        assert!(receipt.is_expired(receipt.expires_at_ms));
        assert!(receipt.is_expired(receipt.expires_at_ms + 1));
    }

    #[test]
    fn issue_then_verify_roundtrips() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        let token = issuer.issue(&receipt).unwrap();
        let verified = issuer.verify(&token, r.submitted_at_ms + 1).unwrap();
        assert_eq!(verified, receipt);
        assert_eq!(issuer.issued_count(), 1);
    }

    #[test]
    fn issue_rejects_wrong_exp_claim() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let r = fresh_request();
        let mut receipt = DsrReceipt::new(&r);
        receipt.expires_at_ms = r.submitted_at_ms + 86_400_000; // 1d not 90d
        let err = issuer.issue(&receipt).unwrap_err();
        assert!(matches!(err, DsrReceiptError::Signing(_)));
    }

    #[test]
    fn issue_rejects_wrong_iss_claim() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let r = fresh_request();
        let mut receipt = DsrReceipt::new(&r);
        receipt.iss = "evil.example/privacy".to_string();
        let err = issuer.issue(&receipt).unwrap_err();
        assert!(matches!(err, DsrReceiptError::Signing(_)));
    }

    #[test]
    fn verify_rejects_tampered_token() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        let token = issuer.issue(&receipt).unwrap();
        // Tamper one char in the signature segment.
        let s = token.as_str();
        let parts: Vec<&str> = s.split('.').collect();
        let mut tampered_sig = parts[2].to_string();
        let last = tampered_sig.pop().unwrap_or('A');
        let new_last = if last == 'A' { 'B' } else { 'A' };
        tampered_sig.push(new_last);
        let tampered = format!("{}.{}.{}", parts[0], parts[1], tampered_sig);
        let tampered_token = JwtReceiptToken { bytes: tampered };
        let err = issuer
            .verify(&tampered_token, r.submitted_at_ms + 1)
            .unwrap_err();
        assert!(matches!(err, DsrReceiptError::SignatureInvalid));
    }

    #[test]
    fn verify_rejects_expired_token() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        let token = issuer.issue(&receipt).unwrap();
        let err = issuer
            .verify(&token, receipt.expires_at_ms + 1)
            .unwrap_err();
        assert!(matches!(err, DsrReceiptError::Expired));
    }

    #[test]
    fn verify_rejects_malformed_token() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        let bad = JwtReceiptToken {
            bytes: "abc.def".to_string(),
        };
        let err = issuer.verify(&bad, 0).unwrap_err();
        assert!(matches!(err, DsrReceiptError::Malformed(_)));
    }

    #[test]
    fn verify_rejects_cross_key_token() {
        let issuer_a = InMemoryJwtReceiptIssuer::new("kid-a", b"key-bytes-a");
        let issuer_b = InMemoryJwtReceiptIssuer::new("kid-b", b"key-bytes-b");
        let r = fresh_request();
        let receipt = DsrReceipt::new(&r);
        let token_a = issuer_a.issue(&receipt).unwrap();
        let err = issuer_b
            .verify(&token_a, r.submitted_at_ms + 1)
            .unwrap_err();
        assert!(matches!(err, DsrReceiptError::SignatureInvalid));
    }

    #[test]
    fn b64url_roundtrip_basic() {
        let cases: &[&[u8]] = &[
            &[],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            b"\x00\x01\x02\x03",
        ];
        for c in cases {
            let enc = b64url_encode(c);
            let dec = b64url_decode(&enc).unwrap();
            assert_eq!(&dec[..], *c);
        }
    }

    #[test]
    fn constant_time_eq_pin() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn synthetic_token_helpers_pin() {
        let t = JwtReceiptToken::synthetic_for_test("foo");
        assert_eq!(t.as_str(), "foo");
        assert!(t.is_non_empty());
        let empty = JwtReceiptToken::synthetic_for_test("");
        assert!(!empty.is_non_empty());
    }

    #[test]
    fn issuer_key_id_pinned() {
        let issuer = InMemoryJwtReceiptIssuer::default();
        assert_eq!(issuer.key_id(), "dsr-receipt-key-v1");
    }

    #[test]
    fn alg_constant_pinned() {
        assert_eq!(RECEIPT_ALG_RS256, "RS256");
        assert_eq!(RECEIPT_ISSUER, "corelink.humangr.com/privacy");
    }
}

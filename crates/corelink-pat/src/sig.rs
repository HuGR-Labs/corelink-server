//! HMAC-SHA256 fast-fail layer for the canonical PAT format.
//!
//! Per `auth_model.md §2.3` step 3a (cycle 9 SEAL decision (a)):
//! validation **MUST** verify the truncated HMAC signature segment
//! BEFORE any DB lookup or Argon2id verify. The sig defends:
//!
//! - **DDoS via random-token spam**: each Argon2id verify costs 64
//!   MiB RAM + ~250ms CPU; rejecting an unsigned random blob in
//!   ≤100µs reduces attacker leverage by ~6 orders of magnitude.
//! - **Phishing: fake corelink prefixes**: a credible-looking
//!   `corelink_pat_…` value that lacks a valid HMAC sig fails
//!   instantly without surfacing a verify-failure metric that an
//!   attacker can use to enumerate tenants.
//!
//! # Constant-time discipline
//!
//! - HMAC compute uses the audited `hmac` crate (constant-time per
//!   the `block-buffer` + `digest` pipeline).
//! - Comparison is via [`subtle::ConstantTimeEq`] on the exact
//!   16-byte truncated MAC; lengths are byte-equal by construction
//!   so there is no length-based oracle.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::PatError;
use crate::format::PAT_HMAC_SIG_RAW_LEN;
use crate::types::PatSigningKey;

type HmacSha256 = Hmac<Sha256>;

/// Compute the canonical 16-byte truncated HMAC-SHA256 signature
/// over `<token_id>.<random_secret>` (the bytes returned by
/// [`crate::format::PatPlaintextParts::hmac_preimage`]).
pub fn compute_hmac_sig(key: &PatSigningKey, preimage: &[u8]) -> [u8; PAT_HMAC_SIG_RAW_LEN] {
    // `Hmac::new_from_slice` only fails on impossible key length
    // bounds; the pipeline accepts any key length but our
    // `PatSigningKey::from_bytes` already rejects < 32 bytes upstream.
    // We re-route the (provably unreachable) error path back through
    // a fixed dummy MAC rather than `expect!()` so the strict lint
    // profile holds.
    let mac = match HmacSha256::new_from_slice(key.as_bytes()) {
        Ok(m) => m,
        Err(_) => {
            // Provably unreachable: `Hmac::new_from_slice` never
            // returns Err for SHA-256. Surface a deterministic
            // non-matching MAC so the verify path falls through to
            // `InvalidPat` cleanly.
            return [0u8; PAT_HMAC_SIG_RAW_LEN];
        }
    };
    let result = mac.chain_update(preimage).finalize().into_bytes();
    let mut sig = [0u8; PAT_HMAC_SIG_RAW_LEN];
    // Copy the leading PAT_HMAC_SIG_RAW_LEN bytes; finalize returns 32.
    for (out, b) in sig.iter_mut().zip(result.iter().take(PAT_HMAC_SIG_RAW_LEN)) {
        *out = *b;
    }
    sig
}

/// Constant-time verify the truncated HMAC signature segment
/// against the expected value computed from the preimage.
pub fn verify_hmac_sig(
    key: &PatSigningKey,
    preimage: &[u8],
    sig_bytes: &[u8],
) -> Result<(), PatError> {
    if sig_bytes.len() != PAT_HMAC_SIG_RAW_LEN {
        return Err(PatError::Malformed);
    }
    let expected = compute_hmac_sig(key, preimage);
    if bool::from(expected.ct_eq(sig_bytes)) {
        Ok(())
    } else {
        Err(PatError::InvalidPat)
    }
}

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
    verify_hmac_sig_multi(std::slice::from_ref(key), preimage, sig_bytes)
}

/// Constant-time verify the truncated HMAC signature against an
/// **overlap key set** — the canonical key plus any rotation
/// predecessor/successor (`key_management.md §3.2.1`, 24h overlap).
///
/// A PAT minted under any key in `keys` validates, so an operator can
/// rotate `PAT_SIGNING_KEY` (incident response, scheduled rotation)
/// while old-key tokens stay valid through the overlap window —
/// instead of an instant fleet-wide auth outage.
///
/// # Constant-time discipline
///
/// Every key in the (small, fixed) set is evaluated; the loop does
/// **not** early-return on the first match. The per-key result is
/// folded into a single accumulator via [`subtle::Choice`] so the
/// observable latency does not leak *which* key matched (which would
/// reveal whether a token is on the old vs. new key during rotation).
///
/// # Fail-closed
///
/// An empty `keys` set returns [`PatError::InvalidPat`] — no key set
/// bound ⇒ nothing verifies.
pub fn verify_hmac_sig_multi(
    keys: &[PatSigningKey],
    preimage: &[u8],
    sig_bytes: &[u8],
) -> Result<(), PatError> {
    if sig_bytes.len() != PAT_HMAC_SIG_RAW_LEN {
        return Err(PatError::Malformed);
    }
    // Fail-closed: an absent key set cannot validate anything.
    if keys.is_empty() {
        return Err(PatError::InvalidPat);
    }
    // Fold over the full set without early-return; OR the per-key
    // constant-time comparisons so neither the match position nor the
    // matching-key identity leaks via timing.
    let mut matched = subtle::Choice::from(0u8);
    for key in keys {
        let expected = compute_hmac_sig(key, preimage);
        matched |= expected.ct_eq(sig_bytes);
    }
    if bool::from(matched) {
        Ok(())
    } else {
        Err(PatError::InvalidPat)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]
mod fold_tests {
    use super::*;
    use crate::types::PatSigningKey;

    fn key(b: u8) -> PatSigningKey {
        PatSigningKey::from_bytes(vec![b; 32]).expect("32-byte key")
    }

    /// Mutation guard (cargo-mutants): the per-key fold in `verify_hmac_sig_multi`
    /// MUST be `|=` (OR), never `^=` (XOR). A key set containing the SAME matching
    /// key twice must still validate — under XOR the two matches cancel
    /// (`1 ^ 1 = 0`) and a valid PAT would be wrongly rejected. This pins the OR
    /// semantics (and the real property: a duplicated key never breaks validation).
    #[test]
    fn multi_fold_is_or_duplicate_matching_key_validates() {
        let k = key(0x42);
        let preimage = b"token_id.random_secret";
        let sig = compute_hmac_sig(&k, preimage);
        assert!(
            verify_hmac_sig_multi(&[k.clone(), k.clone()], preimage, &sig).is_ok(),
            "duplicate matching key must validate (OR-fold, not XOR)"
        );
    }

    /// A single matching key among non-matching ones validates; an all-miss set
    /// rejects. (Complements the dup-key OR guard above.)
    #[test]
    fn multi_fold_matches_one_of_many_and_rejects_none() {
        let good = key(0x11);
        let preimage = b"abc.def";
        let sig = compute_hmac_sig(&good, preimage);
        assert!(verify_hmac_sig_multi(&[key(0x22), good.clone(), key(0x33)], preimage, &sig).is_ok());
        assert!(verify_hmac_sig_multi(&[key(0x22), key(0x33)], preimage, &sig).is_err());
    }
}

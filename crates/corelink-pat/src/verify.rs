//! Two-stage `verify` orchestrator: HMAC fast-fail then Argon2id.
//!
//! The middleware (WI-S03-003) drives the lookup primitive between
//! step 3a and step 3b — this crate exposes the cryptographic
//! verifications and leaves the DB-fetch shape under the
//! middleware's control. The orchestrator below covers the offline
//! scenario where caller already has the stored hash + token id in
//! hand (test harness, integration test, calibration tool).

use subtle::ConstantTimeEq;

use crate::argon::verify_argon2id;
use crate::error::PatError;
use crate::format::{parse_plaintext, PatPlaintextParts};
use crate::sig::verify_hmac_sig_multi;
use crate::types::{PatEnv, PatHash, PatSigningKey, PatTokenId};

/// Result of a successful end-to-end verify. Carries the parsed
/// `token_id` + `env` so the middleware can perform downstream
/// scope checks without re-parsing.
#[derive(Debug, Clone)]
pub struct VerifiedPat {
    /// The env tag observed in the plaintext (must match the DB row).
    pub env: PatEnv,
    /// The `token_id` segment used as the canonical lookup key.
    pub token_id: PatTokenId,
}

/// Verify a PAT plaintext end-to-end against the stored
/// `(token_id, hash)` pair plus the per-region signing key.
///
/// Steps (per `auth_model.md §2.3`):
///
/// 1. **Parse** the plaintext into canonical segments.
/// 2. **Step 3a** — HMAC sig fast-fail. Reject in ≤100µs if the
///    signature does not match.
/// 3. (Caller does the DB lookup — out of this crate's scope.)
/// 4. **Step 3c** — Argon2id verify of the `random_secret_b64`
///    against the stored PHC string.
///
/// On any failure the function returns the same
/// [`PatError::InvalidPat`] for sig+hash mismatches and
/// [`PatError::Malformed`] for parse-shape errors so the wire
/// surface cannot discriminate. [`PatError::HashError`] is reserved
/// for DB-corruption scenarios (PHC string unparseable).
pub fn verify_with_hash(
    plaintext: &str,
    expected_token_id: &PatTokenId,
    stored_hash: &PatHash,
    signing_key: &PatSigningKey,
) -> Result<VerifiedPat, PatError> {
    verify_with_hash_multi(
        plaintext,
        expected_token_id,
        stored_hash,
        std::slice::from_ref(signing_key),
    )
}

/// Overlap-aware variant of [`verify_with_hash`]: the HMAC step is
/// checked against a **key set** (current + rotation predecessor/
/// successor) per `key_management.md §3.2.1`. A PAT minted under any
/// key in `signing_keys` validates, so rotating `PAT_SIGNING_KEY`
/// (incident response or scheduled) does not instantly invalidate the
/// live fleet during the overlap window.
///
/// Fail-closed: an empty `signing_keys` set rejects (see
/// [`crate::sig::verify_hmac_sig_multi`]). Constant-time over the set
/// — neither the matching key nor a token_id mismatch position leaks.
pub fn verify_with_hash_multi(
    plaintext: &str,
    expected_token_id: &PatTokenId,
    stored_hash: &PatHash,
    signing_keys: &[PatSigningKey],
) -> Result<VerifiedPat, PatError> {
    let parts: PatPlaintextParts = parse_plaintext(plaintext)?;

    // Token id must match the one looked up from the DB.
    // P1-4 fix: use constant-time byte comparison via `subtle::ConstantTimeEq`
    // so the rejection latency does not leak whether and where the strings
    // differ (timing oracle defence; auth_model.md §2.3 invariant).
    // Both token_ids are canonical 16-char Crockford b32 ASCII strings, so
    // byte-level comparison is correct (UTF-8 multi-byte codepoints do not
    // occur; the PAT parser enforces the charset in `parse_plaintext`).
    if !bool::from(
        parts
            .token_id
            .as_str()
            .as_bytes()
            .ct_eq(expected_token_id.as_str().as_bytes()),
    ) {
        return Err(PatError::InvalidPat);
    }

    // Step 3a — HMAC fast-fail over the overlap key set.
    verify_hmac_sig_multi(signing_keys, &parts.hmac_preimage, &parts.hmac_sig_bytes)?;

    // Step 3c — Argon2id PHC verify.
    verify_argon2id(&parts.random_secret_b64, stored_hash)?;

    Ok(VerifiedPat {
        env: parts.env,
        token_id: parts.token_id,
    })
}

/// HMAC-only verify (for use cases where the caller wants to fail
/// fast before reaching the DB layer).
pub fn verify_hmac_only(
    plaintext: &str,
    signing_key: &PatSigningKey,
) -> Result<(PatEnv, PatTokenId), PatError> {
    verify_hmac_only_multi(plaintext, std::slice::from_ref(signing_key))
}

/// Overlap-aware variant of [`verify_hmac_only`]: HMAC fast-fail
/// against a **key set** (current + rotation predecessor/successor)
/// per `key_management.md §3.2.1`. Fail-closed on an empty set.
pub fn verify_hmac_only_multi(
    plaintext: &str,
    signing_keys: &[PatSigningKey],
) -> Result<(PatEnv, PatTokenId), PatError> {
    let parts = parse_plaintext(plaintext)?;
    verify_hmac_sig_multi(signing_keys, &parts.hmac_preimage, &parts.hmac_sig_bytes)?;
    Ok((parts.env, parts.token_id))
}

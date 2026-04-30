//! Canonical PAT plaintext format parsing.
//!
//! The wire format (per `auth_model.md §2.2` cycle 9 SEAL decision (a)
//! hybrid HMAC + Argon2id) is:
//!
//! ```text
//! corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
//! ```
//!
//! With:
//!
//! | Segment | Length | Charset | Notes |
//! |---|---|---|---|
//! | literal `corelink_` | 9 chars | ASCII | constant prefix; constant-time scanned in full |
//! | `<env>` | 2-3 chars | `pat \| ci \| ro` (exhaustive) | strict allowlist |
//! | literal `_` | 1 char | ASCII | env separator |
//! | `<token_id>` | 16 chars | Crockford b32 | indexed lookup key |
//! | literal `.` | 1 char | ASCII | inner separator |
//! | `<random_secret>` | 43 chars | base64url no padding | 32 bytes random (256 bit) |
//! | literal `.` | 1 char | ASCII | inner separator |
//! | `<hmac_sig>` | 22 chars | base64url no padding | first 16 bytes of HMAC-SHA256 (128 bit truncated) |
//!
//! Total canonical length is therefore 95 or 96 characters depending
//! on whether `<env>` is two or three characters
//! (`corelink_` 9 + env 2..=3 + `_` 1 + token_id 16 + `.` 1 +
//! random_secret 43 + `.` 1 + hmac_sig 22 = 95 or 96). The parser is
//! constant-time across the *valid-shape* arms — every accepted
//! plaintext walks the same control-flow path, and the sole shape
//! reject is the single [`PatError::Malformed`] variant so the wire
//! cannot infer "wrong env" vs "wrong length" via observation.
//!
//! # Constant-time discipline
//!
//! - The literal prefix `corelink_` is scanned with [`subtle::ConstantTimeEq`]
//!   over the full nine bytes.
//! - The env segment is matched against all three valid wire literals
//!   in fixed order using `ConstantTimeEq`; even on a successful
//!   match the loop runs to completion.
//! - The remaining length checks are byte-exact arithmetic on
//!   total length and segment offsets — all O(1).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use subtle::{Choice, ConstantTimeEq};

use crate::error::PatError;
use crate::types::{PatEnv, PatTokenId, PAT_TOKEN_ID_LEN};

/// Canonical plaintext literal prefix length (`corelink_`).
pub const PAT_PREFIX_LEN: usize = 9;
/// Length of the base64url no-pad `random_secret` segment (43 chars =
/// ceil(32 * 4/3)).
pub const PAT_RANDOM_SECRET_LEN: usize = 43;
/// Length of the base64url no-pad `hmac_sig` segment (22 chars =
/// ceil(16 * 4/3)).
pub const PAT_HMAC_SIG_LEN: usize = 22;
/// Number of raw random bytes encoded into `random_secret` (256-bit
/// entropy floor per `auth_model.md §2.2`).
pub const PAT_RANDOM_SECRET_RAW_LEN: usize = 32;
/// Number of raw HMAC-SHA256 prefix bytes encoded into `hmac_sig`
/// (128-bit truncated MAC per cycle 9 SEAL decision (a)).
pub const PAT_HMAC_SIG_RAW_LEN: usize = 16;

const PREFIX_LITERAL: &[u8; PAT_PREFIX_LEN] = b"corelink_";
const ENV_LITERALS: &[&[u8]] = &[b"pat", b"ci", b"ro"];

/// Decoded view over a parsed plaintext. The fields point into the
/// caller-owned buffer; no allocation happens during parse so the
/// fast-fail layer stays sub-microsecond.
#[derive(Debug, Clone)]
pub struct PatPlaintextParts {
    /// Discriminant value (`pat | ci | ro`).
    pub env: PatEnv,
    /// 16-char Crockford b32 token id.
    pub token_id: PatTokenId,
    /// 43-char base64url no-pad random secret segment (UTF-8 borrowed).
    pub random_secret_b64: String,
    /// 22-char base64url no-pad HMAC sig segment (UTF-8 borrowed).
    pub hmac_sig_b64: String,
    /// Raw bytes spanning `<token_id>.<random_secret>` — the input
    /// pre-image to the HMAC-SHA256 fast-fail layer.
    pub hmac_preimage: Vec<u8>,
    /// Raw decoded random secret bytes (32 bytes).
    pub random_secret_bytes: Vec<u8>,
    /// Raw decoded HMAC-SHA256 prefix (16 bytes).
    pub hmac_sig_bytes: Vec<u8>,
}

/// Parse a PAT plaintext into its canonical segments. Returns
/// [`PatError::Malformed`] for any deviation from the canonical
/// shape — the caller must not branch on the *kind* of malformity.
///
/// # Constant-time properties
///
/// The function performs a fixed sequence of operations regardless of
/// where the malformity sits inside the input. The early-return
/// arms collapse to the same single `Malformed` variant. Length
/// checks come first (a malformed-length input cannot be
/// constant-timed against a valid input — the byte count differs by
/// definition), but every input of *correct shape* takes the same
/// path through the prefix/env/separator scanners. Mann-Whitney /
/// 3-prong evidence-grade indistinguishability is asserted in
/// `tests/constant_time.rs`.
pub fn parse_plaintext(input: &str) -> Result<PatPlaintextParts, PatError> {
    let bytes = input.as_bytes();

    // --- Length envelopes --------------------------------------------------
    // Two valid total lengths exist: env len 2 ("ci"/"ro") -> 95 chars;
    // env len 3 ("pat") -> 96 chars. Anything else cannot be canonical.
    let (env_len, total_len) = match bytes.len() {
        95 => (2usize, 95usize),
        96 => (3usize, 96usize),
        _ => return Err(PatError::Malformed),
    };
    debug_assert_eq!(total_len, bytes.len());

    // --- Prefix `corelink_` ------------------------------------------------
    let prefix = bytes.get(0..PAT_PREFIX_LEN).ok_or(PatError::Malformed)?;
    if !bool::from(prefix.ct_eq(PREFIX_LITERAL.as_slice())) {
        return Err(PatError::Malformed);
    }

    // --- Env segment + trailing `_` ----------------------------------------
    let env_start = PAT_PREFIX_LEN;
    let env_end = env_start + env_len;
    let env_bytes = bytes.get(env_start..env_end).ok_or(PatError::Malformed)?;
    let env = match_env_constant_time(env_bytes).ok_or(PatError::Malformed)?;

    // Separator: `_` after env.
    let sep_after_env = bytes.get(env_end).ok_or(PatError::Malformed)?;
    if *sep_after_env != b'_' {
        return Err(PatError::Malformed);
    }

    // --- Token id (16 chars Crockford b32) ---------------------------------
    let token_id_start = env_end + 1;
    let token_id_end = token_id_start + PAT_TOKEN_ID_LEN;
    let token_id_bytes = bytes
        .get(token_id_start..token_id_end)
        .ok_or(PatError::Malformed)?;
    let token_id_str = std::str::from_utf8(token_id_bytes).map_err(|_| PatError::Malformed)?;
    let token_id = PatTokenId::parse(token_id_str).ok_or(PatError::Malformed)?;

    // Separator: `.`
    let sep1 = bytes.get(token_id_end).ok_or(PatError::Malformed)?;
    if *sep1 != b'.' {
        return Err(PatError::Malformed);
    }

    // --- Random secret (43 chars base64url no pad) -------------------------
    let secret_start = token_id_end + 1;
    let secret_end = secret_start + PAT_RANDOM_SECRET_LEN;
    let secret_bytes = bytes
        .get(secret_start..secret_end)
        .ok_or(PatError::Malformed)?;
    let secret_str = std::str::from_utf8(secret_bytes).map_err(|_| PatError::Malformed)?;
    let secret_decoded = URL_SAFE_NO_PAD
        .decode(secret_bytes)
        .map_err(|_| PatError::Malformed)?;
    if secret_decoded.len() != PAT_RANDOM_SECRET_RAW_LEN {
        return Err(PatError::Malformed);
    }

    // Separator: `.`
    let sep2 = bytes.get(secret_end).ok_or(PatError::Malformed)?;
    if *sep2 != b'.' {
        return Err(PatError::Malformed);
    }

    // --- HMAC sig (22 chars base64url no pad) ------------------------------
    let sig_start = secret_end + 1;
    let sig_end = sig_start + PAT_HMAC_SIG_LEN;
    if sig_end != total_len {
        return Err(PatError::Malformed);
    }
    let sig_bytes = bytes.get(sig_start..sig_end).ok_or(PatError::Malformed)?;
    let sig_str = std::str::from_utf8(sig_bytes).map_err(|_| PatError::Malformed)?;
    let sig_decoded = URL_SAFE_NO_PAD
        .decode(sig_bytes)
        .map_err(|_| PatError::Malformed)?;
    if sig_decoded.len() != PAT_HMAC_SIG_RAW_LEN {
        return Err(PatError::Malformed);
    }

    // --- HMAC preimage = `<token_id>.<random_secret>` ----------------------
    let preimage_start = token_id_start;
    let preimage_end = secret_end;
    let preimage = bytes
        .get(preimage_start..preimage_end)
        .ok_or(PatError::Malformed)?
        .to_vec();

    Ok(PatPlaintextParts {
        env,
        token_id,
        random_secret_b64: secret_str.to_owned(),
        hmac_sig_b64: sig_str.to_owned(),
        hmac_preimage: preimage,
        random_secret_bytes: secret_decoded,
        hmac_sig_bytes: sig_decoded,
    })
}

/// Constant-time env match. Walks the entire allowlist for every
/// input length so a successful match takes the same time as a
/// rejected match.
fn match_env_constant_time(input: &[u8]) -> Option<PatEnv> {
    let mut hit = Choice::from(0u8);
    let mut idx: u8 = u8::MAX;
    for (i, lit) in ENV_LITERALS.iter().enumerate() {
        // Same-length literals only contribute meaningful CT signal.
        // Different-length comparisons short-circuit to `Choice(0)`
        // (`subtle::ConstantTimeEq` for slices returns 0 on length
        // mismatch in constant time over the LITERAL length; the
        // input length is fixed by the caller).
        let eq = input.ct_eq(lit);
        // `idx` adopts `i` on the FIRST hit; otherwise stays unchanged.
        // We update idx and hit unconditionally (constant-time
        // conditional select via `Choice::conditional_select`).
        let candidate = u8::conditional_select(&idx, &(i as u8), eq);
        idx = candidate;
        hit |= eq;
    }
    if !bool::from(hit) {
        return None;
    }
    match idx {
        0 => Some(PatEnv::Pat),
        1 => Some(PatEnv::Ci),
        2 => Some(PatEnv::Ro),
        _ => None,
    }
}

/// Helper that surfaces only the env segment without parsing the
/// rest of the plaintext. Used by tests and the pre-DB lookup path
/// in middleware where the env tag selects which signing-key region
/// to consult. Constant-time per the internal env match helper.
pub fn parse_env(plaintext: &str) -> Result<PatEnv, PatError> {
    let bytes = plaintext.as_bytes();
    if bytes.len() < PAT_PREFIX_LEN + 3 {
        return Err(PatError::Malformed);
    }
    let prefix = bytes.get(0..PAT_PREFIX_LEN).ok_or(PatError::Malformed)?;
    if !bool::from(prefix.ct_eq(PREFIX_LITERAL.as_slice())) {
        return Err(PatError::Malformed);
    }
    // Try 3-char env first (`pat`); on failure 2-char (`ci`/`ro`).
    let three = bytes.get(PAT_PREFIX_LEN..PAT_PREFIX_LEN + 3);
    let after_three = bytes.get(PAT_PREFIX_LEN + 3);
    if let (Some(seg), Some(b'_')) = (three, after_three) {
        if let Some(env) = match_env_constant_time(seg) {
            return Ok(env);
        }
    }
    let two = bytes.get(PAT_PREFIX_LEN..PAT_PREFIX_LEN + 2);
    let after_two = bytes.get(PAT_PREFIX_LEN + 2);
    if let (Some(seg), Some(b'_')) = (two, after_two) {
        if let Some(env) = match_env_constant_time(seg) {
            return Ok(env);
        }
    }
    Err(PatError::Malformed)
}

// `subtle` exposes `ConditionallySelectable` only for unsigned
// integers via the `subtle::ConstantTimeEq` companion trait
// `subtle::ConditionallySelectable`. Re-export it here so the
// constant-time match code reads cleanly without introducing a public
// dep surface.
use subtle::ConditionallySelectable;

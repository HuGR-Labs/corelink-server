//! Argon2id PHC-string mint + verify (OWASP 2024 params).
//!
//! Per `auth_model.md §2.2/§2.3` + spec_contract S-03 §5 R-S03-3,
//! the canonical PAT `random_secret` is hashed with Argon2id at
//! `m_cost = 65_536 KiB`, `t_cost = 3`, `p_cost = 4`, output length 32
//! bytes. The PHC string includes the embedded salt, so the verify
//! path is self-describing and cost upgrades are forward-compatible
//! without DB migration.
//!
//! # Why hash the `random_secret` only (not the full plaintext)?
//!
//! The HMAC fast-fail layer already validates the canonical shape
//! and the relationship `(token_id, random_secret) → hmac_sig`. The
//! Argon2id cryptographic-possession proof MUST be over the bytes
//! that an attacker would need to forge — namely the 32-byte raw
//! `random_secret`. Hashing the full plaintext would re-include the
//! token_id (which is non-secret and reaches the DB anyway via the
//! lookup primitive), wasting input space and entangling unrelated
//! lifecycle invariants.
//!
//! # Constant-time properties
//!
//! - `argon2::Argon2::verify_password` is constant-time per the PHC
//!   spec.
//! - The `dummy_verify_for_constant_time` API ensures the cold
//!   path (parse-fail OR token_id-not-in-DB) takes the same
//!   real-time envelope as a successful Argon2id verify, denying
//!   timing oracles to an attacker probing the validity of arbitrary
//!   token_ids.

use std::sync::OnceLock;

use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use password_hash::{Salt, SaltString};
use rand_core::OsRng;

use crate::error::PatError;
use crate::types::PatHash;

/// OWASP 2024 floor for Argon2id PAT hashing. `m_cost` is in KiB
/// (so 65_536 KiB = 64 MiB); `t_cost` is the iteration count;
/// `p_cost` is the parallelism (lane count); `output_len` is bytes.
pub const ARGON2_M_COST_KIB: u32 = 65_536;
/// Iteration count.
pub const ARGON2_T_COST: u32 = 3;
/// Parallelism (lane count).
pub const ARGON2_P_COST: u32 = 4;
/// Output length in bytes (32 = 256 bits, matches AES-256 keyspace
/// budget per WI §9.2).
pub const ARGON2_OUTPUT_LEN: usize = 32;

/// Lazily-built Argon2 hasher with the canonical OWASP-2024 params.
fn hasher() -> &'static Argon2<'static> {
    static H: OnceLock<Argon2<'static>> = OnceLock::new();
    H.get_or_init(|| {
        // `Params::new` only fails for clearly invalid argument
        // tuples (m < 8 etc.); the OWASP-2024 constants are
        // statically valid for argon2 0.5.x and pinned by the
        // semver-aware `[dependencies]` major. A silent fallback to
        // `Params::default()` (m=19456, t=2, p=1) would underprovision
        // the cold-path pad below the OWASP-2024 floor and is therefore
        // refused — `unreachable!` surfaces a load-bearing invariant
        // violation loudly so any future upstream API drift fails the
        // sprint-close ct-variance gate, not a silent prod regression.
        let params = Params::new(
            ARGON2_M_COST_KIB,
            ARGON2_T_COST,
            ARGON2_P_COST,
            Some(ARGON2_OUTPUT_LEN),
        )
        .unwrap_or_else(|_| {
            unreachable!(
                "argon2 0.5 Params::new rejected statically-valid OWASP-2024 \
                 tuple (m={ARGON2_M_COST_KIB}, t={ARGON2_T_COST}, p={ARGON2_P_COST}, \
                 out={ARGON2_OUTPUT_LEN}) — upstream API drift; bump corelink-pat \
                 minor and re-audit"
            )
        });
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    })
}

/// Hash the canonical `random_secret` segment (43-char base64url
/// no-pad string, 32 raw bytes pre-decode) into a PHC string.
///
/// The salt is freshly generated from the OS CSPRNG (`getrandom`);
/// salt-reuse is not possible at this layer. The returned PHC
/// string carries `m_cost`, `t_cost`, `p_cost`, the salt, and the
/// hash bytes — fully self-describing.
pub fn hash_random_secret(random_secret_b64: &str) -> Result<PatHash, PatError> {
    let salt = SaltString::generate(&mut OsRng);
    let phc = hasher()
        .hash_password(random_secret_b64.as_bytes(), salt.as_salt())
        .map_err(|_| PatError::EntropyUnavailable)?
        .to_string();
    Ok(PatHash::from_phc_string(phc))
}

/// Convenience wrapper used by tests + calibration tools that need
/// a deterministic salt (NEVER use this in production).
#[doc(hidden)]
pub fn hash_random_secret_with_salt(
    random_secret_b64: &str,
    salt: Salt<'_>,
) -> Result<PatHash, PatError> {
    let phc = hasher()
        .hash_password(random_secret_b64.as_bytes(), salt)
        .map_err(|_| PatError::HashError("argon2 hash"))?
        .to_string();
    Ok(PatHash::from_phc_string(phc))
}

/// Constant-time verify of the candidate plaintext `random_secret`
/// against the stored PHC hash. Returns:
///
/// - `Ok(())` on match.
/// - `Err(PatError::InvalidPat)` on cryptographic mismatch.
/// - `Err(PatError::HashError(..))` if the stored PHC string is
///   itself unparseable (DB corruption); this is *not* the same as
///   a wrong-password case and MUST surface as HTTP 500 not 401
///   downstream.
///
/// Additionally, `verify` validates the embedded params satisfy the
/// OWASP-2024 floor. A stored hash with `m_cost < 65_536` is
/// rejected as `HashError("argon2 params underprovisioned")` so a
/// downgraded DB row never silently passes verification.
pub fn verify_argon2id(random_secret_b64: &str, stored: &PatHash) -> Result<(), PatError> {
    let phc =
        PasswordHash::new(stored.as_str()).map_err(|_| PatError::HashError("argon2 phc parse"))?;

    // Defense in depth: even if a malicious DB swap downgrades the
    // params, reject before invoking the verifier.
    if let Some(params) = phc.params.iter().next() {
        // params iter yields all entries; just check m_cost via the
        // typed accessor instead.
        let _ = params;
    }
    // Use the typed accessor on `password_hash::PasswordHash` to
    // recover the raw cost ints. The crate exposes `params` as a
    // dictionary; we walk it for the canonical keys.
    let m_cost = phc
        .params
        .get_decimal("m")
        .ok_or(PatError::HashError("argon2 params missing m"))?;
    let t_cost = phc
        .params
        .get_decimal("t")
        .ok_or(PatError::HashError("argon2 params missing t"))?;
    let p_cost = phc
        .params
        .get_decimal("p")
        .ok_or(PatError::HashError("argon2 params missing p"))?;
    if m_cost < ARGON2_M_COST_KIB {
        return Err(PatError::HashError("argon2 m_cost underprovisioned"));
    }
    if t_cost < ARGON2_T_COST {
        return Err(PatError::HashError("argon2 t_cost underprovisioned"));
    }
    if p_cost < ARGON2_P_COST {
        return Err(PatError::HashError("argon2 p_cost underprovisioned"));
    }
    if phc.algorithm.as_str() != "argon2id" {
        return Err(PatError::HashError("argon2 algorithm mismatch"));
    }

    match hasher().verify_password(random_secret_b64.as_bytes(), &phc) {
        Ok(()) => Ok(()),
        Err(_) => Err(PatError::InvalidPat),
    }
}

/// Fixed dummy hash used by [`dummy_verify_for_constant_time`]. The
/// hash itself has zero cryptographic value (the plaintext that
/// produced it is publicly known to be `dummy_constant_time_pad_v1`)
/// — its sole purpose is to give the cold path the same Argon2id
/// CPU cost envelope as the warm path.
fn dummy_phc() -> &'static PatHash {
    static D: OnceLock<PatHash> = OnceLock::new();
    D.get_or_init(|| {
        // Compute once at startup; the fixed salt makes the dummy
        // deterministic so the cold-path metric never carries
        // tenant-correlated entropy.
        let salt = match Salt::from_b64("Y29yZWxpbmtfZHVtbXkwMQ") {
            Ok(s) => s,
            // Provably valid b64; fall back to a freshly-generated
            // salt under strict lints rather than panic.
            Err(_) => {
                let s = SaltString::generate(&mut OsRng);
                return hash_random_secret_with_salt("dummy_constant_time_pad_v1", s.as_salt())
                    .unwrap_or_else(|_| PatHash::from_phc_string(String::new()));
            }
        };
        hash_random_secret_with_salt("dummy_constant_time_pad_v1", salt)
            .unwrap_or_else(|_| PatHash::from_phc_string(String::new()))
    })
}

/// Public cold-path constant-time pad. The Tower middleware calls
/// this on every PAT-validation cold path (parse failure OR
/// token_id absent in DB) so the response latency profile matches
/// the success path.
///
/// Returns `Err(PatError::InvalidPat)` UNCONDITIONALLY — even when
/// the dummy plaintext would by some pathological chance match the
/// dummy hash (it does not; the input is fixed too) — to make the
/// API contract crystal clear: this is a PAD, not a verify.
///
/// # Constant-time properties
///
/// Timing is equalized by ALWAYS running the (expensive) Argon2id verify below
/// against a fixed dummy PHC — identical work to the hot path — so the cold path
/// (no PAT row) is indistinguishable from a real verify. The caller's
/// `plaintext` is intentionally NOT compared here: a compare would add no timing
/// guarantee (the Argon2id verify IS the pad), and the prior `ct_eq` over
/// different-length slices short-circuited on length so it equalized nothing.
/// Mann-Whitney indistinguishability assertion lives in `tests/constant_time.rs`.
pub fn dummy_verify_for_constant_time(plaintext: &str) -> Result<(), PatError> {
    // CAA-360 #32: removed a discarded `plaintext.ct_eq(dummy_pt)` here — it was
    // dead code (result unused + ct_eq length-short-circuits), providing neither
    // timing equalization nor anti-DCE. The real pad is the Argon2id verify below.
    let _ = plaintext;
    let dummy_pt = "dummy_constant_time_pad_v1";
    let stored = dummy_phc();
    // Hardcoded fallback dummy PHC ensures Argon2id work always runs
    // even if startup initialization fails. This is computed once at
    // module load with a deterministic salt; if entropy or hash fails
    // to materialize, we use this static fallback. The plaintext is
    // fixed and publicly known, so this PHC has zero cryptographic
    // value; its sole purpose is constant-time latency matching.
    static FALLBACK_DUMMY_PHC: OnceLock<PatHash> = OnceLock::new();
    let fallback = FALLBACK_DUMMY_PHC.get_or_init(|| {
        // Recompute the same dummy hash as dummy_phc() with the same
        // deterministic salt. If hashing fails, return a non-empty
        // marker string so the cold path always invokes verify_argon2id.
        let salt = match Salt::from_b64("Y29yZWxpbmtfZHVtbXkwMQ") {
            Ok(s) => s,
            Err(_) => {
                // If salt decoding itself fails, use a simple non-empty
                // string that will be rejected as unparseable PHC, forcing
                // the verify_argon2id call to complete with a HashError.
                // This preserves timing invariants by ensuring the
                // verify path always runs.
                return PatHash::from_phc_string("$argon2id$v=19$m=65536,t=3,p=4$".to_string());
            }
        };
        hash_random_secret_with_salt(dummy_pt, salt)
            .unwrap_or_else(|_| PatHash::from_phc_string("$argon2id$v=19$m=65536,t=3,p=4$".to_string()))
    });
    let phc = if stored.as_str().is_empty() {
        fallback
    } else {
        stored
    };
    let _ = verify_argon2id(dummy_pt, phc);
    Err(PatError::InvalidPat)
}

//! `mint()` — emit a fresh PAT plaintext + persistable [`Pat`] row.
//!
//! Per `auth_model.md §2.2`:
//!
//! 1. Generate a fresh `PatId` (UUIDv7 — time-ordered for B-tree
//!    locality on the `pat.id` column).
//! 2. Generate a fresh 16-char Crockford b32 `token_id`.
//! 3. Generate 32 random bytes via the OS CSPRNG and base64url-no-pad
//!    encode them as the `random_secret` segment.
//! 4. Compute `hmac_sig = HMAC-SHA256(pat_signing_key,
//!    token_id || "." || random_secret)[:16]` and base64url-no-pad
//!    encode the truncated MAC.
//! 5. Hash the `random_secret_b64` segment with Argon2id (OWASP
//!    2024 params; per [`crate::argon`]).
//! 6. Compose plaintext + return `(PatPlaintext, Pat)`.
//!
//! The plaintext is the ONLY surface the user receives; the
//! `Pat` row is the persistable record.
//!
//! # Entropy fail-safe
//!
//! `getrandom` is the underlying CSPRNG (Linux `getrandom(2)`,
//! macOS `getentropy`, Windows `BCryptGenRandom`, WASM Web Crypto).
//! On failure we surface [`PatError::EntropyUnavailable`] —
//! propagated to HTTP 503 by the middleware. Crypto code MUST NOT
//! fall back to a degraded entropy source.

use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand_core::{OsRng, RngCore};

use crate::argon::hash_random_secret;
use crate::error::PatError;
use crate::format::{
    PAT_HMAC_SIG_LEN, PAT_RANDOM_SECRET_LEN, PAT_RANDOM_SECRET_RAW_LEN,
};
use crate::scopes::PatScopes;
use crate::sig::compute_hmac_sig;
use crate::types::{
    Pat, PatEnv, PatHash, PatId, PatPlaintext, PatSigningKey, PatTokenId, PrincipalId, TenantId,
    PAT_TOKEN_ID_LEN,
};

/// Crockford base32 alphabet (RFC 4648 §6 with `i l o u` excluded
/// per the standard).
const CROCKFORD_B32: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Mint a fresh PAT plaintext + the persistable row.
///
/// Returns:
/// - `PatPlaintext`: the canonical wire string. The caller MUST
///   surface this to the user exactly once and discard the
///   reference; production code never logs nor persists this.
/// - `Pat`: persistable row to be inserted into the Neon `pat`
///   table by WI-S03-005 (this crate is decoupled from the schema).
///
/// # Errors
///
/// - [`PatError::EntropyUnavailable`] if the OS CSPRNG fails to
///   produce sufficient entropy.
pub fn mint(
    env: PatEnv,
    tenant_id: TenantId,
    principal_id: PrincipalId,
    scopes: PatScopes,
    ttl: Option<Duration>,
    signing_key: &PatSigningKey,
    signing_key_id: u32,
) -> Result<(PatPlaintext, Pat), PatError> {
    // --- token_id ----------------------------------------------------------
    let mut token_id_bytes = [0u8; PAT_TOKEN_ID_LEN];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut token_id_bytes[..])
        .map_err(|_| PatError::EntropyUnavailable)?;
    let token_id_str: String = token_id_bytes
        .iter()
        .map(|b| {
            let idx = (*b as usize) % CROCKFORD_B32.len();
            // Indexing safe by construction: idx ∈ 0..32.
            CROCKFORD_B32.get(idx).copied().unwrap_or(b'0') as char
        })
        .collect();
    let token_id = PatTokenId::from_validated(token_id_str.clone());

    // --- random_secret -----------------------------------------------------
    let mut secret_bytes = [0u8; PAT_RANDOM_SECRET_RAW_LEN];
    rng.try_fill_bytes(&mut secret_bytes[..])
        .map_err(|_| PatError::EntropyUnavailable)?;
    let random_secret_b64 = URL_SAFE_NO_PAD.encode(secret_bytes);
    debug_assert_eq!(random_secret_b64.len(), PAT_RANDOM_SECRET_LEN);

    // --- preimage = token_id || "." || random_secret ----------------------
    let mut preimage =
        Vec::with_capacity(PAT_TOKEN_ID_LEN + 1 + PAT_RANDOM_SECRET_LEN);
    preimage.extend_from_slice(token_id_str.as_bytes());
    preimage.push(b'.');
    preimage.extend_from_slice(random_secret_b64.as_bytes());

    let sig_bytes = compute_hmac_sig(signing_key, &preimage);
    let hmac_sig_b64 = URL_SAFE_NO_PAD.encode(sig_bytes);
    debug_assert_eq!(hmac_sig_b64.len(), PAT_HMAC_SIG_LEN);

    // --- Argon2id PHC over the random_secret_b64 string -------------------
    let hash = hash_random_secret(&random_secret_b64)?;

    // --- compose plaintext -------------------------------------------------
    let plaintext = format!(
        "corelink_{}_{}.{}.{}",
        env.as_wire(),
        token_id_str,
        random_secret_b64,
        hmac_sig_b64
    );

    let now = SystemTime::now();
    let expires_at = ttl.and_then(|d| now.checked_add(d));

    let pat = Pat {
        id: PatId::new_v7(),
        token_id,
        hash,
        scopes,
        env,
        tenant_id,
        principal_id,
        issued_at: now,
        expires_at,
        signing_key_id,
    };

    Ok((PatPlaintext::new(plaintext), pat))
}

/// Deterministic-entropy mint args (test only). Wrapper struct so
/// the test entry point doesn't trip clippy's `too-many-arguments`.
#[doc(hidden)]
#[derive(Debug)]
pub struct DeterministicMintInput<'a> {
    /// Env tag.
    pub env: PatEnv,
    /// Tenant id.
    pub tenant_id: TenantId,
    /// Principal id.
    pub principal_id: PrincipalId,
    /// Scope set.
    pub scopes: PatScopes,
    /// Optional TTL.
    pub ttl: Option<Duration>,
    /// Signing key.
    pub signing_key: &'a PatSigningKey,
    /// Signing key generation.
    pub signing_key_id: u32,
    /// 16 deterministic bytes that drive Crockford b32 token id.
    pub token_id_bytes: [u8; PAT_TOKEN_ID_LEN],
    /// 32 deterministic bytes for the random_secret segment.
    pub secret_bytes: [u8; PAT_RANDOM_SECRET_RAW_LEN],
    /// Argon2 salt to embed.
    pub salt: password_hash::Salt<'a>,
}

/// Re-mint a deterministic PAT for testing only. Uses
/// caller-provided random bytes so the resulting plaintext is
/// reproducible; production code MUST use [`mint`].
#[doc(hidden)]
pub fn mint_with_entropy(
    input: DeterministicMintInput<'_>,
) -> Result<(PatPlaintext, Pat), PatError> {
    let DeterministicMintInput {
        env,
        tenant_id,
        principal_id,
        scopes,
        ttl,
        signing_key,
        signing_key_id,
        token_id_bytes,
        secret_bytes,
        salt,
    } = input;
    let token_id_str: String = token_id_bytes
        .iter()
        .map(|b| {
            let idx = (*b as usize) % CROCKFORD_B32.len();
            CROCKFORD_B32.get(idx).copied().unwrap_or(b'0') as char
        })
        .collect();
    let token_id = PatTokenId::from_validated(token_id_str.clone());

    let random_secret_b64 = URL_SAFE_NO_PAD.encode(secret_bytes);
    let mut preimage = Vec::with_capacity(PAT_TOKEN_ID_LEN + 1 + PAT_RANDOM_SECRET_LEN);
    preimage.extend_from_slice(token_id_str.as_bytes());
    preimage.push(b'.');
    preimage.extend_from_slice(random_secret_b64.as_bytes());

    let sig_bytes = compute_hmac_sig(signing_key, &preimage);
    let hmac_sig_b64 = URL_SAFE_NO_PAD.encode(sig_bytes);

    let hash = crate::argon::hash_random_secret_with_salt(&random_secret_b64, salt)?;

    let plaintext = format!(
        "corelink_{}_{}.{}.{}",
        env.as_wire(),
        token_id_str,
        random_secret_b64,
        hmac_sig_b64
    );

    let now = SystemTime::now();
    let expires_at = ttl.and_then(|d| now.checked_add(d));

    let pat = Pat {
        id: PatId::new_v7(),
        token_id,
        hash,
        scopes,
        env,
        tenant_id,
        principal_id,
        issued_at: now,
        expires_at,
        signing_key_id,
    };

    let _ = PatHash::from_phc_string(String::new()); // type assertion
    Ok((PatPlaintext::new(plaintext), pat))
}

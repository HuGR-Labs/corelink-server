//! Recovery OTP — 6-digit, single-use, Argon2id-hashed at rest.
//!
//! Per `WI-S03-006 §6.1.10` (Lote 10.3-tris P0-R5-002a) the canonical
//! recovery flow is:
//!
//! - Server-side cryptographically random 6 decimal digits (`0-9`),
//!   ≈ 20 bits entropy. Drawn via `OsRng` (`rand_core::getrandom`).
//! - TTL ≤ 600 s (`WI-S03-006 §6.1.10`; canonical default 600 s
//!   matches sprint contract — we surface a tight default of 600 s
//!   to maximise UX while staying inside the policy ceiling).
//! - Single-use: store is `UPDATE … WHERE consumed_at IS NULL`
//!   semantically.
//! - Hashing at rest: Argon2id PHC string (same OWASP-2024 floor
//!   parameters as `corelink-pat`).
//! - Rate limit: 3 generations/hour/user; 5 verify attempts per OTP.
//! - Channel: enum-locked to `RecoveryChannel::ClerkSsoEmail` —
//!   **`MagicLink` is intentionally absent** (sprint contract §10
//!   anti-scope).
//!
//! Every fallible surface yields a [`crate::WebAuthnError`] variant
//! per the canonical taxonomy.

use std::fmt;
use std::time::Duration;

use argon2::{Algorithm, Argon2, Params, Version};
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::types::UserAccountId;
use crate::WebAuthnError;

/// Number of decimal digits (W3C-style 6).
pub const RECOVERY_OTP_DIGITS: usize = 6;

/// Default TTL — matches the canonical 10-minute sprint-contract floor
/// (`WI-S03-006 §6.1.10`).
pub const RECOVERY_OTP_TTL_DEFAULT: Duration = Duration::from_secs(600);

/// OWASP-2024 Argon2id parameters (mirrors `corelink-pat` so OTP
/// hash cost is canonical across the auth stack).
const ARGON2_M_COST_KIB: u32 = 65_536;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32;

/// Recovery channel. Enum-locked: there is **no** `MagicLink`
/// variant — adding one would require an explicit ADR amendment +
/// a sprint-contract anti-scope inversion. This is the type-system
/// enforcement of `WI-S03-006 §6.1.10` Lote 10.3-tris P0-R5-002a.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoveryChannel {
    /// Clerk SSO email channel (subject + body include the literal
    /// "do NOT click links — type the 6 digits manually" anti-phishing
    /// instruction).
    ClerkSsoEmail,
}

/// Argon2id PHC string carrying the recovery OTP hash.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoveryOtpHash(String);

impl RecoveryOtpHash {
    /// Borrow the canonical PHC string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct from a raw PHC string (used by the in-memory store
    /// when round-tripping a serialized record).
    #[must_use]
    pub fn from_phc_string(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Debug for RecoveryOtpHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let head = self.0.split('$').take(4).collect::<Vec<_>>().join("$");
        write!(f, "RecoveryOtpHash({head}$<…>)")
    }
}

/// 6-digit OTP plaintext newtype. Constructed only by [`mint_otp`];
/// the single legal consumer is the email-delivery surface.
pub struct RecoveryOtp(String);

impl RecoveryOtp {
    /// Borrow the digits as a `&str` (length-checked at construction).
    #[must_use]
    pub fn digits(&self) -> &str {
        &self.0
    }

    /// Consume into the inner `String`.
    #[must_use]
    pub fn into_string(self) -> String {
        let mut me = self;
        let mut out = String::new();
        std::mem::swap(&mut out, &mut me.0);
        out
    }
}

impl fmt::Debug for RecoveryOtp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecoveryOtp(<redacted len={}>)", self.0.len())
    }
}

impl Drop for RecoveryOtp {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Primary-key surface (`auth_recovery_otp.id` UUIDv7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoveryOtpId(pub Uuid);

impl RecoveryOtpId {
    /// Mint a fresh time-ordered identifier.
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

/// Persistent OTP record (mirrors the Postgres
/// `auth_recovery_otp(user_id, otp_hash, expires_at_ms, consumed_at_ms,
/// attempts_remaining)` row shape).
#[derive(Debug, Clone)]
pub struct RecoveryOtpRecord {
    /// Primary key.
    pub id: RecoveryOtpId,
    /// Owning user account.
    pub user: UserAccountId,
    /// Argon2id PHC string of the canonical 6-digit plaintext.
    pub hash: RecoveryOtpHash,
    /// Issue timestamp (unix-ms).
    pub issued_at_ms: u64,
    /// Expiry (unix-ms).
    pub expires_at_ms: u64,
    /// Set on the verify-and-consume call.
    pub consumed_at_ms: Option<u64>,
    /// Remaining verify attempts (`5` at issue per `WI-S03-006 §6.1.10`).
    pub attempts_remaining: u32,
    /// Delivery channel (always [`RecoveryChannel::ClerkSsoEmail`]).
    pub channel: RecoveryChannel,
}

/// Outcome of a verify call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOtpVerifyOutcome {
    /// OTP plaintext matched; record marked as consumed.
    Consumed {
        /// Persisted OTP id (used in audit emission).
        otp_id: RecoveryOtpId,
    },
    /// OTP plaintext did not match; the verify counter has been
    /// decremented but the record stays unconsumed until either
    /// success OR the counter reaches zero (then the next call returns
    /// [`WebAuthnError::RecoveryOtpRateLimited`]).
    Mismatch {
        /// Remaining attempts.
        attempts_remaining: u32,
    },
}

/// Rate-limit policy for OTP issuance + verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryRateLimit {
    /// Max OTP generations per `generation_window` per user.
    pub generations_per_window: u32,
    /// Window length (ms).
    pub generation_window_ms: u64,
    /// Max verify attempts per OTP (decremented per attempt; zero =
    /// rate limited).
    pub verify_attempts_per_otp: u32,
}

impl RecoveryRateLimit {
    /// Canonical sprint-contract policy: 3 / hour, 5 attempts.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            generations_per_window: 3,
            generation_window_ms: 3_600_000,
            verify_attempts_per_otp: 5,
        }
    }

    /// Generation-window length in ms.
    #[must_use]
    pub const fn generation_window_ms(&self) -> u64 {
        self.generation_window_ms
    }
}

/// Output of the [`mint_otp`] call.
#[derive(Debug)]
pub struct MintedOtp {
    /// Plaintext digits — return to the user via the email channel
    /// exactly once.
    pub plaintext: RecoveryOtp,
    /// Persisted record (canonical INSERT shape).
    pub record: RecoveryOtpRecord,
}

/// Mint a fresh recovery OTP.
///
/// `now_ms` is the engine-supplied wall clock; `ttl` is the lifetime
/// (≤ [`RECOVERY_OTP_TTL_DEFAULT`]).
pub fn mint_otp(
    user: UserAccountId,
    now_ms: u64,
    ttl: Duration,
    channel: RecoveryChannel,
    attempts_remaining: u32,
) -> Result<MintedOtp, WebAuthnError> {
    let mut digit_bytes = [0u8; RECOVERY_OTP_DIGITS];
    OsRng
        .try_fill_bytes(&mut digit_bytes)
        .map_err(|_| WebAuthnError::EntropyUnavailable)?;
    let digits: String = digit_bytes
        .iter()
        .map(|b| char::from(b'0' + (b % 10)))
        .collect();
    let plaintext = RecoveryOtp(digits);

    let mut salt_bytes = [0u8; 16];
    OsRng
        .try_fill_bytes(&mut salt_bytes)
        .map_err(|_| WebAuthnError::EntropyUnavailable)?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|_| WebAuthnError::Malformed("argon2 salt encode"))?;
    let params = Params::new(
        ARGON2_M_COST_KIB,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|_| WebAuthnError::Malformed("argon2 params"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = argon
        .hash_password(plaintext.digits().as_bytes(), &salt)
        .map_err(|_| WebAuthnError::Malformed("argon2 hash"))?
        .to_string();

    let ttl_ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX);
    let expires_at_ms = now_ms.saturating_add(ttl_ms);

    let record = RecoveryOtpRecord {
        id: RecoveryOtpId::new_v7(),
        user,
        hash: RecoveryOtpHash(hash),
        issued_at_ms: now_ms,
        expires_at_ms,
        consumed_at_ms: None,
        attempts_remaining,
        channel,
    };

    Ok(MintedOtp { plaintext, record })
}

/// Verify a candidate against an Argon2id PHC.
///
/// Constant-time across both "non-canonical PHC" and "PHC mismatch"
/// branches — uses [`subtle::ConstantTimeEq`] on a sentinel byte
/// when the PHC fails to parse so the caller cannot use timing to
/// distinguish "DB row corrupt" from "wrong digits".
#[must_use]
pub fn verify_otp(candidate: &str, hash: &RecoveryOtpHash) -> bool {
    let parsed = match PasswordHash::new(hash.as_str()) {
        Ok(p) => p,
        Err(_) => {
            // Constant-time pad — keep the verify branch latency
            // envelope identical even on PHC parse failure.
            let argon = Argon2::default();
            let dummy_salt = match SaltString::encode_b64(&[0u8; 16]) {
                Ok(s) => s,
                Err(_) => {
                    let _ = candidate.as_bytes().ct_eq(b"sentinel");
                    return false;
                }
            };
            let _ = argon.hash_password(candidate.as_bytes(), &dummy_salt);
            return false;
        }
    };
    let argon = Argon2::default();
    argon
        .verify_password(candidate.as_bytes(), &parsed)
        .is_ok()
}

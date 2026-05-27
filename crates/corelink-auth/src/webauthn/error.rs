//! Canonical error taxonomy for the WebAuthn ceremony.
//!
//! Every fallible surface in this crate folds to a single concrete
//! [`WebAuthnError`] variant. The taxonomy is the source of truth for
//! the wire mapping in `WI-S03-006 §23`:
//!
//! | Variant | Wire mapping |
//! |---|---|
//! | [`WebAuthnError::InvalidChallenge`] | 401 `COR_AUTH_CHALLENGE_INVALID` |
//! | [`WebAuthnError::ChallengeExpired`] | 401 `COR_AUTH_CHALLENGE_EXPIRED` |
//! | [`WebAuthnError::OriginMismatch`] | 401 `COR_AUTH_ORIGIN_MISMATCH` |
//! | [`WebAuthnError::RpIdMismatch`] | 401 `COR_AUTH_RP_ID_MISMATCH` |
//! | [`WebAuthnError::UserVerificationMissing`] | 403 `COR_AUTH_UV_REQUIRED` |
//! | [`WebAuthnError::UserPresenceMissing`] | 403 `COR_AUTH_UP_REQUIRED` |
//! | [`WebAuthnError::AttestationInvalid`] | 403 `COR_AUTH_ATTESTATION_INVALID` |
//! | [`WebAuthnError::SignatureInvalid`] | 401 `COR_AUTH_SIGNATURE_INVALID` |
//! | [`WebAuthnError::SignCountRegression`] | 401 `COR_AUTH_SIGN_COUNT_REGRESSION` |
//! | [`WebAuthnError::CredentialNotFound`] | 404 `COR_AUTH_CREDENTIAL_NOT_FOUND` |
//! | [`WebAuthnError::AaguidDenied`] | 403 `COR_AUTH_AUTHENTICATOR_DEPRECATED` |
//! | [`WebAuthnError::AaguidNotAllowed`] | 403 `COR_AUTH_AUTHENTICATOR_NOT_ALLOWED` |
//! | [`WebAuthnError::Malformed`] | 400 `COR_AUTH_MALFORMED` |
//! | [`WebAuthnError::EngineNotConfigured`] | 503 `COR_AUTH_ENGINE_UNAVAILABLE` |
//! | [`WebAuthnError::EntropyUnavailable`] | 503 `COR_SERVICE_UNAVAILABLE` |
//! | [`WebAuthnError::RecoveryOtpInvalid`] | 401 `COR_AUTH_RECOVERY_INVALID` |
//! | [`WebAuthnError::RecoveryOtpAlreadyConsumed`] | 401 `COR_AUTH_RECOVERY_CONSUMED` |
//! | [`WebAuthnError::RecoveryOtpRateLimited`] | 429 `COR_AUTH_RECOVERY_RATE_LIMITED` |
//! | [`WebAuthnError::ChallengeReuse`] | 401 `COR_AUTH_CHALLENGE_REUSE` |
//! | [`WebAuthnError::StepUpRequired`] | 403 `COR_AUTH_STEP_UP_REQUIRED` |
//! | [`WebAuthnError::Conflict`] | 409 `COR_AUTH_CONFLICT` |
//!
//! Display impls carry **no PII**, **no plaintext credentials**, and
//! **no challenge bytes**; they are safe to log directly.

use thiserror::Error;

/// Canonical fallible surface produced by every ceremony / store call.
#[derive(Debug, Error)]
pub enum WebAuthnError {
    /// Challenge identifier missing from the store, or its bytes do
    /// not match the canonical 32-byte length. Folded with
    /// [`WebAuthnError::ChallengeExpired`] on the wire so the response
    /// cannot be used as a TTL oracle.
    #[error("invalid challenge or unknown challenge id")]
    InvalidChallenge,

    /// Challenge TTL has elapsed (default 300 s; ≤ 600 s hard cap per
    /// `WI-S03-006 §6.1.2 + §9.4`).
    #[error("challenge expired")]
    ChallengeExpired,

    /// Origin in the response does not exactly match the configured
    /// allowlist. The error is intentionally fixed-shape — exposing
    /// the offending origin would leak which subdomains the attacker
    /// is probing, so we only carry a sentinel.
    #[error("origin not in allowlist")]
    OriginMismatch,

    /// RP-ID hash sent by the client does not match the canonical
    /// SHA-256 of the configured RP-ID.
    #[error("rp-id hash mismatch")]
    RpIdMismatch,

    /// Authenticator did not set the User-Verified flag during admin
    /// step-up. Mitigates UV-downgrade in admin paths
    /// (`INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN`).
    #[error("user verification flag missing")]
    UserVerificationMissing,

    /// Authenticator did not set the User-Presence flag.
    #[error("user presence flag missing")]
    UserPresenceMissing,

    /// Attestation chain failed validation OR attestation was absent
    /// in registration where required.
    #[error("attestation invalid")]
    AttestationInvalid,

    /// Assertion signature did not verify.
    #[error("signature invalid")]
    SignatureInvalid,

    /// Sign-count went backwards. Folded for forensic post-processing
    /// — the `(got, stored)` pair is intentionally NOT exposed on the
    /// wire to deny attacker oracles.
    #[error("sign-count regression detected")]
    SignCountRegression {
        /// Counter value reported by the authenticator.
        got: u64,
        /// Counter value persisted at last legitimate authentication.
        stored: u64,
    },

    /// Credential id absent from the credential store.
    #[error("credential not found")]
    CredentialNotFound,

    /// Authenticator AAGUID is on the explicit denylist.
    #[error("authenticator deprecated (denylist)")]
    AaguidDenied,

    /// Authenticator AAGUID is not on the explicit allowlist
    /// (closed-default policy).
    #[error("authenticator not in allowlist")]
    AaguidNotAllowed,

    /// CBOR/COSE / origin / RP-ID parse failure surfaced as a single
    /// "client request is malformed" wire status.
    #[error("malformed input: {0}")]
    Malformed(&'static str),

    /// Production engine shim is not wired in this build (the
    /// `feature = "host-server"` is off OR the staging Cloudflare
    /// configuration is absent). Returned only from the production
    /// shim path; the in-memory engine never surfaces this.
    #[error("webauthn engine not configured (feature `host-server` disabled)")]
    EngineNotConfigured,

    /// CSPRNG unavailable when minting a challenge OR a recovery OTP.
    #[error("entropy source unavailable")]
    EntropyUnavailable,

    /// Recovery OTP plaintext failed Argon2id verify.
    #[error("recovery OTP verify failed")]
    RecoveryOtpInvalid,

    /// Recovery OTP was already consumed (single-use guarantee per
    /// Lote 10.3-tris P0-R5-002a).
    #[error("recovery OTP already consumed")]
    RecoveryOtpAlreadyConsumed,

    /// Recovery OTP throttle hit (3 generations / hour OR 5 verifies
    /// / OTP per `WI-S03-006 §6.1.10`).
    #[error("recovery OTP rate limit exceeded")]
    RecoveryOtpRateLimited,

    /// Challenge re-used across two distinct ceremonies — a single
    /// challenge id can be consumed at most once.
    #[error("challenge id reused (single-use violation)")]
    ChallengeReuse,

    /// Admin op route entered without a valid step-up token.
    #[error("step-up token required for this operation")]
    StepUpRequired,

    /// Persistent-store conflict (e.g., credential id collision on
    /// the UNIQUE column). Surfaced from the in-memory test fakes so
    /// proptests can pin schema invariants.
    #[error("conflict on unique constraint")]
    Conflict,
}

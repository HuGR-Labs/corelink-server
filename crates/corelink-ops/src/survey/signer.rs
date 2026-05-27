//! [`SurveyLinkSigner`] trait + [`SigningKey`] newtype.
//!
//! ## Production wiring (deferred)
//!
//! The production signer is `D1SurveyLinkSigner`, composed at apps/server
//! wire-up time:
//!
//! - Loads the active 32-byte HMAC key + recipient-salt from the
//!   per-environment KV namespace (rotated by the same surface that
//!   rotates PAT signing keys).
//! - Mints a UUIDv7 `jti` per invite + records the invite row in a
//!   `survey_invites` table (deferred — separate migration).
//! - Sends the encoded token via the existing outbound-email worker
//!   (deferred — wave wiring lives under
//!   `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE.md`).
//!
//! The [`crate::survey::recorder::InMemoryFake`] sink mirrors the sign + record cycle for unit +
//! property + integration tests.

use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::error::SurveyError;
use super::hash::RecipientHash;
use super::ids::{SurveyId, TenantId};
use super::token::{encode_token, SurveyToken, TokenPayload, TOKEN_WIRE_VERSION};
use super::types::SurveyKind;

/// HMAC-SHA256 signing key for survey invite tokens. 32 bytes.
///
/// Zeroized on drop so a panic-unwind or process teardown doesn't leak
/// the key to memory inspection.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SigningKey([u8; 32]);

impl SigningKey {
    /// Wrap a 32-byte key into the canonical newtype.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner 32-byte key.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never render the key bytes in Debug output.
        f.write_str("SigningKey([redacted; 32])")
    }
}

/// Trait for any backend that mints an HMAC-signed survey invite URL.
///
/// Sync by design (matches the `corelink-audit::Emitter` pattern): the
/// production implementation does the actual D1 INSERT under an async
/// runtime in the apps/server route, but the trait surface itself stays
/// sync so downstream wasm32 callers (CF Worker) can reuse the math
/// without pulling tokio.
pub trait SurveyLinkSigner: Send + Sync {
    /// Mint a one-shot HMAC-signed invite token.
    ///
    /// Inputs:
    /// - `tenant` — the tenant owning the survey wave.
    /// - `recipient` — the SHA-256-salted hash of the recipient id (the
    ///   raw email never enters this crate).
    /// - `survey_id` — the survey slug (e.g. `"nps-w1-2026q2"`).
    /// - `kind` — survey kind (NPS / CSAT / FreeText / MultiChoice).
    /// - `now_ms` — issued-at unix ms (test sites pass a deterministic
    ///   clock; production passes `SystemTime::now()`).
    /// - `ttl_ms` — token lifetime in milliseconds. The recorder
    ///   rejects any record-call after `now_ms + ttl_ms`.
    ///
    /// # Errors
    ///
    /// - [`SurveyError::SigningKeyLength`] — the key was constructed
    ///   from a non-32-byte slice (constructor guards against this so
    ///   in practice this variant never fires).
    /// - [`SurveyError::Storage`] — payload serialization failed
    ///   (effectively impossible — `serde_json::to_vec` of a struct
    ///   with primitive + String fields cannot fail in steady state;
    ///   the variant exists for trait completeness).
    #[allow(clippy::too_many_arguments)]
    fn sign_invite(
        &self,
        tenant: TenantId,
        recipient: RecipientHash,
        survey_id: SurveyId,
        kind: SurveyKind,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<SurveyToken, SurveyError>;
}

/// Helper: build the canonical token payload + sign it with the given
/// key. Shared between [`crate::survey::recorder::InMemoryFake`] and any future production
/// adapter so the math stays in one place.
///
/// # Errors
///
/// Bubbles [`SurveyError`] from `encode_token`.
pub fn mint_token(
    key: &SigningKey,
    tenant: TenantId,
    recipient: RecipientHash,
    survey_id: SurveyId,
    kind: SurveyKind,
    now_ms: u64,
    ttl_ms: u64,
) -> Result<SurveyToken, SurveyError> {
    let jti = Uuid::now_v7();
    let payload = TokenPayload {
        v: TOKEN_WIRE_VERSION,
        jti,
        t: tenant,
        s: survey_id,
        k: kind,
        r: recipient,
        iat: now_ms,
        exp: now_ms.saturating_add(ttl_ms),
    };
    encode_token(key, &payload)
}

/// Helper: deterministic variant of [`mint_token`] that takes an
/// explicit `jti`. Used by property tests + the in-memory fake's
/// `sign_invite_with_jti` extension for snapshot tests.
///
/// # Errors
///
/// Bubbles [`SurveyError`] from `encode_token`.
#[allow(clippy::too_many_arguments, reason = "test/property-test helper — every field is mandatory wire-shape data")]
pub fn mint_token_with_jti(
    key: &SigningKey,
    jti: Uuid,
    tenant: TenantId,
    recipient: RecipientHash,
    survey_id: SurveyId,
    kind: SurveyKind,
    now_ms: u64,
    ttl_ms: u64,
) -> Result<SurveyToken, SurveyError> {
    let payload = TokenPayload {
        v: TOKEN_WIRE_VERSION,
        jti,
        t: tenant,
        s: survey_id,
        k: kind,
        r: recipient,
        iat: now_ms,
        exp: now_ms.saturating_add(ttl_ms),
    };
    encode_token(key, &payload)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;
    use super::super::token::decode_and_verify;

    #[test]
    fn debug_redacts_key() {
        let k = SigningKey::from_bytes([0xab; 32]);
        let s = format!("{k:?}");
        assert!(!s.contains("ab"));
        assert!(s.contains("redacted"));
    }

    #[test]
    fn mint_then_decode_roundtrip() {
        let key = SigningKey::from_bytes([0x33; 32]);
        let tenant = TenantId::from_uuid(Uuid::nil());
        let r = RecipientHash::derive_with_salt("a@b.com", b"salt");
        let s = SurveyId::new("csat-on");
        let tok = mint_token(&key, tenant, r.clone(), s.clone(), SurveyKind::Csat, 1, 60).unwrap();
        let dec = decode_and_verify(&key, tok.as_str()).unwrap();
        assert_eq!(dec.tenant(), tenant);
        assert_eq!(dec.recipient(), &r);
        assert_eq!(dec.survey_id(), &s);
        assert_eq!(dec.kind(), SurveyKind::Csat);
        assert_eq!(dec.issued_at_ms(), 1);
        assert_eq!(dec.expires_at_ms(), 61);
    }
}

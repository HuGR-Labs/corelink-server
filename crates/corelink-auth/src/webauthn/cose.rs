//! Minimal COSE algorithm enum surface.
//!
//! Production attestation parsing rides on `webauthn-rs = 0.5` once
//! the host-server feature lands; in this crate we expose the canonical
//! algorithm labels (W3C registered IANA values) so the in-memory
//! engine and adversarial tests can pin which algorithms admit
//! production keys and reject the well-known `alg: none` attack.

use serde::{Deserialize, Serialize};

use super::WebAuthnError;

/// IANA COSE algorithm registry value for ES256 (ECDSA-P256-SHA256;
/// W3C registered: -7). Default for platform authenticators.
pub const COSE_ALG_ES256: i32 = -7;

/// IANA COSE algorithm registry value for EdDSA (Ed25519; -8).
/// Returned by some YubiKey 5 firmwares.
pub const COSE_ALG_EDDSA: i32 = -8;

/// IANA COSE algorithm registry value for RS256 (RSASSA-PKCS1-v1_5 +
/// SHA-256; -257). Allowed but discouraged; older authenticators only.
pub const COSE_ALG_RS256: i32 = -257;

/// Parsed COSE algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoseAlgorithm {
    /// ECDSA-P256-SHA256.
    Es256,
    /// Ed25519.
    EdDsa,
    /// RSASSA-PKCS1-v1_5 + SHA-256.
    Rs256,
}

impl CoseAlgorithm {
    /// Canonical IANA algorithm value.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Es256 => COSE_ALG_ES256,
            Self::EdDsa => COSE_ALG_EDDSA,
            Self::Rs256 => COSE_ALG_RS256,
        }
    }

    /// Human-readable label (used in metrics + audit envelopes).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Es256 => "es256",
            Self::EdDsa => "eddsa",
            Self::Rs256 => "rs256",
        }
    }
}

/// Parse an IANA COSE algorithm value into the canonical enum.
///
/// `0` (the well-known "alg: none" attack vector) is **always
/// rejected** as `Malformed("alg none")` — see the adversarial
/// regression test `test_alg_none_rejected`.
pub fn parse_cose_algorithm(value: i32) -> Result<CoseAlgorithm, WebAuthnError> {
    if value == 0 {
        return Err(WebAuthnError::Malformed("alg none"));
    }
    match value {
        COSE_ALG_ES256 => Ok(CoseAlgorithm::Es256),
        COSE_ALG_EDDSA => Ok(CoseAlgorithm::EdDsa),
        COSE_ALG_RS256 => Ok(CoseAlgorithm::Rs256),
        _ => Err(WebAuthnError::Malformed("unknown cose algorithm")),
    }
}

/// Convenience inverse — render an integer label for emitted metrics.
#[must_use]
pub const fn cose_algorithm_label(value: i32) -> &'static str {
    match value {
        COSE_ALG_ES256 => "es256",
        COSE_ALG_EDDSA => "eddsa",
        COSE_ALG_RS256 => "rs256",
        _ => "unknown",
    }
}

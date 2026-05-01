//! Credential row + registration-response surface.
//!
//! [`Credential`] mirrors the canonical `webauthn_credentials` row
//! shape from `migrations/002_auth_tables.sql §7`; the in-memory
//! credential store ([`crate::store::InMemoryCredentialStore`])
//! enforces every UNIQUE constraint that the production Postgres
//! row would.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::challenge::ChallengeId;
use crate::cose::CoseAlgorithm;
use crate::flags::AuthenticatorFlags;
use crate::sign_count::SignCount;
use crate::types::UserAccountId;
use crate::{Aaguid, Origin};

/// Opaque credential identifier (W3C `credentialId`; up to 1023
/// bytes per spec — typically 32-64 bytes for production
/// authenticators).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialId(Vec<u8>);

impl CredentialId {
    /// Construct from raw bytes. Returns `Err(Malformed)` on empty
    /// input or input longer than W3C's 1023-byte hard cap.
    pub fn new(bytes: Vec<u8>) -> Result<Self, crate::WebAuthnError> {
        if bytes.is_empty() || bytes.len() > 1023 {
            return Err(crate::WebAuthnError::Malformed("credential id length"));
        }
        Ok(Self(bytes))
    }

    /// Synthetic constructor for tests — bypasses the length guard
    /// only when the input is canonical (32-byte digest).
    #[doc(hidden)]
    #[must_use]
    pub fn synthetic(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Hex view (used in audit envelopes + log spans).
    #[must_use]
    pub fn as_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl fmt::Debug for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.as_hex();
        let prefix = hex.get(..16).unwrap_or(hex.as_str());
        write!(f, "CredentialId({prefix}…/{} bytes)", self.0.len())
    }
}

/// Persistent credential row (mirrors `webauthn_credentials`).
#[derive(Debug, Clone)]
pub struct Credential {
    /// Primary key (UUIDv7 minted app-side).
    pub id: Uuid,
    /// Owning user account.
    pub user_account_id: UserAccountId,
    /// W3C `credentialId` (UNIQUE column).
    pub credential_id: CredentialId,
    /// Public-key bytes (COSE encoded; opaque outside this crate —
    /// the production engine performs signature verify).
    pub public_key_bytes: Vec<u8>,
    /// COSE algorithm.
    pub cose_algorithm: CoseAlgorithm,
    /// Persisted sign-count.
    pub sign_count: SignCount,
    /// AAGUID.
    pub aaguid: Aaguid,
    /// Backup-eligible flag at registration.
    pub backup_eligible: bool,
    /// Most recent backup-state observation.
    pub backup_state: bool,
    /// Created at (unix-ms).
    pub created_at_ms: u64,
    /// Last successful authentication (unix-ms).
    pub last_used_at_ms: Option<u64>,
}

impl Credential {
    /// Build a fresh credential row from a registration outcome.
    #[allow(
        clippy::too_many_arguments,
        reason = "deliberate constructor surface mirroring the canonical webauthn_credentials row shape; collapsing into a builder would obscure call-site readability for the engine"
    )]
    pub fn from_registration(
        user_account_id: UserAccountId,
        credential_id: CredentialId,
        public_key_bytes: Vec<u8>,
        cose_algorithm: CoseAlgorithm,
        flags: AuthenticatorFlags,
        sign_count: SignCount,
        aaguid: Aaguid,
        created_at_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            user_account_id,
            credential_id,
            public_key_bytes,
            cose_algorithm,
            sign_count,
            aaguid,
            backup_eligible: flags.backup_eligible(),
            backup_state: flags.backup_state(),
            created_at_ms,
            last_used_at_ms: None,
        }
    }
}

/// Registration response — what the browser sends to
/// `finish_registration`.
#[derive(Debug, Clone)]
pub struct RegistrationResponse {
    /// Echoed challenge id (must match the one the engine issued).
    pub challenge_id: ChallengeId,
    /// AAGUID extracted from the attestation object.
    pub aaguid: Aaguid,
    /// Credential id that the authenticator minted.
    pub credential_id: CredentialId,
    /// COSE public-key bytes (raw).
    pub public_key_bytes: Vec<u8>,
    /// IANA COSE algorithm value.
    pub cose_algorithm_value: i32,
    /// Authenticator-data flags.
    pub flags: AuthenticatorFlags,
    /// Sign-count seed reported at registration.
    pub sign_count: SignCount,
    /// Origin reported by the client (`clientDataJSON.origin`).
    pub origin: Origin,
    /// `clientDataJSON.type` echo (`webauthn.create` for registration).
    pub client_data_type: String,
    /// Whether the response carries an attestation object. The
    /// in-memory engine accepts a synthetic flag; the production
    /// engine performs full chain validation.
    pub attestation_present: bool,
}

impl RegistrationResponse {
    /// Test-only constructor — produces a canonical synthetic that
    /// the in-memory engine accepts when the challenge id matches.
    #[doc(hidden)]
    #[must_use]
    pub fn synthetic_for_test(
        challenge_id: ChallengeId,
        aaguid: Aaguid,
        credential_id: CredentialId,
        cose_algorithm_value: i32,
        flags: AuthenticatorFlags,
        sign_count: u64,
        origin: Origin,
    ) -> Self {
        Self {
            challenge_id,
            aaguid,
            credential_id,
            public_key_bytes: vec![0xAB; 65],
            cose_algorithm_value,
            flags,
            sign_count: SignCount::new(sign_count),
            origin,
            client_data_type: "webauthn.create".to_owned(),
            attestation_present: true,
        }
    }
}

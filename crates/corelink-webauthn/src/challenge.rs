//! Challenge primitives (registration + authentication).
//!
//! Both ceremonies use the same shape: a 32-byte CSPRNG-drawn nonce
//! (`ChallengeBytes`) plus a stable id (`ChallengeId`, also 32 bytes
//! drawn from the same OsRng) for store-side lookup. The split exists
//! so the lookup key never directly exposes the nonce — a leaked
//! `ChallengeId` cannot be used to forge an assertion (the actual
//! signed material is the nonce).

use std::fmt;
use std::time::Duration;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::types::UserAccountId;
use crate::Origin;
use crate::{
    aaguid::Aaguid, store::AuthenticatorAttachment, CredentialId, RpId, WebAuthnError,
};

/// Canonical challenge byte length (W3C recommends ≥ 16; we pin to
/// 32 bytes / 256 bits — collision probability for 100 k registrations
/// per second is negligibly above 0).
pub const CHALLENGE_BYTE_LEN: usize = 32;

/// Stable-id for challenge lookup.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChallengeId([u8; CHALLENGE_BYTE_LEN]);

impl ChallengeId {
    /// Construct from raw bytes (used by store-side fakes that mint a
    /// deterministic id for property tests).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; CHALLENGE_BYTE_LEN]) -> Self {
        Self(bytes)
    }

    /// Generate a fresh CSPRNG id.
    pub fn generate() -> Result<Self, WebAuthnError> {
        let mut buf = [0u8; CHALLENGE_BYTE_LEN];
        OsRng
            .try_fill_bytes(&mut buf)
            .map_err(|_| WebAuthnError::EntropyUnavailable)?;
        Ok(Self(buf))
    }

    /// Hex-encoded view (canonical lookup key for store-side
    /// indexing).
    #[must_use]
    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Borrow raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; CHALLENGE_BYTE_LEN] {
        &self.0
    }
}

impl fmt::Debug for ChallengeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hex prefix only — 64 hex chars is a lot of log noise.
        let hex = self.as_hex();
        let prefix = hex.get(..16).unwrap_or(hex.as_str());
        write!(f, "ChallengeId({prefix}…)")
    }
}

/// Signed-over nonce.
///
/// Zeroizes on drop — the CSPRNG output is logically a one-time
/// secret until the ceremony completes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeBytes(Vec<u8>);

impl ChallengeBytes {
    /// Construct from raw bytes (32-byte canonical length).
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, WebAuthnError> {
        if bytes.len() != CHALLENGE_BYTE_LEN {
            return Err(WebAuthnError::Malformed("challenge bytes length"));
        }
        Ok(Self(bytes))
    }

    /// Generate fresh CSPRNG bytes.
    pub fn generate() -> Result<Self, WebAuthnError> {
        let mut buf = vec![0u8; CHALLENGE_BYTE_LEN];
        OsRng
            .try_fill_bytes(&mut buf)
            .map_err(|_| WebAuthnError::EntropyUnavailable)?;
        Ok(Self(buf))
    }

    /// Borrow the canonical bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for ChallengeBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChallengeBytes(<redacted len={}>)", self.0.len())
    }
}

impl Drop for ChallengeBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Challenge TTL — bounded ≤ 600 s (`WI-S03-006 §6.1.2 + §9.4`).
#[derive(Debug, Clone, Copy)]
pub struct ChallengeTtl(Duration);

impl ChallengeTtl {
    /// Default 300 s (W3C-recommended balance).
    pub const DEFAULT: Duration = Duration::from_secs(300);
    /// Hard upper bound 600 s — replay-surface ceiling.
    pub const MAX: Duration = Duration::from_secs(600);

    /// Construct a [`ChallengeTtl`].
    pub fn new(ttl: Duration) -> Result<Self, WebAuthnError> {
        if ttl.is_zero() || ttl > Self::MAX {
            return Err(WebAuthnError::Malformed("challenge ttl out of bounds"));
        }
        Ok(Self(ttl))
    }

    /// Inner duration.
    #[must_use]
    pub fn as_duration(self) -> Duration {
        self.0
    }
}

impl Default for ChallengeTtl {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

/// Registration ceremony challenge (W3C `PublicKeyCredentialCreationOptions`).
#[derive(Debug, Clone)]
pub struct RegistrationChallenge {
    id: ChallengeId,
    bytes: ChallengeBytes,
    user: UserAccountId,
    rp_id: RpId,
    rp_name: String,
    attachment: AuthenticatorAttachment,
    expires_at_ms: u64,
}

impl RegistrationChallenge {
    /// Internal constructor — the engine populates every field.
    pub(crate) fn new(
        id: ChallengeId,
        bytes: ChallengeBytes,
        user: UserAccountId,
        rp_id: RpId,
        rp_name: String,
        attachment: AuthenticatorAttachment,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            id,
            bytes,
            user,
            rp_id,
            rp_name,
            attachment,
            expires_at_ms,
        }
    }

    /// Public stable id (the value the client must echo back on
    /// `finish_registration`).
    #[must_use]
    pub fn id(&self) -> &ChallengeId {
        &self.id
    }

    /// Signed-over nonce.
    #[must_use]
    pub fn bytes(&self) -> &ChallengeBytes {
        &self.bytes
    }

    /// User account.
    #[must_use]
    pub fn user(&self) -> UserAccountId {
        self.user
    }

    /// RP-ID echo.
    #[must_use]
    pub fn rp_id(&self) -> &RpId {
        &self.rp_id
    }

    /// RP display name.
    #[must_use]
    pub fn rp_name(&self) -> &str {
        &self.rp_name
    }

    /// Authenticator selection criteria.
    #[must_use]
    pub fn attachment(&self) -> AuthenticatorAttachment {
        self.attachment
    }

    /// TTL milestone in unix-ms.
    #[must_use]
    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

/// Authentication ceremony challenge (W3C `PublicKeyCredentialRequestOptions`).
#[derive(Debug, Clone)]
pub struct AuthenticationChallenge {
    id: ChallengeId,
    bytes: ChallengeBytes,
    user: UserAccountId,
    rp_id: RpId,
    allowed_credentials: Vec<CredentialId>,
    require_user_verification: bool,
    expires_at_ms: u64,
    /// Marker: did the engine route this as an admin step-up
    /// ceremony? Drives UV requirement at finish-time.
    admin_step_up: bool,
}

impl AuthenticationChallenge {
    /// Internal constructor.
    #[allow(clippy::too_many_arguments, reason = "internal-only constructor; readable param order")]
    pub(crate) fn new(
        id: ChallengeId,
        bytes: ChallengeBytes,
        user: UserAccountId,
        rp_id: RpId,
        allowed_credentials: Vec<CredentialId>,
        require_user_verification: bool,
        expires_at_ms: u64,
        admin_step_up: bool,
    ) -> Self {
        Self {
            id,
            bytes,
            user,
            rp_id,
            allowed_credentials,
            require_user_verification,
            expires_at_ms,
            admin_step_up,
        }
    }

    /// Stable id.
    #[must_use]
    pub fn id(&self) -> &ChallengeId {
        &self.id
    }

    /// Nonce.
    #[must_use]
    pub fn bytes(&self) -> &ChallengeBytes {
        &self.bytes
    }

    /// User account.
    #[must_use]
    pub fn user(&self) -> UserAccountId {
        self.user
    }

    /// RP-ID.
    #[must_use]
    pub fn rp_id(&self) -> &RpId {
        &self.rp_id
    }

    /// Allowed-credentials list (W3C §5.1.4 `allowCredentials`).
    #[must_use]
    pub fn allowed_credentials(&self) -> &[CredentialId] {
        &self.allowed_credentials
    }

    /// UV-required flag (true for admin step-up, configurable for
    /// non-admin authentication).
    #[must_use]
    pub fn requires_user_verification(&self) -> bool {
        self.require_user_verification
    }

    /// TTL milestone.
    #[must_use]
    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    /// Admin-step-up flag — drives the strict UV requirement at
    /// finish-time.
    #[must_use]
    pub fn is_admin_step_up(&self) -> bool {
        self.admin_step_up
    }
}

/// Stored challenge record (used by [`crate::ChallengeStore`]).
#[derive(Debug, Clone)]
pub struct StoredChallenge {
    /// Lookup id.
    pub id: ChallengeId,
    /// Nonce; used to bind the response to the challenge.
    pub bytes_hex: String,
    /// User the challenge was issued for.
    pub user: UserAccountId,
    /// Whether this is a registration or authentication challenge.
    pub purpose: ChallengePurpose,
    /// For registration: the requested attachment.
    pub registration_attachment: Option<AuthenticatorAttachment>,
    /// For authentication: whether UV is required.
    pub require_user_verification: bool,
    /// For authentication: whether admin step-up was requested.
    pub admin_step_up: bool,
    /// For authentication: list of expected credentials.
    pub allowed_credentials: Vec<CredentialId>,
    /// Optional anchor AAGUID — for registration we may pre-pick the
    /// expected attachment.
    pub expected_aaguid: Option<Aaguid>,
    /// Optional anchor origin.
    pub expected_origin_in_allowlist: Option<Origin>,
    /// Expiry.
    pub expires_at_ms: u64,
}

/// Discriminator between the two ceremony types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChallengePurpose {
    /// Registration ceremony.
    Registration,
    /// Authentication ceremony.
    Authentication,
}

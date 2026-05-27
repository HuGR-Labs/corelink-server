//! Step-up token primitives consumed by the Tower middleware
//! (`crates/corelink-worker/src/middleware/webauthn_step_up.rs` —
//! lands in WI-S13-XXX admin plane).
//!
//! A step-up token binds three things atomically: the user account,
//! the canonical credential id used in the ceremony, and the admin
//! op class (e.g., `mass_revoke`, `billing_change`, `tenant_delete`).
//! TTL is 5 min by default per `WI-S03-006 §6.1.4 + §9.6`.

use std::fmt;
use std::time::Duration;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::credential::CredentialId;
use super::types::UserAccountId;
use super::WebAuthnError;

/// Default step-up TTL — 5 min per `WI-S03-006 §6.1.4`.
pub const STEP_UP_TTL_DEFAULT: Duration = Duration::from_secs(300);

/// Stable id surface for the step-up token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StepUpTokenId(pub Uuid);

impl StepUpTokenId {
    /// Mint a fresh time-ordered identifier.
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

/// Step-up token (opaque).
///
/// Drops scrub the inner secret bytes; equality is constant-time.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct StepUpToken {
    /// Stable id (used as the lookup key).
    #[zeroize(skip)]
    id: StepUpTokenId,
    /// Bound user account.
    #[zeroize(skip)]
    user: UserAccountId,
    /// Bound credential id.
    #[zeroize(skip)]
    credential_id: CredentialId,
    /// Bound admin-op class label (e.g. "mass_revoke",
    /// "billing_change"). Free-form string — the caller validates
    /// against an enum-like allowlist before issuing.
    #[zeroize(skip)]
    op_class: String,
    /// CSPRNG-drawn 32-byte secret.
    secret: Vec<u8>,
    /// Expiry (unix-ms).
    #[zeroize(skip)]
    expires_at_ms: u64,
}

impl fmt::Debug for StepUpToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StepUpToken")
            .field("id", &self.id)
            .field("user", &self.user)
            .field("credential_id", &self.credential_id)
            .field("op_class", &self.op_class)
            .field("secret", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl StepUpToken {
    /// Construct a fresh token.
    pub fn new(
        user: UserAccountId,
        credential_id: CredentialId,
        op_class: impl Into<String>,
        ttl: Duration,
        now_ms: u64,
    ) -> Result<Self, WebAuthnError> {
        let mut secret = vec![0u8; 32];
        OsRng
            .try_fill_bytes(&mut secret)
            .map_err(|_| WebAuthnError::EntropyUnavailable)?;
        let ttl_ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX);
        let expires_at_ms = now_ms.saturating_add(ttl_ms);
        Ok(Self {
            id: StepUpTokenId::new_v7(),
            user,
            credential_id,
            op_class: op_class.into(),
            secret,
            expires_at_ms,
        })
    }

    /// Token id.
    #[must_use]
    pub fn id(&self) -> StepUpTokenId {
        self.id
    }

    /// Bound user.
    #[must_use]
    pub fn user(&self) -> UserAccountId {
        self.user
    }

    /// Bound credential.
    #[must_use]
    pub fn credential_id(&self) -> &CredentialId {
        &self.credential_id
    }

    /// Bound op class.
    #[must_use]
    pub fn op_class(&self) -> &str {
        &self.op_class
    }

    /// Expiry.
    #[must_use]
    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    /// Whether the token is still within its TTL.
    #[must_use]
    pub fn is_active(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }

    /// Verify (a) the secret matches via [`subtle::ConstantTimeEq`],
    /// (b) the TTL has not elapsed, (c) the op class matches, and
    /// (d) the user matches. Mismatch on any axis returns
    /// [`WebAuthnError::StepUpRequired`] (caller renders a 403 +
    /// re-prompts).
    pub fn validate(
        &self,
        candidate_secret: &[u8],
        expected_user: UserAccountId,
        expected_op_class: &str,
        now_ms: u64,
    ) -> Result<(), WebAuthnError> {
        let secret_match: bool = self.secret.ct_eq(candidate_secret).into();
        let user_match = self.user == expected_user;
        let op_match = self.op_class == expected_op_class;
        if !self.is_active(now_ms) || !secret_match || !user_match || !op_match {
            return Err(WebAuthnError::StepUpRequired);
        }
        Ok(())
    }

    /// Borrow the secret bytes (used by the Tower middleware to mint
    /// the cookie value; never logged).
    #[must_use]
    pub fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }
}

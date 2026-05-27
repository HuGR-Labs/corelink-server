//! Shared opaque newtypes for the WebAuthn ceremony surface.
//!
//! Newtypes prevent accidental cross-table key leakage (a
//! `UserAccountId` in `webauthn_credentials.user_account_id` MUST NOT
//! be confused with a `TenantId` in `pat.tenant_id`) and let property
//! tests build canonical fixtures without leaking implementation
//! details.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User-account identifier (`webauthn_credentials.user_account_id` FK
/// → `user_account.user_id`). Mirrors the canonical UUIDv7 surface
/// established by `corelink-auth-schema`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserAccountId(pub Uuid);

impl UserAccountId {
    /// Construct a fresh time-ordered identifier.
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Debug for UserAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UserAccountId({})", self.0)
    }
}

impl fmt::Display for UserAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

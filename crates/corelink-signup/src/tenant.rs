//! Tenant / signup identifier + user email hash canonical newtypes.
//!
//! Per CTRL-PRIV-001 + spec contract S-19 §26 STRIDE Information
//! Disclosure: `user_email` is NEVER persisted plaintext; only
//! `UserEmailHash` (sha256 hex of normalized email) appears in
//! `signup_orchestration` rows + audit events. The orchestrator carries
//! the hash through the entire pipeline; production wiring computes the
//! hash inside the Cloudflare Worker handler from the Clerk webhook
//! payload before reaching this crate.

/// Tenant identifier — UUID v7 string in production wiring.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(String);

impl TenantId {
    /// Construct a tenant id from any string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for TenantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Signup identifier — opaque token returned to the API caller and
/// reused across idempotent retries (same `IdempotencyKey` → same
/// `SignupId`). UUID v7 string in production wiring.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignupId(String);

impl SignupId {
    /// Construct a signup id from any string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SignupId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// User-email hash — sha256 hex of the normalized email (RFC 5321 +
/// lowercase). Production wiring computes this in the Worker handler
/// before invoking the orchestrator. The crate carries only the hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserEmailHash(String);

impl UserEmailHash {
    /// Construct a hash newtype. Caller MUST pass a 64-char lowercase
    /// hex sha256 digest in production wiring; this crate intentionally
    /// does not validate format so tests can use short fixture values.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying hex digest string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for UserEmailHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn newtypes_round_trip() {
        let t = TenantId::new("t-1");
        let s = SignupId::new("s-1");
        let h = UserEmailHash::new("deadbeef");
        assert_eq!(t.as_str(), "t-1");
        assert_eq!(s.as_str(), "s-1");
        assert_eq!(h.as_str(), "deadbeef");
        assert_eq!(t.to_string(), "t-1");
        assert_eq!(s.to_string(), "s-1");
        assert_eq!(h.to_string(), "deadbeef");
    }
}

//! [`ClerkPrincipal`] + opaque newtype identifiers.
//!
//! WI-S03-001 §9.8 mandates type-driven security: distinct newtypes
//! around the canonical `String`-typed Clerk identifiers prevent
//! callers from accidentally swapping a `user_id` with an `org_id`
//! at a function boundary. Constructors are `pub(crate)` so the only
//! way to obtain a `ClerkUserId` is to round-trip a JWT through
//! [`crate::ClerkAdapter::validate`] — caller cannot forge.

use std::fmt;
use std::time::SystemTime;
use thiserror::Error;

/// Opaque Clerk user identifier (the `sub` claim).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ClerkUserId(String);

impl ClerkUserId {
    /// Internal constructor; the only path to a `ClerkUserId` is
    /// through a successful [`crate::ClerkAdapter::validate`] call.
    #[must_use]
    pub(crate) fn new(s: String) -> Self {
        Self(s)
    }

    /// Canonical string form for audit-log emission downstream.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Custom `Debug` redacts the raw `sub` (PII) per WI §26 / privacy_model.
impl fmt::Debug for ClerkUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClerkUserId(<redacted len={}>)", self.0.len())
    }
}

/// Opaque Clerk organisation identifier (the `org_id` custom claim).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ClerkOrgId(String);

impl ClerkOrgId {
    #[must_use]
    pub(crate) fn new(s: String) -> Self {
        Self(s)
    }

    /// Canonical string form for audit-log emission.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClerkOrgId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClerkOrgId(<redacted len={}>)", self.0.len())
    }
}

/// Opaque Clerk session identifier (the `sid` claim).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ClerkSessionId(String);

impl ClerkSessionId {
    #[must_use]
    pub(crate) fn new(s: String) -> Self {
        Self(s)
    }

    /// Canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClerkSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClerkSessionId(<redacted len={}>)", self.0.len())
    }
}

/// Verified email surfaced from JWT claim. Validated to look like
/// `local@domain` (presence of `@` and non-empty halves) — no full
/// RFC 5322 grammar; Clerk already enforces validity.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

/// Errors surfaced by [`Email::parse`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmailParseError {
    /// Input was empty.
    #[error("email empty")]
    Empty,
    /// Input did not contain exactly one `@` separator with non-empty
    /// local and domain parts.
    #[error("email malformed: {0}")]
    Malformed(&'static str),
}

impl Email {
    /// Parse an email surface (claim string). Returns
    /// [`EmailParseError`] for empties / malformed inputs.
    pub fn parse(s: &str) -> Result<Self, EmailParseError> {
        if s.is_empty() {
            return Err(EmailParseError::Empty);
        }
        let parts: Vec<&str> = s.splitn(2, '@').collect();
        if parts.len() != 2 {
            return Err(EmailParseError::Malformed("missing @"));
        }
        // Indexing-slicing is denied by crate lints; use first/last.
        let (local, domain) = match (parts.first(), parts.last()) {
            (Some(l), Some(d)) => (*l, *d),
            _ => return Err(EmailParseError::Malformed("split failed")),
        };
        if local.is_empty() {
            return Err(EmailParseError::Malformed("empty local"));
        }
        if domain.is_empty() || !domain.contains('.') {
            return Err(EmailParseError::Malformed("invalid domain"));
        }
        Ok(Self(s.to_owned()))
    }

    /// Canonical lowercased domain part.
    #[must_use]
    pub fn domain_lowercase(&self) -> String {
        self.0
            .split_once('@')
            .map_or("", |(_, domain)| domain)
            .to_ascii_lowercase()
    }

    /// Canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redact local-part; surface domain only for diagnosis.
        let domain = self
            .0
            .split_once('@')
            .map_or("<no-domain>", |(_, d)| d);
        write!(f, "Email(<redacted>@{domain})")
    }
}

/// Clerk role surfaced via custom `clerk_role` claim. Defaults to
/// [`ClerkRole::Member`] when absent (Clerk free tier emits no role
/// claim for individual user tokens).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClerkRole {
    /// Organisation administrator.
    Admin,
    /// Standard organisation member.
    Member,
    /// Guest / read-only.
    Guest,
}

impl ClerkRole {
    /// Map a claim string to a canonical role. Unknown values fold to
    /// [`ClerkRole::Guest`] — fail-safe least-privilege.
    #[must_use]
    pub fn from_claim(s: Option<&str>) -> Self {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("admin") => Self::Admin,
            Some("member") => Self::Member,
            None => Self::Member,
            _ => Self::Guest,
        }
    }
}

/// Validated principal surfaced from a successful JWT validation.
///
/// All identifiers are opaque newtypes; PII fields (`sub`, `email`)
/// are wrapped to redact in `Debug`.
#[derive(Clone, Debug)]
pub struct ClerkPrincipal {
    /// Opaque Clerk user id (the `sub` claim).
    pub user_id: ClerkUserId,
    /// Optional Clerk organisation id (`org_id` custom claim).
    pub org_id: Option<ClerkOrgId>,
    /// Verified email claim.
    pub email: Email,
    /// Role claim.
    pub clerk_role: ClerkRole,
    /// Session id (`sid` claim).
    pub session_id: ClerkSessionId,
    /// Issued-at timestamp (`iat`).
    pub issued_at: SystemTime,
    /// Expires-at timestamp (`exp`).
    pub expires_at: SystemTime,
}

//! PII redaction primitives — [`PrincipalIdHash`] / [`PatIdHash`] /
//! [`EmailHash`] newtypes + [`redact_pat_str`] helper + the
//! [`crate::redact_pat`] macro.
//!
//! ## Design rationale (INV-AUDIT-NO-RAW-PII, CRITICAL)
//!
//! Every hash newtype:
//!
//! - has **no public `From<String>` / `new(&str)`** that takes raw PII —
//!   the only constructor is [`PrincipalIdHash::derive`] /
//!   [`PatIdHash::derive`] / [`EmailHash::derive`], which runs SHA-256
//!   over the input and stores the **first 16 hex chars (64-bit
//!   pseudonymous prefix)**.
//! - implements `Display` + serde-`Serialize` to render the canonical
//!   16-char hex string only — never the raw plaintext.
//! - rejects empty input with [`crate::AuditError::EmptyHashInput`]
//!   because the SHA-256 of an empty string is a well-known constant
//!   that would collapse pseudonymity across tenants.
//!
//! The 64-bit prefix is sufficient for cross-event correlation (the
//! birthday bound at CoreLink scale is ~4 B distinct principals per
//! 50% collision probability — orders of magnitude beyond plausible
//! workload). Privileged-role reverse lookup is handled by a separate
//! S-09 pipeline that has access to the global principal_id index;
//! that lookup is itself audited, closing the forensic loop without
//! exposing raw PII in the chain.

use core::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::AuditError;

/// Canonical placeholder string substituted by the [`crate::redact_pat`]
/// macro when caller code attempts to format a raw PAT secret. CI lint
/// (`tools/audit_pii_lint/`, follow-up) flags any other path that would
/// land raw PAT text into a serialized event.
pub const REDACTED_PAT_PLACEHOLDER: &str = "[REDACTED-PAT]";

/// Length of the canonical 16-hex-char (64-bit) pseudonymous prefix.
const HASH_HEX_LEN: usize = 16;

/// Pseudonymous principal id hash. Derived from the raw Clerk
/// `principal_id` (or any principal-identifier string) via SHA-256
/// truncated to the first 16 hex chars.
///
/// Example: `PrincipalIdHash::derive("user_2NkX8a3Bq")` → 16-char hex
/// such as `"a1b2c3d4e5f60718"` (deterministic; same input → same hash).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PrincipalIdHash(String);

impl PrincipalIdHash {
    /// Derive a [`PrincipalIdHash`] from a raw principal id string.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError::EmptyHashInput`] if `raw` is empty.
    pub fn derive(raw: &str) -> Result<Self, AuditError> {
        Ok(Self(prefix_hex_sha256("principal_id", raw)?))
    }

    /// Borrow the canonical 16-hex-char string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalIdHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Pseudonymous PAT id hash. Derived from the raw `pat_id` string via
/// SHA-256 truncated to the first 16 hex chars.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PatIdHash(String);

impl PatIdHash {
    /// Derive a [`PatIdHash`] from a raw PAT id string.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError::EmptyHashInput`] if `raw` is empty.
    pub fn derive(raw: &str) -> Result<Self, AuditError> {
        Ok(Self(prefix_hex_sha256("pat_id", raw)?))
    }

    /// Borrow the canonical 16-hex-char string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PatIdHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Pseudonymous email hash. Derived from the raw email string via
/// SHA-256 truncated to the first 16 hex chars.
///
/// **Note**: the production email hash for the `user_account` row uses
/// HKDF-SHA256 with a per-tenant salt (`corelink-auth-schema::EmailHashKey`)
/// to defeat rainbow-table attacks. The audit-chain pseudonym intentionally
/// uses the **un-salted prefix** so cross-tenant forensic correlation
/// (e.g. "the same email logged into N tenants under different principals")
/// remains tractable. This is documented in `privacy_model.md §3.5`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct EmailHash(String);

impl EmailHash {
    /// Derive an [`EmailHash`] from a raw email string.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError::EmptyHashInput`] if `raw` is empty.
    pub fn derive(raw: &str) -> Result<Self, AuditError> {
        Ok(Self(prefix_hex_sha256("email", raw)?))
    }

    /// Borrow the canonical 16-hex-char string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmailHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Return the canonical placeholder string for a raw PAT plaintext.
///
/// The macro [`crate::redact_pat`] expands to a call to this function
/// at every audit-format call site so an accidental `format!("{}", pat)`
/// becomes a clippy / CI-lint signal rather than a silent leak.
#[must_use]
pub const fn redact_pat_str(_plaintext: &str) -> &'static str {
    REDACTED_PAT_PLACEHOLDER
}

/// Compile-time-checked redaction macro for PAT plaintext.
///
/// Expands to the canonical placeholder string regardless of input.
/// CI grep lint (`tools/audit_pii_lint/`, follow-up) enforces use of
/// this macro at every audit-emit call site that touches a PAT.
///
/// ```
/// use corelink_audit::redact_pat;
/// let pat = "corelink_pat_live_xyz.secret.sig";
/// assert_eq!(redact_pat!(pat), "[REDACTED-PAT]");
/// ```
#[macro_export]
macro_rules! redact_pat {
    ($plaintext:expr) => {{
        // Touch the input so call sites that pass a local binding
        // don't fire `unused_variables`; never inspect the value, so
        // the raw secret stays unobservable from this expansion.
        let _ = &$plaintext;
        $crate::redact::REDACTED_PAT_PLACEHOLDER
    }};
}

/// Compute SHA-256 of `raw` and return the first 16 hex chars (64-bit
/// pseudonymous prefix). Rejects empty input.
fn prefix_hex_sha256(kind: &'static str, raw: &str) -> Result<String, AuditError> {
    if raw.is_empty() {
        return Err(AuditError::EmptyHashInput { kind });
    }
    let digest = Sha256::digest(raw.as_bytes());
    // Encode the 32-byte digest as 64 lowercase hex chars, then truncate
    // to the canonical 16-char prefix. `hex::encode` produces a `String`
    // whose contents are pure ASCII hex; slicing by `HASH_HEX_LEN` chars
    // is byte-equivalent to slicing by `HASH_HEX_LEN` bytes.
    let full = hex::encode(digest);
    // `full.len() == 64` by construction (SHA-256 → 32 bytes → 64 hex).
    // Pull the first `HASH_HEX_LEN` bytes via `.get(..HASH_HEX_LEN)` to
    // satisfy the `indexing_slicing = "deny"` lint without panicking.
    let prefix = full
        .get(..HASH_HEX_LEN)
        .ok_or(AuditError::Canonicalization(
            "sha256 hex output shorter than 16 chars (impossible)".to_string(),
        ))?;
    Ok(prefix.to_string())
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

    #[test]
    fn derive_principal_id_is_deterministic() {
        let a = PrincipalIdHash::derive("user_2NkX8a3Bq").expect("derive");
        let b = PrincipalIdHash::derive("user_2NkX8a3Bq").expect("derive");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 16);
        // Verify hex alphabet (lowercase 0-9a-f).
        for ch in a.as_str().chars() {
            assert!(
                ch.is_ascii_hexdigit() && (!ch.is_alphabetic() || ch.is_lowercase()),
                "non-canonical hex char: {ch}"
            );
        }
    }

    #[test]
    fn derive_principal_id_diverges_on_distinct_inputs() {
        let a = PrincipalIdHash::derive("alice").expect("derive");
        let b = PrincipalIdHash::derive("bob").expect("derive");
        assert_ne!(a, b);
    }

    #[test]
    fn derive_empty_principal_rejected() {
        let err = PrincipalIdHash::derive("").expect_err("must reject empty");
        match err {
            AuditError::EmptyHashInput { kind } => assert_eq!(kind, "principal_id"),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn derive_pat_id_distinct_namespace_from_principal_id() {
        // Distinct newtypes should not be comparable, but the underlying
        // hex prefix should still match for the same input — they share
        // the SHA-256 derivation.
        let p = PrincipalIdHash::derive("foo").expect("derive");
        let q = PatIdHash::derive("foo").expect("derive");
        assert_eq!(p.as_str(), q.as_str());
    }

    #[test]
    fn redact_pat_macro_replaces_input() {
        assert_eq!(
            redact_pat!("corelink_pat_live_xxx.yyy.zzz"),
            "[REDACTED-PAT]"
        );
        assert_eq!(redact_pat!(""), "[REDACTED-PAT]");
    }

    #[test]
    fn email_hash_derive_works() {
        let h = EmailHash::derive("user@example.com").expect("derive");
        assert_eq!(h.as_str().len(), 16);
        let h2 = EmailHash::derive("user@example.com").expect("derive");
        assert_eq!(h, h2);
    }

    #[test]
    fn email_hash_empty_rejected() {
        let err = EmailHash::derive("").expect_err("must reject empty");
        match err {
            AuditError::EmptyHashInput { kind } => assert_eq!(kind, "email"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}

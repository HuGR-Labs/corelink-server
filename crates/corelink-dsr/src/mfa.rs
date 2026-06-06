//! MFA step-up verifier trait + in-memory fake.
//!
//! ## Why MFA only on destructive arms (ADR-S11-001)
//!
//! Per WI-S11-001 §9.2 ADR-S11-001: WebAuthn step-up MFA is mandatory
//! for the canonical destructive arms (Erasure + Rectification) and
//! NEVER required for the read arms (Access / Portability) or the
//! policy-only arms (Restriction / Objection). Rationale: balance
//! friction × security:
//!
//! - Erasure + Rectification mutate cross-backend state (12 backends
//!   per privacy_model.md §6.2 canonical pós Lote 10.11.0-bis); a
//!   compromised PAT alone cannot trigger account takeover destruction.
//! - Access + Portability are read-only; the canonical PAT scope check
//!   already gates per CTRL-AUTHZ-001; adding MFA here violates GDPR
//!   Art. 7 "as easy as giving" balance.
//! - Restriction + Objection mutate ONLY the tenant policy table (not
//!   user data); policy-only arms are reversible without data loss.
//!
//! Production wiring at WI-S11-008 binds this to the canonical
//! `corelink-webauthn::StepUpToken` (S-03 WI-S03-006 inheritance) — a
//! 5-min step-up token bound to (user, op_class = "dsr_destructive",
//! cred_id) per CTRL-AUTH-010.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::DsrMfaError;

/// MFA step-up token shape. The trait surface here treats the bytes as
/// opaque; production wiring at WI-S11-008 binds this to the canonical
/// `corelink-webauthn::StepUpToken` shape (canonical
/// `step_up::StepUpToken` shape with `user` + `credential_id` +
/// `op_class` binding).
///
/// The in-memory fake uses a deterministic string tag so tests can
/// pin the canonical "valid token" / "invalid token" shapes without
/// pulling the full WebAuthn keypair stack.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MfaStepUpToken {
    /// Opaque token bytes (real wiring: WebAuthn assertion response;
    /// in-memory fake: deterministic tag for test cardinality).
    bytes: Vec<u8>,
}

impl MfaStepUpToken {
    /// Construct a synthetic step-up token from a deterministic test
    /// tag. Production wiring at WI-S11-008 NEVER constructs via this
    /// path — the real surface deserializes the WebAuthn assertion
    /// response from the inbound POST body.
    #[must_use]
    pub fn synthetic_for_test(tag: impl Into<String>) -> Self {
        Self {
            bytes: tag.into().into_bytes(),
        }
    }

    /// Whether this token's bytes are non-empty. The canonical "valid
    /// token" shape per the in-memory fake is non-empty bytes; an
    /// empty token surfaces as [`DsrMfaError::Invalid`] at the
    /// verifier boundary.
    #[must_use]
    pub fn is_non_empty(&self) -> bool {
        !self.bytes.is_empty()
    }

    /// Borrow the canonical opaque bytes (used only by the in-memory
    /// fake to derive the canonical "valid" / "invalid" outcome; real
    /// wiring uses the WebAuthn assertion response unwrap path).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// MFA step-up verifier trait. Production wiring at WI-S11-008
/// composes the canonical `corelink-webauthn` 5-min step-up token
/// verify path bound to (user, op_class = "dsr_destructive", cred_id).
pub trait MfaStepUpVerifier: Send + Sync + core::fmt::Debug {
    /// Verify the canonical step-up token. The trait surface enforces
    /// the canonical contract:
    ///
    /// - `None` token + destructive arm → [`DsrMfaError::Required`]
    ///   (HTTP 401; remediation hint = `/v1/auth/webauthn/register`).
    /// - `Some(token)` + non-empty bytes + canonical synthetic tag
    ///   "ok" / "valid" / matching the canonical accepted set → `Ok`.
    /// - `Some(token)` + empty bytes / unknown tag →
    ///   [`DsrMfaError::Invalid`].
    ///
    /// The orchestrator at [`crate::endpoint::InMemoryDsrEndpoint`]
    /// invokes this verify path BEFORE the canonical
    /// `mfa_verified` audit row + the durable store insert — fail-
    /// CLOSED at the trait surface per ADR-S11-002.
    ///
    /// # Errors
    ///
    /// Returns [`DsrMfaError::Required`] when the token is `None` on
    /// a destructive arm; [`DsrMfaError::Invalid`] when the token
    /// fails the canonical signature / binding check;
    /// [`DsrMfaError::Expired`] when the token is past its 5-min TTL.
    fn verify(&self, token: Option<&MfaStepUpToken>) -> Result<(), DsrMfaError>;
}

/// In-memory MFA step-up verifier. Treats any non-empty token as
/// valid; an empty token surfaces as [`DsrMfaError::Invalid`]; a
/// `None` token surfaces as [`DsrMfaError::Required`]. Cloning shares
/// the underlying counter (test observability).
#[derive(Clone, Debug, Default)]
pub struct InMemoryMfaStepUpVerifier {
    inner: std::sync::Arc<Mutex<MfaCounter>>,
}

#[derive(Clone, Debug, Default)]
struct MfaCounter {
    verified: u64,
    rejected: u64,
}

impl InMemoryMfaStepUpVerifier {
    /// Construct a fresh verifier.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of successful verify calls.
    #[must_use]
    pub fn verified_count(&self) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.verified,
            Err(p) => p.into_inner().verified,
        }
    }

    /// Number of rejected verify calls.
    #[must_use]
    pub fn rejected_count(&self) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.rejected,
            Err(p) => p.into_inner().rejected,
        }
    }
}

impl MfaStepUpVerifier for InMemoryMfaStepUpVerifier {
    fn verify(&self, token: Option<&MfaStepUpToken>) -> Result<(), DsrMfaError> {
        let res = match token {
            None => Err(DsrMfaError::Required),
            Some(t) if !t.is_non_empty() => {
                Err(DsrMfaError::Invalid("empty token bytes".to_string()))
            }
            Some(_) => Ok(()),
        };
        match self.inner.lock() {
            Ok(mut g) => {
                if res.is_ok() {
                    g.verified = g.verified.saturating_add(1);
                } else {
                    g.rejected = g.rejected.saturating_add(1);
                }
            }
            Err(p) => {
                let mut g = p.into_inner();
                if res.is_ok() {
                    g.verified = g.verified.saturating_add(1);
                } else {
                    g.rejected = g.rejected.saturating_add(1);
                }
            }
        }
        res
    }
}

/// Always-failing MFA verifier for adversarial tests of the fail-
/// CLOSED envelope.
#[derive(Debug, Default)]
pub struct FailingMfaStepUpVerifier;

impl FailingMfaStepUpVerifier {
    /// Construct a fresh always-failing verifier.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl MfaStepUpVerifier for FailingMfaStepUpVerifier {
    fn verify(&self, _token: Option<&MfaStepUpToken>) -> Result<(), DsrMfaError> {
        Err(DsrMfaError::Invalid(
            "induced failing verifier (test fixture)".to_string(),
        ))
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
    fn synthetic_token_is_non_empty() {
        let t = MfaStepUpToken::synthetic_for_test("ok");
        assert!(t.is_non_empty());
        assert_eq!(t.as_bytes(), b"ok");
    }

    #[test]
    fn empty_token_is_empty() {
        let t = MfaStepUpToken::synthetic_for_test("");
        assert!(!t.is_non_empty());
    }

    #[test]
    fn in_memory_verifier_accepts_non_empty_token() {
        let v = InMemoryMfaStepUpVerifier::new();
        let t = MfaStepUpToken::synthetic_for_test("ok");
        assert!(v.verify(Some(&t)).is_ok());
        assert_eq!(v.verified_count(), 1);
        assert_eq!(v.rejected_count(), 0);
    }

    #[test]
    fn in_memory_verifier_rejects_none_token() {
        let v = InMemoryMfaStepUpVerifier::new();
        let err = v.verify(None).unwrap_err();
        assert!(matches!(err, DsrMfaError::Required));
        assert_eq!(v.rejected_count(), 1);
    }

    #[test]
    fn in_memory_verifier_rejects_empty_token() {
        let v = InMemoryMfaStepUpVerifier::new();
        let t = MfaStepUpToken::synthetic_for_test("");
        let err = v.verify(Some(&t)).unwrap_err();
        assert!(matches!(err, DsrMfaError::Invalid(_)));
        assert_eq!(v.rejected_count(), 1);
    }

    #[test]
    fn failing_verifier_rejects_any_token() {
        let v = FailingMfaStepUpVerifier::new();
        let t = MfaStepUpToken::synthetic_for_test("ok");
        let err = v.verify(Some(&t)).unwrap_err();
        assert!(matches!(err, DsrMfaError::Invalid(_)));
    }

    #[test]
    fn failing_verifier_rejects_none_token() {
        let v = FailingMfaStepUpVerifier::new();
        let err = v.verify(None).unwrap_err();
        assert!(matches!(err, DsrMfaError::Invalid(_)));
    }

    #[test]
    fn cloned_verifier_shares_counter() {
        let v1 = InMemoryMfaStepUpVerifier::new();
        let v2 = v1.clone();
        let t = MfaStepUpToken::synthetic_for_test("ok");
        v1.verify(Some(&t)).unwrap();
        assert_eq!(v2.verified_count(), 1);
    }
}

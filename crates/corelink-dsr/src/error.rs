//! Canonical error taxonomy for the `corelink-dsr` crate.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline +
//! S-09 / S-10 inheritance).
//!
//! ## Fail policy boundary (ADR-S11-002)
//!
//! Per WI-S11-001 §1 invariant + ADR-S11-002 (split-tier discipline
//! canonical) the DSR self-service surface is **fail-CLOSED**:
//! regulatory integrity outranks availability. Audit envelope failure
//! aborts the request; no `DsrRequestStore` insert; no JWT receipt
//! issued; no `mfa_verified` audit row. Distinct from
//! `corelink-billing-emit` (fail-OPEN at the customer hot path) — the
//! DSR self-service primitive is regulatory-grade and silent loss of a
//! DSR submission is an LGPD Art. 18 violation (multa 2% revenue).

use thiserror::Error;

/// Audit sink failure surface (lifted into [`DsrError::Audit`]).
///
/// The audit envelope (`corelink.dsr.{request_received,
/// mfa_step_up_required, mfa_verified, request_accepted, receipt_issued,
/// request_rejected, status_polled}`) emits BEFORE state mutation per
/// the canonical S-07 P1-1 fix + Lote 10.6bis pattern: audit failure
/// aborts the request; the durable store remains at its pre-call
/// state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DsrAuditSinkError {
    /// Backend transport failure (S-09 audit chain append rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`DsrError::Audit`] and returns to the caller; no
    /// store mutation, no MFA verify, no receipt issued.
    #[error("dsr audit sink store error: {0}")]
    Store(String),
}

/// Durable store failure surface (lifted into [`DsrError::Store`]).
///
/// Production wiring at WI-S11-008 binds this to the canonical Neon
/// `dsr_tickets` table per data_model.md §4.1 (7-state machine; UUIDv7
/// PK; idempotency UNIQUE quad
/// `(tenant_id, subject_user_id, request_kind, request_payload_hash)`).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DsrStoreError {
    /// Backend transport failure (Neon INSERT rejected / network
    /// partition / batch failure).
    #[error("dsr store backend error: {0}")]
    Backend(String),

    /// A ticket with the same (tenant_id, request_id) already exists
    /// with a divergent payload (idempotency UNIQUE quad collision
    /// where the request body differs while the request_id matches).
    /// Surfaces as a tampering signal: the canonical idempotency
    /// contract is "same request_id → same payload"; a replayed
    /// request_id paired with a different (tenant, subject_id,
    /// request_kind) tuple is a SEV-1 forensic anomaly.
    #[error("dsr store detected divergent payload for request_id={0}")]
    DivergentPayload(String),
}

/// JWT receipt failure surface (lifted into [`DsrError::Receipt`]).
///
/// Production wiring at WI-S11-008 binds this to the real RS256 sign /
/// verify surface via `jsonwebtoken = 9.3` reused from `corelink-clerk`
/// (S-03 inheritance) + KMS-backed key rotation (HKDF info=`corelink/
/// v1/dsr-receipt`).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DsrReceiptError {
    /// JWT signing key unavailable / rotation in progress / KMS
    /// fetch failed.
    #[error("dsr receipt signing failure: {0}")]
    Signing(String),

    /// JWT signature verification failed (tampered receipt / wrong
    /// key / forgery attempt).
    #[error("dsr receipt signature verification failed")]
    SignatureInvalid,

    /// JWT receipt has expired (now > exp claim) — anti-replay window
    /// is `submitted_at + 90d` per the canonical
    /// [`crate::event::RECEIPT_EXPIRY_DAYS`] cap.
    #[error("dsr receipt expired")]
    Expired,

    /// JWT receipt structurally malformed (header / claims /
    /// signature segment missing or invalid base64url).
    #[error("dsr receipt malformed: {0}")]
    Malformed(String),
}

/// MFA step-up failure surface (lifted into [`DsrError::Mfa`]).
///
/// Production wiring at WI-S11-008 binds this to the canonical
/// `corelink-webauthn::StepUpToken` (S-03 WI-S03-006 inheritance) — a
/// 5-min step-up token bound to (user, op_class, cred_id) per
/// CTRL-AUTH-010.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DsrMfaError {
    /// Step-up token absent on a destructive arm (Erasure +
    /// Rectification require MFA per ADR-S11-001). HTTP 401 surface;
    /// remediation hint = `/v1/auth/webauthn/register`.
    #[error("dsr mfa step-up token required for destructive arm")]
    Required,

    /// Step-up token malformed / signature invalid / not bound to the
    /// canonical (user, op_class) tuple.
    #[error("dsr mfa step-up token invalid: {0}")]
    Invalid(String),

    /// Step-up token expired (older than the canonical 5-min TTL per
    /// `corelink-webauthn::STEP_UP_TTL_DEFAULT`).
    #[error("dsr mfa step-up token expired")]
    Expired,
}

/// Canonical error surface returned by the [`crate::endpoint`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DsrError {
    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per ADR-S11-002 split-tier pattern: no store insert, no MFA
    /// verify, no receipt issued.
    #[error("dsr audit envelope failure (state unchanged): {0}")]
    Audit(#[from] DsrAuditSinkError),

    /// Durable store failure (Neon INSERT rejected). Fail-CLOSED per
    /// ADR-S11-002 — regulatory integrity outranks availability; the
    /// run aborts so the auditor trail is never silently truncated.
    #[error("dsr store failure: {0}")]
    Store(#[from] DsrStoreError),

    /// JWT receipt issuer / verifier failure.
    #[error("dsr receipt failure: {0}")]
    Receipt(#[from] DsrReceiptError),

    /// MFA step-up gate failure.
    #[error("dsr mfa failure: {0}")]
    Mfa(#[from] DsrMfaError),

    /// Configuration invariant violated (e.g. tenant_id mismatch
    /// between request body + authenticated context, or unknown
    /// jurisdiction code). Programmer error; mapped to 5xx in
    /// production wiring.
    #[error("dsr configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex
    /// poisoned by a panicking run). Production wiring maps this to
    /// fail-CLOSED at the trait surface — the run aborts rather than
    /// risk corrupt state.
    #[error("dsr internal state invariant violated: {0}")]
    Internal(String),
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
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = DsrAuditSinkError::Store("induced".to_string());
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Audit(_)));
    }

    #[test]
    fn from_store_error_lifts_cleanly() {
        let inner = DsrStoreError::Backend("induced".to_string());
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Store(_)));
    }

    #[test]
    fn from_store_divergent_lifts_cleanly() {
        let inner = DsrStoreError::DivergentPayload("rid".to_string());
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Store(_)));
    }

    #[test]
    fn from_receipt_signing_lifts_cleanly() {
        let inner = DsrReceiptError::Signing("kms unavailable".to_string());
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Receipt(_)));
    }

    #[test]
    fn from_receipt_expired_lifts_cleanly() {
        let inner = DsrReceiptError::Expired;
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Receipt(_)));
    }

    #[test]
    fn from_mfa_required_lifts_cleanly() {
        let inner = DsrMfaError::Required;
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Mfa(_)));
    }

    #[test]
    fn from_mfa_invalid_lifts_cleanly() {
        let inner = DsrMfaError::Invalid("bad sig".to_string());
        let e: DsrError = inner.into();
        assert!(matches!(e, DsrError::Mfa(_)));
    }

    #[test]
    fn config_displays_diagnostic() {
        let e = DsrError::Config("tenant mismatch".to_string());
        let s = format!("{e}");
        assert!(s.contains("configuration invariant"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = DsrError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }

    #[test]
    fn audit_displays_lifted_inner() {
        let e: DsrError = DsrAuditSinkError::Store("inner-msg".to_string()).into();
        let s = format!("{e}");
        assert!(s.contains("audit envelope failure"));
    }

    #[test]
    fn receipt_signature_invalid_displays() {
        let e = DsrReceiptError::SignatureInvalid;
        let s = format!("{e}");
        assert!(s.contains("verification failed"));
    }

    #[test]
    fn mfa_expired_displays() {
        let e = DsrMfaError::Expired;
        let s = format!("{e}");
        assert!(s.contains("expired"));
    }
}

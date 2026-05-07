//! Canonical error taxonomy for the `corelink-erasure` crate.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline +
//! S-09 / S-10 / S-11 inheritance).
//!
//! ## Fail policy boundary (ADR-S11-002 split-tier)
//!
//! Per WI-S11-002 §1 invariant + ADR-S11-002 (split-tier discipline
//! canonical) the erasure cross-backend pipeline is **fail-CLOSED**:
//! regulatory integrity outranks availability. Audit envelope failure
//! aborts the pipeline; no backend mutation, no idempotency tombstone
//! insert, no report generation. Distinct from `corelink-billing-emit`
//! (fail-OPEN at the customer hot path) — the erasure pipeline is
//! regulatory-grade and silent loss of an erasure step is an LGPD
//! Art. 18 IV / GDPR Art. 17 violation (multa 2% revenue / 4% global).
//!
//! Pseudonymization preserves audit immutability via secondary index
//! update (the pseudonymized backend writes never delete the original
//! audit row — INV-AUDIT-APPEND-ONLY is preserved).

use thiserror::Error;

/// Audit sink failure surface (lifted into [`ErasureError::Audit`]).
///
/// The audit envelope (`corelink.erasure.{plan_generated,
/// fanout_initiated, backend_completed, verification_started,
/// verified_complete, verified_partial, sla_breached,
/// report_generated}`) emits BEFORE state mutation per the canonical
/// S-07 P1-1 fix + Lote 10.6bis pattern: audit failure aborts the
/// pipeline; the durable backends remain at their pre-call state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErasureAuditSinkError {
    /// Backend transport failure (S-09 audit chain append rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`ErasureError::Audit`] and returns to the caller;
    /// no backend mutation, no idempotency tombstone insert, no report
    /// generation.
    #[error("erasure audit sink store error: {0}")]
    Store(String),
}

/// Per-backend erasure failure surface (lifted into
/// [`ErasureError::Backend`]).
///
/// Production wiring at WI-S11-008 binds the 12 canonical backend
/// adapters to the live transport layer (Neon HTTP via wasm-bindgen /
/// R2 DeleteObject + S-07 refcount-aware integration / D1 batched
/// purge / KV key prefix DELETE / Stripe `Customer.update` via S-10
/// StripeClient wrapper / Loki `/loki/api/v1/delete` HTTP API / R2
/// audit Object Lock 7y HKDF pseudonymization / Neon PITR backup
/// tombstone replay / R2 CAS legal_hold partition pseudonymize / R2
/// evidence-* buckets pseudonymize).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErasureBackendError {
    /// Backend transport failure (Neon DELETE rejected / R2 503 /
    /// Stripe API rate-limited / Loki query failed). Triggers the
    /// canonical exponential backoff retry up to retry_count = 5;
    /// SEV-2 alert on retry exhaustion per FM-061 mapping.
    #[error("erasure backend transport failure: {0}")]
    Transport(String),

    /// Backend reported the erasure could not complete (e.g. legal
    /// hold active per CTRL-PRIV-033, residency lock per
    /// INV-DATA-RESIDENCY, refcount-aware blob shared between
    /// tenants per S-07 dedup). Surfaces as
    /// [`crate::event::BackendOutcome::NotApplicable`] in the report;
    /// no SEV alert.
    #[error("erasure backend not applicable: {0}")]
    NotApplicable(String),

    /// Verification sweep detected a remaining-rows mismatch with the
    /// canonical "no rows for tenant" sentinel. Effective backends
    /// expect 0 records; pseudonymized backends expect 100%
    /// `pii_redacted=true` marker presence.
    /// [`crate::event::BackendOutcome::PartialFailure`] in the report
    /// + SEV-1 alert per FM-450.
    #[error("erasure backend verification mismatch: {0}")]
    VerificationMismatch(String),
}

/// Idempotency ledger failure surface (lifted into
/// [`ErasureError::Idempotency`]).
///
/// Production wiring at WI-S11-008 binds this to the canonical D1
/// `dsr_erasure_log` table per WI §6.1.7 (additive migration
/// `migrations/d1/0022_dsr_erasure_log.sql`; UNIQUE
/// `(dsr_id, backend)` constraint enforces replay-safe per
/// PAT-RETRY-IDEMPOTENT-001).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErasureIdempotencyError {
    /// Backend transport failure (D1 INSERT rejected / connection
    /// pool exhausted).
    #[error("erasure idempotency ledger backend error: {0}")]
    Backend(String),

    /// A tombstone with the same `(dsr_id, backend)` already exists
    /// with a divergent payload (same idempotency key but different
    /// outcome). Surfaces as a tampering signal: the canonical
    /// idempotency contract is "same `(dsr_id, backend)` → same
    /// outcome"; a replayed key paired with a divergent outcome is a
    /// SEV-1 forensic anomaly.
    #[error("erasure idempotency ledger detected divergent payload for (dsr_id, backend)")]
    DivergentPayload,
}

/// Report signing / verification failure surface (lifted into
/// [`ErasureError::Report`]).
///
/// Production wiring at WI-S11-008 binds this to the real BLAKE3-keyed
/// MAC sign / verify surface backed by KMS via HKDF
/// info=`corelink/v1/erasure-report` (security_model.md §7.2 +
/// key_management.md §2). The trait surface ships the in-memory
/// deterministic fake; real keyed-BLAKE3 verify is wire-compatible.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErasureReportError {
    /// JCS canonicalization rejected the report payload (e.g. a
    /// non-string map key sneaked into the structure; should be
    /// structurally impossible at the typed boundary).
    #[error("erasure report jcs canonicalization failure: {0}")]
    Canonicalization(String),

    /// BLAKE3-keyed signing key unavailable / rotation in progress /
    /// KMS fetch failed.
    #[error("erasure report signing failure: {0}")]
    Signing(String),

    /// BLAKE3-keyed signature verification failed (tampered report /
    /// wrong key / forgery attempt).
    #[error("erasure report signature verification failed")]
    SignatureInvalid,
}

/// Canonical error surface returned by the [`crate::worker`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErasureError {
    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per ADR-S11-002 split-tier pattern: no backend mutation, no
    /// idempotency tombstone insert, no report generation.
    #[error("erasure audit envelope failure (state unchanged): {0}")]
    Audit(#[from] ErasureAuditSinkError),

    /// Per-backend erasure failure (transport / not-applicable /
    /// verification mismatch). Per-backend transport failures trigger
    /// the canonical retry path; verification mismatches surface
    /// directly to [`crate::event::ErasureDecision::VerifiedPartial`]
    /// + SEV-1 alert per FM-450.
    #[error("erasure backend failure: {0}")]
    Backend(#[from] ErasureBackendError),

    /// Idempotency ledger failure (D1 INSERT rejected). Fail-CLOSED
    /// per ADR-S11-002 — regulatory integrity outranks availability;
    /// the run aborts so the auditor trail is never silently
    /// truncated.
    #[error("erasure idempotency ledger failure: {0}")]
    Idempotency(#[from] ErasureIdempotencyError),

    /// Report signing / verification failure.
    #[error("erasure report failure: {0}")]
    Report(#[from] ErasureReportError),

    /// Configuration invariant violated (e.g. tenant_id mismatch
    /// between request body + authenticated context, or unknown
    /// erasure_salt scope). Programmer error; mapped to 5xx in
    /// production wiring.
    #[error("erasure configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex
    /// poisoned by a panicking run). Production wiring maps this to
    /// fail-CLOSED at the trait surface — the run aborts rather than
    /// risk corrupt state.
    #[error("erasure internal state invariant violated: {0}")]
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
        let inner = ErasureAuditSinkError::Store("induced".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Audit(_)));
    }

    #[test]
    fn from_backend_transport_lifts_cleanly() {
        let inner = ErasureBackendError::Transport("neon timeout".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Backend(_)));
    }

    #[test]
    fn from_backend_not_applicable_lifts_cleanly() {
        let inner = ErasureBackendError::NotApplicable("legal_hold".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Backend(_)));
    }

    #[test]
    fn from_backend_verification_lifts_cleanly() {
        let inner = ErasureBackendError::VerificationMismatch("loki settle delay".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Backend(_)));
    }

    #[test]
    fn from_idempotency_backend_lifts_cleanly() {
        let inner = ErasureIdempotencyError::Backend("d1 503".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Idempotency(_)));
    }

    #[test]
    fn from_idempotency_divergent_lifts_cleanly() {
        let inner = ErasureIdempotencyError::DivergentPayload;
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Idempotency(_)));
    }

    #[test]
    fn from_report_canonicalization_lifts_cleanly() {
        let inner = ErasureReportError::Canonicalization("non-string key".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Report(_)));
    }

    #[test]
    fn from_report_signing_lifts_cleanly() {
        let inner = ErasureReportError::Signing("kms unavailable".to_string());
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Report(_)));
    }

    #[test]
    fn from_report_signature_invalid_lifts_cleanly() {
        let inner = ErasureReportError::SignatureInvalid;
        let e: ErasureError = inner.into();
        assert!(matches!(e, ErasureError::Report(_)));
    }

    #[test]
    fn config_displays_diagnostic() {
        let e = ErasureError::Config("tenant mismatch".to_string());
        let s = format!("{e}");
        assert!(s.contains("configuration invariant"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = ErasureError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }

    #[test]
    fn audit_displays_lifted_inner() {
        let e: ErasureError = ErasureAuditSinkError::Store("inner-msg".to_string()).into();
        let s = format!("{e}");
        assert!(s.contains("audit envelope failure"));
    }

    #[test]
    fn report_signature_invalid_displays() {
        let e = ErasureReportError::SignatureInvalid;
        let s = format!("{e}");
        assert!(s.contains("verification failed"));
    }
}

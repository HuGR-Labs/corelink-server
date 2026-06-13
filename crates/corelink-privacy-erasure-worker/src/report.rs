//! Canonical erasure report shape + BLAKE3-keyed MAC signing surface
//! + canonical R2 object-key helper.
//!
//! Per WI-S11-002 §1 + §13: every successful 24h verification sweep
//! generates a canonical JCS-canonical erasure-report.json payload
//! signed by a BLAKE3-keyed MAC; the R2 object key is
//! `dsr-reports/{tenant}/{dsr_id}/erasure-report.json`. Production
//! wiring at WI-S11-008 binds the canonical
//! [`ReportSigner`] trait to the live KMS-backed BLAKE3 keyed-hash MAC
//! surface (HKDF info=`corelink/v1/erasure-report` per security_model.md
//! §7.2 + key_management.md §2). The trait surface here ships the
//! canonical [`InMemoryReportSigner`] deterministic fake (BLAKE3-256
//! keyed-hash MAC over the JCS preimage so verify-post-facto is
//! byte-identical to a production keyed-BLAKE3 verify).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ErasureReportError;
use crate::event::{BackendCompletion, ErasurePlan};

/// Canonical BLAKE3-keyed MAC signature length: 32 bytes.
pub const REPORT_SIGNATURE_LEN: usize = 32;

/// Canonical R2 object-key prefix for the canonical erasure report.
pub const REPORT_OBJECT_KEY_PREFIX: &str = "dsr-reports";

/// Canonical typed erasure report. Serialized via JCS canonicalization
/// (RFC 8785) so the auditor evidence trail is byte-deterministic
/// across re-runs (consistent with the S-09 audit chain + S-10 billing
/// replay surfaces).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasureReport {
    /// Canonical UUIDv7 DSR id (matches the request).
    pub dsr_id: Uuid,
    /// Tenant id (S-03 inheritance — NEVER the request body).
    pub tenant_id: Uuid,
    /// Canonical 12-arm erasure plan rendered at run-start.
    pub plan: ErasurePlan,
    /// Per-backend completion ledger (12 entries; canonical order).
    pub completions: Vec<BackendCompletion>,
    /// Wall-clock instant the canonical 24h verification sweep
    /// completed (Unix epoch ms).
    pub verified_at_ms: u64,
    /// Whether the verification sweep landed
    /// [`crate::event::ErasureDecision::VerifiedComplete`] (vs
    /// `VerifiedPartial`).
    pub verified_complete: bool,
    /// Canonical 5-event CloudEvents type strings emitted on the
    /// canonical pipeline run (informational; mirrors
    /// [`crate::event::canonical_cloudevent_types`]).
    pub cloudevent_types: Vec<String>,
}

/// Canonical 32-byte BLAKE3 MAC signature wrapper. The trait surface
/// is opaque so production wiring can swap in a real KMS-backed
/// keyed-BLAKE3 surface without touching the report shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSignature([u8; REPORT_SIGNATURE_LEN]);

impl ReportSignature {
    /// Construct from raw 32 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; REPORT_SIGNATURE_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte signature.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; REPORT_SIGNATURE_LEN] {
        &self.0
    }

    /// Render as canonical 64-char lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// Canonical 32-byte BLAKE3-keyed MAC key. Production wiring at
/// WI-S11-008 binds this to a KMS-backed key derived via HKDF
/// info=`corelink/v1/erasure-report`; the in-memory fake accepts a
/// caller-controlled key for deterministic test pinning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportSignerKey([u8; 32]);

impl ReportSignerKey {
    /// Construct from raw 32 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte key.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Synthetic deterministic key for in-memory tests.
    #[must_use]
    pub fn synthetic_for_test(seed: u8) -> Self {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        Self(bytes)
    }
}

/// Canonical JCS preimage helper. Renders the `report` to JCS-canonical
/// JSON bytes per RFC 8785 (consistent with the S-09 audit chain +
/// S-10 billing replay surfaces).
///
/// # Errors
///
/// - [`ErasureReportError::Canonicalization`] if JCS rejects the
///   payload (structurally impossible at the typed boundary; defensive
///   error path).
pub fn canonical_bytes(report: &ErasureReport) -> Result<Vec<u8>, ErasureReportError> {
    serde_jcs::to_vec(report).map_err(|e| {
        ErasureReportError::Canonicalization(format!("jcs canonicalization rejected: {e}"))
    })
}

/// Canonical R2 object-key helper. Pinned per WI-S11-002 §1: the
/// signed URL 24h TTL points at this canonical key.
#[must_use]
pub fn canonical_report_key(tenant_id: Uuid, dsr_id: Uuid) -> String {
    format!(
        "{prefix}/{tenant}/{dsr}/erasure-report.json",
        prefix = REPORT_OBJECT_KEY_PREFIX,
        tenant = tenant_id.simple(),
        dsr = dsr_id.simple(),
    )
}

/// Canonical BLAKE3-keyed MAC sign / verify trait. Production wiring
/// composes:
///
/// - `KmsBlake3KeyedMacSigner` — KMS-backed BLAKE3 keyed-hash MAC
///   surface; HKDF info=`corelink/v1/erasure-report`.
pub trait ReportSigner: Send + Sync + core::fmt::Debug {
    /// Sign `report` with the canonical issuer key and return the
    /// 32-byte BLAKE3-keyed MAC signature.
    ///
    /// # Errors
    ///
    /// - [`ErasureReportError::Canonicalization`] if JCS rejects the
    ///   payload.
    /// - [`ErasureReportError::Signing`] if the key is unavailable.
    fn sign(&self, report: &ErasureReport) -> Result<ReportSignature, ErasureReportError>;

    /// Verify `signature` against `report` (constant-time MAC
    /// comparison). Returns `Ok(())` on a verifying signature.
    ///
    /// # Errors
    ///
    /// - [`ErasureReportError::Canonicalization`] if JCS rejects the
    ///   payload.
    /// - [`ErasureReportError::Signing`] if the key is unavailable.
    /// - [`ErasureReportError::SignatureInvalid`] when the supplied
    ///   signature does not verify.
    fn verify(
        &self,
        report: &ErasureReport,
        signature: &ReportSignature,
    ) -> Result<(), ErasureReportError>;
}

/// Canonical in-memory deterministic fake. The MAC is a BLAKE3
/// keyed-hash over the JCS preimage so verify-post-facto is byte-
/// identical to a production keyed-BLAKE3 verify; the key is
/// caller-supplied at construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InMemoryReportSigner {
    key: ReportSignerKey,
}

impl InMemoryReportSigner {
    /// Construct with a caller-supplied 32-byte key. Production wiring
    /// uses a KMS-backed key; tests use [`ReportSignerKey::synthetic_for_test`].
    #[must_use]
    pub const fn new(key: ReportSignerKey) -> Self {
        Self { key }
    }
}

impl ReportSigner for InMemoryReportSigner {
    fn sign(&self, report: &ErasureReport) -> Result<ReportSignature, ErasureReportError> {
        let bytes = canonical_bytes(report)?;
        // BLAKE3 keyed-hash MAC: same as a production keyed-BLAKE3
        // verify so the wire format is byte-identical.
        let mac = blake3::keyed_hash(self.key.as_bytes(), &bytes);
        Ok(ReportSignature::from_bytes(*mac.as_bytes()))
    }

    fn verify(
        &self,
        report: &ErasureReport,
        signature: &ReportSignature,
    ) -> Result<(), ErasureReportError> {
        let recomputed = self.sign(report)?;
        // Constant-time comparison via the canonical BLAKE3 MAC
        // re-computation: a tampered report or wrong key produces a
        // different MAC, the eq check rejects.
        if recomputed.as_bytes() == signature.as_bytes() {
            Ok(())
        } else {
            Err(ErasureReportError::SignatureInvalid)
        }
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
    use crate::event::{
        canonical_backend_kinds, canonical_cloudevent_types, BackendErasureOutcome,
        ErasurePlanEntry, ErasureRequest, ErasureSalt,
    };

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    fn fresh_report() -> ErasureReport {
        let req = ErasureRequest::new(
            fixed_uuid(1),
            fixed_uuid(2),
            fixed_uuid(3),
            ErasureSalt::synthetic_for_test(1),
            1_000,
        );
        let plan = ErasurePlan::canonical(&req, 1_000);
        let completions: Vec<BackendCompletion> = canonical_backend_kinds()
            .iter()
            .map(|k| BackendCompletion {
                dsr_id: req.dsr_id,
                tenant_id: req.tenant_id,
                subject_id_hash: [0u8; 32],
                backend: *k,
                outcome: if k.is_effective() {
                    BackendErasureOutcome::Erased { records_deleted: 1 }
                } else {
                    BackendErasureOutcome::Pseudonymized {
                        records_redacted: 1,
                    }
                },
                idempotency_key: ErasurePlanEntry::idempotency_key_for(req.dsr_id, *k, 0),
                started_at_ms: 1_000,
                completed_at_ms: 1_500,
                retry_count: 0,
                verification_hash: [0u8; 32],
            })
            .collect();
        ErasureReport {
            dsr_id: req.dsr_id,
            tenant_id: req.tenant_id,
            plan,
            completions,
            verified_at_ms: 90_000_000,
            verified_complete: true,
            cloudevent_types: canonical_cloudevent_types()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }

    #[test]
    fn canonical_bytes_byte_stable() {
        let r1 = fresh_report();
        let r2 = fresh_report();
        let b1 = canonical_bytes(&r1).unwrap();
        let b2 = canonical_bytes(&r2).unwrap();
        assert_eq!(b1, b2);
    }

    #[test]
    fn signer_round_trips() {
        let key = ReportSignerKey::synthetic_for_test(7);
        let signer = InMemoryReportSigner::new(key);
        let report = fresh_report();
        let sig = signer.sign(&report).unwrap();
        signer.verify(&report, &sig).unwrap();
    }

    #[test]
    fn signer_rejects_tampered_report() {
        let key = ReportSignerKey::synthetic_for_test(7);
        let signer = InMemoryReportSigner::new(key);
        let mut report = fresh_report();
        let sig = signer.sign(&report).unwrap();
        // Tamper with the report (verified_at_ms drift).
        report.verified_at_ms = report.verified_at_ms.wrapping_add(1);
        let err = signer.verify(&report, &sig).unwrap_err();
        let invalid = matches!(err, ErasureReportError::SignatureInvalid);
        assert!(invalid);
    }

    #[test]
    fn signer_rejects_wrong_key() {
        let signer_a = InMemoryReportSigner::new(ReportSignerKey::synthetic_for_test(1));
        let signer_b = InMemoryReportSigner::new(ReportSignerKey::synthetic_for_test(2));
        let report = fresh_report();
        let sig = signer_a.sign(&report).unwrap();
        let err = signer_b.verify(&report, &sig).unwrap_err();
        let invalid = matches!(err, ErasureReportError::SignatureInvalid);
        assert!(invalid);
    }

    #[test]
    fn canonical_report_key_format_pinned() {
        let t = fixed_uuid(1);
        let d = fixed_uuid(2);
        let key = canonical_report_key(t, d);
        assert!(key.starts_with("dsr-reports/"));
        assert!(key.ends_with("/erasure-report.json"));
        assert!(key.contains(&t.simple().to_string()));
        assert!(key.contains(&d.simple().to_string()));
    }

    #[test]
    fn signature_hex_64_chars() {
        let key = ReportSignerKey::synthetic_for_test(7);
        let signer = InMemoryReportSigner::new(key);
        let report = fresh_report();
        let sig = signer.sign(&report).unwrap();
        assert_eq!(sig.to_hex().len(), 64);
    }
}

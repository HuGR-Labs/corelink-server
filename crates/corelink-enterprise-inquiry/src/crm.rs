//! CRM API client surface + in-memory fakes.
//!
//! The production wiring HTTPS POSTs the inquiry payload to HubSpot
//! (`/crm/v3/objects/contacts` + `/crm/v3/objects/deals`) at GA;
//! Salesforce is a deferred alternative. The trait surface here is
//! synchronous + transport-agnostic so the in-memory fake can pin
//! every algorithmic invariant the production wiring relies on.
//!
//! # Encryption boundary (R2-11; S-19 P1-NEW-3)
//!
//! Per CTRL-PRIV-001 the inquiry PII is sealed at ledger ingress under
//! the BYOK CMK and only unsealed at permitted decryption boundaries.
//! HubSpot is one such permitted boundary (the legal scope is the
//! HubSpot DPA, with EU/US residency routing enforced in the
//! [`crate::hubspot`] adapter). The trait surface here therefore takes
//! `&SealedInquiry` + a borrowed `&dyn InquiryPayloadEncryptor` — the
//! HubSpot adapter decrypts at its own boundary right before issuing
//! the wire request.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::encryption::{
    InquiryEncryptionError, InquiryPayloadEncryptor, SealedInquiry,
};
use crate::form::InquiryId;

/// Opaque CRM entry id (HubSpot deal id / Salesforce opportunity id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CrmEntryId(String);

impl CrmEntryId {
    /// Wrap a raw CRM entry id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// CRM transport error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CrmError {
    /// CRM API returned a non-2xx (rate-limited 429, 5xx, network
    /// timeout, etc).
    #[error("crm transport failure: {0}")]
    Transport(String),
    /// CRM rejected the payload (validation, schema mismatch).
    #[error("crm payload rejected: {0}")]
    Rejected(String),
    /// CRM-side decryption boundary failed to unseal the sealed inquiry
    /// payload (AAD mismatch, revoked CMK, AES-GCM tag mismatch). The
    /// saga rolls back per `INV-BYOK-CRYPTO-SOVEREIGNTY`.
    #[error("crm decryption boundary failed: {0}")]
    Encryption(String),
}

impl From<InquiryEncryptionError> for CrmError {
    fn from(e: InquiryEncryptionError) -> Self {
        CrmError::Encryption(e.to_string())
    }
}

/// Trait every CRM API client satisfies.
///
/// # Decryption boundary
///
/// Implementations that talk to a remote CRM SHALL decrypt the
/// [`SealedInquiry`] at the wire boundary (immediately before issuing
/// the HTTP request) and SHOULD drop the unsealed PII as soon as the
/// response returns. The in-memory fake [`InMemoryCrmClient`] does
/// unseal so the property tests can ratify that the seal+AAD pipeline
/// round-trips through the CRM leg.
pub trait CrmClient: core::fmt::Debug + Send + Sync {
    /// Create a CRM entry for the given sealed inquiry. Returns the
    /// CRM-side entry id used for downstream Sales follow-up.
    ///
    /// `tenant_id` is supplied by the ledger and is the value bound
    /// into the sealed payload's AAD (per [`InquiryAadContext`]); the
    /// adapter MUST pass it through to the encryptor so the unseal
    /// path can re-derive the AAD and fail-CLOSED on mismatch.
    ///
    /// # Errors
    ///
    /// - [`CrmError::Transport`] for HTTPS transport failures.
    /// - [`CrmError::Rejected`] when the CRM rejects the payload.
    /// - [`CrmError::Encryption`] when the decryption boundary
    ///   rejects (revoked CMK, AAD mismatch, tag mismatch).
    fn create_entry(
        &self,
        inquiry_id: &InquiryId,
        sealed: &SealedInquiry,
        tenant_id: &str,
        encryptor: &dyn InquiryPayloadEncryptor,
        lead_score: u32,
    ) -> Result<CrmEntryId, CrmError>;

    /// Compensating CRM action — invoked when the saga rolls back
    /// AFTER a CRM entry was created. Production wiring HTTPS PATCHes
    /// the entry to `closed_lost / saga_rollback`.
    ///
    /// # Errors
    ///
    /// Returns [`CrmError::Transport`] for HTTPS transport failures
    /// or [`CrmError::Rejected`] when the CRM rejects the compensation
    /// PATCH.
    fn compensate(&self, inquiry_id: &InquiryId, entry_id: &CrmEntryId) -> Result<(), CrmError>;
}

/// Records a single CRM entry write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemoryCrmEntry {
    /// Inquiry the entry pertains to.
    pub inquiry_id: InquiryId,
    /// CRM-side id returned to the caller.
    pub entry_id: CrmEntryId,
    /// Lead score captured at write time.
    pub lead_score: u32,
    /// True once `compensate` advanced the row to rollback.
    pub compensated: bool,
    /// SHA of the sealed ciphertext at write time (lets tests pin
    /// "no plaintext PII reached the CRM fake state").
    pub ciphertext_b64_len: usize,
}

/// In-memory CRM fake — records every entry + compensation for
/// property inspection. Performs an unseal pass at the boundary so
/// tests ratifying the encryption-round-trip can observe the recovered
/// PII via [`Self::last_unsealed_email`].
#[derive(Clone, Debug, Default)]
pub struct InMemoryCrmClient {
    entries: Arc<Mutex<Vec<InMemoryCrmEntry>>>,
    last_unsealed_email: Arc<Mutex<Option<String>>>,
}

impl InMemoryCrmClient {
    /// Construct an empty CRM fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot recorded entries.
    #[must_use]
    pub fn snapshot(&self) -> Vec<InMemoryCrmEntry> {
        match self.entries.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.entries.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot the last email recovered at the CRM decryption boundary
    /// (test-only — pins the unseal pipeline).
    #[must_use]
    pub fn last_unsealed_email(&self) -> Option<String> {
        match self.last_unsealed_email.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl CrmClient for InMemoryCrmClient {
    fn create_entry(
        &self,
        inquiry_id: &InquiryId,
        sealed: &SealedInquiry,
        tenant_id: &str,
        encryptor: &dyn InquiryPayloadEncryptor,
        lead_score: u32,
    ) -> Result<CrmEntryId, CrmError> {
        // Encryption boundary: verify the tenant binding matches BEFORE
        // invoking the unseal path (avoids materialising plaintext PII
        // when the AAD context is already known-bad).
        if sealed.payload.aad.tenant_id != tenant_id {
            return Err(CrmError::Encryption(
                "tenant_id disagrees with payload AAD".to_string(),
            ));
        }
        // Decryption boundary: unseal at the adapter boundary, NOT in
        // the ledger. The unsealed PII goes out-of-scope at the end of
        // this call.
        let unsealed = encryptor
            .unseal(&sealed.payload, &sealed.payload.aad)
            .map_err(CrmError::from)?;
        let mut g = self
            .entries
            .lock()
            .map_err(|e| CrmError::Transport(format!("mutex poisoned: {e}")))?;
        let entry_id = CrmEntryId::new(format!("crm-{}-{}", inquiry_id, g.len()));
        let entry = InMemoryCrmEntry {
            inquiry_id: inquiry_id.clone(),
            entry_id: entry_id.clone(),
            lead_score,
            compensated: false,
            ciphertext_b64_len: sealed.payload.ciphertext_b64.len(),
        };
        g.push(entry);
        drop(g);
        if let Ok(mut s) = self.last_unsealed_email.lock() {
            *s = Some(unsealed.email.clone());
        }
        Ok(entry_id)
    }

    fn compensate(&self, inquiry_id: &InquiryId, entry_id: &CrmEntryId) -> Result<(), CrmError> {
        let mut g = self
            .entries
            .lock()
            .map_err(|e| CrmError::Transport(format!("mutex poisoned: {e}")))?;
        for entry in g.iter_mut() {
            if &entry.inquiry_id == inquiry_id && &entry.entry_id == entry_id {
                entry.compensated = true;
                return Ok(());
            }
        }
        Err(CrmError::Rejected(format!(
            "no CRM entry {entry_id:?} found for inquiry {inquiry_id}"
        )))
    }
}

/// Adversarial fixture CRM client that always rejects on
/// `create_entry` (forces saga rollback path in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingCrmClient;

impl CrmClient for FailingCrmClient {
    fn create_entry(
        &self,
        _inquiry_id: &InquiryId,
        _sealed: &SealedInquiry,
        _tenant_id: &str,
        _encryptor: &dyn InquiryPayloadEncryptor,
        _lead_score: u32,
    ) -> Result<CrmEntryId, CrmError> {
        Err(CrmError::Transport(
            "adversarial fixture: always rejects".to_string(),
        ))
    }

    fn compensate(&self, _inquiry_id: &InquiryId, _entry_id: &CrmEntryId) -> Result<(), CrmError> {
        Err(CrmError::Transport(
            "adversarial fixture: always rejects".to_string(),
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
    use crate::encryption::{seal_inquiry, InMemoryInquiryPayloadEncryptor};
    use crate::form::{BYOKRequirementsKind, EnterpriseInquiryForm, ResidencyKind, Role};

    fn form() -> EnterpriseInquiryForm {
        EnterpriseInquiryForm {
            company: "Acme".to_string(),
            role: Role::Ciso,
            email: "ciso@acme.example".to_string(),
            phone_optional: None,
            expected_gb_per_month: 100,
            byok_requirements: BYOKRequirementsKind::None,
            residency_requirements: ResidencyKind::Us,
            additional_notes: None,
            locale: "en-US".to_string(),
            use_case: "cache".to_string(),
        }
    }

    #[test]
    fn in_memory_records_entry_via_seal_boundary() {
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let c = InMemoryCrmClient::new();
        let sealed = seal_inquiry(&form(), "tenant-1", b"key", &enc).unwrap();
        let id = c
            .create_entry(&InquiryId::new("inq-1"), &sealed, "tenant-1", &enc, 0)
            .unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c.snapshot()[0].entry_id, id);
        assert!(!c.snapshot()[0].compensated);
        // Decryption boundary recovered the email.
        assert_eq!(c.last_unsealed_email().as_deref(), Some("ciso@acme.example"));
    }

    #[test]
    fn compensate_marks_entry() {
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let c = InMemoryCrmClient::new();
        let sealed = seal_inquiry(&form(), "tenant-1", b"k", &enc).unwrap();
        let inquiry = InquiryId::new("inq-1");
        let id = c
            .create_entry(&inquiry, &sealed, "tenant-1", &enc, 0)
            .unwrap();
        c.compensate(&inquiry, &id).unwrap();
        assert!(c.snapshot()[0].compensated);
    }

    #[test]
    fn failing_client_rejects() {
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let c = FailingCrmClient;
        let sealed = seal_inquiry(&form(), "tenant-1", b"k", &enc).unwrap();
        let err = c
            .create_entry(&InquiryId::new("x"), &sealed, "tenant-1", &enc, 0)
            .unwrap_err();
        assert!(matches!(err, CrmError::Transport(_)));
    }

    #[test]
    fn aad_swap_rejected_at_crm_boundary() {
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let c = InMemoryCrmClient::new();
        let sealed = seal_inquiry(&form(), "tenant-A", b"k", &enc).unwrap();
        // Adapter is asked to push as a different tenant — must
        // refuse at the encryption boundary.
        let err = c
            .create_entry(&InquiryId::new("inq-1"), &sealed, "tenant-B", &enc, 0)
            .unwrap_err();
        assert!(matches!(err, CrmError::Encryption(_)));
        assert_eq!(c.len(), 0);
    }
}

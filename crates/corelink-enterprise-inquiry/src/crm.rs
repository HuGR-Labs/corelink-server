//! CRM API client surface + in-memory fakes.
//!
//! The production wiring HTTPS POSTs the inquiry payload to HubSpot
//! (`/crm/v3/objects/contacts` + `/crm/v3/objects/deals`) at GA;
//! Salesforce is a deferred alternative. The trait surface here is
//! synchronous + transport-agnostic so the in-memory fake can pin
//! every algorithmic invariant the production wiring relies on.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::form::{EnterpriseInquiryForm, InquiryId};

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
}

/// Trait every CRM API client satisfies.
pub trait CrmClient: core::fmt::Debug + Send + Sync {
    /// Create a CRM entry for the given inquiry. Returns the CRM-side
    /// entry id used for downstream Sales follow-up.
    ///
    /// # Errors
    ///
    /// Returns [`CrmError::Transport`] for HTTPS transport failures
    /// or [`CrmError::Rejected`] when the CRM rejects the payload
    /// (e.g. schema mismatch / validation error).
    fn create_entry(
        &self,
        inquiry_id: &InquiryId,
        form: &EnterpriseInquiryForm,
        lead_score: u32,
    ) -> Result<CrmEntryId, CrmError>;

    /// Compensating CRM action — invoked when the saga rolls back
    /// AFTER a CRM entry was created (e.g. follow-on Slack failure
    /// while reconciling, manual rollback request). Production wiring
    /// HTTPS PATCHes the entry to `closed_lost / saga_rollback`.
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
}

/// In-memory CRM fake — records every entry + compensation for
/// property inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryCrmClient {
    entries: Arc<Mutex<Vec<InMemoryCrmEntry>>>,
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
}

impl CrmClient for InMemoryCrmClient {
    fn create_entry(
        &self,
        inquiry_id: &InquiryId,
        _form: &EnterpriseInquiryForm,
        lead_score: u32,
    ) -> Result<CrmEntryId, CrmError> {
        let mut g = self
            .entries
            .lock()
            .map_err(|e| CrmError::Transport(format!("mutex poisoned: {e}")))?;
        let entry_id = CrmEntryId::new(format!("crm-{}-{}", inquiry_id, g.len()));
        g.push(InMemoryCrmEntry {
            inquiry_id: inquiry_id.clone(),
            entry_id: entry_id.clone(),
            lead_score,
            compensated: false,
        });
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
        _form: &EnterpriseInquiryForm,
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
    fn in_memory_records_entry() {
        let c = InMemoryCrmClient::new();
        let id = c
            .create_entry(&InquiryId::new("inq-1"), &form(), 0)
            .unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c.snapshot()[0].entry_id, id);
        assert!(!c.snapshot()[0].compensated);
    }

    #[test]
    fn compensate_marks_entry() {
        let c = InMemoryCrmClient::new();
        let inquiry = InquiryId::new("inq-1");
        let id = c.create_entry(&inquiry, &form(), 0).unwrap();
        c.compensate(&inquiry, &id).unwrap();
        assert!(c.snapshot()[0].compensated);
    }

    #[test]
    fn failing_client_rejects() {
        let c = FailingCrmClient;
        let err = c
            .create_entry(&InquiryId::new("x"), &form(), 0)
            .unwrap_err();
        assert!(matches!(err, CrmError::Transport(_)));
    }
}

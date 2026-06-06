//! Enterprise inquiry form schema (canonical surface for WI-S19-005).
//!
//! The form fields mirror the spec `_spec_contract.md §5.4 R-S19-10`
//! contract: company (1..=200) + role (`Role` 5-canonical) + email
//! (RFC 5322 light validation) + phone_optional (E.164 hint) +
//! expected_gb_per_month (u64) + byok_requirements
//! ([`BYOKRequirementsKind`] 5-canonical) + residency_requirements
//! ([`ResidencyKind`] 6-canonical) + additional_notes (0..=2000).

use crate::error::EnterpriseInquiryError;
use crate::{ADDITIONAL_NOTES_MAX, COMPANY_MAX};

/// Canonical opaque inquiry id newtype (uuid v4 in production wiring).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InquiryId(String);

impl InquiryId {
    /// Wrap a raw inquiry id string.
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

impl core::fmt::Display for InquiryId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Canonical idempotency-key newtype. Two submissions sharing an
/// idempotency key are deduped to a single side-effect set.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Wrap a raw idempotency key string.
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Canonical role taxonomy (WI §6.1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Role {
    /// Chief Information Security Officer.
    Ciso,
    /// Chief Technology Officer.
    Cto,
    /// Chief Information Officer.
    Cio,
    /// VP Engineering / Director of Engineering.
    VpEng,
    /// Other (catch-all).
    Other,
}

impl Role {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ciso => "ciso",
            Self::Cto => "cto",
            Self::Cio => "cio",
            Self::VpEng => "vp_eng",
            Self::Other => "other",
        }
    }
}

/// Canonical BYOK-requirement taxonomy (WI §6.1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BYOKRequirementsKind {
    /// No BYOK requirement.
    None,
    /// AWS KMS-rooted customer key.
    AwsKms,
    /// GCP KMS-rooted customer key.
    GcpKms,
    /// Azure Key Vault-rooted customer key.
    AzureKv,
    /// HashiCorp Vault-rooted customer key.
    Vault,
}

impl BYOKRequirementsKind {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AwsKms => "aws_kms",
            Self::GcpKms => "gcp_kms",
            Self::AzureKv => "azure_kv",
            Self::Vault => "vault",
        }
    }
}

/// Canonical residency-requirement taxonomy (WI §6.1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ResidencyKind {
    /// No residency restriction.
    None,
    /// United States residency.
    Us,
    /// European Union residency.
    Eu,
    /// South America residency (SAM canonical per WI §6.1.1).
    Sam,
    /// APAC residency.
    Apac,
    /// Customer-specified region (free-form via `additional_notes`).
    Specific,
}

impl ResidencyKind {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Us => "us",
            Self::Eu => "eu",
            Self::Sam => "sam",
            Self::Apac => "apac",
            Self::Specific => "specific",
        }
    }
}

/// Canonical inquiry lifecycle status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum InquiryStatus {
    /// Outbox dispatch in flight (Slack ok, CRM pending; or both
    /// pending).
    Pending,
    /// Slack + CRM both confirmed — saga committed.
    Committed,
    /// Either Slack or CRM rejected; compensating action dispatched.
    RolledBack,
    /// Outbox advanced beyond 5min partial-state ceiling; reconciled
    /// manually per `RB-FM-ENTERPRISE-HANDOFF-PARTIAL`.
    PartialEscalated,
}

impl InquiryStatus {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::RolledBack => "rolled_back",
            Self::PartialEscalated => "partial_escalated",
        }
    }
}

/// Enterprise inquiry form (WI §1 + §6.1.1 contract).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnterpriseInquiryForm {
    /// Company name (1..=200 chars after trim).
    pub company: String,
    /// Submitter role.
    pub role: Role,
    /// Validated contact email (RFC 5322 light check).
    pub email: String,
    /// Optional E.164-formatted phone hint (no full RFC parse).
    pub phone_optional: Option<String>,
    /// Expected ingestion volume in gigabytes / month.
    pub expected_gb_per_month: u64,
    /// BYOK requirement.
    pub byok_requirements: BYOKRequirementsKind,
    /// Residency requirement.
    pub residency_requirements: ResidencyKind,
    /// Optional additional notes (0..=2000 chars).
    pub additional_notes: Option<String>,
    /// Locale hint for the auto-reply template (`en-US`, `pt-BR`, ...).
    pub locale: String,
    /// Use-case free text (kept short — 0..=500 chars).
    pub use_case: String,
}

impl EnterpriseInquiryForm {
    /// Construct a new form (canonical builder for tests and external
    /// callers: required since the struct is `#[non_exhaustive]`).
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "form field count is contractual")]
    pub fn new(
        company: impl Into<String>,
        role: Role,
        email: impl Into<String>,
        phone_optional: Option<String>,
        expected_gb_per_month: u64,
        byok_requirements: BYOKRequirementsKind,
        residency_requirements: ResidencyKind,
        additional_notes: Option<String>,
        locale: impl Into<String>,
        use_case: impl Into<String>,
    ) -> Self {
        Self {
            company: company.into(),
            role,
            email: email.into(),
            phone_optional,
            expected_gb_per_month,
            byok_requirements,
            residency_requirements,
            additional_notes,
            locale: locale.into(),
            use_case: use_case.into(),
        }
    }

    /// Validate the form per WI §6.1.1.
    ///
    /// # Errors
    ///
    /// Returns [`EnterpriseInquiryError::InvalidForm`] when the form
    /// violates the schema contract (empty company / company too long
    /// / malformed email / use_case too long / additional_notes too
    /// long / locale empty).
    pub fn validate(&self) -> Result<(), EnterpriseInquiryError> {
        let company = self.company.trim();
        if company.is_empty() {
            return Err(EnterpriseInquiryError::InvalidForm(
                "company is required".to_string(),
            ));
        }
        if company.chars().count() > COMPANY_MAX {
            return Err(EnterpriseInquiryError::InvalidForm(format!(
                "company exceeds {COMPANY_MAX} chars"
            )));
        }
        if !is_email_shape(&self.email) {
            return Err(EnterpriseInquiryError::InvalidForm(format!(
                "email malformed: {}",
                self.email
            )));
        }
        if let Some(notes) = self.additional_notes.as_ref() {
            if notes.chars().count() > ADDITIONAL_NOTES_MAX {
                return Err(EnterpriseInquiryError::InvalidForm(format!(
                    "additional_notes exceeds {ADDITIONAL_NOTES_MAX} chars"
                )));
            }
        }
        if self.use_case.chars().count() > 500 {
            return Err(EnterpriseInquiryError::InvalidForm(
                "use_case exceeds 500 chars".to_string(),
            ));
        }
        if self.locale.trim().is_empty() {
            return Err(EnterpriseInquiryError::InvalidForm(
                "locale is required".to_string(),
            ));
        }
        Ok(())
    }

    /// Compute the canonical lead score per WI §6.1.3:
    /// BYOK enterprise + residency EU + expected GB > 1TB = high.
    #[must_use]
    pub fn lead_score(&self) -> u32 {
        let mut score: u32 = 0;
        if self.byok_requirements != BYOKRequirementsKind::None {
            score = score.saturating_add(40);
        }
        if matches!(self.residency_requirements, ResidencyKind::Eu) {
            score = score.saturating_add(30);
        }
        if self.expected_gb_per_month >= 1_000 {
            score = score.saturating_add(30);
        }
        score
    }
}

/// Lightweight RFC 5322-ish email shape check (intentionally tiny:
/// requires a single `@`, no whitespace, and at least one `.` after
/// the `@`). The production HTTP handler additionally runs a stricter
/// `email_address` crate parse before this validator.
fn is_email_shape(email: &str) -> bool {
    let trimmed = email.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return false;
    }
    let at_count = trimmed.matches('@').count();
    if at_count != 1 {
        return false;
    }
    let mut parts = trimmed.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    domain.contains('.')
}

/// Receipt returned to the caller after a successful inquiry submit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InquiryReceipt {
    /// Inquiry id assigned to the form.
    pub inquiry_id: InquiryId,
    /// Idempotency key the caller submitted.
    pub idempotency_key: IdempotencyKey,
    /// Lifecycle status at the moment the receipt was minted.
    pub status: InquiryStatus,
    /// Server-side timestamp (ms since epoch) when the inquiry record
    /// was persisted.
    pub created_ms: u64,
    /// Promised SLA response window (ms).
    pub sla_ms: u64,
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

    fn valid_form() -> EnterpriseInquiryForm {
        EnterpriseInquiryForm {
            company: "Acme Corp".to_string(),
            role: Role::Ciso,
            email: "ciso@acme.example".to_string(),
            phone_optional: None,
            expected_gb_per_month: 5_000,
            byok_requirements: BYOKRequirementsKind::AwsKms,
            residency_requirements: ResidencyKind::Eu,
            additional_notes: None,
            locale: "en-US".to_string(),
            use_case: "Multi-region read-through cache".to_string(),
        }
    }

    #[test]
    fn valid_form_passes_validation() {
        valid_form().validate().unwrap();
    }

    #[test]
    fn empty_company_rejected() {
        let mut f = valid_form();
        f.company = "   ".to_string();
        let err = f.validate().unwrap_err();
        assert!(matches!(err, EnterpriseInquiryError::InvalidForm(_)));
    }

    #[test]
    fn malformed_email_rejected() {
        let mut f = valid_form();
        f.email = "not-an-email".to_string();
        let err = f.validate().unwrap_err();
        assert!(matches!(err, EnterpriseInquiryError::InvalidForm(_)));
    }

    #[test]
    fn additional_notes_too_long_rejected() {
        let mut f = valid_form();
        f.additional_notes = Some("a".repeat(ADDITIONAL_NOTES_MAX + 1));
        let err = f.validate().unwrap_err();
        assert!(matches!(err, EnterpriseInquiryError::InvalidForm(_)));
    }

    #[test]
    fn high_lead_score_for_enterprise_eu_byok() {
        let f = valid_form();
        // BYOK (40) + EU (30) + 5_000 GB (30) = 100.
        assert_eq!(f.lead_score(), 100);
    }

    #[test]
    fn low_lead_score_for_no_byok_no_eu_low_volume() {
        let mut f = valid_form();
        f.byok_requirements = BYOKRequirementsKind::None;
        f.residency_requirements = ResidencyKind::Us;
        f.expected_gb_per_month = 10;
        assert_eq!(f.lead_score(), 0);
    }

    #[test]
    fn role_canonical_strings() {
        assert_eq!(Role::Ciso.as_str(), "ciso");
        assert_eq!(Role::VpEng.as_str(), "vp_eng");
    }

    #[test]
    fn status_canonical_strings() {
        assert_eq!(InquiryStatus::Committed.as_str(), "committed");
        assert_eq!(
            InquiryStatus::PartialEscalated.as_str(),
            "partial_escalated"
        );
    }
}

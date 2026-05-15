//! `corelink-enterprise-inquiry` canonical error taxonomy.

use thiserror::Error;

use crate::audit::InquiryAuditEmitError;
use crate::crm::CrmError;
use crate::encryption::InquiryEncryptionError;
use crate::mailer::AutoReplyError;
use crate::slack::SlackError;

/// Canonical `corelink-enterprise-inquiry` error taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. Salesforce alt CRM forward, multi-DPO routing).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnterpriseInquiryError {
    /// Form schema validation failed (empty company / malformed email
    /// / `additional_notes` > 2000 chars / `expected_gb_per_month`
    /// nonsensical).
    #[error("inquiry form validation failed: {0}")]
    InvalidForm(String),
    /// Audit-of-audit emit failed; the ledger path aborts per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern).
    /// Caller MUST NOT retry the ledger mutation without first
    /// resolving the audit failure.
    #[error("inquiry audit emit failed: {0}")]
    Audit(#[from] InquiryAuditEmitError),
    /// Slack webhook dispatch failed. The saga rolls back: the outbox
    /// record advances to `RolledBack` and the CRM POST is NOT
    /// initiated (Slack step is the first leg).
    #[error("slack dispatch failed: {0}")]
    Slack(#[from] SlackError),
    /// CRM API dispatch failed. The saga rolls back: a compensating
    /// Slack mark/delete is dispatched and the outbox record advances
    /// to `RolledBack`.
    #[error("crm dispatch failed: {0}")]
    Crm(#[from] CrmError),
    /// Auto-reply mailer failed. Recorded as a SLA risk (Sev3 per WI
    /// §24); the inquiry itself remains `Committed`.
    #[error("auto-reply mailer failed: {0}")]
    AutoReply(#[from] AutoReplyError),
    /// Compensating Slack action (rollback message / delete) failed
    /// after CRM rejected the entry. Surfaces a saga-partial-state
    /// fault — the inquiry MUST be reconciled manually per
    /// `RB-FM-ENTERPRISE-HANDOFF-PARTIAL`.
    #[error("compensation failed: {0}")]
    Compensation(String),
    /// Inquiry payload envelope encryption / decryption failure (R2-11).
    /// Surfaces when the BYOK seal at ledger ingress rejects (revoked
    /// CMK, throttled, transport) or when the HubSpot adapter's unseal
    /// at the decryption boundary rejects (AAD mismatch, AES-GCM tag
    /// mismatch). When raised on the seal path the ledger does NOT
    /// persist any D1 row or outbox entry (atomic rollback per
    /// CTRL-PRIV-001).
    #[error("inquiry payload encryption failed: {0}")]
    Encryption(#[from] InquiryEncryptionError),
    /// Internal invariant violation surfaced via `Mutex` poisoning or
    /// state corruption. Treat as a non-recoverable fault: the caller
    /// MUST tear down the ledger instance + reconstruct from the
    /// durable D1 mirror.
    #[error("internal inquiry ledger fault: {0}")]
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
    fn slack_error_to_inquiry_error() {
        let s = SlackError::Transport("503 service unavailable".to_string());
        let e: EnterpriseInquiryError = s.into();
        assert!(matches!(e, EnterpriseInquiryError::Slack(_)));
    }

    #[test]
    fn crm_error_to_inquiry_error() {
        let c = CrmError::Transport("429 rate limit".to_string());
        let e: EnterpriseInquiryError = c.into();
        assert!(matches!(e, EnterpriseInquiryError::Crm(_)));
    }

    #[test]
    fn error_display_canonical() {
        let e = EnterpriseInquiryError::InvalidForm("empty company".to_string());
        assert_eq!(
            e.to_string(),
            "inquiry form validation failed: empty company"
        );
    }
}

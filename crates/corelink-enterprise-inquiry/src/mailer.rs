//! Auto-reply mailer surface + in-memory fakes.
//!
//! The production wiring HTTPS POSTs the inquiry confirmation email
//! to SES with DKIM/SPF/DMARC configured + a static (no PII leak)
//! white-glove template per WI §6.1.5. The trait surface here is
//! synchronous + transport-agnostic so the in-memory fake can pin
//! every algorithmic invariant the production wiring relies on.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::form::InquiryId;

/// Auto-reply mailer error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutoReplyError {
    /// SES (or fake) reported a transport-layer failure.
    #[error("auto-reply transport failure: {0}")]
    Transport(String),
    /// SES (or fake) returned a hard bounce (invalid mailbox).
    #[error("auto-reply bounce: {0}")]
    Bounce(String),
}

/// Trait every auto-reply mailer satisfies.
pub trait AutoReplyMailer: core::fmt::Debug + Send + Sync {
    /// Dispatch the auto-reply email for `inquiry_id` to
    /// `recipient_email`, using the supplied static `locale` template
    /// id.
    ///
    /// # Errors
    ///
    /// Returns [`AutoReplyError::Transport`] for SES transport
    /// failures or [`AutoReplyError::Bounce`] for hard-bounce mailbox
    /// rejections.
    fn send(
        &self,
        inquiry_id: &InquiryId,
        recipient_email: &str,
        locale: &str,
    ) -> Result<(), AutoReplyError>;
}

/// Records a single auto-reply email send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemoryAutoReplyRecord {
    /// Inquiry the reply pertains to.
    pub inquiry_id: InquiryId,
    /// Recipient mailbox.
    pub recipient_email: String,
    /// Locale template id.
    pub locale: String,
}

/// In-memory auto-reply fake — records every send for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAutoReplyMailer {
    records: Arc<Mutex<Vec<InMemoryAutoReplyRecord>>>,
}

impl InMemoryAutoReplyMailer {
    /// Construct an empty mailer fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot recorded sends.
    #[must_use]
    pub fn snapshot(&self) -> Vec<InMemoryAutoReplyRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded sends.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.records.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no sends have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AutoReplyMailer for InMemoryAutoReplyMailer {
    fn send(
        &self,
        inquiry_id: &InquiryId,
        recipient_email: &str,
        locale: &str,
    ) -> Result<(), AutoReplyError> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| AutoReplyError::Transport(format!("mutex poisoned: {e}")))?;
        g.push(InMemoryAutoReplyRecord {
            inquiry_id: inquiry_id.clone(),
            recipient_email: recipient_email.to_string(),
            locale: locale.to_string(),
        });
        Ok(())
    }
}

/// Adversarial fixture mailer that always rejects.
#[derive(Clone, Debug, Default)]
pub struct FailingAutoReplyMailer;

impl AutoReplyMailer for FailingAutoReplyMailer {
    fn send(
        &self,
        _inquiry_id: &InquiryId,
        _recipient_email: &str,
        _locale: &str,
    ) -> Result<(), AutoReplyError> {
        Err(AutoReplyError::Transport(
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

    #[test]
    fn in_memory_records_send() {
        let m = InMemoryAutoReplyMailer::new();
        m.send(&InquiryId::new("inq-1"), "u@example.com", "en-US")
            .unwrap();
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn failing_mailer_rejects() {
        let m = FailingAutoReplyMailer;
        assert!(m.send(&InquiryId::new("x"), "y@z.io", "en-US").is_err());
    }
}

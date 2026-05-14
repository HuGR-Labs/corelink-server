//! Slack webhook client surface + in-memory fakes.
//!
//! The production wiring HTTPS POSTs the inquiry payload to the
//! `#sales-leads` channel via the Slack incoming webhook URL stored
//! in the S-13 secret rotation singleton. The trait surface here is
//! synchronous + transport-agnostic so the in-memory fake can pin
//! every algorithmic invariant the production wiring relies on.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::form::InquiryId;

/// Opaque Slack message id (matches Slack `ts` field shape, e.g.
/// "1715607600.000100").
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlackMessageId(String);

impl SlackMessageId {
    /// Wrap a raw message id.
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

/// Canonical Slack post kind. Distinguishes a fresh notification
/// from a compensating ROLLBACK message dispatched after CRM failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SlackPostKind {
    /// Fresh new-inquiry notification.
    NewInquiry,
    /// Compensating action: marks the original Slack notification as
    /// `[ROLLBACK]` after the CRM leg of the saga rejected.
    Rollback,
}

impl SlackPostKind {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewInquiry => "new_inquiry",
            Self::Rollback => "rollback",
        }
    }
}

/// Slack webhook transport error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlackError {
    /// Slack returned a non-2xx (rate-limited 429, 5xx, network
    /// timeout, etc).
    #[error("slack transport failure: {0}")]
    Transport(String),
    /// Slack webhook outbound HMAC-SHA256 signature verification
    /// failed (impersonation attempt blocked).
    #[error("slack outbound signature invalid: {0}")]
    InvalidSignature(String),
}

/// Trait every Slack webhook client satisfies.
pub trait SlackClient: core::fmt::Debug + Send + Sync {
    /// Dispatch a Slack post for the given inquiry. Returns the
    /// Slack-side message id (used for compensating mark/delete).
    ///
    /// # Errors
    ///
    /// Returns [`SlackError::Transport`] when the webhook reports a
    /// transport-layer failure, or [`SlackError::InvalidSignature`]
    /// when the outbound HMAC-SHA256 signature verification fails.
    fn post(
        &self,
        inquiry_id: &InquiryId,
        kind: SlackPostKind,
        body: &str,
    ) -> Result<SlackMessageId, SlackError>;
}

/// Records a single Slack post (kept inside [`InMemorySlackClient`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemorySlackPost {
    /// Inquiry the post pertains to.
    pub inquiry_id: InquiryId,
    /// Kind of post.
    pub kind: SlackPostKind,
    /// Posted body.
    pub body: String,
    /// Synthesised message id returned to the caller.
    pub message_id: SlackMessageId,
}

/// In-memory Slack fake — records every post for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemorySlackClient {
    posts: Arc<Mutex<Vec<InMemorySlackPost>>>,
}

impl InMemorySlackClient {
    /// Construct an empty Slack fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot recorded posts.
    #[must_use]
    pub fn snapshot(&self) -> Vec<InMemorySlackPost> {
        match self.posts.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded posts.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.posts.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no posts have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SlackClient for InMemorySlackClient {
    fn post(
        &self,
        inquiry_id: &InquiryId,
        kind: SlackPostKind,
        body: &str,
    ) -> Result<SlackMessageId, SlackError> {
        let mut g = self
            .posts
            .lock()
            .map_err(|e| SlackError::Transport(format!("mutex poisoned: {e}")))?;
        let message_id = SlackMessageId::new(format!("msg-{}-{}", inquiry_id, g.len()));
        let post = InMemorySlackPost {
            inquiry_id: inquiry_id.clone(),
            kind,
            body: body.to_string(),
            message_id: message_id.clone(),
        };
        g.push(post);
        Ok(message_id)
    }
}

/// Adversarial fixture client that always rejects (forces saga
/// rollback path in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingSlackClient;

impl SlackClient for FailingSlackClient {
    fn post(
        &self,
        _inquiry_id: &InquiryId,
        _kind: SlackPostKind,
        _body: &str,
    ) -> Result<SlackMessageId, SlackError> {
        Err(SlackError::Transport(
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
    fn in_memory_records_post() {
        let s = InMemorySlackClient::new();
        let id = InquiryId::new("inq-1");
        let msg = s.post(&id, SlackPostKind::NewInquiry, "hello").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s.snapshot()[0].message_id, msg);
        assert_eq!(s.snapshot()[0].kind, SlackPostKind::NewInquiry);
    }

    #[test]
    fn failing_client_always_rejects() {
        let s = FailingSlackClient;
        let id = InquiryId::new("inq-1");
        let err = s.post(&id, SlackPostKind::NewInquiry, "x").unwrap_err();
        assert!(matches!(err, SlackError::Transport(_)));
    }

    #[test]
    fn slack_kind_canonical() {
        assert_eq!(SlackPostKind::NewInquiry.as_str(), "new_inquiry");
        assert_eq!(SlackPostKind::Rollback.as_str(), "rollback");
    }
}

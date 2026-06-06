//! Bridge implementing `corelink_enterprise_inquiry::SlackClient` on
//! top of any [`crate::client::SharedSlackClient`].
//!
//! This is how the enterprise inquiry consumer keeps its existing trait
//! surface while wiring through the new shared real Slack client. The
//! adapter renders inquiry posts using the canonical
//! `MessageTemplate::NewEnterpriseInquiry` template (or a synthesised
//! rollback header for compensating actions).

use std::sync::Arc;

use corelink_enterprise_inquiry::slack::{
    SlackClient as InquirySlackClient, SlackError as InquirySlackError,
    SlackMessageId as InquirySlackMessageId, SlackPostKind,
};
use corelink_enterprise_inquiry::InquiryId;

use crate::channel::SlackChannel;
use crate::client::{SharedSlackClient, SlackClientError};
use crate::message::SlackMessage;

/// Adapter implementing the inquiry-specific `SlackClient` trait on
/// top of a generic [`SharedSlackClient`].
#[derive(Clone, Debug)]
pub struct InquirySlackAdapter {
    inner: Arc<dyn SharedSlackClient>,
}

impl InquirySlackAdapter {
    /// Construct an adapter wrapping `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn SharedSlackClient>) -> Self {
        Self { inner }
    }
}

impl InquirySlackClient for InquirySlackAdapter {
    fn post(
        &self,
        inquiry_id: &InquiryId,
        kind: SlackPostKind,
        body: &str,
    ) -> Result<InquirySlackMessageId, InquirySlackError> {
        // Compose a deterministic Block Kit message routed at the
        // EnterpriseInquiries channel. The trait body string is treated
        // as the *summary* — not raw mrkdwn — and gets escaped by the
        // section renderer.
        let header = match kind {
            SlackPostKind::NewInquiry => format!("New inquiry — {inquiry_id}"),
            SlackPostKind::Rollback => format!("[ROLLBACK] inquiry — {inquiry_id}"),
            _ => format!("inquiry — {inquiry_id}"),
        };
        let msg = SlackMessage::new(
            SlackChannel::EnterpriseInquiries,
            header,
            format!("inquiry {inquiry_id} ({})", kind.as_str()),
            String::new(),
        )
        .with_field("inquiry_id", inquiry_id.to_string())
        .with_field("kind", kind.as_str())
        .with_field("body", body);

        match self.inner.send(&msg) {
            Ok(outcome) => {
                let id = outcome
                    .thread_ts
                    .unwrap_or_else(|| format!("{}:{}", inquiry_id, kind.as_str()));
                Ok(InquirySlackMessageId::new(id))
            }
            Err(e) => Err(InquirySlackError::Transport(map_err(&e))),
        }
    }
}

fn map_err(e: &SlackClientError) -> String {
    match e {
        SlackClientError::Registry(r) => format!("registry: {r}"),
        SlackClientError::TransportExhausted { attempts, reason } => {
            format!("transport_exhausted after {attempts}: {reason}")
        }
        SlackClientError::PermanentReject { status, reason } => {
            format!("permanent_reject {status}: {reason}")
        }
        SlackClientError::AuditFailed(e) => format!("audit: {e}"),
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
    use crate::audit::InMemorySlackAuditSink;
    use crate::memory::InMemorySharedSlackClient;

    #[test]
    fn adapter_routes_new_inquiry_to_enterprise_channel() {
        let audit = Arc::new(InMemorySlackAuditSink::new());
        let inner: Arc<dyn SharedSlackClient> =
            Arc::new(InMemorySharedSlackClient::new(audit.clone()));
        let adapter = InquirySlackAdapter::new(inner.clone());

        let id = InquiryId::new("inq-42");
        let msg_id = adapter
            .post(&id, SlackPostKind::NewInquiry, "hello")
            .unwrap();
        assert!(msg_id.as_str().contains("inq-42"));

        // audit recorded
        assert_eq!(audit.len(), 1);
        assert_eq!(
            audit.snapshot()[0].channel,
            SlackChannel::EnterpriseInquiries
        );
    }

    #[test]
    fn adapter_uses_rollback_header() {
        let audit = Arc::new(InMemorySlackAuditSink::new());
        let in_mem = Arc::new(InMemorySharedSlackClient::new(audit.clone()));
        let inner: Arc<dyn SharedSlackClient> = in_mem.clone();
        let adapter = InquirySlackAdapter::new(inner);
        let id = InquiryId::new("inq-9");
        let msg_id = adapter
            .post(&id, SlackPostKind::Rollback, "compensating")
            .unwrap();
        assert!(msg_id.as_str().contains("inq-9"));
        assert_eq!(audit.len(), 1);
        // Inspect recorded payload — header should carry [ROLLBACK]
        let recorded = in_mem.snapshot();
        assert_eq!(recorded.len(), 1);
        let header = recorded[0]
            .payload
            .get("blocks")
            .and_then(|b| b.as_array())
            .and_then(|a| a.first())
            .and_then(|h| h.get("text"))
            .and_then(|t| t.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(header.contains("ROLLBACK"));
    }
}

//! Saga outbox record + lifecycle status.
//!
//! Mirrors the D1 `enterprise_inquiry_outbox` table 1:1. The outbox
//! is the durable transaction log for the PAT-SAGA-001 atomic Slack +
//! CRM dispatch: a row advances `Pending → Committed` only after BOTH
//! legs succeed; a row advances `Pending → RolledBack` on either
//! failure (with the compensating Slack mark recorded inline). The
//! worker drain reads `Pending` rows older than the 5min partial-state
//! ceiling and escalates them to `PartialEscalated` per
//! `RB-FM-ENTERPRISE-HANDOFF-PARTIAL`.

use crate::crm::CrmEntryId;
use crate::form::InquiryId;
use crate::slack::SlackMessageId;

/// Canonical outbox status. The state machine enforced by the ledger
/// is `Pending → (Committed | RolledBack | PartialEscalated)` — no
/// other transition is permitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OutboxStatus {
    /// Both Slack + CRM legs still in flight (or neither attempted
    /// yet).
    Pending,
    /// Slack + CRM both succeeded; saga committed.
    Committed,
    /// Either Slack or CRM rejected; compensating action dispatched.
    RolledBack,
    /// Outbox sat in `Pending` past the 5min partial-state ceiling;
    /// escalated to `RB-FM-ENTERPRISE-HANDOFF-PARTIAL`.
    PartialEscalated,
}

impl OutboxStatus {
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

/// Outbox row. Mirrors the D1 `enterprise_inquiry_outbox` table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OutboxRecord {
    /// Inquiry the row pertains to.
    pub inquiry_id: InquiryId,
    /// Current lifecycle status.
    pub status: OutboxStatus,
    /// Slack-side message id once the Slack leg succeeded.
    pub slack_message_id: Option<SlackMessageId>,
    /// CRM-side entry id once the CRM leg succeeded.
    pub crm_entry_id: Option<CrmEntryId>,
    /// Server-side timestamp (ms since epoch) when the outbox row was
    /// first inserted.
    pub created_ms: u64,
    /// Server-side timestamp (ms since epoch) of the most recent
    /// status transition.
    pub updated_ms: u64,
}

impl OutboxRecord {
    /// Construct a fresh outbox row in `Pending` status.
    #[must_use]
    pub fn pending(inquiry_id: InquiryId, ts_ms: u64) -> Self {
        Self {
            inquiry_id,
            status: OutboxStatus::Pending,
            slack_message_id: None,
            crm_entry_id: None,
            created_ms: ts_ms,
            updated_ms: ts_ms,
        }
    }

    /// Promote the row to `Committed` once both legs of the saga
    /// confirmed.
    pub fn commit(
        &mut self,
        slack_message_id: SlackMessageId,
        crm_entry_id: CrmEntryId,
        ts_ms: u64,
    ) {
        self.status = OutboxStatus::Committed;
        self.slack_message_id = Some(slack_message_id);
        self.crm_entry_id = Some(crm_entry_id);
        self.updated_ms = ts_ms;
    }

    /// Promote the row to `RolledBack` after either leg of the saga
    /// rejected (the optional Slack message id records the
    /// compensating ROLLBACK mark when applicable).
    pub fn rollback(
        &mut self,
        slack_message_id: Option<SlackMessageId>,
        crm_entry_id: Option<CrmEntryId>,
        ts_ms: u64,
    ) {
        self.status = OutboxStatus::RolledBack;
        if slack_message_id.is_some() {
            self.slack_message_id = slack_message_id;
        }
        if crm_entry_id.is_some() {
            self.crm_entry_id = crm_entry_id;
        }
        self.updated_ms = ts_ms;
    }

    /// Promote the row to `PartialEscalated` (worker drain decision
    /// after 5min partial-state ceiling).
    pub fn escalate_partial(&mut self, ts_ms: u64) {
        self.status = OutboxStatus::PartialEscalated;
        self.updated_ms = ts_ms;
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
    fn pending_then_commit_transitions() {
        let mut r = OutboxRecord::pending(InquiryId::new("inq-1"), 100);
        assert_eq!(r.status, OutboxStatus::Pending);
        r.commit(SlackMessageId::new("m1"), CrmEntryId::new("c1"), 200);
        assert_eq!(r.status, OutboxStatus::Committed);
        assert_eq!(r.updated_ms, 200);
        assert!(r.slack_message_id.is_some());
        assert!(r.crm_entry_id.is_some());
    }

    #[test]
    fn pending_then_rollback_transitions() {
        let mut r = OutboxRecord::pending(InquiryId::new("inq-1"), 100);
        r.rollback(Some(SlackMessageId::new("rb")), None, 150);
        assert_eq!(r.status, OutboxStatus::RolledBack);
        assert_eq!(r.slack_message_id.as_ref().unwrap().as_str(), "rb");
        assert!(r.crm_entry_id.is_none());
    }

    #[test]
    fn escalate_partial_marks() {
        let mut r = OutboxRecord::pending(InquiryId::new("inq-1"), 100);
        r.escalate_partial(400);
        assert_eq!(r.status, OutboxStatus::PartialEscalated);
        assert_eq!(r.updated_ms, 400);
    }
}

//! Canonical event + domain types for WI-S11-005.

use serde::{Deserialize, Serialize};

/// HKDF info string for DKIM broadcast key derivation.
/// Canonical per WI-S11-005 §9.1 DD-004 + security_model.md §374 inheritance.
pub const HKDF_INFO_DKIM_BROADCAST: &[u8] = b"corelink/v1/dkim-broadcast";

/// The 3 canonical CloudEvents types for sub-processor transparency.
///
/// Emitted to `audit-<region>` R2 Object Lock 7y per INV-AUDIT-APPEND-ONLY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SubProcessorEventType {
    /// Sub-processor list published for the first time (initial v1.0.0).
    /// CloudEvent type: `dev.hugr.corelink.sub_processor.published.v1`
    Published,
    /// Sub-processor list changed (add/remove/modify a sub-processor).
    /// CloudEvent type: `dev.hugr.corelink.sub_processor.changed.v1`
    /// Triggers 30d broadcast to all subscribed customers.
    Changed,
    /// Customer filed a sub-processor objection.
    /// CloudEvent type: `dev.hugr.corelink.sub_processor.objection_filed.v1`
    ObjectionFiled,
}

/// Returns the canonical CloudEvent type string for a given event type.
#[must_use]
pub fn canonical_event_type_string(event_type: SubProcessorEventType) -> &'static str {
    match event_type {
        SubProcessorEventType::Published => {
            "dev.hugr.corelink.sub_processor.published.v1"
        }
        SubProcessorEventType::Changed => {
            "dev.hugr.corelink.sub_processor.changed.v1"
        }
        SubProcessorEventType::ObjectionFiled => {
            "dev.hugr.corelink.sub_processor.objection_filed.v1"
        }
    }
}

/// Canonical sub-processor metadata record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubProcessorInfo {
    /// Canonical ID (e.g., "cloudflare", "neon", "sentry").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Role / function description.
    pub role: String,
    /// Data categories processed.
    pub data_categories_processed: Vec<String>,
    /// Deployment region.
    pub region: String,
    /// Certifications held (e.g., "SOC 2 Type II", "ISO 27001").
    pub certifications: Vec<String>,
    /// DPA URL.
    pub dpa_url: String,
    /// Primary jurisdiction.
    pub primary_jurisdiction: String,
    /// ISO 8601 UTC contract signed date.
    pub contract_signed_at: String,
}

/// Diff between two sub-processor list versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubProcessorDiff {
    /// Sub-processor IDs that were added.
    pub added: Vec<String>,
    /// Sub-processor IDs that were removed.
    pub removed: Vec<String>,
    /// Sub-processor IDs that were modified.
    pub modified: Vec<String>,
}

impl SubProcessorDiff {
    /// Returns true if there are any changes (add/remove/modify).
    #[must_use]
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.modified.is_empty()
    }
}

/// Payload for `sub_processor.published.v1` CloudEvent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProcessorPublishedPayload {
    /// Semver of the published list.
    pub version: String,
    /// ISO 8601 UTC timestamp of publication.
    pub published_at: String,
    /// All 7 canonical sub-processors.
    pub sub_processors: Vec<SubProcessorInfo>,
}

/// Payload for `sub_processor.changed.v1` CloudEvent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProcessorChangedPayload {
    /// Previous semver.
    pub old_version: String,
    /// New semver.
    pub new_version: String,
    /// ISO 8601 UTC timestamp of the change.
    pub changed_at: String,
    /// Diff between old and new.
    pub diff: SubProcessorDiff,
    /// Broadcast trigger timestamp (30d countdown starts here).
    pub broadcast_trigger_ts: String,
}

/// Payload for `sub_processor.objection_filed.v1` CloudEvent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProcessorObjectionPayload {
    /// Objection ticket ULID.
    pub objection_id: String,
    /// Tenant ID.
    pub tenant_id: String,
    /// Subject ID hash (sha256, CTRL-PRIV-014).
    pub subject_id_hash: String,
    /// Sub-processor ID being objected to.
    pub sub_processor_id: String,
    /// Sub-processors version context.
    pub sub_processors_version: String,
    /// ISO 8601 UTC filed timestamp.
    pub filed_at: String,
    /// Optional final decision outcome (populated on resolution).
    pub outcome: Option<String>,
}

/// Delivery status for a broadcast log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeliveryStatus {
    /// Email enqueued but not yet sent.
    Enqueued,
    /// Email sent to provider.
    Sent,
    /// Email delivered to recipient.
    Delivered,
    /// Email bounced.
    Bounced,
    /// Email complained (spam report).
    Complained,
    /// Email delivery failed.
    Failed,
}

/// Notification type for broadcast log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NotificationType {
    /// 30-day advance notice of sub-processor change.
    AdvanceNotice30d,
    /// Confirmation email for an objection filed.
    ObjectionConfirmation,
    /// Final decision email (accept or terminate).
    FinalDecision,
}

/// A single row in `sub_processor_broadcast_log` D1 table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastLogEntry {
    /// ULID primary key.
    pub log_id: String,
    /// ULID grouping all recipients for a single broadcast event.
    pub broadcast_id: String,
    /// Semver of sub_processors.md at broadcast time.
    pub sub_processors_version: String,
    /// Tenant receiving the email.
    pub tenant_id: String,
    /// SHA-256 of recipient email (CTRL-PRIV-014).
    pub recipient_email_hash: String,
    /// Locale for the email content.
    pub locale: EmailLocale,
    /// Notification type.
    pub notification_type: NotificationType,
    /// ISO 8601 UTC enqueue timestamp.
    pub enqueued_at: String,
    /// ISO 8601 UTC delivery timestamp (set by webhook).
    pub delivered_at: Option<String>,
    /// Current delivery status.
    pub delivery_status: DeliveryStatus,
    /// Error class if bounced/failed.
    pub delivery_error_class: Option<String>,
}

/// Email locale enum (3 canonical locales).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EmailLocale {
    /// Brazilian Portuguese.
    PtBr,
    /// American English.
    EnUs,
    /// Mexican Spanish.
    EsMx,
}

/// Objection decision types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ObjectionDecision {
    /// Objection accepted; workaround proposed.
    WorkaroundOffered,
    /// Objection accepted; sub-processor adoption rejected.
    Accepted,
    /// Objection requires DPA termination clause activation.
    Terminated,
}

/// Objection payload submitted by a customer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectionPayload {
    /// Tenant filing the objection.
    pub tenant_id: String,
    /// Subject (data controller contact) ID.
    pub subject_id: String,
    /// Sub-processor ID being objected to.
    pub sub_processor_id: String,
    /// Sub-processors version context.
    pub sub_processors_version: String,
    /// Customer-provided objection rationale.
    pub objection_reason: String,
    /// Customer-proposed alternative (optional).
    pub proposed_alternative: Option<String>,
    /// ISO 8601 UTC filed timestamp.
    pub filed_at: String,
}

/// Ticket status for objection state machine (5 canonical states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ObjectionTicketStatus {
    /// Filed; awaiting Privacy Officer review.
    Pending,
    /// Under active review by Privacy Officer + Legal.
    InReview,
    /// Accepted (workaround offered or sub-processor not adopted).
    Accepted,
    /// DPA termination clause activated.
    Terminated,
    /// Customer withdrew the objection.
    Withdrawn,
}

impl ObjectionTicketStatus {
    /// Returns true if the ticket is in a terminal state.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ObjectionTicketStatus::Accepted
                | ObjectionTicketStatus::Terminated
                | ObjectionTicketStatus::Withdrawn
        )
    }

    /// Validates that a state transition is legal.
    /// Valid transitions: Pending→InReview, Pending→Accepted, Pending→Terminated,
    /// Pending→Withdrawn, InReview→Accepted, InReview→Terminated, InReview→Withdrawn.
    #[must_use]
    pub fn can_transition_to(self, next: ObjectionTicketStatus) -> bool {
        matches!(
            (self, next),
            (ObjectionTicketStatus::Pending, ObjectionTicketStatus::InReview)
                | (ObjectionTicketStatus::Pending, ObjectionTicketStatus::Accepted)
                | (ObjectionTicketStatus::Pending, ObjectionTicketStatus::Terminated)
                | (ObjectionTicketStatus::Pending, ObjectionTicketStatus::Withdrawn)
                | (ObjectionTicketStatus::InReview, ObjectionTicketStatus::Accepted)
                | (ObjectionTicketStatus::InReview, ObjectionTicketStatus::Terminated)
                | (ObjectionTicketStatus::InReview, ObjectionTicketStatus::Withdrawn)
        )
    }
}

/// Returns all canonical event type strings.
#[must_use]
pub fn canonical_event_type_strings() -> [&'static str; 3] {
    [
        canonical_event_type_string(SubProcessorEventType::Published),
        canonical_event_type_string(SubProcessorEventType::Changed),
        canonical_event_type_string(SubProcessorEventType::ObjectionFiled),
    ]
}

/// Returns all canonical delivery statuses.
#[must_use]
pub fn canonical_delivery_statuses() -> [DeliveryStatus; 6] {
    [
        DeliveryStatus::Enqueued,
        DeliveryStatus::Sent,
        DeliveryStatus::Delivered,
        DeliveryStatus::Bounced,
        DeliveryStatus::Complained,
        DeliveryStatus::Failed,
    ]
}

/// Returns all canonical objection ticket statuses.
#[must_use]
pub fn canonical_objection_statuses() -> [ObjectionTicketStatus; 5] {
    [
        ObjectionTicketStatus::Pending,
        ObjectionTicketStatus::InReview,
        ObjectionTicketStatus::Accepted,
        ObjectionTicketStatus::Terminated,
        ObjectionTicketStatus::Withdrawn,
    ]
}

/// Compute SHA-256 hash of a recipient email address (CTRL-PRIV-014).
/// Raw email MUST NEVER appear in logs or audit records.
#[must_use]
pub fn hash_recipient_email(email: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(email.as_bytes());
    hex::encode(h.finalize())
}

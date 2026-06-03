//! Objection store trait + implementations for `sub_processor_objection`.
//!
//! D1 table `sub_processor_objection` tracks customer objections with a
//! UNIQUE constraint `(tenant_id, subject_id, sub_processor_id,
//! sub_processors_version)` to prevent duplicate submissions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::error::SubProcessorObjectionStoreError;
use super::event::{ObjectionDecision, ObjectionPayload, ObjectionTicketStatus};

/// A stored objection ticket record.
#[derive(Debug, Clone)]
pub struct ObjectionTicket {
    /// ULID primary key.
    pub objection_id: String,
    /// Tenant ID.
    pub tenant_id: String,
    /// Subject ID.
    pub subject_id: String,
    /// SHA-256 of subject ID (CTRL-PRIV-014).
    pub subject_id_hash: String,
    /// Sub-processor ID being objected to.
    pub sub_processor_id: String,
    /// Sub-processors version context.
    pub sub_processors_version: String,
    /// Customer-provided objection reason.
    pub objection_reason: String,
    /// Customer-proposed alternative.
    pub proposed_alternative: Option<String>,
    /// Current ticket status.
    pub ticket_status: ObjectionTicketStatus,
    /// Resolution decision (set on resolution).
    pub resolution_decision: Option<ObjectionDecision>,
    /// Privacy Officer + Legal resolution note.
    pub resolution_note: Option<String>,
    /// ISO 8601 UTC filed timestamp.
    pub filed_at: String,
    /// ISO 8601 UTC resolved timestamp.
    pub resolved_at: Option<String>,
    /// ISO 8601 UTC expected resolution timestamp (filed_at + 14d).
    pub expected_resolution_at: String,
}

impl ObjectionTicket {
    /// Construct a new pending objection ticket from a payload.
    ///
    /// `objection_id`: caller-provided ULID.
    /// `subject_id_hash`: sha256 of subject_id.
    /// `expected_resolution_at`: ISO 8601 UTC = filed_at + 14 calendar days.
    #[must_use]
    pub fn new(
        objection_id: String,
        payload: &ObjectionPayload,
        subject_id_hash: String,
        expected_resolution_at: String,
    ) -> Self {
        Self {
            objection_id,
            tenant_id: payload.tenant_id.clone(),
            subject_id: payload.subject_id.clone(),
            subject_id_hash,
            sub_processor_id: payload.sub_processor_id.clone(),
            sub_processors_version: payload.sub_processors_version.clone(),
            objection_reason: payload.objection_reason.clone(),
            proposed_alternative: payload.proposed_alternative.clone(),
            ticket_status: ObjectionTicketStatus::Pending,
            resolution_decision: None,
            resolution_note: None,
            filed_at: payload.filed_at.clone(),
            resolved_at: None,
            expected_resolution_at,
        }
    }
}

/// Unique key for objection UNIQUE constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ObjectionUniqueKey {
    tenant_id: String,
    subject_id: String,
    sub_processor_id: String,
    sub_processors_version: String,
}

/// Trait for objection ticket persistence (D1 `sub_processor_objection`).
pub trait ObjectionStore: std::fmt::Debug + Send + Sync {
    /// Insert a new objection ticket.
    fn insert(&self, ticket: ObjectionTicket) -> Result<(), SubProcessorObjectionStoreError>;

    /// Retrieve an objection ticket by ID.
    fn get(
        &self,
        objection_id: &str,
    ) -> Result<Option<ObjectionTicket>, SubProcessorObjectionStoreError>;

    /// Update the ticket status (state machine transition).
    fn update_status(
        &self,
        objection_id: &str,
        new_status: ObjectionTicketStatus,
        decision: Option<ObjectionDecision>,
        resolution_note: Option<String>,
        resolved_at: Option<String>,
    ) -> Result<(), SubProcessorObjectionStoreError>;

    /// List open tickets (pending + in_review) sorted by expected_resolution_at ASC.
    fn list_open(&self) -> Result<Vec<ObjectionTicket>, SubProcessorObjectionStoreError>;
}

/// In-memory objection store for testing.
///
/// Per-instance `Arc<Mutex<>>` — NEVER `static LazyLock<Mutex<>>` (F-001).
#[derive(Debug, Clone)]
pub struct InMemoryObjectionStore {
    tickets: Arc<Mutex<HashMap<String, ObjectionTicket>>>,
    unique_keys: Arc<Mutex<HashMap<ObjectionUniqueKey, String>>>,
}

impl InMemoryObjectionStore {
    /// Construct a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(Mutex::new(HashMap::new())),
            unique_keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return the number of tickets in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tickets.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Return true if no tickets are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryObjectionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectionStore for InMemoryObjectionStore {
    fn insert(&self, ticket: ObjectionTicket) -> Result<(), SubProcessorObjectionStoreError> {
        let uk = ObjectionUniqueKey {
            tenant_id: ticket.tenant_id.clone(),
            subject_id: ticket.subject_id.clone(),
            sub_processor_id: ticket.sub_processor_id.clone(),
            sub_processors_version: ticket.sub_processors_version.clone(),
        };

        let mut keys = self.unique_keys.lock().unwrap_or_else(|p| p.into_inner());
        if keys.contains_key(&uk) {
            return Err(SubProcessorObjectionStoreError::Duplicate {
                tenant_id: ticket.tenant_id,
                subject_id: ticket.subject_id,
                sub_processor_id: ticket.sub_processor_id,
                version: ticket.sub_processors_version,
            });
        }
        keys.insert(uk, ticket.objection_id.clone());
        drop(keys);

        self.tickets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(ticket.objection_id.clone(), ticket);
        Ok(())
    }

    fn get(
        &self,
        objection_id: &str,
    ) -> Result<Option<ObjectionTicket>, SubProcessorObjectionStoreError> {
        let tickets = self.tickets.lock().unwrap_or_else(|p| p.into_inner());
        Ok(tickets.get(objection_id).cloned())
    }

    fn update_status(
        &self,
        objection_id: &str,
        new_status: ObjectionTicketStatus,
        decision: Option<ObjectionDecision>,
        resolution_note: Option<String>,
        resolved_at: Option<String>,
    ) -> Result<(), SubProcessorObjectionStoreError> {
        let mut tickets = self.tickets.lock().unwrap_or_else(|p| p.into_inner());
        match tickets.get_mut(objection_id) {
            Some(ticket) => {
                ticket.ticket_status = new_status;
                ticket.resolution_decision = decision;
                ticket.resolution_note = resolution_note;
                ticket.resolved_at = resolved_at;
                Ok(())
            }
            None => Err(SubProcessorObjectionStoreError::Unavailable(format!(
                "objection ticket not found: objection_id={objection_id}"
            ))),
        }
    }

    fn list_open(&self) -> Result<Vec<ObjectionTicket>, SubProcessorObjectionStoreError> {
        let tickets = self.tickets.lock().unwrap_or_else(|p| p.into_inner());
        let mut open: Vec<_> = tickets
            .values()
            .filter(|t| {
                matches!(
                    t.ticket_status,
                    ObjectionTicketStatus::Pending | ObjectionTicketStatus::InReview
                )
            })
            .cloned()
            .collect();
        open.sort_by(|a, b| a.expected_resolution_at.cmp(&b.expected_resolution_at));
        Ok(open)
    }
}

/// Always-failing objection store for fail-CLOSED envelope testing.
#[derive(Debug, Clone)]
pub struct FailingObjectionStore;

impl ObjectionStore for FailingObjectionStore {
    fn insert(&self, _ticket: ObjectionTicket) -> Result<(), SubProcessorObjectionStoreError> {
        Err(SubProcessorObjectionStoreError::Unavailable(
            "FailingObjectionStore always fails".into(),
        ))
    }

    fn get(
        &self,
        _objection_id: &str,
    ) -> Result<Option<ObjectionTicket>, SubProcessorObjectionStoreError> {
        Err(SubProcessorObjectionStoreError::Unavailable(
            "FailingObjectionStore always fails".into(),
        ))
    }

    fn update_status(
        &self,
        _objection_id: &str,
        _new_status: ObjectionTicketStatus,
        _decision: Option<ObjectionDecision>,
        _resolution_note: Option<String>,
        _resolved_at: Option<String>,
    ) -> Result<(), SubProcessorObjectionStoreError> {
        Err(SubProcessorObjectionStoreError::Unavailable(
            "FailingObjectionStore always fails".into(),
        ))
    }

    fn list_open(&self) -> Result<Vec<ObjectionTicket>, SubProcessorObjectionStoreError> {
        Err(SubProcessorObjectionStoreError::Unavailable(
            "FailingObjectionStore always fails".into(),
        ))
    }
}

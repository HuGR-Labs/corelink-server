//! Sub-processor emitter orchestrator.
//!
//! Implements the canonical fail-CLOSED audit ordering for all sub-processor
//! transparency operations:
//!
//! ```text
//! lookup → emit_audit → mutate_state
//! ```
//!
//! Per AC-007: if audit emit fails, the operation aborts and state is NOT
//! mutated. This aligns with INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116).

use std::sync::{Arc, Mutex};

use super::audit::{SubProcessorAuditRecord, SubProcessorAuditSink};
use super::broadcast::BroadcastStore;
use super::error::SubProcessorEmitError;
use super::event::{
    BroadcastLogEntry, DeliveryStatus, NotificationType, ObjectionPayload,
    ObjectionTicketStatus, SubProcessorChangedPayload, SubProcessorEventType,
    SubProcessorObjectionPayload, SubProcessorPublishedPayload,
};
use super::event::ObjectionDecision;
use super::objection::{ObjectionStore, ObjectionTicket};

/// Request to publish the initial sub-processor list.
#[derive(Debug, Clone)]
pub struct PublishRequest {
    /// Event source string.
    pub source: String,
    /// Event ID (ULID recommended).
    pub event_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Audit region.
    pub region: String,
    /// Published payload.
    pub payload: SubProcessorPublishedPayload,
}

/// Request to record a sub-processor list change and trigger 30d broadcast.
#[derive(Debug, Clone)]
pub struct ChangeRequest {
    /// Event source string.
    pub source: String,
    /// Event ID (ULID recommended).
    pub event_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Audit region.
    pub region: String,
    /// Changed payload with diff.
    pub payload: SubProcessorChangedPayload,
    /// Broadcast log entries to seed (one per subscribed customer).
    pub broadcast_entries: Vec<BroadcastLogEntry>,
}

/// Request to file a customer objection.
#[derive(Debug, Clone)]
pub struct ObjectionRequest {
    /// Event source string.
    pub source: String,
    /// Event ID (ULID recommended).
    pub event_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Audit region.
    pub region: String,
    /// Objection ULID.
    pub objection_id: String,
    /// Objection payload.
    pub payload: ObjectionPayload,
    /// SHA-256 of subject_id (CTRL-PRIV-014).
    pub subject_id_hash: String,
    /// Expected resolution timestamp (filed_at + 14 calendar days).
    pub expected_resolution_at: String,
}

/// Request to resolve an objection ticket.
#[derive(Debug, Clone)]
pub struct ResolutionRequest {
    /// Objection ticket ID.
    pub objection_id: String,
    /// Event source string.
    pub source: String,
    /// Event ID (ULID recommended).
    pub event_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Audit region.
    pub region: String,
    /// New ticket status.
    pub new_status: ObjectionTicketStatus,
    /// Resolution decision.
    pub decision: Option<ObjectionDecision>,
    /// Privacy Officer + Legal note.
    pub resolution_note: Option<String>,
    /// ISO 8601 UTC resolved timestamp.
    pub resolved_at: Option<String>,
    /// Sub-processors version context.
    pub sub_processors_version: String,
}

/// Trait for sub-processor emitter operations.
///
/// All operations follow: `lookup → emit_audit → mutate_state`.
pub trait SubProcessorEmitter: std::fmt::Debug + Send + Sync {
    /// Emit initial sub-processor publication event.
    /// AC-001: No broadcast triggered (initial publish).
    fn publish(
        &self,
        request: PublishRequest,
    ) -> Result<(), SubProcessorEmitError>;

    /// Emit sub-processor change event + seed broadcast log.
    /// AC-002: Triggers 30d broadcast to all subscribed customers.
    fn record_change(
        &self,
        request: ChangeRequest,
    ) -> Result<(), SubProcessorEmitError>;

    /// File a customer objection.
    /// AC-004: Creates ticket + emits audit event.
    fn file_objection(
        &self,
        request: ObjectionRequest,
    ) -> Result<(), SubProcessorEmitError>;

    /// Resolve an objection ticket (Privacy Officer + Legal decision).
    /// AC-005: accept or terminate.
    fn resolve_objection(
        &self,
        request: ResolutionRequest,
    ) -> Result<(), SubProcessorEmitError>;

    /// Update delivery status for a broadcast log entry (webhook callback).
    /// AC-003: delivery confirmation tracking.
    fn update_delivery_status(
        &self,
        log_id: &str,
        status: DeliveryStatus,
        delivered_at: Option<String>,
        error_class: Option<String>,
    ) -> Result<(), SubProcessorEmitError>;

    /// Query delivery rate for a broadcast.
    /// AC-003 / AC-008: FM-453 detection.
    fn delivery_rate(
        &self,
        broadcast_id: &str,
    ) -> Result<(usize, usize), SubProcessorEmitError>;
}

/// In-memory orchestrator implementing the full emitter protocol.
///
/// Per-instance `Arc<Mutex<()>>` F-001 closure — NEVER `static LazyLock`.
#[derive(Debug, Clone)]
pub struct InMemorySubProcessorEmitter {
    _lock: Arc<Mutex<()>>,
    audit_sink: Arc<dyn SubProcessorAuditSink>,
    broadcast_store: Arc<dyn BroadcastStore>,
    objection_store: Arc<dyn ObjectionStore>,
}

impl InMemorySubProcessorEmitter {
    /// Construct a new emitter with the given trait implementations.
    #[must_use]
    pub fn new(
        audit_sink: Arc<dyn SubProcessorAuditSink>,
        broadcast_store: Arc<dyn BroadcastStore>,
        objection_store: Arc<dyn ObjectionStore>,
    ) -> Self {
        Self {
            _lock: Arc::new(Mutex::new(())),
            audit_sink,
            broadcast_store,
            objection_store,
        }
    }
}

impl SubProcessorEmitter for InMemorySubProcessorEmitter {
    fn publish(
        &self,
        request: PublishRequest,
    ) -> Result<(), SubProcessorEmitError> {
        // lookup (nothing to look up for publish)
        // emit_audit BEFORE mutate_state (fail-CLOSED)
        let payload_json = serde_json::to_string(&request.payload)
            .map_err(|e| SubProcessorEmitError::Internal(format!("serialize payload: {e}")))?;
        self.audit_sink.emit(SubProcessorAuditRecord {
            event_type: SubProcessorEventType::Published,
            source: request.source,
            event_id: request.event_id,
            timestamp: request.timestamp,
            payload_json,
            region: request.region,
        })?;
        // mutate_state: nothing to mutate for publish (CD pipeline handles Pages deploy)
        Ok(())
    }

    fn record_change(
        &self,
        request: ChangeRequest,
    ) -> Result<(), SubProcessorEmitError> {
        // lookup: nothing to look up for change record
        // emit_audit BEFORE mutate_state
        let payload_json = serde_json::to_string(&request.payload)
            .map_err(|e| SubProcessorEmitError::Internal(format!("serialize payload: {e}")))?;
        self.audit_sink.emit(SubProcessorAuditRecord {
            event_type: SubProcessorEventType::Changed,
            source: request.source,
            event_id: request.event_id,
            timestamp: request.timestamp,
            payload_json,
            region: request.region,
        })?;
        // mutate_state: seed broadcast log entries
        for entry in request.broadcast_entries {
            self.broadcast_store.insert(entry)?;
        }
        Ok(())
    }

    fn file_objection(
        &self,
        request: ObjectionRequest,
    ) -> Result<(), SubProcessorEmitError> {
        // lookup: check for duplicate via ObjectionStore (deferred to insert)
        // emit_audit BEFORE mutate_state (fail-CLOSED)
        let audit_payload = SubProcessorObjectionPayload {
            objection_id: request.objection_id.clone(),
            tenant_id: request.payload.tenant_id.clone(),
            subject_id_hash: request.subject_id_hash.clone(),
            sub_processor_id: request.payload.sub_processor_id.clone(),
            sub_processors_version: request.payload.sub_processors_version.clone(),
            filed_at: request.payload.filed_at.clone(),
            outcome: None,
        };
        let payload_json = serde_json::to_string(&audit_payload)
            .map_err(|e| SubProcessorEmitError::Internal(format!("serialize payload: {e}")))?;
        self.audit_sink.emit(SubProcessorAuditRecord {
            event_type: SubProcessorEventType::ObjectionFiled,
            source: request.source,
            event_id: request.event_id,
            timestamp: request.timestamp,
            payload_json,
            region: request.region,
        })?;
        // mutate_state: insert ticket
        let ticket = ObjectionTicket::new(
            request.objection_id,
            &request.payload,
            request.subject_id_hash,
            request.expected_resolution_at,
        );
        self.objection_store.insert(ticket)?;
        Ok(())
    }

    fn resolve_objection(
        &self,
        request: ResolutionRequest,
    ) -> Result<(), SubProcessorEmitError> {
        // lookup: fetch existing ticket
        let ticket = self
            .objection_store
            .get(&request.objection_id)?
            .ok_or_else(|| {
                SubProcessorEmitError::Internal(format!(
                    "objection ticket not found: {}",
                    request.objection_id
                ))
            })?;

        // Validate state transition
        if !ticket.ticket_status.can_transition_to(request.new_status) {
            return Err(SubProcessorEmitError::InvalidStateTransition {
                from: format!("{:?}", ticket.ticket_status),
                to: format!("{:?}", request.new_status),
            });
        }

        // emit_audit BEFORE mutate_state (fail-CLOSED)
        let audit_payload = SubProcessorObjectionPayload {
            objection_id: request.objection_id.clone(),
            tenant_id: ticket.tenant_id.clone(),
            subject_id_hash: ticket.subject_id_hash.clone(),
            sub_processor_id: ticket.sub_processor_id.clone(),
            sub_processors_version: request.sub_processors_version.clone(),
            filed_at: ticket.filed_at.clone(),
            outcome: request.decision.map(|d| format!("{d:?}")),
        };
        let payload_json = serde_json::to_string(&audit_payload)
            .map_err(|e| SubProcessorEmitError::Internal(format!("serialize payload: {e}")))?;
        self.audit_sink.emit(SubProcessorAuditRecord {
            event_type: SubProcessorEventType::ObjectionFiled,
            source: request.source,
            event_id: request.event_id,
            timestamp: request.timestamp,
            payload_json,
            region: request.region,
        })?;

        // mutate_state
        self.objection_store.update_status(
            &request.objection_id,
            request.new_status,
            request.decision,
            request.resolution_note,
            request.resolved_at,
        )?;
        Ok(())
    }

    fn update_delivery_status(
        &self,
        log_id: &str,
        status: DeliveryStatus,
        delivered_at: Option<String>,
        error_class: Option<String>,
    ) -> Result<(), SubProcessorEmitError> {
        self.broadcast_store
            .update_delivery_status(log_id, status, delivered_at, error_class)?;
        Ok(())
    }

    fn delivery_rate(
        &self,
        broadcast_id: &str,
    ) -> Result<(usize, usize), SubProcessorEmitError> {
        let (delivered, total) = self.broadcast_store.delivery_rate(broadcast_id)?;
        Ok((delivered, total))
    }
}

/// Create a broadcast log entry for a given tenant.
///
/// Helper for seeding the broadcast log (one entry per subscribed tenant).
#[must_use]
pub fn make_broadcast_entry(
    log_id: &str,
    broadcast_id: &str,
    sub_processors_version: &str,
    tenant_id: &str,
    recipient_email_hash: &str,
    locale: super::event::EmailLocale,
    enqueued_at: &str,
) -> BroadcastLogEntry {
    BroadcastLogEntry {
        log_id: log_id.to_owned(),
        broadcast_id: broadcast_id.to_owned(),
        sub_processors_version: sub_processors_version.to_owned(),
        tenant_id: tenant_id.to_owned(),
        recipient_email_hash: recipient_email_hash.to_owned(),
        locale,
        notification_type: NotificationType::AdvanceNotice30d,
        enqueued_at: enqueued_at.to_owned(),
        delivered_at: None,
        delivery_status: DeliveryStatus::Enqueued,
        delivery_error_class: None,
    }
}

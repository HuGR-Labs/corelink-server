//! `corelink-privacy-sub-processor-emit` — Sub-Processor Register CloudEvents emitter
//! (WI-S11-005 — S-11 Privacy Pipeline HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the CoreLink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the sub-processor transparency primitive. The trait
//! surfaces match what every production wiring (CD pipeline CloudEvents
//! fan-out to R2 audit-`<region>` Object Lock 7y, Cloudflare D1 broadcast
//! log, PagerDuty objection routing) will satisfy, plus in-memory
//! orchestrators that exercise every load-bearing invariant.
//!
//! ## Regulatory baseline
//!
//! - **GDPR Art. 28.2**: processor must inform controller of any intended
//!   change to sub-processors ≥ 30 days before the change takes effect,
//!   giving the controller the opportunity to object.
//! - **LGPD Art. 39**: mirrors GDPR Art. 28.2 for Brazilian data
//!   subjects.
//! - **CTRL-PRIV-021**: sub-processor list publication + ≥30d notification
//!   before any change.
//!
//! ## Crate contents
//!
//! 1. [`event`] — [`SubProcessorEventType`] `#[non_exhaustive]` 3-arm
//!    taxonomy (Published / Changed / ObjectionFiled), [`SubProcessorInfo`]
//!    canonical sub-processor metadata, [`SubProcessorDiff`] for change
//!    detection, [`ObjectionPayload`] + [`ObjectionDecision`]
//!    `#[non_exhaustive]`, [`ObjectionTicketStatus`] `#[non_exhaustive]`
//!    5-arm state machine, [`BroadcastLogEntry`] + [`DeliveryStatus`]
//!    `#[non_exhaustive]` 6-arm.
//! 2. [`audit`] — [`SubProcessorAuditRecord`] + [`SubProcessorAuditSink`]
//!    trait + [`InMemorySubProcessorAuditSink`] +
//!    [`FailingSubProcessorAuditSink`] (canonical fail-CLOSED envelope per
//!    ADR-S11-002).
//! 3. [`broadcast`] — [`BroadcastStore`] trait +
//!    [`InMemoryBroadcastStore`] + [`FailingBroadcastStore`] (D1
//!    `sub_processor_broadcast_log` mirror).
//! 4. [`objection`] — [`ObjectionStore`] trait +
//!    [`InMemoryObjectionStore`] + [`FailingObjectionStore`] (D1
//!    `sub_processor_objection` mirror).
//! 5. [`dkim`] — DKIM tenant-scoped key derivation via HKDF
//!    (info=`corelink/v1/dkim-broadcast`); cross-tenant isolation
//!    property.
//! 6. [`emitter`] — [`SubProcessorEmitter`] trait +
//!    [`InMemorySubProcessorEmitter`] orchestrator implementing the
//!    canonical fail-CLOSED audit ordering:
//!    `lookup → emit_audit → mutate_state`.
//! 7. [`error`] — [`SubProcessorEmitError`] `#[non_exhaustive]` taxonomy.
//!
//! ## Invariants enforced
//!
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL §3.6 L116): all 3 CloudEvents
//!   types emitted to audit-`<region>` Object Lock 7y; emit fail-CLOSED
//!   (CD pipeline aborts on emit failure per AC-007).
//! - **INV-SUB-PROCESSOR-BROADCAST-IDEMPOTENT** (HIGH, NEW §3.19): UNIQUE
//!   constraint `(broadcast_id, tenant_id, recipient_email_hash,
//!   notification_type)` prevents duplicate sends; property test 10k iter.
//! - **INV-SUB-PROCESSOR-BROADCAST-ALL-PLANS** (HIGH, NEW §3.19):
//!   `sub_processor_notifications` purpose has `legal_obligation` basis
//!   (privacy_model.md §5.6.1); ALL 5 canonical plans receive; NOT
//!   opt-out-able via consent_revoke. Pinned by
//!   `prop_mandatory_all_plans`.
//! - **INV-SUB-PROCESSOR-DKIM-TENANT-SCOPED** (HIGH, NEW §3.19): DKIM
//!   key derived per-tenant via HKDF; cross-tenant isolation property
//!   test 10k pairs → 0 collisions.
//! - **F-001 closure**: per-instance `Arc<Mutex<>>` never
//!   `static LazyLock<Mutex<>>`.
//!
//! ## Audit fail-CLOSED ordering
//!
//! Every state mutation follows: `lookup → emit_audit → mutate_state`.
//! Test verifies state UNCHANGED on audit emit failure.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod broadcast;
pub mod dkim;
pub mod emitter;
pub mod error;
pub mod event;
pub mod objection;

pub use audit::{
    FailingSubProcessorAuditSink, InMemorySubProcessorAuditSink, SubProcessorAuditRecord,
    SubProcessorAuditSink,
};
pub use broadcast::{
    BroadcastStore, FailingBroadcastStore, InMemoryBroadcastStore,
};
pub use dkim::derive_dkim_key;
pub use emitter::{InMemorySubProcessorEmitter, SubProcessorEmitter};
pub use error::SubProcessorEmitError;
pub use event::{
    BroadcastLogEntry, DeliveryStatus, NotificationType, ObjectionDecision, ObjectionPayload,
    ObjectionTicketStatus, SubProcessorDiff, SubProcessorEventType, SubProcessorInfo,
    SubProcessorPublishedPayload, SubProcessorChangedPayload, SubProcessorObjectionPayload,
    HKDF_INFO_DKIM_BROADCAST,
};
pub use objection::{FailingObjectionStore, InMemoryObjectionStore, ObjectionStore};

/// Schema version for the D1 migration (sub_processor_broadcast_log +
/// sub_processor_objection tables — migrations/N+4__sub_processor_tables.sql).
#[must_use]
pub const fn sub_processor_schema_version() -> u32 {
    4
}

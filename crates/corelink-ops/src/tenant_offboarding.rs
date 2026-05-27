//! `corelink-tenant-offboarding` — Tenant-level offboarding state
//! machine (R-prep).
//!
//! # Why this crate exists (distinct from DSR)
//!
//! `corelink-dsr` ships the **individual** data subject rights surface
//! (LGPD Art. 18 / GDPR Art. 17): one natural person asks for their
//! data to be erased / exported / rectified. The orchestrator there
//! issues a per-subject ticket, runs the canonical 7-event audit
//! taxonomy, and (at WI-S11-002) hands off to
//! `corelink-privacy-erasure-worker` for cross-backend cascade.
//!
//! This crate covers a different lifecycle: a **whole customer**
//! ("we are leaving CoreLink, please offboard the entire tenant") and
//! the 5-state lifecycle that this triggers. The reasons to model it
//! separately:
//!
//! - **State carrier is the tenant, not the subject.** Individual DSR
//!   erasure leaves the tenant ACTIVE; tenant offboarding moves the
//!   whole tenant through a regulated lifecycle.
//! - **Grace period semantics.** Customers get T+0..T+30 to export
//!   everything and T+0..T+45 to revert with full data. DSR has no
//!   equivalent — an erasure ticket is a one-way commitment under
//!   LGPD Art. 19 (15d SLA) / GDPR Art. 12.3 (1mo SLA).
//! - **Final erasure is cryptographic + bulk.** At T+90 the canonical
//!   action is BYOK CMK destroy (when applicable) + R2/D1/KV
//!   tombstone-and-purge across the whole tenant. DSR's erasure
//!   worker handles per-record cascade; this crate orchestrates the
//!   whole-tenant boundary.
//! - **Anti-fraud verification.** Tenant cancellation requires
//!   support-agent verification (someone who can sign on the
//!   account; canonical anti-fraud T-0 check). DSR rights are
//!   exercisable by the data subject directly via WebAuthn step-up.
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the tenant-offboarding state machine. The trait
//! surfaces match what production wiring (CF Worker route at
//! `POST /v1/tenant/{tenant_id}/offboard`, D1 `tenant_offboarding_state`
//! mirror per `migrations/d1/0046_tenant_offboarding_state.sql`,
//! daily cron tick for time-based transitions, BYOK CMK destroy hook
//! via `corelink-byok-revocation`, admin force-advance with dry-run
//! preview) will satisfy, plus an in-memory orchestrator that
//! exercises every load-bearing invariant the production wiring
//! relies on.
//!
//! Specifically:
//!
//! 1. The [`state`] module ships [`TenantOffboardingState`]
//!    `#[non_exhaustive]` 6-arm taxonomy (ACTIVE / CANCEL_REQUESTED /
//!    GRACE_PERIOD / READ_ONLY / SUSPENDED / ERASED), the canonical
//!    [`state::TransitionTrigger`] taxonomy
//!    (`#[non_exhaustive]`: CustomerInitiated / CustomerReverted /
//!    TimerExpired / OpsForced / AdminCommitErasure), the canonical
//!    per-state capability flags (read_allowed / write_allowed /
//!    admin_allowed / export_allowed / restore_allowed) and the
//!    canonical transition table.
//! 2. The [`audit`] module ships [`audit::TenantOffboardingAuditEventType`]
//!    `#[non_exhaustive]` 6-event taxonomy
//!    (`corelink.tenant.offboarding.{cancel_requested, grace_started,
//!    read_only_entered, suspended_entered, restored, erased}`) +
//!    [`audit::TenantOffboardingAuditRecord`] +
//!    [`audit::TenantOffboardingAuditSink`] trait +
//!    [`audit::InMemoryTenantOffboardingAuditSink`] +
//!    [`audit::FailingTenantOffboardingAuditSink`] (fail-CLOSED per
//!    ADR-S11-002 split-tier — regulatory-grade NEVER tolerates
//!    silent loss).
//! 3. The [`store`] module ships [`store::TenantOffboardingStore`]
//!    trait + [`store::InMemoryTenantOffboardingStore`] +
//!    [`store::FailingTenantOffboardingStore`] (production wiring at
//!    PRR ship gate binds this to D1 `tenant_offboarding_state` per
//!    `migrations/d1/0046_tenant_offboarding_state.sql`).
//! 4. The [`orchestrator`] module ships
//!    [`orchestrator::TenantOffboardingOrchestrator`] trait +
//!    [`orchestrator::InMemoryTenantOffboardingOrchestrator`] (run
//!    pipeline: audit BEFORE every state mutation; reject illegal
//!    transitions at the typed-enum boundary).
//! 5. The [`error`] module ships the canonical
//!    [`error::TenantOffboardingError`] `#[non_exhaustive]` taxonomy
//!    (audit / store / illegal-transition / config / internal).
//!
//! # Canonical timeline
//!
//! ```text
//!   T+0       T+30        T+45         T+90
//!    |          |           |            |
//!    v          v           v            v
//!  CANCEL → GRACE → READ_ONLY → SUSPENDED → ERASED
//!  (anti-  (export  (restore-   (admin-     (cryptographic
//!   fraud   window  -window-cap revoke      erasure +
//!   gate)   open)   reached)    only)       BYOK CMK destroy)
//!
//!  ACTIVE may also receive CustomerReverted from
//!  CANCEL_REQUESTED / GRACE_PERIOD / READ_ONLY (T+0..T+45).
//!  SUSPENDED and ERASED are terminal w.r.t. self-service revert.
//! ```
//!
//! # Invariants
//!
//! - **INV-OFFBOARDING-GRACE-RESPECTED** — every state advances only
//!   after the canonical timer threshold (or operator force-advance
//!   with audit trail). The orchestrator validates the timer at the
//!   trait boundary; a request that asks for an advance before the
//!   canonical wall-clock threshold returns
//!   [`error::TenantOffboardingError::IllegalTransition`].
//! - **INV-OFFBOARDING-AUDIT-COMPLETE** — every state mutation is
//!   preceded by the canonical audit row (per ADR-S11-002 split-tier
//!   fail-CLOSED discipline). Audit emit failure aborts the
//!   transition; the durable store remains at its pre-call state.
//!
//! Both pinned by property tests:
//! `prop_state_machine_reachability_respects_grace`,
//! `prop_audit_emit_precedes_every_state_mutation`.
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - CF Worker `POST /v1/tenant/{tenant_id}/offboard` (initiates) +
//!   `POST /v1/tenant/{tenant_id}/offboard/revert` (revert) +
//!   `GET /v1/tenant/{tenant_id}/offboard/status` (poll).
//! - D1 `tenant_offboarding_state` mirror per
//!   `migrations/d1/0046_tenant_offboarding_state.sql` (additive;
//!   schema in §6.1.6 of `WI-R-PREP-TENANT-OFFBOARDING.md`).
//! - Daily cron at 03:00 UTC for time-based transitions
//!   (GRACE_PERIOD → READ_ONLY at T+30; READ_ONLY → SUSPENDED at
//!   T+45; SUSPENDED → ERASED at T+90 with dry-run preview).
//! - BYOK CMK destroy hook via `corelink-byok-revocation` when the
//!   tenant has BYOK enabled (cryptographic erasure ≡ destroy the
//!   tenant DEK-wrap CMK → all R2 envelope-encrypted blobs become
//!   unrecoverable; LGPD Art. 18 VI / GDPR Art. 17 equivalent at
//!   the tenant boundary).
//! - Admin force-advance with dry-run preview (returns the canonical
//!   tombstone set BEFORE commit; operator confirms; commit writes
//!   audit + advances state).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::doc_lazy_continuation)]

pub mod audit;
pub mod error;
pub mod orchestrator;
pub mod state;
pub mod store;

pub use audit::{
    canonical_tenant_offboarding_audit_event_strings, FailingTenantOffboardingAuditSink,
    InMemoryTenantOffboardingAuditSink, TenantOffboardingAuditEventType,
    TenantOffboardingAuditRecord, TenantOffboardingAuditSink,
};
pub use error::{
    TenantOffboardingAuditSinkError, TenantOffboardingError, TenantOffboardingStoreError,
};
pub use orchestrator::{
    AdminCommitErasureRequest, InMemoryTenantOffboardingOrchestrator,
    TenantOffboardingOrchestrator, CANONICAL_GRACE_PERIOD_DAYS, CANONICAL_READ_ONLY_DAYS,
    CANONICAL_SUSPENDED_DAYS, CANONICAL_TOTAL_T_PLUS_90_DAYS,
};
pub use state::{
    canonical_tenant_offboarding_states, canonical_transition_triggers, TenantOffboardingCaps,
    TenantOffboardingState, TenantOffboardingTransition, TransitionTrigger,
};
pub use store::{
    FailingTenantOffboardingStore, InMemoryTenantOffboardingStore, TenantOffboardingRecord,
    TenantOffboardingStore,
};

/// Crate canonical schema version constant. Pinned to the canonical
/// D1 migration `migrations/d1/0046_tenant_offboarding_state.sql`.
#[must_use]
pub const fn tenant_offboarding_schema_version() -> u32 {
    46
}

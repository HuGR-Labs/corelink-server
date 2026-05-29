//! `corelink-handler-customer` — customer self-serve HTTP handler
//! skeleton (R-prep).
//!
//! # Why this crate
//!
//! The `apps/admin-ui` customer dashboard calls six endpoint groups:
//! `/v1/customer/{overview,usage,billing,keys,team,audit}`. None of
//! these exist in the backend today. This crate ships the **typed
//! handler surface** every customer HTTP handler implements + the
//! emit-once-per-entry `SliObserver` hook (using
//! [`corelink_slo::definition::Sli::AvailControlPlane`]) and the
//! audit-fail-CLOSED ordering envelope. The wasm32 CF-Worker impl is
//! deferred per the autonomous-execution charter
//! `trait-abstraction-defer` rule.
//!
//! # Crate contents
//!
//! - [`audit`] — `AuditEventKind` `#[non_exhaustive]` taxonomy (14
//!   variants covering all 6 endpoints) + `AuditEvent` envelope +
//!   `AuditSink` trait + `InMemoryAuditSink` capture-everything fake.
//! - [`observer`] — `SliObserver` trait (emit-once-per-handler-entry)
//!   + `InMemorySliObserver` capture-everything fake.
//! - [`request`] — 11 request/response structs (6 endpoints, billing
//!   split into `billing` + `portal_url`).
//! - [`handler`] — 6 traits + `InMemoryCustomerHandler` deterministic
//!   in-process fake (used by every test and by `apps/server` until
//!   the CF-Worker impl lands).
//! - [`error`] — `CustomerHandlerError` `#[non_exhaustive]` taxonomy.
//!
//! # Invariants enforced
//!
//! - **INV-HANDLER-SLI-EMIT-ENTRY** — every handler entry emits one
//!   `SliObserver::observe` call BEFORE returning
//!   (`Sli::AvailControlPlane`; customer dashboard is control-plane).
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** — every state mutation
//!   (PAT create/revoke, team invite) emits the corresponding audit row
//!   BEFORE the mutation; audit failure aborts the op + returns
//!   [`CustomerHandlerError::AuditFailed`].
//! - **INV-CROSS-TENANT-DENIED** — every request carries `caller_tenant`;
//!   if `caller_tenant != tenant` the handler emits `*Denied` audit BEFORE
//!   returning `CrossTenantDenied`. Read-only endpoints (overview, usage,
//!   billing, audit query) emit `ReadDenied`; mutation endpoints emit their
//!   respective `*Denied` variant.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod handler;
pub mod observer;
pub mod request;

pub use audit::{AuditEvent, AuditEventKind, AuditSink, InMemoryAuditSink};
pub use error::CustomerHandlerError;
pub use handler::{
    CustomerAuditHandler, CustomerBillingHandler, CustomerKeysHandler, CustomerOverviewHandler,
    CustomerTeamHandler, CustomerUsageHandler, InMemoryCustomerHandler,
};
pub use observer::{InMemorySliObserver, SliObservation, SliObserver};
pub use request::{
    AuditQueryRequest, AuditQueryResponse, BillingRequest, BillingResponse, KeyCreateRequest,
    KeyCreateResponse, KeyRevokeRequest, KeyRevokeResponse, KeysListRequest, KeysListResponse,
    OverviewRequest, OverviewResponse, PortalRequest, PortalResponse, TeamInviteRequest,
    TeamInviteResponse, TeamListRequest, TeamListResponse, UsageRequest, UsageResponse,
};

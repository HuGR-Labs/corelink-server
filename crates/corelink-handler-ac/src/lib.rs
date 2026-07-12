//! `corelink-handler-ac` — Action Cache HTTP handler skeleton (R-prep).
//!
//! Closes the handler-layer SLO emit gap for `SLO-AVAIL-AC` +
//! `SLO-LAT-AC-HIT` per `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md`.
//!
//! # Crate contents
//!
//! - [`audit`] — `AuditEventKind` AC audit taxonomy + `AuditEvent` +
//!   `AuditSink` trait + `InMemoryAuditSink` fake.
//! - [`observer`] — re-exports the canonical
//!   `corelink_slo::definition::Sli` and ships `SliObservation` +
//!   `SliObserver` + `InMemorySliObserver`.
//! - [`handler`] — `AcLookupHandler` + `AcUpdateHandler` traits +
//!   `InMemoryAcHandler` fake.
//! - [`error`] — `AcHandlerError` `#[non_exhaustive]` taxonomy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod handler;
pub mod observer;

pub use audit::{AuditEvent, AuditEventKind, AuditSink, InMemoryAuditSink};
pub use error::AcHandlerError;
pub use handler::{
    AcDeleteHandler, AcDeleteRequest, AcDeleteResponse, AcListHandler, AcListRequest,
    AcListResponse, AcLookupHandler, AcLookupRequest, AcLookupResponse, AcRefEntry,
    AcUpdateHandler, AcUpdateRequest, AcUpdateResponse, InMemoryAcHandler,
};
pub use observer::{InMemorySliObserver, Sli, SliObservation, SliObserver};

//! `corelink-handler-cas` — CAS (Content-Addressable Storage) HTTP handler
//! skeleton (R-prep).
//!
//! # Why this crate
//!
//! Per `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §6`, the
//! `apps/server` binary currently exposes only the Stripe webhook
//! route. None of the CAS read/write SLOs (`SLO-AVAIL-CAS-GET`,
//! `SLO-AVAIL-CAS-PUT`, `SLO-LAT-CAS-GET`, `SLO-LAT-CAS-PUT`,
//! `SLO-CORRECT-CAS`) have a handler-layer emit site, so the
//! multi-burn-rate alert path in `corelink-slo` evaluates against
//! a non-existent metric surface. This crate ships the **typed
//! handler surface** every CAS HTTP handler implements + the
//! emit-once-per-entry `SliObserver` hook that supplies the alert
//! evaluator. The wasm32 CF-Worker impl is deferred per the
//! autonomous-execution charter `trait-abstraction-defer` rule.
//!
//! # Crate contents
//!
//! - [`audit`] — `AuditEventKind` `#[non_exhaustive]` taxonomy of CAS
//!   audit event types + `AuditEvent` envelope + `AuditSink` trait
//!   + `InMemoryAuditSink` capture-everything fake.
//! - [`observer`] — `SliObserver` trait (emit-once-per-handler-entry)
//!   + `InMemorySliObserver` capture-everything fake.
//! - [`request`] — `CasReadRequest` / `CasWriteRequest` /
//!   `CasReadResponse` / `CasWriteResponse` envelopes (CAS canonical
//!   hash-keyed surface).
//! - [`handler`] — `CasReadHandler` + `CasWriteHandler` traits +
//!   `InMemoryCasHandler` deterministic in-process fake (used by
//!   every test + by `apps/server` until the CF-Worker impl lands).
//! - [`error`] — `CasHandlerError` `#[non_exhaustive]` taxonomy.
//!
//! # Invariants enforced
//!
//! - **INV-HANDLER-SLI-EMIT-ENTRY** — every handler entry emits one
//!   `SliObserver::observe_*` call BEFORE returning (covers
//!   `Sli::AvailCasGet` / `Sli::AvailCasPut` availability counters
//!   regardless of outcome) so the multi-burn-rate alert evaluator
//!   has the input.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** — every state mutation
//!   (CAS write) emits the corresponding audit row BEFORE the
//!   mutation; audit failure aborts the write + returns
//!   [`CasHandlerError::AuditFailed`]. The in-memory fake mirrors
//!   that ordering so cross-tenant proptests can falsify it.
//! - **INV-CAS-CORRECTNESS** — every read returns either bytes
//!   whose hash matches the requested key OR a hash-mismatch
//!   error. The fake mirrors this: a hash-mismatch injection emits
//!   `Sli::CorrectnessCas` failure observation.
//!
//! # Deferred — wasm32 CF-Worker impl
//!
//! The real `CfWorkerCasHandler` against R2 + KV + DO bindings lives
//! at the placeholder `#[cfg(target_arch = "wasm32")]` module below
//! (currently doc-only; per `trait-abstraction-defer`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod handler;
pub mod observer;
pub mod request;

pub use audit::{AuditEvent, AuditEventKind, AuditSink, InMemoryAuditSink};
pub use error::CasHandlerError;
pub use handler::{CasReadHandler, CasWriteHandler, InMemoryCasHandler};
pub use observer::{InMemorySliObserver, SliObservation, SliObserver};
pub use request::{CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse};

/// Placeholder for the real wasm32 CF-Worker CAS handler. Currently
/// gated to `cfg(target_arch = "wasm32")` and **not implemented**;
/// the production impl lands in `WI-S04-CF-WIRING` per the
/// autonomous-execution charter `trait-abstraction-defer` rule.
///
/// The wire-up shape is:
///
/// ```ignore
/// #[cfg(target_arch = "wasm32")]
/// pub mod cf_worker {
///     use crate::handler::{CasReadHandler, CasWriteHandler};
///     use corelink_cf_bindings::{R2Bucket, KvNamespace};
///
///     pub struct CfWorkerCasHandler {
///         r2: R2Bucket,
///         meta_kv: KvNamespace,
///         sli: std::sync::Arc<dyn crate::observer::SliObserver>,
///         audit: std::sync::Arc<dyn crate::audit::AuditSink>,
///     }
///
///     impl CasReadHandler for CfWorkerCasHandler { /* … */ }
///     impl CasWriteHandler for CfWorkerCasHandler { /* … */ }
/// }
/// ```
///
/// Until then, `apps/server` wires the in-memory fake under the
/// native `cfg(not(target_arch = "wasm32"))` branch.
#[cfg(target_arch = "wasm32")]
pub mod cf_worker_placeholder {
    //! Reserved for the real CF-Worker R2/KV-bound CAS handler.
}

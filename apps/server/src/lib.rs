//! `corelink-server` — CoreLink server binary support library.
//!
//! Hosts modules that are exercised by integration tests (which need
//! to link against the crate as a library, not just the binary).
//!
//! # Modules
//!
//! - [`webhook`] — R2-12 Stripe webhook HTTP route. Signature verify,
//!   idempotent dedup, and event-type dispatchers. See module docs
//!   for the full request flow.
//! - [`routes`] — HTTP route surface. Currently exposes the CAS read
//!   end-to-end as the example wire-up for the R-prep handler-crate
//!   skeleton (see
//!   `specs/_audits/2026-05-14-slo-instrumentation-gaps.md §6`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod routes;
pub mod webhook;

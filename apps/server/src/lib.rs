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
//! - [`byok`] — feature-gated BYOK provider factory (only built when
//!   `--features byok-aws-real`). Returns
//!   `Arc<dyn corelink_byok::KmsProvider>` so future GCP / Azure /
//!   Vault providers swap in behind the same trait object. See
//!   `specs/_audits/2026-05-15-byok-real-provider-pattern.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

#[cfg(feature = "byok-aws-real")]
pub mod byok;

pub mod routes;
pub mod webhook;

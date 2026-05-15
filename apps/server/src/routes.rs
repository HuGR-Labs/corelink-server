//! HTTP route surface for the CoreLink server.
//!
//! Currently exposes the CAS read end-to-end as the **example
//! wire-up** for the R-prep handler-crate skeleton (see
//! `specs/_audits/2026-05-14-slo-instrumentation-gaps.md §6`):
//! `apps/server::routes::cas::CasReadHandler` wires
//! `corelink-handler-cas::CasReadHandler` + in-memory fakes into an
//! axum route, demonstrating where the `#[cfg(target_arch = "wasm32")]`
//! CF-Worker handler would slot in once it lands.
//!
//! AC + Admin handlers ride along the same pattern; they remain
//! library-only (handler crate + tests) until their route surfaces
//! are agreed.

/// CAS HTTP routes (R-prep example wire-up).
pub mod cas;

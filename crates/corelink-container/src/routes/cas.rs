//! `GET /v1/cas/:tenant/:hash` — CAS read route wired against
//! [`corelink_handler_cas::CasReadHandler`].
//!
//! This is the **example end-to-end wire-up** demonstrating how the
//! R-prep handler-crate skeleton plugs into `apps/server`. The
//! production CF-Worker R2-bound handler would replace
//! `InMemoryCasHandler` under the `#[cfg(target_arch = "wasm32")]`
//! branch in the trait-object construction site (see crate-level
//! `corelink_handler_cas` doc).
//!
//! # Why a trait object
//!
//! The route handler accepts `Arc<dyn CasReadHandler>` so the route
//! table stays binary-shape stable as we swap the in-memory fake for
//! the wasm32 CF-Worker impl. The same shape is used for the
//! Stripe webhook route's
//! [`corelink_stripe_real::webhook_dispatch::StateMaterializer`]
//! collaborator (wave 16 unification —
//! `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`
//! §unification).
//!
//! # SLO emit
//!
//! Every entry into this route emits one `Sli::AvailCasGet` + one
//! `Sli::LatencyCasGetP99` observation through the
//! `SliObserver` collaborator on the handler. Per the audit
//! 2026-05-14 closure list, this transitions `SLO-AVAIL-CAS-GET`
//! from "Sli-bound deferred" to "Sli emitted at handler layer".
//!
//! Re-anchor ledger (the source units remain in this module's namespace):
//! - `foundation.rs`: imports, route constants, `CasRouteState`, budgets,
//!   guards, and state constructors.
//! - `single.rs`: handler construction, router wiring, request parsing, PAT
//!   gates, and single-object read/write handlers.
//! - `batch.rs`: batch write/read/exists handlers and error formatting.
//! - `list_delete.rs`: NDJSON parsing plus delete/list/error mapping helpers.
//! - `tests_core_part1.rs`, `tests_core_part2.rs`, `tests_batch_part1.rs`,
//!   `tests_batch_part2.rs`, `tests_batch_write_part2.rs`, and
//!   `tests_edges.rs`: original `tests` module items, split by existing
//!   source order only; batch-write coverage has its own include to keep the
//!   B-326 source cap mechanical.
//! - `tests_read_ceiling.rs`: original read-size ceiling test items.

include!("cas/foundation_core.rs");
include!("cas/foundation_state.rs");
include!("cas/single_setup.rs");
include!("cas/single_handlers.rs");
include!("cas/batch_write.rs");
include!("cas/batch_read.rs");
include!("cas/list_delete.rs");

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use corelink_handler_cas::handler::fake_hash;

    include!("cas/tests_core_part1.rs");
    include!("cas/tests_core_part2.rs");
    include!("cas/tests_batch_part1.rs");
    include!("cas/tests_batch_part2.rs");
    include!("cas/tests_batch_write_part2.rs");
    include!("cas/tests_edges.rs");
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod read_size_ceiling_tests {
    use super::*;

    include!("cas/tests_read_ceiling.rs");
}

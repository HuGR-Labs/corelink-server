//! `corelink-server` — CoreLink server binary support library.
//!
//! Hosts modules that are exercised by integration tests (which need
//! to link against the crate as a library, not just the binary).
//!
//! # Modules
//!
//! - [`webhook`] — Stripe webhook HTTP shell (axum). Thin boundary
//!   over the canonical
//!   [`corelink_stripe_real::webhook_dispatch::WebhookDispatcher`]
//!   pipeline (wave-16 unification, audit doc
//!   `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`).
//! - [`routes`] — HTTP route surface. Currently exposes the CAS read
//!   end-to-end as the example wire-up for the R-prep handler-crate
//!   skeleton (see
//!   `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §6`).
//! - `byok` — feature-gated AWS-only BYOK provider factory (built
//!   when `--features byok-aws-real`). Preserved as a thin convenience
//!   wrapper; new code should use [`byok_orchestrator`].
//! - [`byok_orchestrator`] — singleton trait-object dispatch over the
//!   four production BYOK providers (AWS / GCP / Azure / Vault),
//!   feature-flag-selected at compile time. Default (no flag) returns
//!   an `InMemoryFake`. Multiple `byok-*-real` flags is a HARD
//!   compile error. See
//!   `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7`.
//! - [`wall_clock`] — cross-route wall-clock trait (`WallClock` +
//!   `SystemWallClock` + `InMemoryFakeWallClock`). Wave-21 closure of
//!   the `A-P2-05` (audit-export) + `B-P2-03` (audit-analytics)
//!   findings: both routes consume `Arc<dyn WallClock>` in their route
//!   state so the rate-limit `now_ms` becomes wall-clock-derived rather
//!   than window-derived. See
//!   `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamA-audit-export.md`
//!   and `…-streamB-neon-shadow.md`.
//!
//! # INV pin map (W36-PROPTEST-FU-001 closure)
//!
//! This crate carries INV references in route-boundary doc comments
//! that name **which invariant is pinned at the HTTP boundary**, not
//! where the load-bearing property tests live. Per WI-PROPTEST-FU-W33-001
//! closure (`specs/_audits/sealed/2026-05-26-w36-proptest-fu-001-seal.md`),
//! the property tests for each pinned INV live in the owning crate
//! listed below; this crate is listed in
//! `scripts/proptest-density-allowlist.txt` as an
//! "INV-pin documentation" exemption:
//!
//! | INV ref pinned here              | Property-test owner crate(s)                         |
//! |----------------------------------|------------------------------------------------------|
//! | `INV-AUTH-MIGRATION-ADDITIVE`    | `corelink-d1-migrations` (`tests/prop_migration_additivity.rs`) |
//! | `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` | `corelink-audit` + `corelink-slack-real` (`tests/prop_slack_emit_atomic.rs`) |
//! | `INV-TENANT-ISOLATION`           | `corelink-tenant-path` + `corelink-auth::schema` (`corelink-auth/tests/schema_prop_schema.rs`) |
//!
//! The route-boundary references in `src/routes/*.rs` are **pin
//! annotations** documenting which invariant the route enforces; they
//! do NOT relocate the property logic. See the SEAL audit above for
//! the full closure narrative.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

#[cfg(feature = "byok-aws-real")]
pub mod byok;

pub mod byok_orchestrator;
#[cfg(feature = "neon-real")]
pub mod neon_shadow_factory;
pub mod routes;
pub mod wall_clock;
pub mod webhook;

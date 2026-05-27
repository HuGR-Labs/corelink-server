//! `corelink-dpa-versioning` — DPA versioning + re-acceptance + 30d grace
//! + PAT-DEGRADE-001 read-only middleware (WI-S19-003).
//!
//! # What this crate ships
//!
//! Per the `trait-abstraction-defer` charter pattern, this crate ships
//! the **pure-logic skeleton** of the DPA versioning lifecycle. The
//! trait surfaces match what every production wiring (CF Worker
//! `POST /v1/dpa/re-accept` route, D1 `dpa_versions` mirror, SES email
//! broadcast, in-app banner backend, customer support escalation) will
//! satisfy, plus in-memory orchestrators / cron / middleware that
//! exercise every load-bearing invariant the production wiring relies
//! on.
//!
//! # GDPR Art. 7§3 + LGPD Art. 8§5 alignment
//!
//! A DPA is a **versioned legal contract**, not a one-shot acceptance.
//! Material changes (new sub-processors, new regions, breach SLA
//! changes) require re-acceptance per GDPR Art. 7§3 (freely revoke) +
//! LGPD Art. 8§5 (purpose change). Customer must (a) be notified, (b)
//! be given reasonable time to review, (c) retain read access during
//! the grace period to avoid being coerced into accepting under
//! service-degradation duress. Post-grace, write degrade (NOT full
//! shutoff) is the canonical resolution: data remains accessible to
//! the Controller (consumer protection) while the Processor stops
//! accepting *new* processing instructions until consent refreshes.
//!
//! # Invariants enforced
//!
//! - **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL): every re-acceptance
//!   bumps the tenant's `current_dpa_version` atomically; the version
//!   number on the receipt MUST match the DPA version published at the
//!   time of acceptance (replay-old-acceptance attack mitigated).
//!   Pinned by `prop_re_accept_idempotency`.
//!
//! - **PAT-DEGRADE-001** (resilience_patterns.md): post-grace tenants
//!   reject writes (POST / PUT / PATCH / DELETE) on `/v1/*` EXCEPT the
//!   `/v1/dpa/re-accept` endpoint (so the customer can self-recover);
//!   reads (GET) always allowed. Writes blocked iff `grace_expired`.
//!   Pinned by `prop_read_only_enforcement`.
//!
//! - **Grace boundary** (canonical 30d UTC): on a Major bump at `t0`,
//!   `grace_expires_at = t0 + 30d`. Tenants with `now < grace_expires_at`
//!   stay writable; tenants with `now >= grace_expires_at` degrade.
//!   Pinned by `prop_grace_boundary`.
//!
//! # Modules
//!
//! 1. [`version`] — [`version::SemverVersion`] + [`version::BumpKind`]
//!    `#[non_exhaustive]` 3-arm enum (Major / Minor / Patch). Bump kind
//!    derivation from old → new semver pair (Major iff `major` bump).
//! 2. [`schema`] — [`schema::DpaVersionRecord`] (published DPA entry),
//!    [`schema::TenantDpaState`] (current_version + grace_expires_at +
//!    pending flag), [`schema::ReAcceptanceReceipt`] (mirrors WI-002
//!    receipt shape).
//! 3. [`broadcast`] — [`broadcast::BroadcastSink`] trait (SES email
//!    broadcast + bounce / retry handling stub) + in-memory capture
//!    sink + always-failing sink for fail-CLOSED testing.
//! 4. [`store`] — [`store::DpaStore`] trait (D1 mirror surface for
//!    `dpa_versions` table + `tenants.dpa_*` columns) + in-memory
//!    fake + always-failing fake.
//! 5. [`versioning`] — [`versioning::DpaVersioning`] trait +
//!    [`versioning::InMemoryDpaVersioning`] orchestrator (run pipeline:
//!    classify bump → persist version → broadcast (Major only) → seed
//!    grace deadlines on all tenants → emit metric scaffold).
//! 6. [`re_accept`] — re-acceptance handler primitive (verify pending,
//!    version match, atomic update, idempotent on replay).
//! 7. [`cron`] — [`cron::GraceExpirationCron`] daily scanner (flag
//!    tenants approaching grace ≤7d → email reminder via
//!    [`broadcast::BroadcastSink`]; degrade tenants past `grace_expires_at`).
//! 8. [`middleware`] — [`middleware::ReadOnlyDegradeGate`] +
//!    [`middleware::HttpMethod`] enum + [`middleware::GateDecision`].
//!    Pure function: `(tenant_state, method, path) -> Allow | Deny`.
//! 9. [`metrics`] — Prometheus snake_case scaffold names + counter /
//!    gauge primitives.
//! 10. [`error`] — [`error::DpaVersioningError`] `#[non_exhaustive]`
//!     taxonomy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::format_in_format_args, clippy::uninlined_format_args)]
// `versioning::versioning` is an unavoidable inception arising from
// W35-P2 absorbing the `corelink-dpa-versioning` crate into the
// `corelink-privacy::dpa::versioning` submodule while preserving the
// inner `versioning.rs` orchestrator file name (which mirrors the
// public-API anchor `DpaVersioning` trait name).
#![allow(clippy::module_inception)]

pub mod broadcast;
pub mod cron;
pub mod error;
pub mod metrics;
pub mod middleware;
pub mod re_accept;
pub mod schema;
pub mod store;
pub mod version;
pub mod versioning;

//! `corelink-quota` — Quota enforcement middleware engine
//! (WI-S07-003).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the quota enforcement plane: trait surfaces every
//! production Cloudflare DO singleton + Tower layer will satisfy, plus
//! in-memory fakes that exercise every load-bearing invariant the
//! production wiring relies on. Property tests pinned at 10 k iter
//! against the fakes cover INV-QUOTA-ENFORCEMENT (real-time tenant
//! check; DO actor model serialisation; FM-059 race elimination),
//! INV-QUOTA-RESERVATION-TTL (size-proportional TTL formula auto-release;
//! no quota leak), INV-TENANT-ISOLATION (CTRL-ISO-005 strict
//! tenant-scope; per-instance Mutex; F-001 closure preserved), and the
//! PROVISIONAL 100% 429 + Retry-After arm (transitional per ADR-0020
//! FROZEN; canonical S-08 CAP-QUOTA-001 rate-limit DO ships hard-block).
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0009_quota_reservations.sql`
//!    embedded via [`MIGRATION_0009_QUOTA_RESERVATIONS`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the DO in-memory pending
//!    reservation map; durable reload across worker restarts.
//! 2. The [`reservation`] module ships [`ReservationId`] (newtype wrap
//!    around UUIDv7) + [`ReservationRow`] (materialised projection of
//!    the SQL row) + [`ReservationTracker`] trait + [`InMemoryReservationTracker`]
//!    fake whose semantics mirror the SQL `quota_reservations` table
//!    byte-for-byte (`(tenant_id, reservation_id)` PK, expires_at TTL
//!    sweep, monotone immutable `requested_bytes`).
//! 3. The [`audit`] module ships [`QuotaEventType`] +
//!    [`QuotaAuditRecord`] + [`QuotaAuditSink`] +
//!    [`InMemoryQuotaAuditSink`] capture sink (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The canonical 5-event
//!    taxonomy is `corelink.quota.{check_passed, denied_429, reserved,
//!    reservation_expired, reservation_rolled_in}`.
//! 4. The [`metrics`] module ships [`QuotaMetricsObserver`] +
//!    [`InMemoryQuotaMetrics`] capture sink (4 canonical metrics:
//!    `corelink.quota.check_total{result=allow|deny|reserve}`,
//!    `corelink.quota.denials_total`,
//!    `corelink.quota.reservation_active{tenant,region}`,
//!    `corelink.quota.check_duration_ms`).
//! 5. The [`error`] module ships the canonical [`QuotaError`]
//!    `#[non_exhaustive]` taxonomy.
//! 6. The [`config`] module ships [`QuotaConfig`] (knobs pinned to
//!    canonical thresholds: 95% eviction trigger, 100% deny boundary,
//!    Retry-After floor) + per-instance F-001 closure semantics.
//! 7. The [`check`] module ships [`QuotaDecision`] `#[non_exhaustive]`
//!    (`Allow / Deny429 / Reserve`) + [`QuotaCheck`] trait +
//!    [`InMemoryQuotaCheck`] decision engine wired to the dependencies
//!    above.
//! 8. The [`retry_after`] module ships [`provisional_retry_after_secs`]
//!    — the canonical PROVISIONAL Retry-After computation (transitional
//!    per ADR-0020 FROZEN; production canonical formula
//!    `Retry-After: days-until-month-reset` ships in S-08 CAP-QUOTA-001).
//!
//! # Why quota is `trait + fake` here, real DO/D1 in WI-S07-005
//!
//! S-07 lands without Cloudflare DO bindings wired into CI (no remote +
//! Cloudflare Workers + D1 staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! FM-059 race elimination via per-instance Mutex serialisation
//! (mirrors DO actor model); reservation TTL auto-release (no leak);
//! tenant isolation; idempotent reservation lookup; the 100% boundary
//! 429 + provisional Retry-After arm. The live-DO + D1 conformance
//! tests run alongside WI-S07-005 (PRR ship gate) once
//! miniflare/wrangler-dev integration tests land.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-QUOTA-ENFORCEMENT** (HIGH; spec_contract §8): tenant
//!   real-time check; the in-memory fake's per-instance `Mutex`
//!   serialises concurrent `check_and_reserve` calls so 1k QPS at
//!   99.9% boundary cannot over-quota. Pinned by
//!   `prop_quota_atomic_no_race`.
//! - **INV-QUOTA-RESERVATION-TTL** (HIGH; spec_contract §8): pending
//!   reservations auto-release at size-proportional TTL
//!   (`max(60s, request_bytes/1MB/s × 2x), capped 7d` per Lote 10.7bis
//!   R5 P0-2; reused from `corelink-eviction::reservation_ttl_ms`).
//!   Pinned by `prop_reservation_expiry_releases_bytes`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+): `(tenant_id,
//!   reservation_id)` PK tenant-leftmost; tenant A reservations NEVER
//!   visible/mutable from tenant B context. Pinned by `prop_tenant_isolation`.
//! - **PROVISIONAL 429 boundary**: `bytes_used + active_reservations +
//!   request_bytes > bytes_quota` → `Deny429` arm. The Retry-After
//!   value is computed via `provisional_retry_after_secs(utilization)`
//!   (transitional per ADR-0020 FROZEN). Pinned by
//!   `prop_quota_check_at_100pct_denies_429`.
//! - **Audit fail-closed** (Lote 10.6bis pattern): every check decision
//!   emits an audit BEFORE state mutation (reservation insert).
//!   Pinned by `prop_audit_emit_per_decision_arm`.
//! - **< 3ms p99 hot path** (spec_contract §10.s07.3): in-memory
//!   operations only; no external IO in the decision hot path.
//!   Documented + pinned by `prop_check_duration_under_3ms_p99`
//!   (informational).
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - **No `tokio`** in `src/` (wasm32-clean lib code; tokio only in
//!   tests if needed).
//! - The fake is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the reservation-relevant columns and
//!   reports structured [`QuotaError`] errors when an invariant fires.
//!
//! # Scope boundary vs S-08 CAP-QUOTA-001
//!
//! S-07 WI-S07-003 ships the **PROVISIONAL transitional** 429 +
//! Retry-After implementation. The middleware-level response shape
//! (status code, header presence, body taxonomy) is established here,
//! but the canonical rate-limit DO infrastructure with
//! `Retry-After: days-until-month-reset` semantic ships in S-08
//! CAP-QUOTA-001 (per ADR-0020 FROZEN). Boundary:
//! - **≤ 95%** = S-07 owns (eviction trigger via
//!   [`corelink_eviction::should_fire_quota_trigger`]).
//! - **≥ 100%** = S-08 owns (canonical hard-block); S-07 emits
//!   PROVISIONAL 429 deferring to S-08 for canonical rate-limit DO.
//!
//! # Wiring into the Tower middleware (forward; WI-S07-005 ship gate)
//!
//! The production wiring composes [`QuotaCheck::check_and_reserve`] on
//! top of the existing `AuthLayer` (WI-S03-003 SEALED; provides
//! `AuthCtx` extracted tenant_id) and the 5-Layer Defense from S-04.
//! The Tower layer construction itself (gated by feature
//! `corelink-worker/tower-middleware`) lives outside this crate to
//! preserve wasm32-clean compilation; this crate's
//! [`InMemoryQuotaCheck`] returns a [`QuotaDecision`] enum that the
//! Tower layer interprets into the appropriate `http::Response`.

// W35-P2: this module's `//!` docs were inherited verbatim from the
// absorbed `corelink-quota` crate. Many intra-doc refs (`[reservation]`,
// `[QuotaDecision]`, etc.) resolved at the former crate root; under the
// new umbrella crate they would require `crate::quota::core::` prefixes
// to keep working. Suppressing the lint here preserves the original
// reference text without churning every sentence; consumers reading the
// rendered docs still get the short form and can navigate via the
// `pub mod` / `pub use` lines visible below.
#![allow(rustdoc::broken_intra_doc_links)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `quota_reservations` (WI-S07-003; Lote 10.7bis R5 P0-2 size-proportional
/// reservation TTL durable mirror of the DO singleton in-memory map).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0009_QUOTA_RESERVATIONS: &str =
    include_str!("../../../../migrations/d1/0009_quota_reservations.sql");

pub mod audit;
pub mod check;
pub mod config;
pub mod error;
pub mod metrics;
pub mod reservation;
pub mod retry_after;

pub use audit::{
    canonical_audit_event_strings, FailingQuotaAuditSink, InMemoryQuotaAuditSink, QuotaAuditRecord,
    QuotaAuditSink, QuotaAuditSinkError, QuotaEventType,
};
pub use check::{InMemoryQuotaCheck, QuotaCheck, QuotaCheckOutcome, QuotaDecision, RequestKind};
pub use config::{
    QuotaConfig, DEFAULT_DENY_THRESHOLD_PCT, DEFAULT_RETRY_AFTER_FLOOR_SECS,
    DEFAULT_TRIGGER_THRESHOLD_PCT,
};
pub use error::QuotaError;
pub use metrics::{
    canonical_metric_names, FailingQuotaMetrics, InMemoryQuotaMetrics, QuotaCheckResultLabel,
    QuotaMetricKind, QuotaMetricsObserver, QuotaMetricsObserverError,
};
pub use reservation::{
    InMemoryReservationTracker, ReservationId, ReservationRow, ReservationTracker,
    ReservationTrackerError,
};
pub use retry_after::{provisional_retry_after_secs, RETRY_AFTER_HARD_CEILING_SECS};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `quota` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = schema 1,
/// `ac_meta` = schema 2, …, `tenant_storage_state` = schema 8,
/// **`quota_reservations` = schema 9**).
#[must_use]
pub const fn quota_schema_version() -> u32 {
    9
}

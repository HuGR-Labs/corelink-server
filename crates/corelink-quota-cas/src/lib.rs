//! `corelink-quota-cas` — Canonical quota checker (atomic CAS hard-block
//! at 100% boundary; canonical days-until-month-reset Retry-After per
//! ADR-0020 FROZEN) (WI-S08-003).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **canonical
//! pure-logic skeleton** of the S-08 CAP-QUOTA-001 hard-block path,
//! superseding the S-07 WI-S07-003 PROVISIONAL transitional 429 +
//! Retry-After arm (which remains intact as the closest-pattern
//! template; this crate ships the production-grade boundary).
//!
//! Specifically:
//!
//! 1. The canonical SQL artifact `migrations/d1/0012_quota_cas_attempts.sql`
//!    embedded via [`MIGRATION_0012_QUOTA_CAS_ATTEMPTS`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the in-memory CAS attempt
//!    audit ring; durable reload across worker restarts.
//! 2. The [`retry_after`] module ships [`days_until_month_reset_secs`],
//!    [`RETRY_AFTER_MIN_SECS`] (canonical floor when the request lands
//!    on the last second of the month), and the
//!    [`next_month_first_utc_midnight_secs`] primitive aligned with
//!    WI-S08-003 §1 invariant 8. The arithmetic is hand-rolled
//!    Gregorian (no `chrono` dep — wasm32-clean; deterministic at every
//!    boundary including December → January year increment, February
//!    non-leap / leap year, end-of-month spillover).
//! 3. The [`cas`] module ships [`QuotaCasDecision`] `#[non_exhaustive]`
//!    (`Allow` / `Deny429` carrying canonical
//!    `retry_after_secs: days-until-month-reset`) +
//!    [`QuotaCasOutcome`] (decision + duration probe + CAS attempt
//!    count for race-detection observability) + [`AtomicQuotaChecker`]
//!    trait + [`InMemoryAtomicQuotaChecker`] orchestrator wired to
//!    [`AtomicCasState`] + [`QuotaCasAuditSink`] +
//!    [`QuotaCasMetricsObserver`].
//! 4. The [`state`] module ships the canonical
//!    [`AtomicTenantBytesState`] (tenant_storage_state.bytes_used +
//!    bytes_quota + monotone CAS version + per-tenant `Mutex` envelope;
//!    F-001 closure preserved) + [`AtomicCasState`] trait +
//!    [`InMemoryAtomicCasState`] fake.
//! 5. The [`audit`] module ships [`QuotaCasEventType`]
//!    (`#[non_exhaustive]` 6-event taxonomy:
//!    `corelink.quota.cas_{check_passed, denied_429_hard_block,
//!    race_detected, commit_succeeded, release_idempotent,
//!    retry_after_emitted}`) + fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
//! 6. The [`metrics`] module ships [`QuotaCasMetricsObserver`] +
//!    canonical 5-metric ladder.
//! 7. The [`error`] module ships [`QuotaCasError`] `#[non_exhaustive]`.
//! 8. The [`config`] module ships [`QuotaCasConfig`] (hard-block
//!    boundary at 100%; max CAS retry attempts per request; F-001
//!    closure semantics).
//!
//! # Why CAS not D1 atomic
//!
//! D1 SQLite has **no native compare-and-swap** primitive — atomic
//! UPDATE-WHERE-condition exists but serialises at the SQLite WAL lock
//! level under contention (1k QPS concurrent writes from the same
//! tenant would suffer p99 50–200ms, unacceptable per WI §2). The DO
//! actor model (single-threaded actor per `Quota-<tenant_id>`)
//! delivers race-free + sub-ms latency with durable storage backing.
//! This crate's [`InMemoryAtomicCasState`] mirrors the actor semantic
//! byte-for-byte: a per-tenant `Mutex` serialises the
//! `read → check predicate → write` sequence so two concurrent
//! `try_acquire` calls observing the same `bytes_used` cannot both
//! pass the boundary check at 99.9%.
//!
//! # Race-aware strict-< predicate
//!
//! The boundary check is `bytes_used + request_bytes < bytes_quota`
//! (strict; absorbs S-06 INV-GC-004 + S-07 WI-S07-002 lessons): the
//! relaxed `<=` predicate permits the 2-concurrent-reservations-at-
//! exactly-equal-quota race where both observe `bytes_used + req == max`
//! and both pass; `<` rejects this configuration. Pinned by
//! `prop_cas_no_double_spend` and the boundary regression tests.
//!
//! # Canonical Retry-After: days-until-month-reset
//!
//! Per ADR-0020 FROZEN + sprint contract §10.s08.5 (calendar reset
//! determinism), the canonical Retry-After value is the seconds-until
//! the next 1st-UTC-midnight calendar boundary. The S-07 PROVISIONAL
//! formula `floor × max(1.0, used/quota)` is superseded here. The
//! [`days_until_month_reset_secs`] primitive handles the four
//! boundary cases the WI calls out:
//!
//! - **Jan 1 00:00:01** → next Feb 1 (~31 days).
//! - **Dec 31 23:59:59** → next Jan 1 (year increment; 1 second).
//! - **Feb 28 non-leap** → Mar 1 (1 day).
//! - **Feb 28/29 leap year** → Feb 29 / Mar 1 (1–2 days).
//!
//! Pinned by `prop_retry_after_days_until_month_reset` +
//! `retry_after_boundary_cases`.
//!
//! # CAS-version semantic
//!
//! Each [`AtomicTenantBytesState`] carries a monotone `cas_version`
//! counter. Every successful mutation (commit_reservation /
//! eviction_reclaim / period_reset) bumps the version. The CAS
//! `try_acquire` reads the current version, computes the predicate,
//! and writes back only if the version observed at write equals the
//! version observed at read; a mid-flight bump (e.g. concurrent
//! commit by an evictor) fires the [`QuotaCasError::CasRaceDetected`]
//! arm; the orchestrator retries up to `max_cas_attempts` (canonical
//! 3 per WI §1; bounded retry to prevent infinite loops on
//! pathological contention). Race-detection events emit the
//! `corelink.quota.cas_race_detected` audit (SEV-3 observability).
//!
//! # Trait-abstraction-defer
//!
//! Real CF DO singleton + real D1 atomic batch + Tower-layer
//! production wiring (gated `corelink-worker/tower-middleware`) +
//! 100k nightly proptest sustained 7d + chaos 12 + crypto SME
//! advisory (race correctness atomic CAS) — all consolidated
//! alongside WI-S08-006 PRR ship gate.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-QUOTA-ENFORCEMENT** (HIGH; spec_contract §8 +
//!   invariant_registry §3.11): atomic enforcement via CAS; race-aware
//!   strict-< predicate; pinned by `prop_cas_no_double_spend` +
//!   `prop_cas_race_detected_retry_succeeds`.
//! - **INV-AVAIL-ISOLATION** (HIGH; spec_contract §8 +
//!   invariant_registry §3.8): per-tenant Mutex; cross-tenant
//!   architecturally impossible. Pinned by `prop_tenant_isolation`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+): inherited via
//!   per-tenant state + tenant-id-leftmost CAS key.
//! - **Audit fail-closed** (Lote 10.6bis pattern + S-07 sprint-close
//!   P1-1 fix): every decision emits the canonical audit BEFORE state
//!   mutation; emit failure rolls back the orchestration. Pinned by
//!   `prop_audit_emit_per_decision_arm` +
//!   `audit_failure_aborts_acquire`.
//! - **Idempotent zero-byte check**: `request_bytes == 0` always
//!   returns Allow without state mutation. Pinned by
//!   `prop_idempotent_zero_byte_check`.
//! - **Retry-After canonical bounds**: result is in
//!   `[RETRY_AFTER_MIN_SECS, MAX_SECS_PER_MONTH]`; never zero (would
//!   create hot-spin retry loops); never larger than the calendar
//!   period. Pinned by `prop_retry_after_days_until_month_reset`.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code.
//! - **No `tokio`** in `src/` (wasm32-clean lib code).
//! - **No process-global `static LazyLock<Mutex<>>`** — F-001 closure
//!   preserved via per-instance `Arc<Mutex<HashMap<…>>>`.
//! - **No `chrono`** dep — month-reset arithmetic is hand-rolled
//!   Gregorian per WI §1 invariant 8 (canonical primitive named
//!   [`days_until_month_reset_secs`] + supporting helpers).
//!
//! # Composition with WI-S08-001 + WI-S08-002 (camadas 1 + 2)
//!
//! The 4-layer bulkhead PAT-RATE-LIMIT-001 composes:
//!
//! 1. **Camada 2 — `corelink-edge::EdgePolicy`** (pre-auth IP / CIDR
//!    blocklist; consulted FIRST; zero-cost adversarial drop).
//! 2. **Camada 1 — `corelink-ratelimit::RateLimiter`** (per-tenant +
//!    per-IP token bucket; consulted SECOND; aggregate enforcement).
//! 3. **Camada 3 — THIS crate `corelink-quota-cas::AtomicQuotaChecker`**
//!    (per-tenant storage hard-block; consulted THIRD; supersedes the
//!    S-07 PROVISIONAL 429 with canonical days-until-month-reset
//!    Retry-After).
//! 4. Camada 4 — global circuit breaker (forward; WI-S08-006 PRR
//!    ship gate).
//!
//! Production wiring at WI-S08-006 composes the four in this canonical
//! order. The S-07 [`corelink_quota`] crate's PROVISIONAL path stays
//! intact for the transitional window per ADR-0020 FROZEN; new
//! middleware mounts pin THIS crate's canonical path.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `quota_cas_attempts` (WI-S08-003; durable audit row of every CAS
/// attempt — successful / denied / race-detected — for SEV-3 race
/// detection observability + admin-plane forensics).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0012_QUOTA_CAS_ATTEMPTS: &str =
    include_str!("../../../migrations/d1/0012_quota_cas_attempts.sql");

pub mod audit;
pub mod cas;
pub mod config;
pub mod error;
pub mod metrics;
pub mod retry_after;
pub mod state;

pub use audit::{
    canonical_audit_event_strings, FailingQuotaCasAuditSink,
    InMemoryQuotaCasAuditSink, QuotaCasAuditRecord, QuotaCasAuditSink,
    QuotaCasAuditSinkError, QuotaCasEventType,
};
pub use cas::{
    AtomicQuotaChecker, InMemoryAtomicQuotaChecker, QuotaCasDecision,
    QuotaCasOutcome,
};
pub use config::{
    QuotaCasConfig, DEFAULT_HARD_BLOCK_PCT, DEFAULT_MAX_CAS_ATTEMPTS,
};
pub use error::QuotaCasError;
pub use metrics::{
    canonical_metric_names, FailingQuotaCasMetrics, InMemoryQuotaCasMetrics,
    QuotaCasMetricKind, QuotaCasMetricsObserver,
    QuotaCasMetricsObserverError, QuotaCasResultLabel,
};
pub use retry_after::{
    days_until_month_reset_secs, next_month_first_utc_midnight_secs,
    MAX_SECS_PER_MONTH, RETRY_AFTER_MIN_SECS,
};
pub use state::{
    AtomicCasState, AtomicCasStateError, AtomicTenantBytesState,
    InMemoryAtomicCasState,
};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `quota_cas` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = 1,
/// `ac_meta` = 2, …, `quota_reservations` = 9, `ratelimit_buckets` = 10,
/// `edge_blocklist` = 11, **`quota_cas_attempts` = 12**).
#[must_use]
pub const fn quota_cas_schema_version() -> u32 {
    12
}

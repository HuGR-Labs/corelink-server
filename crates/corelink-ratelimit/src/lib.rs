//! `corelink-ratelimit` — Rate-limit token-bucket engine
//! (WI-S08-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the rate-limiting plane: trait surfaces every
//! production Cloudflare DO singleton + Tower layer will satisfy, plus
//! in-memory orchestrators that exercise every load-bearing invariant
//! the production wiring relies on. Property tests pinned at 10 k iter
//! against the orchestrator cover INV-RATE-LIMIT-PROPORTIONALITY (HIGH;
//! `refill × window` consistent with tenant plan), INV-AVAIL-ISOLATION
//! (HIGH; per-tenant DO singleton; cross-tenant impossible by design),
//! INV-TENANT-ISOLATION (CRITICAL; tenant-leftmost PK; per-instance
//! Mutex F-001 closure), and the canonical RFC 6585 §4 + RFC 9331
//! Retry-After semantic (seconds; clamped to per-config floor + hard
//! ceiling).
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0010_ratelimit_buckets.sql`
//!    embedded via [`MIGRATION_0010_RATELIMIT_BUCKETS`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the per-instance HashMap
//!    `(tenant_id, key_dimension, scope_key) → TokenBucketState` shape;
//!    durable reload across worker restarts.
//! 2. The [`key`] module ships [`KeyDimension`] (`#[non_exhaustive]`
//!    canonical 3-literal: `per_tenant` / `per_ip` /
//!    `per_tenant_per_endpoint`) + [`BucketKey`] (composite key with
//!    tenant-leftmost ordering) + the canonical 3-dimension list
//!    [`KEY_DIMENSION_LIST`] for cross-component regression tests.
//! 3. The [`tier`] module ships [`refill_rate_for_tier`] resolver
//!    against the canonical 5-tier ladder (free/solo/team/business/
//!    enterprise) per WI-S08-001 §6.1.4 + Lote 10.7bis P0-7 (5-tier
//!    vocabulary FROZEN at the data model layer).
//! 4. The [`bucket`] module ships [`TokenBucketState`] (the durable
//!    state machine; `available_tokens: f64` for sub-1-token refill
//!    precision per WI §1 invariant 5) + [`BucketDecision`]
//!    (`#[non_exhaustive]` Allow / Deny429) + [`bucket::try_acquire`] (the
//!    canonical lazy-refill formula; monotonic clock clamp;
//!    canceled-tenant guard per Lote 10.8bis P1-1).
//! 5. The [`audit`] module ships [`RateLimitEventType`]
//!    (`#[non_exhaustive]` 3-event taxonomy: `corelink.ratelimit.{allowed,
//!    denied_429, bucket_refilled}`) + [`RateLimitAuditRecord`] +
//!    [`RateLimitAuditSink`] + [`InMemoryRateLimitAuditSink`] capture
//!    sink (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 6. The [`metrics`] module ships [`RateLimitMetricsObserver`] +
//!    [`InMemoryRateLimitMetrics`] capture sink (7 canonical metrics:
//!    `corelink.ratelimit.{check_total, tokens_remaining, refill_rate,
//!    plan_sync_lag_ms, do_cold_start_total, middleware_duration_us,
//!    cross_tenant_violation_total}`).
//! 7. The [`error`] module ships the canonical [`RateLimitError`]
//!    `#[non_exhaustive]` taxonomy.
//! 8. The [`config`] module ships [`RateLimitConfig`] (knobs pinned to
//!    canonical defaults: team-tier refill + burst, RFC 6585 §4
//!    Retry-After floor 1s, hard ceiling 1d, canceled-tenant 7d) +
//!    per-instance F-001 closure semantics.
//! 9. The [`limiter`] module ships [`RateLimitDecision`]
//!    `#[non_exhaustive]` (Allow / Deny429) + [`RateLimiter`] trait +
//!    [`InMemoryTokenBucketRateLimiter`] orchestrator wired to the
//!    dependencies above.
//!
//! # Why rate-limit is `trait + fake` here, real DO/D1 in WI-S08-006
//!
//! S-08 lands without Cloudflare DO bindings wired into CI (no remote +
//! Cloudflare Workers + D1 staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! FM-401 thundering-herd race elimination via per-instance Mutex
//! serialisation (mirrors DO actor model); monotonic clock clamp;
//! canceled-tenant guard; RFC 6585 §4 + RFC 9331 Retry-After clamping;
//! tenant-leftmost isolation; cost-overflow guard. The live-DO + D1
//! conformance tests run alongside WI-S08-006 (PRR ship gate) once
//! miniflare/wrangler-dev integration tests land.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-RATE-LIMIT-PROPORTIONALITY** (HIGH; spec_contract §8 +
//!   invariant_registry §3.12): `refill_rate × window` consistent with
//!   tenant Plan tier; mudança plan reflete via `update_plan`.
//!   Pinned by `prop_token_bucket_proportionality` +
//!   `prop_token_bucket_never_exceeds_capacity`.
//! - **INV-AVAIL-ISOLATION** (HIGH; spec_contract §8 +
//!   invariant_registry §3.8): per-tenant bucket key; cross-tenant
//!   state contamination architecturally impossible (the
//!   `BucketKey.tenant_id` is the leftmost field; the per-instance
//!   Mutex serialises). Pinned by `prop_tenant_isolation` +
//!   `prop_concurrent_acquire_does_not_double_spend`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+): inherited via the
//!   `BucketKey` shape + the orchestrator's tenant-mismatch guard.
//!   Pinned by the `tenant_mismatch_returns_typed_error_and_bumps_canary`
//!   regression.
//! - **Token-bucket non-negativity**: `available_tokens` clamp
//!   `[0.0, burst_capacity as f64]`. Pinned by
//!   `prop_token_bucket_never_negative`.
//! - **Refill monotonicity in time**: later timestamps produce >= tokens
//!   for the same starting state. Pinned by
//!   `prop_refill_monotonic_in_time`.
//! - **Retry-After lower bound**: the canonical RFC 6585 §4 minimum
//!   wait until capacity available. Pinned by
//!   `prop_retry_after_lower_bound`.
//! - **Audit fail-closed** (Lote 10.6bis pattern): every decision
//!   emits an audit BEFORE state mutation. Pinned by
//!   `prop_audit_emit_per_decision_arm` +
//!   `audit_failure_aborts_decision_and_bucket_unchanged`.
//! - **< 3ms p99 hot path** (spec_contract §10.s08.2): in-memory
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
//! - **No process-global `static LazyLock<Mutex<>>`** — F-001 closure
//!   preserved via per-instance `Arc<Mutex<HashMap<…>>>`.
//!
//! # Scope boundary vs S-07 PROVISIONAL 429 + WI-S08-002 / S08-003 / S08-005
//!
//! WI-S07-003 ships the **PROVISIONAL transitional** 429 + Retry-After
//! arm at the storage-quota boundary (per ADR-0020 FROZEN). This crate
//! ships the canonical token-bucket engine that powers:
//!
//! - **Camada 1 (per-tenant; THIS WI)** — `BucketKey::per_tenant`.
//! - **Camada 2 (per-IP; WI-S08-002 hand-off)** — `BucketKey::per_ip`.
//!   The DO + edge wiring lives in WI-S08-002 (CF Ruleset Engine
//!   bindings).
//! - **Camada 3 (per-endpoint isolation)** —
//!   `BucketKey::per_tenant_per_endpoint`. The Tower-layer wiring
//!   lives in WI-S08-005 (RFC 9331 headers + global circuit breaker).
//!
//! The middleware-level response shape (status code, header presence,
//! body taxonomy) is established here; the canonical `Retry-After:
//! days-until-month-reset` semantic for storage quota (per ADR-0020
//! FROZEN) is a follow-on at WI-S08-003 (Quota checker middleware).
//!
//! # Wiring into the Tower middleware (forward; WI-S08-005 ship gate)
//!
//! The production wiring composes [`RateLimiter::try_acquire`] on top
//! of the existing `AuthLayer` (WI-S03-003 SEALED; provides `AuthCtx`
//! extracted tenant_id) and the 5-Layer Defense from S-04. The Tower
//! layer construction itself (gated by feature
//! `corelink-worker/tower-middleware`) lives outside this crate to
//! preserve wasm32-clean compilation; this crate's
//! [`InMemoryTokenBucketRateLimiter`] returns a [`RateLimitDecision`]
//! enum that the Tower layer interprets into the appropriate
//! `http::Response` (RFC 9331 `RateLimit` + `RateLimit-Policy` headers
//! land in WI-S08-005).

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `ratelimit_buckets` (WI-S08-001; per-tenant token-bucket durable
/// mirror of the DO singleton in-memory state machine).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0010_RATELIMIT_BUCKETS: &str =
    include_str!("../../../migrations/d1/0010_ratelimit_buckets.sql");

pub mod audit;
pub mod bucket;
pub mod config;
pub mod error;
pub mod key;
pub mod limiter;
pub mod metrics;
pub mod tier;

pub use audit::{
    canonical_audit_event_strings, FailingRateLimitAuditSink,
    InMemoryRateLimitAuditSink, RateLimitAuditRecord, RateLimitAuditSink,
    RateLimitAuditSinkError, RateLimitEventType,
};
pub use bucket::{try_acquire as bucket_try_acquire, BucketDecision, TokenBucketState};
pub use config::{
    RateLimitConfig, DEFAULT_BURST_CAPACITY, DEFAULT_REFILL_RATE_PER_SEC,
    DEFAULT_RETRY_AFTER_FLOOR_SECS, RETRY_AFTER_CANCELED_TENANT_SECS,
    RETRY_AFTER_HARD_CEILING_SECS,
};
pub use error::RateLimitError;
pub use key::{BucketKey, KeyDimension, KEY_DIMENSION_LIST};
pub use limiter::{
    InMemoryTokenBucketRateLimiter, RateLimitDecision, RateLimitOutcome,
    RateLimiter,
};
pub use metrics::{
    canonical_metric_names, FailingRateLimitMetrics, InMemoryRateLimitMetrics,
    RateLimitMetricKind, RateLimitMetricsObserver,
    RateLimitMetricsObserverError, RateLimitResultLabel,
};
pub use tier::{
    refill_rate_for_tier, BUSINESS_BURST, BUSINESS_REFILL_RPS, ENTERPRISE_BURST,
    ENTERPRISE_REFILL_RPS, FREE_BURST, FREE_REFILL_RPS, SOLO_BURST,
    SOLO_REFILL_RPS, TEAM_BURST, TEAM_REFILL_RPS, TIER_RATE_LADDER,
};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `ratelimit` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = schema 1,
/// `ac_meta` = schema 2, …, `quota_reservations` = schema 9,
/// **`ratelimit_buckets` = schema 10**).
#[must_use]
pub const fn ratelimit_schema_version() -> u32 {
    10
}

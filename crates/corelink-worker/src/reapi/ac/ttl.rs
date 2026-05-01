//! TTL infrastructure for the Action Cache (WI-S04-005).
//!
//! This module ships the **infrastructure layer** of the AC TTL flow:
//!
//! 1. [`resolver::TierTtlResolver`] — boundary trait between S-04
//!    infrastructure and S-07 per-tier defaults (ADR-0019). The S-04
//!    GA fallback ([`resolver::EnvConfigTierTtlResolver`]) reads a
//!    single global TTL from env config; S-07 swaps in
//!    `S07PerTierTtlResolver` keyed off `tenant_quota.tier`.
//! 2. [`refresh::refresh_if_needed`] — pure-logic threshold gate the
//!    GET handler consults before issuing the `last_hit_at + ttl`
//!    write to D1. Reduces UPDATE storm under high-rate workloads
//!    (without the gate, 1k req/s on a hot digest = 1k D1 UPDATE/s ⇒
//!    lock contention; with the canonical 60 s gate, ≤ 1 UPDATE/min
//!    per digest).
//! 3. [`evict::EvictBatch`] — tenant-scoped batched eviction logic
//!    that drains expired rows in the canonical R2-first → D1 → KV →
//!    audit order per WI §6.1. Bounded at [`evict::MAX_BATCH_SIZE`] =
//!    250 rows per tick (WI-S04-005 Lote 10.4bis P0 fix; D1 100 KB
//!    batch ceiling).
//! 4. [`worker::TtlWorker`] — the per-region cron orchestrator trait
//!    surface. The in-memory fake [`worker::InMemoryTtlWorker`]
//!    exercises the same algorithmic invariants the production
//!    Cloudflare Cron Durable Object will (the real DO binding shim
//!    is deferred to WI-S04-006 alongside the REAPI conformance suite
//!    plus miniflare/wrangler-dev integration; per-charter
//!    trait-abstraction-defer pattern).
//!
//! ## Five canonical invariants (per WI §12)
//!
//! - **`INV-AC-TTL-MONOTONIC`** — `refresh_on_hit` increments
//!   `last_hit_at` + extends `expires_at`; never decreases. Pinned at
//!   the [`super::meta::AcMetaStore::refresh_on_hit`] boundary +
//!   property-tested at 10k iter.
//! - **`INV-AC-EVICT-TENANT-SCOPED`** — every DELETE goes through
//!   [`super::meta::AcMetaStore::delete_tenant_scoped`] which takes
//!   `tenant_id` + `action_digest` + `region` by value; the trait
//!   surface intentionally has no "DELETE WHERE expires_at < ?" bulk
//!   method. Cross-tenant DELETE is structurally unreachable.
//! - **`INV-AC-EVICT-CONSISTENCY`** — R2 envelope DELETE precedes D1
//!   DELETE (WI §9.3); on R2 failure D1 is preserved (orphan R2
//!   recoverable via S-06 reconcile); on D1 failure R2 is gone but
//!   the next cron tick re-attempts (idempotent — `expires_at < now`
//!   still selects the row).
//! - **`INV-AC-EVICT-REGION-PINNED`** — every cron worker is pinned
//!   to a single region; per-region shards never touch other regions'
//!   rows (`region` is mandatory in `select_expired_for_region` +
//!   `delete_tenant_scoped`). Cross-region pollution is structurally
//!   impossible.
//! - **`INV-AC-EVICT-AUDIT-EMITTED`** — every successful row
//!   eviction emits a typed audit record via the [`super::audit::AuditSink`]
//!   trait (`AcEventType::EvictTtlExpired`). Audit emission failure
//!   surfaces as a per-row error; the row stays alive and the next
//!   tick retries.
//!
//! ## S-07 boundary alignment (ADR-0019)
//!
//! S-04 owns the **infrastructure** (cron worker + refresh-on-hit +
//! tenant-scoped expiry); S-07 owns the **per-tier defaults**
//! (`free=7d`, `solo=30d`, `team=90d`, `business=365d`,
//! `enterprise=customer-configurable`). The boundary is the
//! [`resolver::TierTtlResolver`] trait — S-04 GA wires
//! [`resolver::EnvConfigTierTtlResolver`] as a single global default;
//! S-07 SEALED swaps in `S07PerTierTtlResolver` without re-deploying
//! the cron worker.
//!
//! ## S-07 LRU vs S-04 TTL composition
//!
//! The S-07 eviction worker (WI-S07-002) is **size-based** (LRU per
//! `last_accessed_at` + per-tenant quota threshold); this S-04 cron
//! worker is **time-based** (`expires_at < now`). Both honor
//! tenant-scoped DELETE; both run as per-region shards; neither emits
//! a DELETE that the other does not also emit (idempotent: a row
//! evicted by either is the same `(tenant_id, action_digest)` row in
//! `ac_meta`). No double-eviction risk: the second worker to look at
//! the row sees `Ok(false)` from `delete_tenant_scoped` (already gone)
//! and treats it as a no-op. No skew risk: both workers share the
//! same `expires_at` column semantics — `refresh_on_hit` extends
//! `expires_at`, S-07 LRU never reads `expires_at`, S-04 cron never
//! reads `last_accessed_at`.

#![allow(
    clippy::module_name_repetitions,
    reason = "module path `ac::ttl::ttl_worker` would be redundant; canonical names mirror the WI §6.1 module layout (resolver / refresh / evict / worker)"
)]

pub mod evict;
pub mod refresh;
pub mod resolver;
pub mod worker;

pub use evict::{EvictBatch, EvictBatchOutcome, EvictError, EvictRowOutcome, MAX_BATCH_SIZE};
pub use refresh::{refresh_if_needed, DEFAULT_REFRESH_THRESHOLD_MS};
pub use resolver::{
    EnvConfigTierTtlResolver, MockTierTtlResolver, TenantTier, TierTtlResolver,
    DEFAULT_TIER_TTL_MS,
};
pub use worker::{InMemoryTtlWorker, TtlWorker, TtlWorkerError, TtlWorkerTickOutcome};

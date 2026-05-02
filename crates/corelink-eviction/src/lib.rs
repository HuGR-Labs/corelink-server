//! `corelink-eviction` — LRU + per-tier TTL + 95% quota-trigger
//! eviction worker (WI-S07-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the eviction plane: trait surfaces every production
//! Cloudflare Cron Durable Object binding will satisfy, plus in-memory
//! fakes that exercise every load-bearing invariant the production
//! wiring relies on. Property tests pinned at 10 k iter against the
//! fakes cover INV-GC-001 inheritance via soft-delete + grace,
//! BLOB-only scope (Lote 10.7bis P0-8 — chunks owned by S-06 GC),
//! tenant-scoped strict isolation, idempotent re-run, the race-aware
//! reachable check off-by-one boundary (`a.created_at <
//! evict_started_at_ms` strict-`<` evict / protect-if-`>=` mirror of
//! S-06 INV-GC-004), the size-proportional reservation TTL formula
//! (Lote 10.7bis R5 P0-2), and the 95% quota-trigger boundary.
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0008_tenant_storage_state.sql`
//!    embedded via [`MIGRATION_0008_TENANT_STORAGE_STATE`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema separates STATE (mutable
//!    `bytes_used` running counter) from POLICY (`tenant_quota.max_storage_bytes`
//!    immutable per period) per Lote 10.7bis P0-2 R4 option B.
//! 2. The [`tier`] module ships [`Tier`] (canonical 5-tier domain) +
//!    [`ttl_for_tier`] resolver (free=7d, solo=30d, team=90d,
//!    business=365d, enterprise=365d default capped 730d via admin
//!    override per ADR-0019; Lote 10.7-tris cycle 3 fix) +
//!    `MAX_ENTERPRISE_TTL_DAYS` hard cap.
//! 3. The [`reservation`] module ships [`reservation_ttl_ms`] — the
//!    canonical size-proportional reservation TTL formula
//!    `max(60s, request_bytes/1MB/s × 2x), capped 7d` (Lote 10.7bis
//!    R5 P0-2 fix; multipart 160 GiB no longer over-quota mid-upload).
//! 4. The [`storage_state`] module ships [`TenantStorageStateRow`] +
//!    [`TenantStorageStateStore`] trait + [`InMemoryTenantStorageStateStore`]
//!    fake whose semantics mirror the SQL `tenant_storage_state` table
//!    byte-for-byte (`(tenant_id, region)` PK, monotone watermarks,
//!    NOT NULL invariants, region domain).
//! 5. The [`reachable`] module ships [`AcReferenceProbe`] trait +
//!    [`InMemoryAcReferenceProbe`] fake implementing the canonical
//!    race-aware reachable check `SELECT COUNT(*) FROM ac_meta a,
//!    json_each(a.blob_refs) j WHERE a.tenant_id = $1 AND j.value = $2
//!    AND a.deleted_at IS NULL AND a.created_at < $3 (evict_started_at_ms)`.
//!    The strict `<` evict-arm + `>=` protect-arm mirrors the S-06
//!    INV-GC-004 protect-if-equal-or-newer canonical TLA semantics
//!    (`gc_correctness.tla` L152-154).
//! 6. The [`blob_meta`] module ships [`EvictionBlobDigest`] +
//!    [`BlobLruRow`] + [`BlobMetaSoftDeleteStore`] trait +
//!    [`InMemoryBlobMetaSoftDeleteStore`] fake whose
//!    `soft_delete_for_eviction` semantic mirrors the canonical
//!    SQL `UPDATE blob_meta SET deleted_at = ? WHERE tenant_id = ?
//!    AND digest = ? AND deleted_at IS NULL` envelope. NEVER R2 DELETE
//!    direct (INV-EVICT-SOFT-DELETE-FIRST per spec contract §8); S-06
//!    GC physical-delete (WI-S06-004) handles the R2 cleanup post-grace.
//! 7. The [`audit`] module ships [`EvictionEventType`] +
//!    [`EvictionAuditRecord`] + [`EvictionAuditSink`] +
//!    [`InMemoryEvictionAuditSink`] capture sink (fail-closed envelope
//!    per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The canonical 5-event
//!    taxonomy is `corelink.evict.{evicted,skipped_reachable,skipped_ttl,
//!    skipped_quota_ok,quota_trigger_fired}`.
//! 8. The [`metrics`] module ships [`EvictionMetricsObserver`] +
//!    [`InMemoryEvictionMetrics`] capture sink (canonical 9 metrics
//!    per WI §6.1.10).
//! 9. The [`error`] module ships the canonical [`EvictionError`]
//!    `#[non_exhaustive]` taxonomy.
//! 10. The [`phase`] module ships [`EvictionConfig`] (knobs pinned to
//!     canonical per-tier TTLs + 95%/90% quota thresholds) +
//!     [`EvictionDecision`] `#[non_exhaustive]` (`Evict /
//!     SkipReachable / SkipTtlNotExpired / SkipQuotaOk`) +
//!     [`EvictionResult`] aggregate counters + [`EvictionPhase`]
//!     trait + [`InMemoryEvictionPhase`] orchestrator wired to the
//!     dependencies above + [`EvictionClock`] seam +
//!     [`CountingEvictionClock`] fake.
//! 11. The [`trigger`] module ships [`should_fire_quota_trigger`]
//!     (95% boundary check) + [`target_bytes_to_reclaim`] (reclaim
//!     until 90% headroom) + [`spawn_quota_trigger_in_memory`]
//!     simulator that mirrors the production `worker::send_future()`
//!     fire-and-forget envelope (Lote 10.7bis R5 P0-3 — async-spawn
//!     does NOT block hot-path write).
//!
//! # Why eviction is `trait + fake` here, real D1/Cron in WI-S07-005
//!
//! S-07 lands without Cloudflare Cron DO bindings wired into CI (no
//! remote + Cloudflare Workers + D1 staging are HARD inflection points
//! per `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! INV-GC-001 inheritance violation via cascade leak; race-aware
//! reachable check off-by-one (data loss bug); BLOB-vs-chunk scope
//! confusion (chunks owned by S-06 GC); TTL boundary off-by-one;
//! quota-trigger 95% boundary; size-proportional reservation TTL formula.
//! The live-D1 + Cron DO conformance tests run alongside WI-S07-005
//! (DASH-DEDUP + alerts) once miniflare/wrangler-dev integration tests
//! land.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-GC-001 inheritance** (CRITICAL): eviction NEVER deletes
//!   reachable blob. The race-aware reachable check uses the canonical
//!   `>=` protect arm + `<` evict arm (strict less-than for evict is
//!   the contrapositive of S-06's protect-if-equal-or-newer per
//!   `gc_correctness.tla` L152-154). Off-by-one (`<=` instead of `<`)
//!   would delete a blob just-re-referenced during the eviction phase —
//!   data loss. Pinned by `prop_evict_protect_if_re_referenced_strict_boundary`.
//! - **BLOB-only scope** (Lote 10.7bis P0-8): eviction iterates
//!   `blob_meta` (BlobLruRow); chunks (with their refcount lifecycle)
//!   are owned by S-06 GC sweep + reconcile. Eviction NEVER touches
//!   the `chunks` table. Pinned by `prop_blob_only_scope`.
//! - **Tenant-scoped strict** (CTRL-ISO-005 + INV-TENANT-ISOLATION):
//!   every storage-state lookup + soft-delete + reachable probe is
//!   keyed leftmost on `tenant_id`. Pinned by `prop_tenant_isolation`.
//! - **Soft-delete-first** (INV-EVICT-SOFT-DELETE-FIRST): eviction
//!   sets `blob_meta.deleted_at = now()`; physical R2 delete is
//!   delegated to S-06 WI-S06-004 cron post-grace 72h. Pinned by
//!   the in-memory fake's `soft_delete_for_eviction` envelope.
//! - **TTL hard cap** (INV-EVICT-TTL-CAP-RESPECTED): enterprise TTL
//!   override > 730d rejected by `ttl_for_tier_with_override` per
//!   ADR-0019.
//! - **Size-proportional reservation TTL** (INV-QUOTA-RESERVATION-TTL):
//!   `max(60s, req_bytes/1MB/s × 2x), capped 7d` per Lote 10.7bis R5
//!   P0-2; multipart 160 GiB no longer over-quota mid-upload. Pinned
//!   by `prop_ttl_size_proportional_reservation`.
//! - **95% quota trigger** (CAP-EVICT-003): exact boundary at
//!   `bytes_used / bytes_quota >= 0.95`. Pinned by
//!   `prop_quota_trigger_fires_at_95pct`.
//! - **Audit fail-closed** (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER): a
//!   failed audit emit aborts the per-candidate eviction (no
//!   soft-delete persists on the in-memory fake; production wiring
//!   rolls back the D1 batch).
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The fake is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the eviction-relevant columns and reports
//!   structured [`EvictionError`] errors when an invariant fires.
//!
//! # Scope boundary vs S-06 GC + S-07 quota middleware
//!
//! - **S-06 GC**: owns `chunks` lifecycle (refcount + sweep + reconcile)
//!   AND the canonical `mark_started_at_ms` anchor + INV-GC-004
//!   protect-if-`>=`. Eviction (this crate) inherits the soft-delete
//!   pattern + grace 72h via S-06 GC `physical_delete` (WI-S06-004).
//! - **WI-S07-003 quota middleware**: owns the DO-singleton
//!   `bytes_used` authoritative counter + reservation lifecycle. This
//!   crate consumes the resulting `tenant_storage_state.bytes_used`
//!   row to decide the 95% trigger arm; the trigger fires the eviction
//!   pass via `worker::send_future()` (Lote 10.7bis R5 P0-3
//!   fire-and-forget; NOT `tokio::spawn`).
//! - **WI-S07-005 PRR ship gate**: live D1 + Cron DO conformance
//!   suite + DASH-DEDUP dashboard + alerts; this crate's fakes ship
//!   the algorithmic invariants only.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `tenant_storage_state` (WI-S07-002 + WI-S07-003 dependency; Lote
/// 10.7bis P0-2 NEW table separating STATE from POLICY).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0008_TENANT_STORAGE_STATE: &str =
    include_str!("../../../migrations/d1/0008_tenant_storage_state.sql");

pub mod audit;
pub mod blob_meta;
pub mod error;
pub mod metrics;
pub mod phase;
pub mod reachable;
pub mod region;
pub mod reservation;
pub mod storage_state;
pub mod tier;
pub mod trigger;

pub use audit::{
    canonical_audit_event_strings, EvictionAuditRecord, EvictionAuditSink,
    EvictionAuditSinkError, EvictionEventType, FailingEvictionAuditSink,
    InMemoryEvictionAuditSink,
};
pub use blob_meta::{
    BlobLruRow, BlobMetaError, BlobMetaSoftDeleteStore, EvictionBlobDigest,
    InMemoryBlobMetaSoftDeleteStore, LruUpdateOutcome, SoftDeleteOutcome,
};
pub use error::EvictionError;
pub use metrics::{
    canonical_metric_names, EvictionMetricKind, EvictionMetricsObserver,
    EvictionMetricsObserverError, FailingEvictionMetrics, InMemoryEvictionMetrics,
};
pub use phase::{
    CountingEvictionClock, EvictionClock, EvictionConfig, EvictionDecision,
    EvictionPhase, EvictionResult, InMemoryEvictionPhase, EVICTION_COOLDOWN_MS,
    QUOTA_TARGET_HEADROOM_PCT, QUOTA_TRIGGER_THRESHOLD_PCT,
};
pub use reachable::{
    AcReferenceProbe, AcReferenceWitness, InMemoryAcReferenceProbe,
};
pub use region::{EvictionRegion, UnknownRegion, REGION_LIST};
pub use reservation::{
    reservation_ttl_ms, MAX_RESERVATION_TTL_MS, MIN_RESERVATION_TTL_MS,
    RESERVATION_PROPORTIONAL_MULTIPLIER, RESERVATION_THROUGHPUT_BYTES_PER_MS,
};
pub use storage_state::{
    InMemoryTenantStorageStateStore, StorageStateError, TenantStorageStateRow,
    TenantStorageStateStore,
};
pub use tier::{
    ttl_for_tier, ttl_for_tier_with_override, Tier, TierTtlOverrideError,
    BUSINESS_TTL_DAYS, ENTERPRISE_TTL_DAYS_DEFAULT, FREE_TTL_DAYS, MAX_ENTERPRISE_TTL_DAYS,
    SOLO_TTL_DAYS, TEAM_TTL_DAYS,
};
pub use trigger::{
    should_fire_quota_trigger, spawn_quota_trigger_in_memory, target_bytes_to_reclaim,
    InMemorySpawnHandle, QuotaTriggerOutcome,
};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `eviction` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = schema 1,
/// `ac_meta` = schema 2, `multipart_chunks_manifest` = schema 3, …,
/// `gc_run` = schema 6, `gc_candidates` = schema 7, **`tenant_storage_state`
/// = schema 8**).
#[must_use]
pub const fn eviction_schema_version() -> u32 {
    8
}

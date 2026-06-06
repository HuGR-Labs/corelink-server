//! Revocation lifecycle orchestrator (WI-S03-004).
//!
//! This module is the canonical Rust surface that implements the
//! revocation flow described in `auth_model.md §5` and detailed in
//! WI-S03-004:
//!
//! 1. **Neon `pat` UPDATE** — set `revoked_at = now()` in the same
//!    Postgres transaction as the audit_outbox INSERT (canonical SoT
//!    per `data_model.md §4.1`; INV-AUTH-NEON-IS-SOT,
//!    INV-AUTH-MASS-REVOKE-ATOMIC).
//! 2. **DO storage upsert** — local revocation list (broadcast cache,
//!    not SoT). Idempotent under retry via `(pat_id, revoked_at)`
//!    UNIQUE semantics.
//! 3. **KV session-cache invalidate** — best-effort hot-path
//!    optimisation (the cold path always re-checks Neon).
//! 4. **Queue broadcast** — at-least-once propagation to peer
//!    regions (INV-AUTH-PROPAGATION-AT-LEAST-ONCE) within the
//!    SLO-FRESH-PAT-REVOKE ≤ 60 s p99 single SLA bound.
//!
//! ## Trait abstraction defer pattern
//!
//! The same pattern that S-01-003 / S-02-005 used for R2 / KV is
//! reused here: the orchestrator depends on **traits** that the
//! production Cloudflare bindings will implement, while host-side
//! tests drive `InMemory*` fakes that preserve the documented
//! semantics. The four traits are:
//!
//! - [`RevocationStore`] — Durable-Object-equivalent persistent map
//!   from [`corelink_pat::PatId`] to [`RevokedEntry`]. Idempotent upsert; range
//!   query for the reconciliation Cron.
//! - [`MetaRevocationSink`] — Neon transactional writer that updates
//!   `pat.revoked_at` and inserts an `audit_outbox` row in the same
//!   batch (so the two are all-or-none). Owns the canonical SoT
//!   update.
//! - [`SessionCacheInvalidator`] — KV-namespace adapter that drops
//!   the cached `(token_hash) → AuthCtx` entry. Best-effort: a
//!   transient KV outage does NOT abort the revoke (the cold path
//!   recovers via Neon).
//! - [`RevocationBroadcast`] — Queue producer that fans out
//!   [`RevokedEntry`] batches to peer regions. At-least-once with
//!   [`PropagationOutcome::DlqFallback`] when the producer is
//!   itself unavailable.
//!
//! ## Idempotency contract
//!
//! Per **INV-AUTH-REVOCATION-IDEMPOTENT**, replays of the same
//! `(pat_id, revoked_at)` MUST yield the same observable state and
//! emit no duplicate audit events. The orchestrator implements this
//! via a single discriminator: when [`MetaRevocationSink::revoke`]
//! returns [`MetaRevokeOutcome::AlreadyRevoked`], the orchestrator
//! short-circuits — the DO upsert, KV invalidate, and queue
//! broadcast are all skipped (they were performed on the original
//! call) and the existing `revoked_at` timestamp is returned. The
//! Postgres `UPDATE … WHERE revoked_at IS NULL` with `RETURNING`
//! pattern is the canonical implementation seam (see
//! [`MetaRevocationSink`] doc).
//!
//! ## Mass revoke two-phase contract
//!
//! Per **INV-AUTH-MASS-REVOKE-ATOMIC** the mass revoke flow has
//! two phases:
//!
//! 1. **Phase 1 — atomic UPDATE**: a single Postgres transaction
//!    sets `revoked_at = now()` for every active row matching the
//!    tenant filter. All-or-none. Returns the affected `Vec<PatId>`.
//! 2. **Phase 2 — chunked outbox + broadcast**: the orchestrator
//!    iterates the affected ids in chunks of
//!    [`MASS_REVOKE_OUTBOX_CHUNK_SIZE`] = 1000 rows, inserting
//!    audit_outbox events and enqueuing broadcast batches of
//!    [`MASS_REVOKE_BROADCAST_BATCH_SIZE`] = 100 entries per CF
//!    Queue message. Eventual-completeness ≤ 5 min via at-least-once
//!    semantics.
//!
//! ## Single SLA stale-window axes (P0 fix Lote 10.3bis)
//!
//! The revocation orchestrator does NOT pin a global propagation
//! deadline; it surfaces the propagation outcome to the caller and
//! the SRE-side metric pipeline asserts the
//! SLO-FRESH-PAT-REVOKE ≤ 60 s p99 invariant. The single-SLA story
//! (hot-path TTL = 60 s, cold-path D1 ≤ 100 ms, cross-region
//! propagation ≤ 60 s — orthogonal axes, NOT additive) is documented
//! in WI-S03-004 §3 SLA addendum + §9.8.
//!
//! ## Internal layout (wave-33 stage 2.PRE-A.1)
//!
//! The original 2024-LOC monolith is decomposed into five private
//! submodules + a tests module, re-exported through this parent
//! file so every `auth::revocation::*` path consumer keeps resolving
//! at parity:
//!
//! - [`types`] — constants, `RevocationReason`, `RevokedEntry`,
//!   request/response payloads, `RevocationError`.
//! - [`traits`] — `RevocationStore`, `MetaRevocationSink`,
//!   `SessionCacheInvalidator` + `KvSessionCacheInvalidator`,
//!   `RevocationBroadcast`.
//! - [`in_memory_store`] — `InMemoryRevocationStore` fake.
//! - [`in_memory_meta`] — `InMemoryMetaRevocationSink` fake +
//!   `TestClock` + `MonotonicTestClock` + `TestAuditRow`.
//! - [`in_memory_broadcast`] — `InMemoryBroadcast` fake.
//! - [`orchestrator`] — `RevocationOrchestrator` +
//!   `MassRevokeResponse` + `IngestOutcome` +
//!   `ReconciliationSummary` + `DriftRow`.

pub mod in_memory_broadcast;
pub mod in_memory_meta;
pub mod in_memory_store;
pub mod orchestrator;
pub mod traits;
pub mod types;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split `auth::revocation::*`
// public surface verbatim.
// ---------------------------------------------------------------------------

pub use in_memory_broadcast::InMemoryBroadcast;
pub use in_memory_meta::{InMemoryMetaRevocationSink, MonotonicTestClock, TestAuditRow, TestClock};
pub use in_memory_store::InMemoryRevocationStore;
pub use orchestrator::{
    DriftRow, IngestOutcome, MassRevokeResponse, ReconciliationSummary, RevocationOrchestrator,
};
pub use traits::{
    KvSessionCacheInvalidator, MetaRevocationSink, RevocationBroadcast, RevocationStore,
    SessionCacheInvalidator,
};
pub use types::{
    HookOutcome, MassRevokeId, MassRevokeRow, MetaMassRevokeOutcome, MetaRevokeOutcome,
    PropagationOutcome, PropagationStatus, RevocationDedupKey, RevocationError, RevocationReason,
    RevokeRequest, RevokeResponse, RevokedEntry, SessionCacheKey, MASS_REVOKE_BROADCAST_BATCH_SIZE,
    MASS_REVOKE_OUTBOX_CHUNK_SIZE, MASS_REVOKE_RATE_PER_TENANT_PER_SEC, SESSION_CACHE_KEY_PREFIX,
    SESSION_CACHE_TOMB_TTL_SECS,
};

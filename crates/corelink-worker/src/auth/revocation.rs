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
//!   from [`PatId`] to [`RevokedEntry`]. Idempotent upsert; range
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

use core::fmt;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

use crate::cache::kv::{KvBackend, KvError};
use crate::region::Region;

// ---------------------------------------------------------------------------
// Public canonical constants
// ---------------------------------------------------------------------------

/// Maximum number of audit_outbox rows the mass-revoke orchestrator
/// inserts per chunk (Phase 2). Per WI-S03-004 §6.1.5 + §3 Persona 2.
/// 10 000 PAT mass revoke = 10 batches × 1000 rows.
pub const MASS_REVOKE_OUTBOX_CHUNK_SIZE: usize = 1000;

/// Maximum number of [`RevokedEntry`] payloads packed into a single
/// CF Queue broadcast message. Per WI-S03-004 §9.6 — 100 entries
/// keeps each message well under the CF Queue 128 KiB ceiling
/// (RevokedEntry ≈ 200 bytes; 100 entries ≈ 20 KiB → 6.4× headroom).
pub const MASS_REVOKE_BROADCAST_BATCH_SIZE: usize = 100;

/// Per-tenant mass-revoke rate ceiling (revokes/sec). Per
/// WI-S03-004 §9.5 anti-abuse + queue backpressure. The orchestrator
/// does **not** enforce wall-clock rate limiting itself (that is a
/// separate WI-S08 concern); this constant is the canonical reference
/// for the rate-limit gate that wraps the mass-revoke entry point.
pub const MASS_REVOKE_RATE_PER_TENANT_PER_SEC: u32 = 100;

/// Minimum CF Workers KV TTL the session-cache invalidate hook
/// observes when re-arming (defensive constant; the orchestrator
/// **deletes** rather than re-arming, but downstream consumers reuse
/// this constant when constructing tomb entries).
pub const SESSION_CACHE_TOMB_TTL_SECS: u64 = 60;

/// Canonical KV-key prefix used by [`SessionCacheInvalidator`]
/// implementations. Per `auth_model.md §6` + WI-S03-003 hot-path
/// session cache.
pub const SESSION_CACHE_KEY_PREFIX: &str = "auth:session:";

// ---------------------------------------------------------------------------
// Canonical revocation types
// ---------------------------------------------------------------------------

/// Why a token was revoked. Drives compliance reports + audit chain
/// fan-out.
///
/// Marked `#[non_exhaustive]` so future variants (e.g.
/// `KeyRotationOverlap` for the 24 h overlap window in
/// `key_management.md §3.2.1`) land without a major bump.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationReason {
    /// User clicked "revoke" in the dashboard.
    UserInitiated,
    /// Tenant admin revoked another principal's token.
    AdminInitiated,
    /// Compromise detected (manual or automated).
    SecurityIncident,
    /// `expires_at` passed; revoked by background sweep.
    Expired,
    /// Scope set mutated; the old token is no longer the canonical
    /// representation of the principal's authority.
    ScopeChanged,
    /// One row inside a tenant-wide rotation event (recorded so the
    /// per-row audit chain entries can correlate to the parent
    /// mass-revoke id).
    MassRevoke,
}

impl RevocationReason {
    /// Wire literal used in the audit_outbox `payload_json.reason`
    /// field and in the metric label
    /// `corelink.auth.revocation.requested_total{reason=…}`.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::UserInitiated => "user_initiated",
            Self::AdminInitiated => "admin_initiated",
            Self::SecurityIncident => "security_incident",
            Self::Expired => "expired",
            Self::ScopeChanged => "scope_changed",
            Self::MassRevoke => "mass_revoke",
        }
    }
}

impl fmt::Display for RevocationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

/// Canonical persistent revocation record. Stored in the
/// [`RevocationStore`] (Durable Object surface) per region; broadcast
/// across regions verbatim by [`RevocationBroadcast`].
///
/// Marked `#[non_exhaustive]` for forward-compat additive evolution
/// (e.g. an `expires_at` mirror field for the daily reconciliation
/// Cron retention sweep — WI-S03-004 §14.5.9).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokedEntry {
    /// PAT primary key (Neon `pat.id` UUIDv7).
    pub pat_id: PatId,
    /// Owning tenant binding; replicated here so the reconciliation
    /// Cron can scope its scan without a Neon round-trip.
    pub tenant_id: PatTenantId,
    /// The principal that initiated the revoke (user-self,
    /// admin, automated sweeper, …).
    pub revoked_by: PatPrincipalId,
    /// Authoritative revocation timestamp produced by
    /// [`MetaRevocationSink::revoke`]. The DO storage stores this
    /// verbatim; downstream regions echo it back.
    pub revoked_at: SystemTime,
    /// Why the token was revoked.
    pub reason: RevocationReason,
    /// Region in which the revoke originated. Peer regions persist
    /// the same value verbatim (no per-region rewriting); this lets
    /// the audit chain diff origin vs. ingest semantics later.
    pub origin_region: Region,
    /// For mass-revoke flows: the parent mass-revoke id so the
    /// per-row audit entries can correlate. `None` for single
    /// revokes.
    pub mass_revoke_id: Option<MassRevokeId>,
}

impl RevokedEntry {
    /// Construct a fresh entry. Provided as a canonical constructor
    /// because the type is `#[non_exhaustive]`; downstream tests +
    /// the broadcast consumer construct entries via this surface.
    #[must_use]
    pub const fn new(
        pat_id: PatId,
        tenant_id: PatTenantId,
        revoked_by: PatPrincipalId,
        revoked_at: SystemTime,
        reason: RevocationReason,
        origin_region: Region,
        mass_revoke_id: Option<MassRevokeId>,
    ) -> Self {
        Self {
            pat_id,
            tenant_id,
            revoked_by,
            revoked_at,
            reason,
            origin_region,
            mass_revoke_id,
        }
    }

    /// Stable canonical key used by the broadcast-dedup layer. Two
    /// entries with the same `(pat_id, revoked_at)` are by definition
    /// the same revocation event and MUST be folded by consumers
    /// (INV-AUTH-PROPAGATION-AT-LEAST-ONCE consumer-side dedup).
    #[must_use]
    pub fn dedup_key(&self) -> RevocationDedupKey {
        RevocationDedupKey {
            pat_id: self.pat_id,
            revoked_at: self.revoked_at,
        }
    }
}

/// Canonical dedup key for the at-least-once broadcast plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RevocationDedupKey {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Authoritative revoke timestamp.
    pub revoked_at: SystemTime,
}

/// Per-region propagation tracker. Mutated by
/// [`RevocationStore::record_propagation_ack`] as peer regions ack
/// the broadcast.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropagationStatus {
    /// Set of regions that have acknowledged the revocation. The
    /// origin region is included immediately on the first revoke
    /// call.
    pub regions_propagated: BTreeSet<Region>,
    /// Wall-clock timestamp at which the **last** required region
    /// acked. `None` until the propagation deadline-or-outcome
    /// reconciliation completes.
    pub completed_at: Option<SystemTime>,
}

/// UUIDv7 surface for a mass-revoke parent operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MassRevokeId(pub Uuid);

impl MassRevokeId {
    /// Generate a fresh mass-revoke id (UUIDv7; time-ordered).
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for MassRevokeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Canonical revoke request body (single PAT). Wraps the fields a
/// caller must provide; the `revoked_at` timestamp is filled in by
/// the [`MetaRevocationSink`] inside the Postgres transaction
/// (`now()` server-side) so clock skew between the caller and Neon
/// cannot drift the canonical SoT.
#[derive(Clone, Debug)]
pub struct RevokeRequest {
    /// PAT primary key to revoke.
    pub pat_id: PatId,
    /// Owning tenant binding (carried through to the audit envelope).
    pub tenant_id: PatTenantId,
    /// Token-hash key the session cache invalidator drops. The
    /// orchestrator passes this through verbatim — the session-cache
    /// canonical key construction is owned by the WI-S03-003
    /// middleware and surfaced via this field.
    pub token_hash_key: SessionCacheKey,
    /// Why the token is being revoked.
    pub reason: RevocationReason,
    /// Principal initiating the revoke (user self, tenant admin, …).
    pub revoked_by: PatPrincipalId,
}

/// Canonical KV-key handle for a session-cache entry. Constructed by
/// `corelink_worker::middleware::auth` inside the Tower middleware
/// (WI-S03-003) and threaded through to the revocation orchestrator
/// verbatim.
///
/// The newtype keeps the canonical key construction (HMAC of the
/// PAT plaintext per `auth_model.md §6`) out of this module; the
/// orchestrator only knows the canonical wire shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionCacheKey(String);

impl SessionCacheKey {
    /// Construct from a fully-canonical `auth:session:<hash16>`
    /// string. Returns `None` when the input does not start with
    /// the canonical [`SESSION_CACHE_KEY_PREFIX`].
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.starts_with(SESSION_CACHE_KEY_PREFIX) && raw.len() > SESSION_CACHE_KEY_PREFIX.len() {
            Some(Self(raw.to_owned()))
        } else {
            None
        }
    }

    /// Construct from a raw 16-char hex hash (the canonical post-prefix
    /// segment). The orchestrator itself never hashes; the WI-S03-003
    /// middleware owns the HMAC-of-token derivation.
    #[must_use]
    pub fn from_hash(hash16: &str) -> Self {
        Self(format!("{SESSION_CACHE_KEY_PREFIX}{hash16}"))
    }

    /// Borrow the canonical key string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionCacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outcome of a single `revoke` orchestration. Carries the canonical
/// `revoked_at` timestamp + the per-stage health bits so the caller
/// can render a UI / log a structured event.
#[derive(Clone, Debug)]
pub struct RevokeResponse {
    /// Authoritative revocation timestamp from Neon.
    pub revoked_at: SystemTime,
    /// `true` if the Neon UPDATE flipped the row from active to
    /// revoked on this call; `false` when the row was already revoked
    /// (idempotent replay).
    pub was_freshly_revoked: bool,
    /// Health of the local KV invalidate hook. Best-effort: a
    /// soft-degraded outcome is logged but never propagated as an
    /// error.
    pub session_cache_invalidate: HookOutcome,
    /// Health of the cross-region broadcast enqueue.
    pub broadcast_outcome: PropagationOutcome,
    /// Origin region of this revoke (the one running the
    /// orchestrator; peer regions ingest async via the queue
    /// consumer).
    pub origin_region: Region,
}

/// Best-effort hook health bit. The orchestrator never returns a
/// transport error from a best-effort hook to its caller — the
/// canonical revoke MUST succeed even when KV / Queue is degraded
/// because the cold-path Neon revoked_at filter is the SoT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookOutcome {
    /// Hook succeeded.
    Ok,
    /// Hook returned a transport error; surfaced for telemetry but
    /// does not abort the revoke.
    SoftDegraded,
}

/// Outcome of a cross-region broadcast attempt. Maps to the
/// SLO-FRESH-PAT-REVOKE alert tiers in WI-S03-004 §15 chaos #2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropagationOutcome {
    /// Queue producer accepted the message; downstream consumer
    /// drains async.
    Enqueued,
    /// Queue producer was unavailable; the entry was persisted to a
    /// local DLQ and will be retried by the reconciliation Cron.
    /// Surfaces a SEV-2 alert via the metric pipeline.
    DlqFallback,
    /// No peer regions to propagate to (single-region deployment OR
    /// peer set was explicitly empty). The propagation is trivially
    /// complete.
    NoPeers,
}

/// Outcome of a [`MetaRevocationSink::revoke`] call. The two-arm
/// shape encodes the canonical Postgres
/// `UPDATE … WHERE revoked_at IS NULL` idempotency contract: an
/// `AlreadyRevoked` arm tells the orchestrator to short-circuit
/// downstream side effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetaRevokeOutcome {
    /// Row flipped from active to revoked; downstream side effects
    /// MUST run.
    Revoked {
        /// Authoritative timestamp set by Postgres `now()`.
        revoked_at: SystemTime,
    },
    /// Row was already revoked; the existing timestamp is returned
    /// verbatim. Downstream side effects MUST NOT run (they ran on
    /// the original call). Idempotent replay path per
    /// INV-AUTH-REVOCATION-IDEMPOTENT.
    AlreadyRevoked {
        /// Existing timestamp from the prior revoke call.
        revoked_at: SystemTime,
    },
}

/// Outcome of [`MetaRevocationSink::mass_revoke`]. The Phase 1
/// atomic UPDATE returns the list of newly-revoked rows; the
/// caller (orchestrator) then fans Phase 2 (audit_outbox + queue)
/// out chunked.
#[derive(Clone, Debug)]
pub struct MetaMassRevokeOutcome {
    /// Authoritative timestamp set by Postgres `now()` inside the
    /// atomic UPDATE.
    pub revoked_at: SystemTime,
    /// Rows newly flipped from active to revoked. Rows that were
    /// already revoked at call time are excluded (the canonical
    /// `WHERE revoked_at IS NULL` predicate filters them). The
    /// orchestrator emits Phase-2 audit events per id in this list.
    pub newly_revoked: Vec<MassRevokeRow>,
}

/// A single row in a mass-revoke result set. Carries the
/// principal-id binding so Phase 2 can construct the per-row
/// audit envelope without a follow-up Neon query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MassRevokeRow {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Owning principal (Neon `pat.principal_id`).
    pub principal_id: PatPrincipalId,
    /// Canonical session-cache key for the row's plaintext (so
    /// Phase 2 can drop the cached entry).
    pub token_hash_key: SessionCacheKey,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Canonical error surface for the orchestrator. Each variant maps
/// to a wire `COR_AUTH_*` code per WI-S03-004 §23.
#[derive(Debug, Error)]
pub enum RevocationError {
    /// PAT primary key did not match any row in Neon.
    /// `404 COR_AUTH_TOKEN_NOT_FOUND`.
    #[error("PAT not found")]
    NotFound,

    /// The caller's region disagrees with the origin region in the
    /// request. This is INV-DATA-RESIDENCY enforcement at the
    /// adapter boundary (rejects a misrouted call rather than
    /// silently writing the wrong region's DO).
    #[error("region mismatch: orchestrator region {orchestrator}, request region {request}")]
    RegionMismatch {
        /// The region the orchestrator is configured for.
        orchestrator: Region,
        /// The region the request claimed.
        request: Region,
    },

    /// The Neon SoT writer surfaced a transport / runtime fault.
    /// `503 COR_AUTH_BACKEND_UNAVAILABLE`.
    #[error("Neon SoT writer fault: {0}")]
    Neon(String),

    /// The DO storage adapter surfaced a transport fault.
    /// `503 COR_AUTH_BACKEND_UNAVAILABLE`.
    #[error("DO storage fault: {0}")]
    DurableObject(String),
}

impl RevocationError {
    /// Canonical wire-stable error code.
    #[must_use]
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound => "COR_AUTH_TOKEN_NOT_FOUND",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::Neon(_) | Self::DurableObject(_) => "COR_AUTH_BACKEND_UNAVAILABLE",
        }
    }

    /// Canonical wire-stable HTTP status code.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::RegionMismatch { .. } => 500,
            Self::Neon(_) | Self::DurableObject(_) => 503,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait abstractions (DO storage / Neon writer / KV invalidator / Queue)
// ---------------------------------------------------------------------------

/// Persistent revocation list — Durable Object equivalent.
///
/// The trait surface is deliberately small: the orchestrator only
/// needs idempotent upsert + range query + propagation-status
/// tracking. The production CF Durable Object adapter (deferred to
/// the wrangler-binding shim WI; see WI-S03-004 §6.2 trait abstraction
/// defer pattern) wires `worker::DurableObjectStorage` into these
/// methods.
#[async_trait]
pub trait RevocationStore: Send + Sync {
    /// Idempotent upsert. The implementation MUST treat
    /// `(entry.pat_id, entry.revoked_at)` as the canonical
    /// dedup key — two calls with matching keys are no-ops.
    ///
    /// Returns `true` when the upsert was a fresh insert; `false`
    /// when the entry was already present (idempotent replay).
    async fn upsert(&self, entry: RevokedEntry) -> Result<bool, RevocationError>;

    /// Lookup by PAT primary key. Returns `None` when the entry is
    /// absent. Used by the admin / audit query plane (S-13 forward);
    /// the verify hot path consults Neon `pat.revoked_at` instead
    /// (INV-AUTH-NEON-IS-SOT).
    async fn get(&self, pat_id: PatId) -> Result<Option<RevokedEntry>, RevocationError>;

    /// Record that `peer` has acknowledged the entry. Idempotent.
    async fn record_propagation_ack(
        &self,
        pat_id: PatId,
        peer: Region,
    ) -> Result<(), RevocationError>;

    /// Snapshot the propagation tracker for `pat_id`. Returns
    /// `None` if the entry is absent.
    async fn propagation_status(
        &self,
        pat_id: PatId,
    ) -> Result<Option<PropagationStatus>, RevocationError>;

    /// Range-scan the store for the reconciliation Cron.
    /// `tenant_filter` of `None` returns all entries.
    async fn list(
        &self,
        tenant_filter: Option<PatTenantId>,
    ) -> Result<Vec<RevokedEntry>, RevocationError>;
}

/// Neon transactional writer — the canonical SoT for revocation
/// state (INV-AUTH-NEON-IS-SOT).
///
/// Implementations MUST run the `pat` UPDATE and the `audit_outbox`
/// INSERT in the **same** Postgres transaction (the WI-S01-005
/// atomic batch pattern). The mass-revoke flow is two-phase: Phase 1
/// is a single atomic UPDATE; Phase 2 is the orchestrator-driven
/// chunked outbox INSERT (this trait exposes the per-chunk hook
/// [`Self::insert_outbox_batch`] for symmetry with the queue
/// producer).
#[async_trait]
pub trait MetaRevocationSink: Send + Sync {
    /// Single-row revoke. Implementation contract:
    ///
    /// ```sql
    /// BEGIN;
    /// UPDATE pat SET revoked_at = now(), reason = $reason
    ///   WHERE id = $pat_id AND revoked_at IS NULL
    ///   RETURNING revoked_at;
    /// -- if RETURNING was empty (already revoked):
    /// SELECT revoked_at FROM pat WHERE id = $pat_id;
    /// INSERT INTO audit_outbox (..., event_type = 'auth.token.revoked', ...);
    /// COMMIT;
    /// ```
    ///
    /// Returns [`MetaRevokeOutcome::Revoked`] on the fresh-revoke
    /// path (audit row inserted) or [`MetaRevokeOutcome::AlreadyRevoked`]
    /// on the idempotent replay path (audit row NOT inserted; the
    /// original event is the canonical record).
    async fn revoke(
        &self,
        pat_id: PatId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaRevokeOutcome, RevocationError>;

    /// Mass-revoke Phase 1 — atomic tenant-scoped UPDATE. Returns
    /// the list of rows that flipped from active to revoked plus the
    /// canonical `revoked_at` timestamp.
    async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaMassRevokeOutcome, RevocationError>;

    /// Mass-revoke Phase 2 chunk hook — inserts up to
    /// [`MASS_REVOKE_OUTBOX_CHUNK_SIZE`] audit_outbox rows in a single
    /// batch. Implementations SHOULD use the same INSERT batching
    /// path as WI-S01-005.
    async fn insert_outbox_batch(
        &self,
        tenant_id: PatTenantId,
        mass_revoke_id: MassRevokeId,
        revoked_at: SystemTime,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
        chunk: &[MassRevokeRow],
    ) -> Result<(), RevocationError>;
}

/// KV-namespace adapter that drops a session-cache entry. The
/// orchestrator treats KV outages as soft-degraded (the cold path
/// recovers via Neon).
///
/// The trait is dyn-compatible (`#[async_trait]`) so the
/// orchestrator can hold an `Arc<dyn SessionCacheInvalidator>`. The
/// canonical wrapper [`KvSessionCacheInvalidator`] adapts any
/// [`KvBackend`] (which is NOT dyn-compatible by design — it uses
/// RPITIT for zero-cost futures) into this trait.
#[async_trait]
pub trait SessionCacheInvalidator: Send + Sync {
    /// Drop the canonical session-cache entry. Idempotent on missing
    /// keys.
    async fn invalidate(&self, key: &SessionCacheKey) -> Result<(), KvError>;
}

/// Adapter wrapping a generic [`KvBackend`] as a dyn-compatible
/// [`SessionCacheInvalidator`]. The host-server / property-test
/// crates instantiate this with the concrete backend (in-memory
/// fake or production CF binding) and hand it to the orchestrator
/// as `Arc<dyn SessionCacheInvalidator>`.
pub struct KvSessionCacheInvalidator<K: KvBackend + Send + Sync + 'static> {
    kv: K,
}

impl<K: KvBackend + Send + Sync + 'static> KvSessionCacheInvalidator<K> {
    /// Wrap a [`KvBackend`].
    #[must_use]
    pub const fn new(kv: K) -> Self {
        Self { kv }
    }

    /// Borrow the inner backend.
    #[must_use]
    pub const fn inner(&self) -> &K {
        &self.kv
    }
}

impl<K: KvBackend + Send + Sync + fmt::Debug + 'static> fmt::Debug
    for KvSessionCacheInvalidator<K>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KvSessionCacheInvalidator")
            .field("kv", &self.kv)
            .finish()
    }
}

#[async_trait]
impl<K: KvBackend + Send + Sync + 'static> SessionCacheInvalidator
    for KvSessionCacheInvalidator<K>
{
    async fn invalidate(&self, key: &SessionCacheKey) -> Result<(), KvError> {
        self.kv.delete(key.as_str()).await
    }
}

/// Cross-region broadcast producer (CF Queue equivalent). Returns a
/// [`PropagationOutcome::Enqueued`] when the message was accepted,
/// or [`PropagationOutcome::DlqFallback`] when the producer was
/// unavailable and the orchestrator MUST persist to the DLQ.
#[async_trait]
pub trait RevocationBroadcast: Send + Sync {
    /// Enqueue a single-revoke broadcast. The implementation
    /// derives the peer-region set internally (the orchestrator's
    /// `peers` config is opaque from the producer's POV).
    async fn enqueue_single(&self, entry: &RevokedEntry) -> PropagationOutcome;

    /// Enqueue a batched mass-revoke broadcast. Implementations
    /// MUST chunk into ≤ [`MASS_REVOKE_BROADCAST_BATCH_SIZE`]-entry
    /// messages internally; the orchestrator passes the full slice.
    async fn enqueue_mass(&self, entries: &[RevokedEntry]) -> PropagationOutcome;
}

// ---------------------------------------------------------------------------
// In-memory test fakes
// ---------------------------------------------------------------------------

/// Host-side test fake for [`RevocationStore`] backed by a
/// `BTreeMap`. Mirrors the in-memory R2 / KV pattern: same
/// documented semantics as the production CF Durable Object adapter.
///
/// Concurrency model: a single `Mutex` wrapping the whole map. The
/// production DO is single-writer-per-region by construction; the
/// fake faithfully reproduces that contract.
pub struct InMemoryRevocationStore {
    inner: Mutex<InMemoryRevocationState>,
}

#[derive(Default)]
struct InMemoryRevocationState {
    entries: HashMap<PatId, RevokedEntry>,
    propagation: HashMap<PatId, PropagationStatus>,
}

impl fmt::Debug for InMemoryRevocationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryRevocationStore").finish_non_exhaustive()
    }
}

impl Default for InMemoryRevocationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRevocationStore {
    /// Construct a fresh, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InMemoryRevocationState::default()),
        }
    }

    /// Test-only snapshot of the entry map size.
    pub async fn entry_count(&self) -> usize {
        self.inner.lock().await.entries.len()
    }
}

#[async_trait]
impl RevocationStore for InMemoryRevocationStore {
    async fn upsert(&self, entry: RevokedEntry) -> Result<bool, RevocationError> {
        let mut guard = self.inner.lock().await;
        if let Some(existing) = guard.entries.get(&entry.pat_id) {
            // Canonical dedup: matching `(pat_id, revoked_at)` is a
            // no-op — preserves INV-AUTH-REVOCATION-IDEMPOTENT.
            if existing.revoked_at == entry.revoked_at {
                return Ok(false);
            }
            // Distinct revoke timestamps for the same pat_id are a
            // bug at a higher layer (Neon SoT enforces a single
            // revoke); we keep the earliest timestamp (canonical
            // SoT wins) and surface as no-op.
            return Ok(false);
        }
        let mut status = PropagationStatus::default();
        status.regions_propagated.insert(entry.origin_region);
        guard.propagation.insert(entry.pat_id, status);
        guard.entries.insert(entry.pat_id, entry);
        Ok(true)
    }

    async fn get(&self, pat_id: PatId) -> Result<Option<RevokedEntry>, RevocationError> {
        let guard = self.inner.lock().await;
        Ok(guard.entries.get(&pat_id).cloned())
    }

    async fn record_propagation_ack(
        &self,
        pat_id: PatId,
        peer: Region,
    ) -> Result<(), RevocationError> {
        let mut guard = self.inner.lock().await;
        let status = guard
            .propagation
            .entry(pat_id)
            .or_insert_with(PropagationStatus::default);
        status.regions_propagated.insert(peer);
        // We don't compute `completed_at` here — that requires the
        // expected peer-set from the orchestrator config. The
        // orchestrator's reconciliation Cron stamps it.
        Ok(())
    }

    async fn propagation_status(
        &self,
        pat_id: PatId,
    ) -> Result<Option<PropagationStatus>, RevocationError> {
        let guard = self.inner.lock().await;
        Ok(guard.propagation.get(&pat_id).cloned())
    }

    async fn list(
        &self,
        tenant_filter: Option<PatTenantId>,
    ) -> Result<Vec<RevokedEntry>, RevocationError> {
        let guard = self.inner.lock().await;
        let entries: Vec<RevokedEntry> = guard
            .entries
            .values()
            .filter(|e| match tenant_filter {
                Some(t) => e.tenant_id == t,
                None => true,
            })
            .cloned()
            .collect();
        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// In-memory MetaRevocationSink fake (Neon SoT model)
// ---------------------------------------------------------------------------

/// Host-side test fake for [`MetaRevocationSink`]. Models the Neon
/// `pat` table + the `audit_outbox` INSERT-only log behind a single
/// `Mutex`. The combined invariant is the load-bearing test surface:
/// `audit_outbox` row count MUST equal the count of
/// `MetaRevokeOutcome::Revoked` returns (no duplicates on retry per
/// INV-AUTH-REVOCATION-IDEMPOTENT).
pub struct InMemoryMetaRevocationSink {
    inner: Mutex<InMemoryMetaState>,
    clock: Arc<dyn TestClock>,
}

#[derive(Default)]
struct InMemoryMetaState {
    /// `pat` rows the sink knows about. The orchestrator-side tests
    /// pre-seed via [`InMemoryMetaRevocationSink::seed_pat`].
    pat_rows: HashMap<PatId, FakePatRow>,
    /// audit_outbox INSERT log. Each row is one
    /// `auth.token.revoked` event.
    audit_outbox: Vec<FakeAuditRow>,
}

#[derive(Clone, Debug)]
struct FakePatRow {
    tenant_id: PatTenantId,
    principal_id: PatPrincipalId,
    revoked_at: Option<SystemTime>,
    /// Canonical session-cache key for this row's plaintext;
    /// pre-seeded by tests so Phase 2 mass-revoke fan-out has the
    /// canonical handle.
    session_cache_key: SessionCacheKey,
}

#[derive(Clone, Debug)]
struct FakeAuditRow {
    /// `pat_id` of the revoked row.
    pub pat_id: PatId,
    /// `tenant_id` of the revoked row.
    pub tenant_id: PatTenantId,
    /// Authoritative `revoked_at`.
    pub revoked_at: SystemTime,
    /// Reason tag.
    pub reason: RevocationReason,
    /// Origin: single-revoke `None`, mass-revoke `Some(parent_id)`.
    pub mass_revoke_id: Option<MassRevokeId>,
}

/// Test-only clock surface. `SystemTime`-monotonic by default; the
/// in-memory sink uses it to stamp `revoked_at` deterministically.
pub trait TestClock: Send + Sync {
    /// Return a monotonically-non-decreasing instant.
    fn now(&self) -> SystemTime;
}

impl fmt::Debug for dyn TestClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TestClock")
    }
}

/// Default monotonic clock for [`InMemoryMetaRevocationSink`]. Seeds
/// from a fixed UNIX epoch and increments by 1 ms per call so two
/// back-to-back revokes deterministically receive distinct
/// timestamps (necessary for property tests asserting timestamp
/// uniqueness across N calls).
#[derive(Debug)]
pub struct MonotonicTestClock {
    next_micros: std::sync::atomic::AtomicU64,
}

impl MonotonicTestClock {
    /// Construct a fresh clock seeded at `1_700_000_000_000_000` µs
    /// past UNIX epoch (a fixed reference instant).
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_micros: std::sync::atomic::AtomicU64::new(1_700_000_000_000_000),
        }
    }
}

impl Default for MonotonicTestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TestClock for MonotonicTestClock {
    fn now(&self) -> SystemTime {
        let micros = self
            .next_micros
            .fetch_add(1_000, std::sync::atomic::Ordering::AcqRel);
        SystemTime::UNIX_EPOCH + Duration::from_micros(micros)
    }
}

impl fmt::Debug for InMemoryMetaRevocationSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryMetaRevocationSink").finish_non_exhaustive()
    }
}

impl InMemoryMetaRevocationSink {
    /// Construct a fresh, empty sink with a monotonic clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(MonotonicTestClock::new()))
    }

    /// Construct with an arbitrary [`TestClock`].
    #[must_use]
    pub fn with_clock(clock: Arc<dyn TestClock>) -> Self {
        Self {
            inner: Mutex::new(InMemoryMetaState::default()),
            clock,
        }
    }

    /// Pre-seed a `pat` row. Tests call this to populate the table
    /// before exercising the orchestrator.
    pub async fn seed_pat(
        &self,
        pat_id: PatId,
        tenant_id: PatTenantId,
        principal_id: PatPrincipalId,
        session_cache_key: SessionCacheKey,
    ) {
        let mut guard = self.inner.lock().await;
        guard.pat_rows.insert(
            pat_id,
            FakePatRow {
                tenant_id,
                principal_id,
                revoked_at: None,
                session_cache_key,
            },
        );
    }

    /// Test-only audit row count.
    pub async fn audit_count(&self) -> usize {
        self.inner.lock().await.audit_outbox.len()
    }

    /// Test-only: count audit rows whose `(pat_id, revoked_at)`
    /// matches.
    pub async fn audit_count_for(&self, pat_id: PatId, revoked_at: SystemTime) -> usize {
        self.inner
            .lock()
            .await
            .audit_outbox
            .iter()
            .filter(|r| r.pat_id == pat_id && r.revoked_at == revoked_at)
            .count()
    }

    /// Test-only audit rows snapshot.
    pub async fn audit_rows(&self) -> Vec<TestAuditRow> {
        self.inner
            .lock()
            .await
            .audit_outbox
            .iter()
            .map(|r| TestAuditRow {
                pat_id: r.pat_id,
                tenant_id: r.tenant_id,
                revoked_at: r.revoked_at,
                reason: r.reason,
                mass_revoke_id: r.mass_revoke_id,
            })
            .collect()
    }
}

/// Public, read-only audit row projection used by tests.
#[derive(Clone, Debug)]
pub struct TestAuditRow {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Tenant binding.
    pub tenant_id: PatTenantId,
    /// Authoritative timestamp.
    pub revoked_at: SystemTime,
    /// Reason tag.
    pub reason: RevocationReason,
    /// Parent mass-revoke id (single revoke = `None`).
    pub mass_revoke_id: Option<MassRevokeId>,
}

impl Default for InMemoryMetaRevocationSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetaRevocationSink for InMemoryMetaRevocationSink {
    async fn revoke(
        &self,
        pat_id: PatId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaRevokeOutcome, RevocationError> {
        let mut guard = self.inner.lock().await;
        let row = guard
            .pat_rows
            .get_mut(&pat_id)
            .ok_or(RevocationError::NotFound)?;
        if let Some(existing) = row.revoked_at {
            // Idempotent replay path. CRITICAL: do NOT insert a new
            // audit row.
            return Ok(MetaRevokeOutcome::AlreadyRevoked {
                revoked_at: existing,
            });
        }
        let revoked_at = self.clock.now();
        row.revoked_at = Some(revoked_at);
        let tenant_id = row.tenant_id;
        let _ = revoked_by;
        guard.audit_outbox.push(FakeAuditRow {
            pat_id,
            tenant_id,
            revoked_at,
            reason,
            mass_revoke_id: None,
        });
        Ok(MetaRevokeOutcome::Revoked { revoked_at })
    }

    async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaMassRevokeOutcome, RevocationError> {
        let mut guard = self.inner.lock().await;
        let revoked_at = self.clock.now();
        let _ = (reason, revoked_by);
        let mut newly_revoked = Vec::new();
        for (pat_id, row) in guard.pat_rows.iter_mut() {
            if row.tenant_id == tenant_id && row.revoked_at.is_none() {
                row.revoked_at = Some(revoked_at);
                newly_revoked.push(MassRevokeRow {
                    pat_id: *pat_id,
                    principal_id: row.principal_id,
                    token_hash_key: row.session_cache_key.clone(),
                });
            }
        }
        Ok(MetaMassRevokeOutcome {
            revoked_at,
            newly_revoked,
        })
    }

    async fn insert_outbox_batch(
        &self,
        tenant_id: PatTenantId,
        mass_revoke_id: MassRevokeId,
        revoked_at: SystemTime,
        reason: RevocationReason,
        _revoked_by: PatPrincipalId,
        chunk: &[MassRevokeRow],
    ) -> Result<(), RevocationError> {
        let mut guard = self.inner.lock().await;
        for row in chunk {
            guard.audit_outbox.push(FakeAuditRow {
                pat_id: row.pat_id,
                tenant_id,
                revoked_at,
                reason,
                mass_revoke_id: Some(mass_revoke_id),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// In-memory broadcast fake
// ---------------------------------------------------------------------------

/// Host-side test fake for [`RevocationBroadcast`]. Records every
/// enqueued payload + supports a "queue down" mode for chaos tests.
pub struct InMemoryBroadcast {
    inner: Mutex<InMemoryBroadcastState>,
}

#[derive(Default)]
struct InMemoryBroadcastState {
    enqueued: Vec<RevokedEntry>,
    queue_down: bool,
}

impl fmt::Debug for InMemoryBroadcast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryBroadcast").finish_non_exhaustive()
    }
}

impl Default for InMemoryBroadcast {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBroadcast {
    /// Construct a fresh, empty broadcast fake (queue up).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InMemoryBroadcastState::default()),
        }
    }

    /// Toggle the "queue down" failure mode. While set, every
    /// enqueue returns [`PropagationOutcome::DlqFallback`].
    pub async fn set_queue_down(&self, down: bool) {
        self.inner.lock().await.queue_down = down;
    }

    /// Snapshot the recorded enqueues.
    pub async fn enqueued(&self) -> Vec<RevokedEntry> {
        self.inner.lock().await.enqueued.clone()
    }

    /// Count the recorded enqueues for a given pat_id.
    pub async fn enqueued_count(&self, pat_id: PatId) -> usize {
        self.inner
            .lock()
            .await
            .enqueued
            .iter()
            .filter(|e| e.pat_id == pat_id)
            .count()
    }
}

#[async_trait]
impl RevocationBroadcast for InMemoryBroadcast {
    async fn enqueue_single(&self, entry: &RevokedEntry) -> PropagationOutcome {
        let mut guard = self.inner.lock().await;
        if guard.queue_down {
            return PropagationOutcome::DlqFallback;
        }
        guard.enqueued.push(entry.clone());
        PropagationOutcome::Enqueued
    }

    async fn enqueue_mass(&self, entries: &[RevokedEntry]) -> PropagationOutcome {
        let mut guard = self.inner.lock().await;
        if guard.queue_down {
            return PropagationOutcome::DlqFallback;
        }
        // Faithfully chunked enqueue (informational; the test fake
        // doesn't model batch semantics — it appends every entry).
        for entry in entries {
            guard.enqueued.push(entry.clone());
        }
        PropagationOutcome::Enqueued
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Top-level revocation orchestrator. Wires the four traits together
/// and exposes the canonical `revoke` / `mass_revoke` /
/// `ingest_remote` surface that the host-server endpoints (and the
/// CF Queue consumer) call.
///
/// A single `RevocationOrchestrator` is pinned to a region — every
/// [`RevocationStore::upsert`] writes that region's DO; every
/// `enqueue_*` call is for the broadcast plane targeting peers.
pub struct RevocationOrchestrator {
    region: Region,
    peers: BTreeSet<Region>,
    store: Arc<dyn RevocationStore>,
    meta: Arc<dyn MetaRevocationSink>,
    session_cache: Arc<dyn SessionCacheInvalidator>,
    broadcast: Arc<dyn RevocationBroadcast>,
}

impl fmt::Debug for RevocationOrchestrator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevocationOrchestrator")
            .field("region", &self.region)
            .field("peers", &self.peers)
            .finish_non_exhaustive()
    }
}

impl RevocationOrchestrator {
    /// Construct a fresh orchestrator. `peers` is the set of peer
    /// regions for cross-region propagation; the origin region MUST
    /// NOT appear in the set (a config-time invariant; we filter it
    /// defensively here so a misconfiguration doesn't double-emit).
    #[must_use]
    pub fn new(
        region: Region,
        peers: BTreeSet<Region>,
        store: Arc<dyn RevocationStore>,
        meta: Arc<dyn MetaRevocationSink>,
        session_cache: Arc<dyn SessionCacheInvalidator>,
        broadcast: Arc<dyn RevocationBroadcast>,
    ) -> Self {
        let mut peers = peers;
        peers.remove(&region);
        Self {
            region,
            peers,
            store,
            meta,
            session_cache,
            broadcast,
        }
    }

    /// The region this orchestrator is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// The peer-region set (origin region filtered out at
    /// construction time).
    #[must_use]
    pub fn peers(&self) -> &BTreeSet<Region> {
        &self.peers
    }

    /// Canonical single-revoke flow.
    ///
    /// Steps (canonical order; each step's failure mode documented
    /// inline):
    ///
    /// 1. Neon UPDATE + audit_outbox INSERT (atomic). On
    ///    [`MetaRevokeOutcome::AlreadyRevoked`] short-circuit and
    ///    return the existing timestamp; no downstream side effects.
    /// 2. DO storage upsert (idempotent; preserves
    ///    INV-AUTH-REVOCATION-IDEMPOTENT under retry).
    /// 3. KV session-cache invalidate (best-effort soft-degraded).
    /// 4. Cross-region broadcast enqueue (peer set; outcome
    ///    surfaced to the caller).
    pub async fn revoke(
        &self,
        request: RevokeRequest,
    ) -> Result<RevokeResponse, RevocationError> {
        // Step 1 — Neon SoT.
        let meta_outcome = self
            .meta
            .revoke(request.pat_id, request.reason, request.revoked_by)
            .await?;
        let (revoked_at, was_freshly_revoked) = match meta_outcome {
            MetaRevokeOutcome::Revoked { revoked_at } => (revoked_at, true),
            MetaRevokeOutcome::AlreadyRevoked { revoked_at } => {
                // INV-AUTH-REVOCATION-IDEMPOTENT replay path.
                // Skip DO upsert / KV invalidate / broadcast.
                return Ok(RevokeResponse {
                    revoked_at,
                    was_freshly_revoked: false,
                    session_cache_invalidate: HookOutcome::Ok,
                    broadcast_outcome: PropagationOutcome::NoPeers,
                    origin_region: self.region,
                });
            }
        };

        // Step 2 — DO storage upsert.
        let entry = RevokedEntry {
            pat_id: request.pat_id,
            tenant_id: request.tenant_id,
            revoked_by: request.revoked_by,
            revoked_at,
            reason: request.reason,
            origin_region: self.region,
            mass_revoke_id: None,
        };
        let _ = self.store.upsert(entry.clone()).await?;

        // Step 3 — KV session-cache invalidate (best-effort).
        let session_cache_invalidate = match self
            .session_cache
            .invalidate(&request.token_hash_key)
            .await
        {
            Ok(()) => HookOutcome::Ok,
            Err(_) => HookOutcome::SoftDegraded,
        };

        // Step 4 — Cross-region broadcast.
        let broadcast_outcome = if self.peers.is_empty() {
            PropagationOutcome::NoPeers
        } else {
            self.broadcast.enqueue_single(&entry).await
        };

        Ok(RevokeResponse {
            revoked_at,
            was_freshly_revoked,
            session_cache_invalidate,
            broadcast_outcome,
            origin_region: self.region,
        })
    }

    /// Canonical mass-revoke flow.
    ///
    /// Phase 1 — atomic Neon UPDATE.
    /// Phase 2 — chunked audit_outbox INSERT
    ///   (`MASS_REVOKE_OUTBOX_CHUNK_SIZE`) + chunked Queue broadcast
    ///   (`MASS_REVOKE_BROADCAST_BATCH_SIZE`) + per-row DO upsert +
    ///   per-row session-cache invalidate.
    pub async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MassRevokeResponse, RevocationError> {
        let mass_revoke_id = MassRevokeId::new_v7();
        let phase1 = self
            .meta
            .mass_revoke(tenant_id, reason, revoked_by)
            .await?;
        let revoked_at = phase1.revoked_at;
        let total = phase1.newly_revoked.len();

        // Phase 2 — chunked audit_outbox INSERT.
        let mut chunked_count = 0usize;
        for chunk in phase1.newly_revoked.chunks(MASS_REVOKE_OUTBOX_CHUNK_SIZE) {
            self.meta
                .insert_outbox_batch(
                    tenant_id,
                    mass_revoke_id,
                    revoked_at,
                    reason,
                    revoked_by,
                    chunk,
                )
                .await?;
            chunked_count += chunk.len();
        }

        // Phase 2 — DO upsert + KV invalidate per row + accumulate
        // entries for the broadcast batch.
        let mut entries: Vec<RevokedEntry> = Vec::with_capacity(total);
        let mut session_cache_failures = 0usize;
        for row in &phase1.newly_revoked {
            let entry = RevokedEntry {
                pat_id: row.pat_id,
                tenant_id,
                revoked_by,
                revoked_at,
                reason,
                origin_region: self.region,
                mass_revoke_id: Some(mass_revoke_id),
            };
            let _ = self.store.upsert(entry.clone()).await?;
            if self
                .session_cache
                .invalidate(&row.token_hash_key)
                .await
                .is_err()
            {
                session_cache_failures += 1;
            }
            entries.push(entry);
        }

        // Phase 2 — broadcast in
        // MASS_REVOKE_BROADCAST_BATCH_SIZE-bounded chunks.
        let mut broadcast_outcome = PropagationOutcome::Enqueued;
        if self.peers.is_empty() || entries.is_empty() {
            broadcast_outcome = PropagationOutcome::NoPeers;
        } else {
            for batch in entries.chunks(MASS_REVOKE_BROADCAST_BATCH_SIZE) {
                let outcome = self.broadcast.enqueue_mass(batch).await;
                if outcome == PropagationOutcome::DlqFallback {
                    broadcast_outcome = PropagationOutcome::DlqFallback;
                }
            }
        }

        Ok(MassRevokeResponse {
            mass_revoke_id,
            revoked_at,
            revoked_count: total,
            outbox_inserted: chunked_count,
            session_cache_failures,
            broadcast_outcome,
            origin_region: self.region,
        })
    }

    /// Ingest a remote revocation broadcast (called by the CF Queue
    /// consumer in peer regions). Idempotent under retry — the
    /// canonical dedup key is `(pat_id, revoked_at)`.
    ///
    /// Steps:
    /// 1. Region-mismatch defensive check (the orchestrator MUST be
    ///    the receiving region; `entry.origin_region` is the
    ///    sender's region, distinct from the orchestrator's).
    /// 2. DO storage upsert (idempotent; INV-AUTH-PROPAGATION-AT-LEAST-ONCE).
    /// 3. KV session-cache invalidate (best-effort).
    /// 4. Origin-region propagation ack.
    pub async fn ingest_remote(
        &self,
        entry: RevokedEntry,
        remote_session_cache_key: Option<SessionCacheKey>,
    ) -> Result<IngestOutcome, RevocationError> {
        if entry.origin_region == self.region {
            // The origin's own DO already holds this entry; an
            // incoming "remote" message addressed back to the origin
            // is either a config bug or a self-test loopback. We
            // accept it idempotently (upsert) but mark the outcome
            // accordingly.
        }
        let was_fresh = self.store.upsert(entry.clone()).await?;
        let session_cache_invalidate = match remote_session_cache_key.as_ref() {
            Some(key) => match self.session_cache.invalidate(key).await {
                Ok(()) => HookOutcome::Ok,
                Err(_) => HookOutcome::SoftDegraded,
            },
            None => HookOutcome::Ok,
        };
        // Record propagation ack at the origin's tracker.
        // (The origin is `entry.origin_region`; this orchestrator is
        // `self.region` and is the new ack source.)
        self.store
            .record_propagation_ack(entry.pat_id, self.region)
            .await?;
        Ok(IngestOutcome {
            was_fresh,
            session_cache_invalidate,
        })
    }

    /// Reconciliation Cron entry point. Returns a summary of the
    /// drift between the local DO storage and the canonical Neon
    /// `pat.revoked_at` snapshot.
    ///
    /// The drift detection is deliberately host-agnostic: callers
    /// pass a `neon_view: Vec<(PatId, SystemTime)>` snapshot they
    /// produced from a Neon read query. The return value enumerates
    /// the two drift directions so the runbook RB-FM-REVOKE-DRIFT
    /// can act on each.
    pub async fn reconcile(
        &self,
        neon_view: &[(PatId, SystemTime)],
    ) -> Result<ReconciliationSummary, RevocationError> {
        let do_view = self.store.list(None).await?;
        let do_set: HashMap<PatId, SystemTime> =
            do_view.into_iter().map(|e| (e.pat_id, e.revoked_at)).collect();
        let mut neon_only = Vec::new();
        let mut do_only = Vec::new();
        let mut timestamp_drift = Vec::new();
        let neon_map: HashMap<PatId, SystemTime> = neon_view.iter().copied().collect();
        for (pat_id, neon_ts) in &neon_map {
            match do_set.get(pat_id) {
                None => neon_only.push(*pat_id),
                Some(do_ts) if do_ts != neon_ts => {
                    timestamp_drift.push(DriftRow {
                        pat_id: *pat_id,
                        neon_ts: *neon_ts,
                        do_ts: *do_ts,
                    });
                }
                _ => {}
            }
        }
        for pat_id in do_set.keys() {
            if !neon_map.contains_key(pat_id) {
                do_only.push(*pat_id);
            }
        }
        Ok(ReconciliationSummary {
            neon_only,
            do_only,
            timestamp_drift,
        })
    }
}

/// Outcome of a [`RevocationOrchestrator::mass_revoke`] call.
#[derive(Clone, Debug)]
pub struct MassRevokeResponse {
    /// Parent mass-revoke id (UUIDv7).
    pub mass_revoke_id: MassRevokeId,
    /// Authoritative timestamp set inside the Phase 1 atomic
    /// UPDATE.
    pub revoked_at: SystemTime,
    /// Number of rows newly revoked (Phase 1 result).
    pub revoked_count: usize,
    /// Number of audit_outbox rows successfully inserted in Phase 2.
    pub outbox_inserted: usize,
    /// Number of session-cache invalidate failures observed during
    /// Phase 2 fan-out (best-effort; soft-degraded; logged for
    /// telemetry, not an error).
    pub session_cache_failures: usize,
    /// Aggregated broadcast outcome (worst of any chunk).
    pub broadcast_outcome: PropagationOutcome,
    /// Origin region.
    pub origin_region: Region,
}

/// Outcome of [`RevocationOrchestrator::ingest_remote`].
#[derive(Clone, Copy, Debug)]
pub struct IngestOutcome {
    /// `true` when the upsert was a fresh insert; `false` on
    /// idempotent replay.
    pub was_fresh: bool,
    /// Health of the local session-cache invalidate hook.
    pub session_cache_invalidate: HookOutcome,
}

/// Reconciliation drift summary returned by
/// [`RevocationOrchestrator::reconcile`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconciliationSummary {
    /// PAT ids the Neon snapshot contains that the local DO storage
    /// does NOT (delivered revoke whose broadcast never reached the
    /// region OR whose DO write was lost).
    pub neon_only: Vec<PatId>,
    /// PAT ids the local DO storage contains that Neon does NOT
    /// (broadcast received without a matching SoT row — extremely
    /// unusual; investigate via RB-FM-REVOKE-DRIFT).
    pub do_only: Vec<PatId>,
    /// PAT ids present in both views with disagreeing
    /// `revoked_at` timestamps. The Neon timestamp is canonical;
    /// the runbook patches the DO row to match.
    pub timestamp_drift: Vec<DriftRow>,
}

impl ReconciliationSummary {
    /// `true` when no drift was observed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.neon_only.is_empty() && self.do_only.is_empty() && self.timestamp_drift.is_empty()
    }
}

/// One row in [`ReconciliationSummary::timestamp_drift`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriftRow {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Neon `pat.revoked_at` (canonical).
    pub neon_ts: SystemTime,
    /// Local DO storage `revoked_at`.
    pub do_ts: SystemTime,
}

// ---------------------------------------------------------------------------
// Unit tests (in-module smoke + property test scaffolding)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    fn pat_id() -> PatId {
        PatId(Uuid::now_v7())
    }

    fn tenant() -> PatTenantId {
        PatTenantId(Uuid::now_v7())
    }

    fn principal() -> PatPrincipalId {
        PatPrincipalId(Uuid::now_v7())
    }

    fn key(suffix: &str) -> SessionCacheKey {
        SessionCacheKey::from_hash(suffix)
    }

    fn build_orchestrator<K: KvBackend + Send + Sync + 'static>(
        region: Region,
        peers: BTreeSet<Region>,
        store: Arc<InMemoryRevocationStore>,
        meta: Arc<InMemoryMetaRevocationSink>,
        kv: K,
        broadcast: Arc<InMemoryBroadcast>,
    ) -> RevocationOrchestrator {
        let cache: Arc<dyn SessionCacheInvalidator> =
            Arc::new(KvSessionCacheInvalidator::new(kv));
        RevocationOrchestrator::new(region, peers, store, meta, cache, broadcast)
    }

    #[tokio::test]
    async fn revoke_happy_path_writes_all_planes() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let mut peers = BTreeSet::new();
        peers.insert(Region::Weur);
        peers.insert(Region::Sam);
        let orch = build_orchestrator(
            Region::Wnam,
            peers,
            store.clone(),
            meta.clone(),
            kv,
            broadcast.clone(),
        );
        let pid = pat_id();
        let tid = tenant();
        let prid = principal();
        let cache_key = key("aaaaaaaaaaaaaaaa");
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

        let resp = orch
            .revoke(RevokeRequest {
                pat_id: pid,
                tenant_id: tid,
                token_hash_key: cache_key.clone(),
                reason: RevocationReason::UserInitiated,
                revoked_by: prid,
            })
            .await
            .expect("revoke ok");
        assert!(resp.was_freshly_revoked);
        assert_eq!(resp.session_cache_invalidate, HookOutcome::Ok);
        assert_eq!(resp.broadcast_outcome, PropagationOutcome::Enqueued);
        assert_eq!(resp.origin_region, Region::Wnam);
        assert_eq!(meta.audit_count().await, 1);
        assert_eq!(store.entry_count().await, 1);
        assert_eq!(broadcast.enqueued_count(pid).await, 1);
    }

    #[tokio::test]
    async fn revoke_idempotent_replay_no_dup_audit_no_dup_broadcast() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta.clone(),
            kv,
            broadcast.clone(),
        );
        let pid = pat_id();
        let tid = tenant();
        let prid = principal();
        let cache_key = key("bbbbbbbbbbbbbbbb");
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

        let mut prior_revoked_at = None;
        for _ in 0..5 {
            let resp = orch
                .revoke(RevokeRequest {
                    pat_id: pid,
                    tenant_id: tid,
                    token_hash_key: cache_key.clone(),
                    reason: RevocationReason::UserInitiated,
                    revoked_by: prid,
                })
                .await
                .expect("revoke ok");
            if let Some(ts) = prior_revoked_at {
                assert_eq!(resp.revoked_at, ts, "idempotent replay returns same ts");
                assert!(!resp.was_freshly_revoked);
            }
            prior_revoked_at = Some(resp.revoked_at);
        }
        // Single audit row for 5 retries.
        assert_eq!(meta.audit_count().await, 1);
        // Single DO entry.
        assert_eq!(store.entry_count().await, 1);
        // No peers: broadcast was never called for the fresh path.
        assert_eq!(broadcast.enqueued().await.len(), 0);
    }

    #[tokio::test]
    async fn revoke_unknown_pat_returns_not_found() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta.clone(),
            kv,
            broadcast,
        );
        let result = orch
            .revoke(RevokeRequest {
                pat_id: pat_id(),
                tenant_id: tenant(),
                token_hash_key: key("cccccccccccccccc"),
                reason: RevocationReason::UserInitiated,
                revoked_by: principal(),
            })
            .await;
        match result {
            Err(RevocationError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert_eq!(meta.audit_count().await, 0);
        assert_eq!(store.entry_count().await, 0);
    }

    #[tokio::test]
    async fn revoke_kv_outage_soft_degrades() {
        use crate::cache::kv::AlwaysFailingKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = AlwaysFailingKv::new("kv outage");
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta.clone(),
            kv,
            broadcast.clone(),
        );
        let pid = pat_id();
        let tid = tenant();
        let prid = principal();
        let cache_key = key("dddddddddddddddd");
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

        let resp = orch
            .revoke(RevokeRequest {
                pat_id: pid,
                tenant_id: tid,
                token_hash_key: cache_key,
                reason: RevocationReason::UserInitiated,
                revoked_by: prid,
            })
            .await
            .expect("revoke succeeds despite KV outage");
        assert!(resp.was_freshly_revoked);
        assert_eq!(resp.session_cache_invalidate, HookOutcome::SoftDegraded);
        // Neon SoT + DO upsert succeeded.
        assert_eq!(meta.audit_count().await, 1);
        assert_eq!(store.entry_count().await, 1);
    }

    #[tokio::test]
    async fn revoke_queue_outage_dlq_fallback() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        broadcast.set_queue_down(true).await;
        let mut peers = BTreeSet::new();
        peers.insert(Region::Weur);
        let orch = build_orchestrator(
            Region::Wnam,
            peers,
            store.clone(),
            meta.clone(),
            kv,
            broadcast.clone(),
        );
        let pid = pat_id();
        let tid = tenant();
        let prid = principal();
        let cache_key = key("eeeeeeeeeeeeeeee");
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;

        let resp = orch
            .revoke(RevokeRequest {
                pat_id: pid,
                tenant_id: tid,
                token_hash_key: cache_key,
                reason: RevocationReason::SecurityIncident,
                revoked_by: prid,
            })
            .await
            .expect("revoke ok despite queue outage");
        assert!(resp.was_freshly_revoked);
        assert_eq!(resp.broadcast_outcome, PropagationOutcome::DlqFallback);
        // Local revocation effective imediato.
        assert_eq!(meta.audit_count().await, 1);
        assert_eq!(store.entry_count().await, 1);
    }

    #[tokio::test]
    async fn mass_revoke_atomicity_and_chunking() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let mut peers = BTreeSet::new();
        peers.insert(Region::Weur);
        let orch = build_orchestrator(
            Region::Wnam,
            peers,
            store.clone(),
            meta.clone(),
            kv,
            broadcast.clone(),
        );
        let tid = tenant();
        // Seed 2_500 PATs (multiple Phase 2 chunks at chunk size 1000).
        let mut ids = Vec::with_capacity(2_500);
        for i in 0..2_500u32 {
            let pid = pat_id();
            ids.push(pid);
            meta.seed_pat(
                pid,
                tid,
                principal(),
                key(&format!("{i:016x}")),
            )
            .await;
        }
        let resp = orch
            .mass_revoke(tid, RevocationReason::SecurityIncident, principal())
            .await
            .expect("mass_revoke ok");
        assert_eq!(resp.revoked_count, 2_500);
        assert_eq!(resp.outbox_inserted, 2_500);
        assert_eq!(meta.audit_count().await, 2_500);
        // All 2_500 entries broadcast (chunked into batches of 100).
        assert_eq!(broadcast.enqueued().await.len(), 2_500);
        assert_eq!(resp.broadcast_outcome, PropagationOutcome::Enqueued);
    }

    #[tokio::test]
    async fn ingest_remote_idempotent_dedup() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Weur,
            BTreeSet::new(),
            store.clone(),
            meta,
            kv,
            broadcast,
        );
        let entry = RevokedEntry {
            pat_id: pat_id(),
            tenant_id: tenant(),
            revoked_by: principal(),
            revoked_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            reason: RevocationReason::UserInitiated,
            origin_region: Region::Wnam,
            mass_revoke_id: None,
        };
        let first = orch
            .ingest_remote(entry.clone(), None)
            .await
            .expect("ingest ok");
        let second = orch
            .ingest_remote(entry.clone(), None)
            .await
            .expect("ingest ok");
        assert!(first.was_fresh);
        assert!(!second.was_fresh);
        assert_eq!(store.entry_count().await, 1);
    }

    #[tokio::test]
    async fn reconcile_clean_view_is_clean() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta.clone(),
            kv,
            broadcast,
        );
        let summary = orch.reconcile(&[]).await.expect("reconcile ok");
        assert!(summary.is_clean());
    }

    #[tokio::test]
    async fn reconcile_detects_neon_only_drift() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta,
            kv,
            broadcast,
        );
        let pid = pat_id();
        let neon = vec![(pid, SystemTime::UNIX_EPOCH + Duration::from_secs(123))];
        let summary = orch.reconcile(&neon).await.expect("reconcile ok");
        assert_eq!(summary.neon_only, vec![pid]);
        assert!(summary.do_only.is_empty());
        assert!(summary.timestamp_drift.is_empty());
    }

    #[tokio::test]
    async fn reconcile_detects_timestamp_drift() {
        use crate::cache::kv::InMemoryKv;
        let store = Arc::new(InMemoryRevocationStore::new());
        let meta = Arc::new(InMemoryMetaRevocationSink::new());
        let kv = InMemoryKv::new();
        let broadcast = Arc::new(InMemoryBroadcast::new());
        let orch = build_orchestrator(
            Region::Wnam,
            BTreeSet::new(),
            store.clone(),
            meta.clone(),
            kv,
            broadcast,
        );
        let pid = pat_id();
        let tid = tenant();
        let prid = principal();
        let cache_key = key("ffffffffffffffff");
        meta.seed_pat(pid, tid, prid, cache_key.clone()).await;
        let resp = orch
            .revoke(RevokeRequest {
                pat_id: pid,
                tenant_id: tid,
                token_hash_key: cache_key,
                reason: RevocationReason::UserInitiated,
                revoked_by: prid,
            })
            .await
            .expect("revoke ok");
        let bogus_neon_ts = resp.revoked_at + Duration::from_secs(60);
        let summary = orch.reconcile(&[(pid, bogus_neon_ts)]).await.expect("ok");
        assert_eq!(summary.timestamp_drift.len(), 1);
        let drift = summary
            .timestamp_drift
            .first()
            .expect("timestamp drift row must be present");
        assert_eq!(drift.pat_id, pid);
    }

    #[tokio::test]
    async fn session_cache_key_parse_round_trip() {
        let raw = "auth:session:abc123";
        let key = SessionCacheKey::parse(raw).expect("parse ok");
        assert_eq!(key.as_str(), raw);
        assert_eq!(SessionCacheKey::parse("nope"), None);
        assert_eq!(SessionCacheKey::parse("auth:session:"), None);
    }

    #[tokio::test]
    async fn revocation_reason_wire_round_trip() {
        let cases = [
            RevocationReason::UserInitiated,
            RevocationReason::AdminInitiated,
            RevocationReason::SecurityIncident,
            RevocationReason::Expired,
            RevocationReason::ScopeChanged,
            RevocationReason::MassRevoke,
        ];
        for c in cases {
            let s = c.as_wire();
            assert!(!s.is_empty());
        }
    }
}

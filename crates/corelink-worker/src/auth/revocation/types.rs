//! Canonical revocation types — constants, request/response payloads,
//! revoke reason enum, dedup key, error enum.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1):
//! this file owns the public type surface (no orchestrator, no traits,
//! no in-memory fakes). Every name re-exports through
//! `auth::revocation::*` for path-stable consumers.

use core::fmt;
use std::collections::BTreeSet;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

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

/// Canonical KV-key prefix used by [`crate::auth::revocation::SessionCacheInvalidator`]
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
/// [`crate::auth::revocation::RevocationStore`] (Durable Object surface) per region; broadcast
/// across regions verbatim by [`crate::auth::revocation::RevocationBroadcast`].
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
    /// [`crate::auth::revocation::MetaRevocationSink::revoke`]. The DO storage stores this
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
/// [`crate::auth::revocation::RevocationStore::record_propagation_ack`] as peer regions ack
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
/// the [`crate::auth::revocation::MetaRevocationSink`] inside the Postgres transaction
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

/// Outcome of a [`crate::auth::revocation::MetaRevocationSink::revoke`] call. The two-arm
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

/// Outcome of [`crate::auth::revocation::MetaRevocationSink::mass_revoke`]. The Phase 1
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

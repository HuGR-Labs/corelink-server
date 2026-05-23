//! Error enum + `From` impls + GET/UPDATE result types.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

use thiserror::Error;

use super::super::audit::AuditSinkError;
use super::super::merkle::MerkleError;
use super::super::meta::{AcMetaError, AcMetaRow, AcMetaUpsertOutcome};
use super::super::outputs::OutputsCheckError;
use super::super::sig::SigError;
use super::super::types::{ActionResult, ResultHash};
use crate::cache::negative::NegativeCacheError;
use crate::Region;

/// Canonical AC envelope version byte.
pub const AC_ENVELOPE_VERSION: u8 = 1;

/// Default TTL extension applied on GET refresh-on-hit + UPDATE upsert.
/// Tier-specific overrides land in S-07 / ADR-0019; for S-04 the
/// handler accepts an injected `ttl_extend_ms: Option<u64>` so test
/// boundaries are explicit.
pub const DEFAULT_AC_TTL_EXTEND_MS: u64 = 60 * 60 * 1000; // 1h

/// Errors surfaced by [`super::handler_trait::ActionCacheHandler`] methods.
///
/// 10-variant taxonomy mapping 1:1 to WI §23 + WI §1 `AcError` enum.
/// Maps to canonical `COR_AC_*` error codes per WI §23.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AcError {
    /// `(tenant_id, action_digest)` row absent (or cross-tenant masked
    /// per ADR-0028 uniform-404 freeze).
    /// Maps to 404 + `COR_AC_ACTION_NOT_FOUND` (gRPC NotFound).
    #[error("ac entry not found")]
    NotFound,
    /// Row exists with `expires_at <= now_ms`. Maps to 410 +
    /// `COR_AC_TTL_EXPIRED` (gRPC FailedPrecondition).
    #[error("ac entry expired")]
    Expired,
    /// Sig verification failed mid-flight (CRITICAL — tampering signal).
    /// Maps to 422 + `COR_AC_SIG_INVALID`.
    #[error("ac envelope signature invalid")]
    SigInvalid,
    /// Merkle tree validation rejected the `ActionResult`. Maps to
    /// 422 + `COR_AC_MERKLE_INVALID`.
    #[error("ac merkle invalid: {reason}")]
    MerkleInvalid {
        /// Short canonical reason code from
        /// `MerkleError::audit_code`.
        reason: &'static str,
    },
    /// One or more output blobs are tombstoned / never existed in
    /// `blob_meta`. Maps to 422 + `COR_AC_OUTPUTS_MISSING`.
    #[error("ac outputs missing: {count} digests not alive")]
    OutputsMissing {
        /// How many digests are missing.
        count: usize,
    },
    /// Idempotent re-update with a mismatched `result_hash`. Maps to
    /// 409 + `COR_AC_RESULT_HASH_MISMATCH`.
    #[error("ac result_hash mismatch")]
    ResultHashMismatch {
        /// `result_hash` already on the row.
        existing: ResultHash,
        /// `result_hash` the client attempted to write.
        attempted: ResultHash,
    },
    /// `auth_ctx.scopes()` lacks the required bit. Maps to 403 +
    /// `COR_AUTH_SCOPE_INSUFFICIENT`.
    #[error("ac scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits (canonical `SCOPE_CACHE_R` /
        /// `SCOPE_CACHE_W`).
        required: u64,
    },
    /// Region pinning mismatch — the `AuthCtx` is pinned to a region
    /// different from the handler's. Programmer error. Maps to 500 +
    /// `COR_INTERNAL`.
    #[error("ac handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable (D1 / R2 / KV / sig backend).
    /// Maps to 503 + `COR_AC_BACKEND_UNAVAILABLE`.
    #[error("ac backend unavailable: {0}")]
    BackendUnavailable(String),
    /// Internal handler error. Maps to 500 + `COR_INTERNAL`.
    #[error("ac internal: {0}")]
    Internal(String),
}

impl AcError {
    /// Stable canonical error code (`COR_AC_*` per WI §23). The gRPC
    /// surface wrappers (and the REST mirror; both deferred to the
    /// follow-up integration WI) consume this directly to build the
    /// wire-error envelope.
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::NotFound => "COR_AC_ACTION_NOT_FOUND",
            Self::Expired => "COR_AC_TTL_EXPIRED",
            Self::SigInvalid => "COR_AC_SIG_INVALID",
            Self::MerkleInvalid { .. } => "COR_AC_MERKLE_INVALID",
            Self::OutputsMissing { .. } => "COR_AC_OUTPUTS_MISSING",
            Self::ResultHashMismatch { .. } => "COR_AC_RESULT_HASH_MISMATCH",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_AC_BACKEND_UNAVAILABLE",
            Self::Internal(_) => "COR_INTERNAL",
        }
    }
}

impl From<AcMetaError> for AcError {
    fn from(value: AcMetaError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<OutputsCheckError> for AcError {
    fn from(value: OutputsCheckError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<AuditSinkError> for AcError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<NegativeCacheError> for AcError {
    fn from(value: NegativeCacheError) -> Self {
        // RegionMismatch is the only variant the handler can
        // structurally trigger — it's a programmer wiring error.
        match value {
            NegativeCacheError::RegionMismatch { cache, ctx } => {
                Self::RegionMismatch { handler: cache, ctx }
            }
            other => Self::BackendUnavailable(format!("{other}")),
        }
    }
}

impl From<SigError> for AcError {
    fn from(value: SigError) -> Self {
        match value {
            SigError::Mismatch => Self::SigInvalid,
            SigError::KeyIdReserved => Self::Internal("sig_key_id 0 reserved".to_string()),
            SigError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<MerkleError> for AcError {
    fn from(value: MerkleError) -> Self {
        Self::MerkleInvalid {
            reason: value.audit_code(),
        }
    }
}

/// Outcome of a successful [`super::handler_trait::ActionCacheHandler::get_action_result`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetActionResult {
    /// Verified [`ActionResult`] echoed back to the client.
    pub action_result: ActionResult,
    /// Backing `ac_meta` row with refreshed `last_hit_at_ms`.
    pub row: AcMetaRow,
}

/// Outcome of a successful [`super::handler_trait::ActionCacheHandler::update_action_result`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateActionResult {
    /// Echoed [`ActionResult`] (REAPI conformance — same bytes the
    /// client supplied).
    pub action_result: ActionResult,
    /// Whether this was a fresh INSERT or an idempotent refresh.
    pub upsert_outcome: AcMetaUpsertOutcome,
    /// Backing row.
    pub row: AcMetaRow,
}

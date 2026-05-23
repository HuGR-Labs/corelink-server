//! Error enums for the SplitBlob/SpliceBlob handler.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2).

use thiserror::Error;

use super::super::assembler::AssemblerError;
use super::super::audit::AuditSinkError;
use super::super::chunk_store::ChunkStoreError;
use super::super::session::SessionStoreError;
use crate::Region;

/// Errors surfaced by the SplitBlob flow methods.
///
/// 9-variant taxonomy aligned with WI §1 + §23 error mapping.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SplitError {
    /// `(tenant_id, session_id)` row absent. Maps to 404 +
    /// `COR_MULTIPART_SESSION_NOT_FOUND`.
    #[error("split session not found")]
    SessionNotFound,
    /// Session is already finalized (cannot append/abort). Maps to 409 +
    /// `COR_MULTIPART_SESSION_FINALIZED`.
    #[error("split session already finalized")]
    SessionAlreadyFinalized,
    /// Session is aborted (cannot append/finalize). Maps to 410 +
    /// `COR_MULTIPART_SESSION_ABORTED`.
    #[error("split session aborted")]
    SessionAborted,
    /// `chunk_index` order violation: gap or duplicate-with-different-
    /// bytes. Maps to 422 + `COR_MULTIPART_CHUNK_ORDERING`.
    #[error("chunk ordering violation at index {index}: {reason}")]
    ChunkOrderingViolation {
        /// Offending index.
        index: u32,
        /// Short canonical reason (`"out_of_order"` / `"gap"` /
        /// `"bytes_mismatch"`).
        reason: &'static str,
    },
    /// Chunk bytes exceed [`super::types::MAX_CHUNK_BYTES`]. Maps to 413 +
    /// `COR_MULTIPART_CHUNK_TOO_LARGE`.
    #[error("chunk exceeds max size: {size} > {max}")]
    ChunkTooLarge {
        /// Submitted chunk size.
        size: usize,
        /// Configured max.
        max: usize,
    },
    /// Manifest's chunk count would exceed `MAX_CHUNKS_PER_BLOB`.
    /// Maps to 413 + `COR_MULTIPART_BLOB_TOO_LARGE`.
    #[error("blob exceeds max chunks: {count} > {max}")]
    BlobTooLarge {
        /// Submitted chunk count.
        count: u32,
        /// Configured max.
        max: u32,
    },
    /// `auth_ctx.scopes()` lacks the required bit. Maps to 403 +
    /// `COR_AUTH_SCOPE_INSUFFICIENT`.
    #[error("scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits.
        required: u64,
    },
    /// Region pinning mismatch. Maps to 500 + `COR_INTERNAL`.
    #[error("handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable (D1 / R2 / KV). Maps to 503 +
    /// `COR_MULTIPART_BACKEND_UNAVAILABLE`.
    #[error("multipart backend unavailable: {0}")]
    BackendUnavailable(String),
}

impl SplitError {
    /// Stable canonical error code (`COR_MULTIPART_*` per WI §23).
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::SessionNotFound => "COR_MULTIPART_SESSION_NOT_FOUND",
            Self::SessionAlreadyFinalized => "COR_MULTIPART_SESSION_FINALIZED",
            Self::SessionAborted => "COR_MULTIPART_SESSION_ABORTED",
            Self::ChunkOrderingViolation { .. } => "COR_MULTIPART_CHUNK_ORDERING",
            Self::ChunkTooLarge { .. } => "COR_MULTIPART_CHUNK_TOO_LARGE",
            Self::BlobTooLarge { .. } => "COR_MULTIPART_BLOB_TOO_LARGE",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_MULTIPART_BACKEND_UNAVAILABLE",
        }
    }

    /// Short canonical audit reason — fed into
    /// `BlobAuditRecord::reason` so the chain consumer can pivot on
    /// the failure mode without parsing the message body.
    #[must_use]
    pub const fn audit_reason(&self) -> &'static str {
        match self {
            Self::SessionNotFound => "session_not_found",
            Self::SessionAlreadyFinalized => "session_already_finalized",
            Self::SessionAborted => "session_aborted",
            Self::ChunkOrderingViolation { reason, .. } => reason,
            Self::ChunkTooLarge { .. } => "chunk_too_large",
            Self::BlobTooLarge { .. } => "blob_too_large",
            Self::ScopeInsufficient { .. } => "scope_insufficient",
            Self::RegionMismatch { .. } => "region_mismatch",
            Self::BackendUnavailable(_) => "backend_unavailable",
        }
    }
}

impl From<SessionStoreError> for SplitError {
    fn from(value: SessionStoreError) -> Self {
        match value {
            SessionStoreError::NotFound => Self::SessionNotFound,
            SessionStoreError::AlreadyFinalized => Self::SessionAlreadyFinalized,
            SessionStoreError::Aborted => Self::SessionAborted,
            SessionStoreError::OrderingViolation { index, reason } => {
                Self::ChunkOrderingViolation { index, reason }
            }
            SessionStoreError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<ChunkStoreError> for SplitError {
    fn from(value: ChunkStoreError) -> Self {
        Self::BackendUnavailable(format!("chunk store: {value}"))
    }
}

impl From<AssemblerError> for SplitError {
    fn from(value: AssemblerError) -> Self {
        match value {
            AssemblerError::ChunkOrderingViolation { index, reason } => {
                Self::ChunkOrderingViolation { index, reason }
            }
            other => Self::BackendUnavailable(format!("assembler: {other}")),
        }
    }
}

impl From<AuditSinkError> for SplitError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("audit sink: {value}"))
    }
}

/// Errors surfaced by [`super::handler_trait::SplitSpliceHandler::splice_blob`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpliceError {
    /// Manifest digest absent. Maps to 404 +
    /// `COR_MULTIPART_MANIFEST_NOT_FOUND`.
    #[error("manifest not found")]
    ManifestNotFound,
    /// Per-chunk hash verify failed mid-stream. Stream is cancelled
    /// before any further bytes reach the sink. Maps to 422 +
    /// `COR_MULTIPART_CHUNK_VERIFY_FAILED` (CRITICAL audit emit —
    /// tampering signal).
    #[error("chunk {chunk_index} verification failed")]
    ChunkVerificationFailed {
        /// Offending chunk index in canonical order.
        chunk_index: u32,
    },
    /// Manifest references a chunk that is missing from the chunk
    /// store. Maps to 422 + `COR_MULTIPART_CHUNK_MISSING`.
    #[error("chunk {chunk_index} missing from chunk store")]
    ChunkMissing {
        /// Offending chunk index in canonical order.
        chunk_index: u32,
    },
    /// `auth_ctx.scopes()` lacks `SCOPE_CACHE_R`. Maps to 403.
    #[error("scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits.
        required: u64,
    },
    /// Region pinning mismatch. Maps to 500.
    #[error("handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable. Maps to 503.
    #[error("splice backend unavailable: {0}")]
    BackendUnavailable(String),
}

impl SpliceError {
    /// Stable canonical error code (`COR_MULTIPART_*` per WI §23).
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::ManifestNotFound => "COR_MULTIPART_MANIFEST_NOT_FOUND",
            Self::ChunkVerificationFailed { .. } => "COR_MULTIPART_CHUNK_VERIFY_FAILED",
            Self::ChunkMissing { .. } => "COR_MULTIPART_CHUNK_MISSING",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_MULTIPART_BACKEND_UNAVAILABLE",
        }
    }
}

impl From<AssemblerError> for SpliceError {
    fn from(value: AssemblerError) -> Self {
        match value {
            AssemblerError::ManifestNotFound => Self::ManifestNotFound,
            AssemblerError::ChunkMissing { chunk_index } => Self::ChunkMissing { chunk_index },
            AssemblerError::ChunkVerificationFailed { chunk_index } => {
                Self::ChunkVerificationFailed { chunk_index }
            }
            AssemblerError::ChunkOrderingViolation { .. } => Self::BackendUnavailable(
                "assembler ordering violation on splice path (programmer error)".to_string(),
            ),
            AssemblerError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<ChunkStoreError> for SpliceError {
    fn from(value: ChunkStoreError) -> Self {
        Self::BackendUnavailable(format!("chunk store: {value}"))
    }
}

impl From<AuditSinkError> for SpliceError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("audit sink: {value}"))
    }
}

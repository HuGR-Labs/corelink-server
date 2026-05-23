//! Outcome enums + constants for the SplitBlob/SpliceBlob handler.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2). Pure type surface — no orchestrator logic.

use super::super::types::{ManifestDigest, SessionId};

/// Maximum bytes per chunk submission. Mirrors the chunker default
/// (2 MiB) per ADR-0022 / sprint contract §5.1; oversize submissions
/// are rejected to keep the per-request stack budget bounded.
pub const MAX_CHUNK_BYTES: usize = 2 * 1024 * 1024;

/// Outcome of [`super::handler_trait::SplitSpliceHandler::init_split`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitSplitOutcome {
    /// Fresh session created. The handler emits `blob.split.start`.
    Started {
        /// Server-minted session id (UUIDv7 in production wiring).
        session_id: SessionId,
    },
    /// Session for `(tenant_id, blob_digest)` already exists in the
    /// `Live` state — handler echoes the existing id.
    LiveSessionEcho {
        /// Session id of the previously-started session.
        session_id: SessionId,
    },
    /// Blob is already chunked + finalized — handler echoes the
    /// existing manifest digest without re-running the pipeline.
    AlreadyChunked {
        /// Manifest digest of the cached finalize.
        manifest_digest: ManifestDigest,
        /// Number of chunks bound to the manifest.
        chunk_count: u32,
    },
}

/// Outcome of a successful [`super::handler_trait::SplitSpliceHandler::finalize_split`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizeSplitOutcome {
    /// Manifest digest produced by the assembler.
    pub manifest_digest: ManifestDigest,
    /// Number of chunks bound to the manifest.
    pub chunk_count: u32,
    /// Whether this was a fresh finalize or an idempotent echo of a
    /// prior finalize on the same session.
    pub idempotent: bool,
}

/// Outcome of [`super::handler_trait::SplitSpliceHandler::splice_blob`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpliceOutcome {
    /// Number of chunks streamed (matches manifest's `chunk_count`).
    pub chunks_streamed: u32,
    /// Total bytes streamed (sum of every chunk's size).
    pub bytes_streamed: u64,
}

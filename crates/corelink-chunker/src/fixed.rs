//! Fixed-size chunker (WI-S05-002 §6.1.2).
//!
//! Streaming, pull-based, zero-allocation. The chunker maintains a
//! single staging buffer sized to `chunk_size`; it accepts input
//! slices via [`FixedChunker::feed`] and emits a chunk every
//! `chunk_size` cumulative bytes. Trailing bytes (when the blob length
//! is not a multiple of `chunk_size`) are emitted via
//! [`FixedChunker::finalize`].
//!
//! BLAKE3 hashing happens incrementally via [`blake3::Hasher::update`]
//! during `feed`, with `finalize` called once per emitted chunk —
//! satisfying WI §1.4 "BLAKE3 hash inline".

use crate::bounds::{FIXED_DEFAULT_CHUNK_SIZE, MAX_BLOB_SIZE, MAX_CHUNKS_PER_BLOB};
use crate::chunker::{Chunker, ChunkerStep};
use crate::error::ChunkerError;
use crate::Chunk;

/// Fixed-size chunker — emits a chunk every `chunk_size` cumulative
/// bytes plus a final partial via [`Chunker::finalize`].
#[derive(Debug)]
pub struct FixedChunker {
    /// Bytes per emitted chunk (final chunk may be smaller).
    chunk_size: usize,
    /// Staging buffer holding the partial chunk currently being
    /// assembled. Capacity = `chunk_size`. Cleared at the start of
    /// every `feed` that follows a chunk emission.
    staging: Vec<u8>,
    /// Incremental BLAKE3 hasher for the chunk currently in
    /// `staging`. Reset every emission.
    hasher: blake3::Hasher,
    /// Cumulative input bytes absorbed since the last `reset`.
    bytes_absorbed: u64,
    /// Cumulative chunks emitted since the last `reset`.
    chunks_emitted: u32,
    /// Sticky flag: `true` once the previous `feed` call returned
    /// a `Chunk` arm (so the staging buffer holds the just-emitted
    /// chunk's bytes). The next `feed` MUST drain the staging
    /// buffer before absorbing fresh input. The borrow-check
    /// guarantees the caller has finished consuming the previous
    /// `Chunk<'a>` before re-entering `feed`, so the drain is safe.
    pending_drain: bool,
    /// `true` once `finalize` ran; further `feed`/`finalize` calls
    /// short-circuit to `Error(BlobTooLarge)` (semantic: chunker
    /// cannot accept more input on a finalized blob).
    finalized: bool,
}

impl FixedChunker {
    /// Build a chunker with the canonical default
    /// [`crate::bounds::FIXED_DEFAULT_CHUNK_SIZE`] (2 MiB).
    #[must_use]
    pub fn new_default() -> Self {
        Self::with_size(FIXED_DEFAULT_CHUNK_SIZE)
    }

    /// Build a chunker with a custom `chunk_size`. Caller MUST
    /// guarantee `chunk_size > 0` (validate via
    /// [`crate::ChunkerConfig::validate`] upstream); we tolerate
    /// invalid sizes by clamping to a sane minimum so the
    /// constructor never panics.
    #[must_use]
    pub fn with_size(chunk_size: usize) -> Self {
        let cap = chunk_size.max(1);
        Self {
            chunk_size: cap,
            staging: Vec::with_capacity(cap),
            hasher: blake3::Hasher::new(),
            bytes_absorbed: 0,
            chunks_emitted: 0,
            pending_drain: false,
            finalized: false,
        }
    }

    /// Drain the staging buffer if the previous step was a chunk
    /// emission. Idempotent — `pending_drain == false` short-circuits.
    fn drain_if_pending(&mut self) {
        if self.pending_drain {
            self.staging.clear();
            self.pending_drain = false;
        }
    }
}

impl Chunker for FixedChunker {
    fn feed<'a>(&'a mut self, input: &'a [u8]) -> ChunkerStep<'a> {
        if self.finalized {
            return ChunkerStep::Error(ChunkerError::BlobTooLarge {
                found: self.bytes_absorbed,
                max: MAX_BLOB_SIZE,
            });
        }

        self.drain_if_pending();

        if input.is_empty() {
            return ChunkerStep::NeedMore { consumed: 0 };
        }

        // Bounded-parser checks BEFORE any byte absorption.
        let prospective_total = self.bytes_absorbed.saturating_add(input.len() as u64);
        if prospective_total > MAX_BLOB_SIZE {
            return ChunkerStep::Error(ChunkerError::BlobTooLarge {
                found: prospective_total,
                max: MAX_BLOB_SIZE,
            });
        }

        let needed = self.chunk_size.saturating_sub(self.staging.len());

        if input.len() < needed {
            // Partial: absorb everything, no boundary yet.
            self.staging.extend_from_slice(input);
            self.hasher.update(input);
            self.bytes_absorbed = self.bytes_absorbed.saturating_add(input.len() as u64);
            return ChunkerStep::NeedMore {
                consumed: input.len(),
            };
        }

        // We have enough to fire a boundary. Absorb exactly `needed`
        // bytes; remaining input stays for the caller to re-feed.
        let head = match input.get(..needed) {
            Some(s) => s,
            None => {
                return ChunkerStep::Error(ChunkerError::BlobTooLarge {
                    found: prospective_total,
                    max: MAX_BLOB_SIZE,
                });
            }
        };
        self.staging.extend_from_slice(head);
        self.hasher.update(head);
        self.bytes_absorbed = self.bytes_absorbed.saturating_add(needed as u64);
        self.emit_chunk(needed)
    }

    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>> {
        if self.finalized {
            return None;
        }
        self.drain_if_pending();
        if self.staging.is_empty() {
            self.finalized = true;
            return None;
        }
        if self.chunks_emitted >= MAX_CHUNKS_PER_BLOB {
            self.finalized = true;
            return None;
        }
        let digest = self.hasher.finalize();
        let offset = self.bytes_absorbed.saturating_sub(self.staging.len() as u64);
        let size_bytes = self.staging.len();
        self.chunks_emitted = self.chunks_emitted.saturating_add(1);
        self.finalized = true;
        let mut digest_bytes = [0u8; 32];
        digest_bytes.copy_from_slice(digest.as_bytes());
        Some(Chunk {
            bytes: self.staging.as_slice(),
            digest: digest_bytes,
            offset_in_blob: offset,
            size_bytes,
        })
    }

    fn reset(&mut self) {
        self.staging.clear();
        self.hasher.reset();
        self.bytes_absorbed = 0;
        self.chunks_emitted = 0;
        self.pending_drain = false;
        self.finalized = false;
    }

    fn bytes_absorbed(&self) -> u64 {
        self.bytes_absorbed
    }

    fn chunks_emitted(&self) -> u32 {
        self.chunks_emitted
    }
}

impl FixedChunker {
    /// Emit the chunk that has just filled `staging`. `consumed`
    /// is propagated as the `consumed` field of the returned
    /// `ChunkerStep::Chunk` arm.
    fn emit_chunk<'a>(&'a mut self, consumed: usize) -> ChunkerStep<'a> {
        if self.chunks_emitted >= MAX_CHUNKS_PER_BLOB {
            return ChunkerStep::Error(ChunkerError::TooManyChunks {
                found: u64::from(self.chunks_emitted).saturating_add(1),
                max: MAX_CHUNKS_PER_BLOB,
            });
        }
        let digest = self.hasher.finalize();
        let mut digest_bytes = [0u8; 32];
        digest_bytes.copy_from_slice(digest.as_bytes());
        let offset = self.bytes_absorbed.saturating_sub(self.staging.len() as u64);
        let size_bytes = self.staging.len();
        self.chunks_emitted = self.chunks_emitted.saturating_add(1);
        self.hasher.reset();
        // Mark the staging buffer for drain on the next mutating
        // call. The borrow checker prevents mutation while the
        // returned `Chunk<'a>` is alive, so this is safe.
        self.pending_drain = true;
        ChunkerStep::Chunk {
            chunk: Chunk {
                bytes: self.staging.as_slice(),
                digest: digest_bytes,
                offset_in_blob: offset,
                size_bytes,
            },
            consumed,
        }
    }
}

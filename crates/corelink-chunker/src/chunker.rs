//! Canonical [`Chunker`] trait + dispatch surface (WI-S05-002 §6.1).
//!
//! The trait is the contract every concrete chunker honours
//! ([`crate::fixed::FixedChunker`] /
//! [`crate::fastcdc::FastCdcChunker`]). The shared dispatch enum
//! [`ChunkerKind`] wraps both impls so callers can hold a homogeneous
//! handle without `Box<dyn Chunker>` (which would re-introduce the
//! heap allocation per `feed` the WI §1 source listing comment
//! Lote 10.5bis P0 fix flagged).

use crate::error::ChunkerError;
use crate::{Chunk, ChunkerAlgorithm, ChunkerConfig};

/// Outcome of a single [`Chunker::feed`] call.
///
/// Pull-based + zero-allocation: every variant either emits a chunk
/// borrowing from the chunker's internal staging buffer (lifetime
/// `'a` tied to the `&mut self` borrow) or signals "more input
/// please". A single `feed(input)` call consumes a prefix of `input`
/// (`consumed` bytes); the caller is responsible for re-presenting
/// the unconsumed tail on the next `feed` call.
///
/// Deliberately **NOT** `#[non_exhaustive]`: every caller MUST
/// handle every arm of the state-machine outcome (a forgotten arm
/// is a stuck pipeline). Adding a new variant in a future release
/// is therefore a deliberate breaking change and gates a major
/// version bump (ADR-0039 chunker semver discipline). The
/// forward-compat surface is on the *configuration* types
/// ([`crate::ChunkerConfig`], [`crate::ChunkerAlgorithm`]) and
/// [`crate::ChunkerError`], all `#[non_exhaustive]`.
#[derive(Debug)]
pub enum ChunkerStep<'a> {
    /// A chunk boundary was detected. The chunk borrows from the
    /// chunker's internal buffer; the caller must consume `chunk`
    /// before the next `&mut self` call. `consumed` bytes of the
    /// input slice were absorbed before the boundary fired.
    Chunk {
        /// The emitted chunk (digest + bytes + offset).
        chunk: Chunk<'a>,
        /// Number of bytes from the supplied `input` slice that were
        /// absorbed up to the boundary (could be less than
        /// `input.len()`).
        consumed: usize,
    },

    /// `consumed` bytes of `input` were absorbed but no boundary was
    /// detected — caller must feed more (or `finalize` if EOF).
    NeedMore {
        /// Bytes absorbed; always equals `input.len()` unless
        /// `input` was empty.
        consumed: usize,
    },

    /// Bounded-parser limit hit: the chunker refuses further input
    /// because cumulative size or chunk count would exceed the
    /// canonical bounds (`crate::bounds`). The chunker MUST be
    /// `reset` before reuse.
    Error(ChunkerError),
}

impl ChunkerStep<'_> {
    /// `true` iff this step yielded a chunk (`Chunk` arm).
    #[must_use]
    pub const fn is_chunk(&self) -> bool {
        matches!(self, Self::Chunk { .. })
    }

    /// Bytes absorbed from `input` regardless of arm
    /// (`Chunk.consumed` or `NeedMore.consumed`; `0` on `Error`).
    #[must_use]
    pub const fn consumed(&self) -> usize {
        match self {
            Self::Chunk { consumed, .. } | Self::NeedMore { consumed } => *consumed,
            Self::Error(_) => 0,
        }
    }
}

/// Pull-based content-defined chunker contract.
///
/// # Lifetime contract
///
/// `feed` and `finalize` borrow `&'a mut self`; the returned
/// [`Chunk<'a>`] borrows from the chunker's internal staging buffer.
/// The caller must consume (read `chunk.bytes`, persist the digest)
/// or *copy out* before the next mutating call. Storing a `Chunk`
/// past the next `feed` is a borrow-check error (defense-in-depth
/// against dangling references — `crate::Chunk<'a>::to_owned`
/// available for callers that want a heap-owned copy).
///
/// # Determinism contract
///
/// `INV-CAS-IDEMPOTENCY` + `INV-MULTIPART-CHUNK-DETERMINISTIC`: for
/// any deterministic [`ChunkerConfig`] (= the [`ChunkerConfig::default`]
/// shape with fixed FastCDC mask seeds), feeding the same byte
/// stream MUST produce the same chunk boundaries + digests
/// byte-for-byte. The 10k-iter property suite under `tests/` pins
/// this contract.
pub trait Chunker {
    /// Feed `input`; return a [`ChunkerStep`] describing the outcome.
    /// `input.len() == 0` is legal (returns `NeedMore { consumed: 0 }`)
    /// so callers can probe without consuming bytes.
    fn feed<'a>(&'a mut self, input: &'a [u8]) -> ChunkerStep<'a>;

    /// Emit any remaining buffered bytes as a final partial chunk.
    /// Returns `None` if the chunker has no buffered bytes (empty
    /// input or boundary just fired). Once `finalize` returns
    /// `Some(chunk)` (or `None` on the empty-input fast path), the
    /// chunker is in the *finalized* state: subsequent `feed` /
    /// `finalize` calls return [`ChunkerError::BlobTooLarge`] until
    /// `reset` is invoked.
    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>>;

    /// Reset the chunker to the initial state for reuse on a fresh
    /// blob. Drops any buffered bytes; resets cumulative counters.
    fn reset(&mut self);

    /// Total bytes the chunker has absorbed since the last `reset`.
    /// Useful for instrumentation; MUST monotonically increase.
    fn bytes_absorbed(&self) -> u64;

    /// Total chunks the chunker has emitted since the last `reset`.
    fn chunks_emitted(&self) -> u32;
}

/// Static-dispatch chunker handle wrapping either concrete impl.
///
/// Constructors validate the config; runtime selection of the
/// algorithm is via [`ChunkerKind::new`]. Polymorphism is realised
/// through enum dispatch (no heap allocation, no `Box<dyn Chunker>`
/// — Lote 10.5bis P0 fix preserves the zero-allocation contract).
#[derive(Debug)]
#[non_exhaustive]
pub enum ChunkerKind {
    /// [`crate::fixed::FixedChunker`].
    Fixed(crate::fixed::FixedChunker),
    /// [`crate::fastcdc::FastCdcChunker`].
    FastCdc(crate::fastcdc::FastCdcChunker),
}

impl ChunkerKind {
    /// Build a chunker for `config`. Validates the config; returns
    /// [`ChunkerError::FastCdcConfigInvalid`] /
    /// [`ChunkerError::FixedChunkSizeInvalid`] on bad inputs.
    pub fn new(config: ChunkerConfig) -> Result<Self, ChunkerError> {
        config.validate()?;
        Ok(match config.algorithm {
            ChunkerAlgorithm::Fixed2MiB => {
                Self::Fixed(crate::fixed::FixedChunker::with_size(config.fixed_chunk_size))
            }
            ChunkerAlgorithm::FastCDC2MiB => Self::FastCdc(crate::fastcdc::FastCdcChunker::new(
                config.fastcdc_min,
                config.fastcdc_avg,
                config.fastcdc_max,
                config.fastcdc_mask_s,
                config.fastcdc_mask_l,
            )),
        })
    }
}

impl Chunker for ChunkerKind {
    fn feed<'a>(&'a mut self, input: &'a [u8]) -> ChunkerStep<'a> {
        match self {
            Self::Fixed(c) => c.feed(input),
            Self::FastCdc(c) => c.feed(input),
        }
    }

    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>> {
        match self {
            Self::Fixed(c) => c.finalize(),
            Self::FastCdc(c) => c.finalize(),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Fixed(c) => c.reset(),
            Self::FastCdc(c) => c.reset(),
        }
    }

    fn bytes_absorbed(&self) -> u64 {
        match self {
            Self::Fixed(c) => c.bytes_absorbed(),
            Self::FastCdc(c) => c.bytes_absorbed(),
        }
    }

    fn chunks_emitted(&self) -> u32 {
        match self {
            Self::Fixed(c) => c.chunks_emitted(),
            Self::FastCdc(c) => c.chunks_emitted(),
        }
    }
}

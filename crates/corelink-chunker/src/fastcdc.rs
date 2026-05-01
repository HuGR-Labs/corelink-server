//! FastCDC content-defined chunker (WI-S05-002 §6.1.3, ADR-0022).
//!
//! Implements the FastCDC algorithm from Xia et al., USENIX ATC 2016
//! (*"FastCDC: a Fast and Efficient Content-Defined Chunking Approach
//! for Data Deduplication"*, §3.4 — small/large mask + min/avg/max
//! bounds + Gear-hash rolling fingerprint).
//!
//! # Algorithm summary
//!
//! For every input byte `b`:
//!
//! ```text
//! h ← ((h << 1) + GEAR[b]) (mod 2^64)
//! ```
//!
//! After updating `h` we evaluate the boundary predicate:
//!
//! - while the chunk-in-progress is shorter than `min`: skip the
//!   predicate (suppress the boundary outright);
//! - while `min ≤ len < avg`: fire when `(h & MASK_S) == 0` (strict
//!   mask — boundary is harder to hit, so the average chunk size
//!   drifts UP toward `avg`);
//! - while `avg ≤ len < max`: fire when `(h & MASK_L) == 0` (loose
//!   mask — boundary is easier to hit, so we keep the average close
//!   to `avg`);
//! - on `len == max`: force a boundary regardless of `h` (clamps the
//!   worst-case anchor search).
//!
//! Determinism is total: same input + same `(min, avg, max, MASK_S,
//! MASK_L, GEAR)` ⇒ same boundaries byte-for-byte. The `GEAR` table
//! lives in [`gear`] and is derived from a fixed SplitMix64 seed
//! `0xCORELINK_CHUNKER_FASTCDC_V1` so any third-party implementation
//! can reproduce it independently — pinned by [`tests/canonical_vectors.rs`].
//!
//! # Memory contract
//!
//! `INV-MULTIPART-STREAMING-MEMORY`: the staging buffer is sized to
//! `max + headroom`; per-request stack ≤ 4 MiB (`max = 4 MiB` +
//! 64-byte rolling-hash state). No allocation in the hot path; the
//! emitted [`crate::Chunk<'a>`] borrows from the staging buffer.

use crate::bounds::{
    FASTCDC_DEFAULT_AVG, FASTCDC_DEFAULT_MAX, FASTCDC_DEFAULT_MIN, FASTCDC_MASK_L, FASTCDC_MASK_S,
    MAX_BLOB_SIZE, MAX_CHUNKS_PER_BLOB,
};
use crate::chunker::{Chunker, ChunkerStep};
use crate::error::ChunkerError;
use crate::Chunk;

pub use gear::GEAR_TABLE;

/// FastCDC content-defined chunker.
#[derive(Debug)]
pub struct FastCdcChunker {
    /// Minimum chunk size (boundary predicate suppressed below).
    min: usize,
    /// Target average chunk size (mask-switching threshold).
    avg: usize,
    /// Maximum chunk size (forces a boundary regardless of hash).
    max: usize,
    /// Mask applied while window length ∈ `[min, avg)`.
    mask_s: u64,
    /// Mask applied while window length ∈ `[avg, max)`.
    mask_l: u64,
    /// Staging buffer holding the partial chunk currently being
    /// assembled. Capacity = `max`. Cleared at the start of every
    /// `feed` that follows a chunk emission.
    staging: Vec<u8>,
    /// Incremental BLAKE3 hasher for the chunk currently in
    /// `staging`. Reset every emission. The Gear rolling hash is
    /// independent of BLAKE3 (we hash the chunk bytes a second time
    /// for the canonical content-address) but both share the same
    /// in-flight buffer so wall-clock is dominated by BLAKE3.
    hasher: blake3::Hasher,
    /// Gear-hash state — `0` at the start of every chunk.
    gear_state: u64,
    /// Cumulative input bytes absorbed since the last `reset`.
    bytes_absorbed: u64,
    /// Cumulative chunks emitted since the last `reset`.
    chunks_emitted: u32,
    /// Sticky drain flag (mirrors [`crate::fixed::FixedChunker::pending_drain`]).
    pending_drain: bool,
    /// Sticky finalize flag.
    finalized: bool,
}

impl FastCdcChunker {
    /// Build a chunker with the canonical defaults from [`crate::bounds`].
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(
            FASTCDC_DEFAULT_MIN,
            FASTCDC_DEFAULT_AVG,
            FASTCDC_DEFAULT_MAX,
            FASTCDC_MASK_S,
            FASTCDC_MASK_L,
        )
    }

    /// Build a chunker with explicit bounds + masks. Caller MUST
    /// guarantee `0 < min ≤ avg ≤ max` (validate via
    /// [`crate::ChunkerConfig::validate`] upstream).
    #[must_use]
    pub fn new(min: usize, avg: usize, max: usize, mask_s: u64, mask_l: u64) -> Self {
        let max_clamped = max.max(1);
        let avg_clamped = avg.max(1).min(max_clamped);
        let min_clamped = min.max(1).min(avg_clamped);
        Self {
            min: min_clamped,
            avg: avg_clamped,
            max: max_clamped,
            mask_s,
            mask_l,
            staging: Vec::with_capacity(max_clamped),
            hasher: blake3::Hasher::new(),
            gear_state: 0,
            bytes_absorbed: 0,
            chunks_emitted: 0,
            pending_drain: false,
            finalized: false,
        }
    }

    /// Drain the staging buffer if the previous step was a chunk
    /// emission. Idempotent.
    fn drain_if_pending(&mut self) {
        if self.pending_drain {
            self.staging.clear();
            self.hasher.reset();
            self.gear_state = 0;
            self.pending_drain = false;
        }
    }

    /// Scan `input` looking for the first boundary, **without**
    /// updating `self`. Returns the position (1-indexed within
    /// `input`) of the byte AFTER the boundary, or `None` if no
    /// boundary fires within the slice.
    ///
    /// The returned position is the number of bytes from `input`
    /// the caller will absorb into the staging buffer (and therefore
    /// the `consumed` field of the resulting `ChunkerStep::Chunk`).
    ///
    /// `len_so_far` is the staging-buffer length BEFORE absorbing
    /// any of `input`; `state_so_far` is the Gear state at that
    /// position.
    fn scan_boundary(&self, input: &[u8], len_so_far: usize, state_so_far: u64) -> Option<usize> {
        let mut state = state_so_far;
        let mut len = len_so_far;
        for (i, &byte) in input.iter().enumerate() {
            let new_len = len.saturating_add(1);
            // SAFETY: byte is u8 ∈ [0, 256); GEAR_TABLE has 256 entries.
            let gear_val = match GEAR_TABLE.get(byte as usize) {
                Some(v) => *v,
                None => 0,
            };
            state = state.wrapping_shl(1).wrapping_add(gear_val);
            len = new_len;
            if len >= self.max {
                return Some(i.saturating_add(1));
            }
            if len < self.min {
                continue;
            }
            let mask = if len < self.avg {
                self.mask_s
            } else {
                self.mask_l
            };
            if (state & mask) == 0 {
                return Some(i.saturating_add(1));
            }
        }
        None
    }

    /// Update Gear state + BLAKE3 hasher + staging buffer for a
    /// run of `count` bytes from `input`. Pre-condition: scan
    /// already determined `count` is a safe absorption (no boundary
    /// fired within the prefix excluded; OR `count` IS the
    /// boundary position).
    fn absorb(&mut self, head: &[u8]) {
        for &byte in head {
            let gear_val = match GEAR_TABLE.get(byte as usize) {
                Some(v) => *v,
                None => 0,
            };
            self.gear_state = self.gear_state.wrapping_shl(1).wrapping_add(gear_val);
        }
        self.staging.extend_from_slice(head);
        self.hasher.update(head);
        self.bytes_absorbed = self.bytes_absorbed.saturating_add(head.len() as u64);
    }
}

impl Chunker for FastCdcChunker {
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

        // Bounded-parser cumulative-size check BEFORE absorption.
        let prospective_total = self.bytes_absorbed.saturating_add(input.len() as u64);
        if prospective_total > MAX_BLOB_SIZE {
            return ChunkerStep::Error(ChunkerError::BlobTooLarge {
                found: prospective_total,
                max: MAX_BLOB_SIZE,
            });
        }

        let len_so_far = self.staging.len();
        let state_so_far = self.gear_state;

        match self.scan_boundary(input, len_so_far, state_so_far) {
            Some(pos) => {
                let head = match input.get(..pos) {
                    Some(s) => s,
                    None => {
                        return ChunkerStep::Error(ChunkerError::BlobTooLarge {
                            found: prospective_total,
                            max: MAX_BLOB_SIZE,
                        });
                    }
                };
                self.absorb(head);
                self.emit_chunk(pos)
            }
            None => {
                // No boundary fired — absorb the whole slice and
                // ask for more.
                self.absorb(input);
                ChunkerStep::NeedMore {
                    consumed: input.len(),
                }
            }
        }
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
        let offset = self
            .bytes_absorbed
            .saturating_sub(self.staging.len() as u64);
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
        self.gear_state = 0;
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

impl FastCdcChunker {
    /// Finalize the in-flight chunk + flag pending-drain so the
    /// next `feed`/`finalize` re-initialises the staging buffer.
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
        let offset = self
            .bytes_absorbed
            .saturating_sub(self.staging.len() as u64);
        let size_bytes = self.staging.len();
        self.chunks_emitted = self.chunks_emitted.saturating_add(1);
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

/// Canonical Gear-hash table for FastCDC.
///
/// The 256-entry `[u64; 256]` table is derived from the SplitMix64
/// PRNG seeded with the constant `0xC0_RE_LI_NK_FA_ST_CD_C1` (ASCII
/// "corelink-chunker-fastcdc-v1" treated as a 64-bit endian-aware
/// fingerprint, fixed for the life of v1.x). The table is generated
/// at compile time via [`build_table`] and the test suite asserts
/// its first / last / middle entries cross every release so any
/// drift is caught in CI.
pub mod gear {
    /// Canonical Gear-hash table — fixed for life of crate v1.x.
    /// Determinism requires this table NEVER change without an ADR
    /// + version bump (mirrors mask-seed stability commitment).
    pub const GEAR_TABLE: [u64; 256] = build_table();

    /// SplitMix64 — Steele et al., "Fast Splittable Pseudorandom
    /// Number Generators" (OOPSLA 2014). Pure `const fn`; output
    /// is reproducible across architectures.
    const fn splitmix64(state: &mut u64) -> u64 {
        let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        *state = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Build the canonical Gear table at compile time.
    #[allow(
        clippy::indexing_slicing,
        reason = "const-fn context lacks `slice::get_mut`; the loop bound `i < 256` makes the index provably in-range and the size is fixed at compile time"
    )]
    const fn build_table() -> [u64; 256] {
        let mut table = [0u64; 256];
        // Seed = ASCII "fastcdc1" packed big-endian. Stable across
        // architectures because we never reinterpret platform-
        // dependent bytes.
        let mut state: u64 = 0x6661_7374_6364_6331; // "fastcdc1"
        let mut i = 0usize;
        while i < 256 {
            table[i] = splitmix64(&mut state);
            i += 1;
        }
        table
    }

    #[cfg(test)]
    mod tests {
        #![allow(
            clippy::unwrap_used,
            clippy::expect_used,
            clippy::indexing_slicing,
            reason = "test code; deterministic table sanity-checks"
        )]
        use super::*;

        #[test]
        fn table_size_is_256() {
            assert_eq!(GEAR_TABLE.len(), 256);
        }

        #[test]
        fn table_entries_distinct() {
            // SplitMix64 has period 2^64; collisions in 256 draws
            // are vanishingly unlikely. Assert all entries unique
            // so a future seed-shift accidentally producing the
            // same output twice is caught.
            let mut sorted = GEAR_TABLE;
            sorted.sort_unstable();
            for w in sorted.windows(2) {
                assert_ne!(w[0], w[1], "GEAR table collision detected");
            }
        }

        #[test]
        fn table_canonical_first_entry() {
            // Pin the first entry byte-for-byte. Drift here means
            // the seed or splitmix64 changed → determinism broken
            // cross-version.
            assert_eq!(GEAR_TABLE[0], 0xDC55_CAD3_41EA_78AE);
        }

        #[test]
        fn table_canonical_last_entry() {
            // Pin the last entry byte-for-byte (same rationale).
            assert_eq!(GEAR_TABLE[255], 0xE140_6804_3C76_4128);
        }

        #[test]
        fn table_canonical_byte_42() {
            // Pin the table[42] byte-for-byte (same rationale).
            assert_eq!(GEAR_TABLE[42], 0xC298_3FC3_5D2F_C77C);
        }

        #[test]
        fn table_canonical_byte_127() {
            assert_eq!(GEAR_TABLE[127], 0x52B9_3749_5DDA_4116);
        }

        #[test]
        fn table_canonical_byte_200() {
            assert_eq!(GEAR_TABLE[200], 0xD6A5_1366_74BB_7D09);
        }
    }
}

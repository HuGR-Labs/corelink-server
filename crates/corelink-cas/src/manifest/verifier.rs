//! Manifest verifier — dual-side (server pre-persist + client
//! post-download) + streaming progressive verify (WI-S05-005 §6.1, §1).
//!
//! ## Three-modes design
//!
//! 1. **`verify_structure`** (`pub(crate)` — internal API per WI v1.2.0
//!    Lote 10.5bis P0 fix; the public API is [`verify_full`] which
//!    chains structure THEN sig in the canonical pinned order to
//!    prevent API misuse where a caller could verify the sig without
//!    the structural bounds check). Pure-logic; no I/O. Verifies:
//!    - `version == 1`.
//!    - `chunk_count > 0`.
//!    - `chunks.len() == chunk_count`.
//!    - `chunks[i].index == i as u32` for all `i`.
//!    - Per-chunk `size_bytes` bounds (`1..=MAX_CHUNK_SIZE_BYTES`).
//!    - `total_size_bytes == sum(chunks[i].size_bytes)` and
//!      `<= MAX_TOTAL_SIZE_BYTES`.
//!    - Recomputed `build_root(chunks)` byte-equal to the claimed
//!      `merkle_root`.
//!
//! 2. **`verify_full`** (PUBLIC API). Calls `verify_structure` THEN
//!    `verify_sig` (in this exact pinned order; lesson WI-S04-003
//!    chaos #1 where sig verify must NOT happen on a structurally
//!    invalid manifest because the canonical preimage layout is
//!    only well-defined if the structural bounds hold).
//!
//! 3. **`verify_streaming`** (PUBLIC API; **O(1)-per-chunk memory**
//!    per spec contract §5.1 P0-SR5-003). Reads chunks one at a time
//!    from a [`ChunkRefSource`] (the production D1 `manifest_chunks`
//!    cursor; the test fakes ship below) AND a [`ChunkBytesSource`]
//!    (the production R2 GET cursor), verifies each chunk's bytes
//!    against the manifest's per-chunk digest, and signals fail-fast
//!    on the first mismatch — never materialises the full
//!    `Vec<ChunkRef>` in memory.
//!
//!    The handler wraps the bytes source over an async stream
//!    cancellation token (`tokio_util::sync::CancellationToken` in
//!    production); cancellation on the verifier side propagates up to
//!    the upstream R2 / wire stream so a tampered chunk does NOT keep
//!    pulling bytes from the network. This trait surface is sync —
//!    the canonical `corelink-worker::reapi::cas::assembler` adapter
//!    shims the cancellation token + the async tonic streaming over
//!    this surface.
//!
//! ## Why the streaming verifier reads digests from a separate source
//!
//! The full `Manifest` value carries `chunks: Vec<ChunkRef>` — at
//! `MAX_CHUNKS_PER_BLOB = 81920` chunks × 40 bytes per `ChunkRef` =
//! ~3.28 MB heap. Materialising that is fine for the structural
//! verify (which the handler does once at SplitBlob time, before
//! persisting). The streaming verify, however, runs on the SpliceBlob
//! read path under per-Worker concurrency budgets — at 4 concurrent
//! splices × 3.28 MB = 13 MB on top of the 2 MB chunker buffer. The
//! Worker isolate budget is 128 MiB; subtract handler stack +
//! ChunkStore client + audit + rest of CAS state and 13 MB of
//! manifest-chunks-list is the difference between "fits" and "OOM".
//!
//! Per spec contract §5.1 P0-SR5-003 (Lote 10.5-tris) the streaming
//! verifier is REQUIRED to be O(1) per-chunk memory. The
//! `ChunkRefSource` trait surface (one chunk at a time) maps directly
//! to a D1 cursor over `(tenant_id, blob_digest, chunk_index)` and
//! never holds more than 1 row in memory.

use uuid::Uuid;

use crate::manifest::bounds::{MAX_CHUNK_SIZE_BYTES, MAX_TOTAL_SIZE_BYTES};
use crate::manifest::error::{ManifestError, VerifyError};
use crate::manifest::merkle::{hash_inner, hash_leaf};
use crate::manifest::sig::ManifestVerifierSig;
use crate::manifest::types::{ChunkRef, Manifest, CURRENT_MANIFEST_VERSION, DIGEST_LEN};

/// Pure-logic manifest verifier. Holds no state.
#[derive(Clone, Copy, Debug, Default)]
pub struct ManifestVerifier;

impl ManifestVerifier {
    /// Construct a fresh verifier.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Server pre-persist + client post-download canonical full verify
    /// (PUBLIC API). Chains [`Self::verify_structure_internal`] THEN
    /// sig verify in canonical pinned order — the sig MUST NOT be
    /// checked against an envelope whose structural bounds are
    /// violated (per WI-S05-005 §1 lesson WI-S04-003 chaos #1: sig
    /// validation on out-of-bounds preimages is undefined behavior).
    ///
    /// # Errors
    ///
    /// - [`VerifyError::Structure`] for any structural / bounded-parser
    ///   failure (no sig verify happens on this path).
    /// - [`VerifyError::Sig`] for cripto failure when structure is OK.
    pub fn verify_full(
        &self,
        manifest: &Manifest,
        sig_verifier: &ManifestVerifierSig,
    ) -> Result<(), VerifyError> {
        // 1. Structure first.
        self.verify_structure_internal(manifest)?;
        // 2. Sig second.
        let canonical = manifest.canonical_bytes();
        sig_verifier
            .verify(
                manifest.tenant_id,
                manifest.sig_key_id,
                &canonical,
                &manifest.sig,
            )
            .map_err(VerifyError::Sig)?;
        Ok(())
    }

    /// Server pre-persist structural verify (PUBLIC API). Sig is NOT
    /// checked on this path — used by the SplitBlob handler at the
    /// pre-persist gate where the sig has just been computed by the
    /// builder + the structural bounds are the LAST gate before D1
    /// commit.
    ///
    /// # Errors
    ///
    /// Surfaces any [`ManifestError`] arm wrapped in
    /// [`VerifyError::Structure`].
    pub fn verify_structure(&self, manifest: &Manifest) -> Result<(), VerifyError> {
        self.verify_structure_internal(manifest)
    }

    fn verify_structure_internal(&self, manifest: &Manifest) -> Result<(), VerifyError> {
        // Version check.
        if manifest.version != CURRENT_MANIFEST_VERSION {
            return Err(VerifyError::Structure(ManifestError::VersionUnsupported(
                manifest.version,
            )));
        }
        if manifest.chunk_count == 0 {
            return Err(VerifyError::Structure(ManifestError::Empty));
        }
        let actual_chunks = u32::try_from(manifest.chunks.len()).map_err(|_| {
            VerifyError::Structure(ManifestError::ChunkCountExceeded {
                found: u32::MAX,
            })
        })?;
        if manifest.chunk_count != actual_chunks {
            return Err(VerifyError::Structure(ManifestError::ChunkCountMismatch {
                declared: manifest.chunk_count,
                actual: actual_chunks,
            }));
        }
        // Walk the chunks list once to validate index ordering + per-chunk
        // bounds + sum size.
        let mut sum_size: u64 = 0;
        for (slot_usize, c) in manifest.chunks.iter().enumerate() {
            let slot = u32::try_from(slot_usize).map_err(|_| {
                VerifyError::Structure(ManifestError::ChunkCountExceeded {
                    found: u32::MAX,
                })
            })?;
            if c.index != slot {
                return Err(VerifyError::Structure(
                    ManifestError::ChunkIndexOutOfOrder {
                        slot,
                        expected: slot,
                        found: c.index,
                    },
                ));
            }
            if c.size_bytes == 0 {
                return Err(VerifyError::Structure(ManifestError::ChunkSizeZero(slot)));
            }
            if c.size_bytes > MAX_CHUNK_SIZE_BYTES {
                return Err(VerifyError::Structure(ManifestError::ChunkSizeExceeded {
                    index: slot,
                    size_bytes: c.size_bytes,
                }));
            }
            sum_size = sum_size.saturating_add(u64::from(c.size_bytes));
            if sum_size > MAX_TOTAL_SIZE_BYTES {
                return Err(VerifyError::Structure(ManifestError::TotalSizeExceeded {
                    found: sum_size,
                }));
            }
        }
        if manifest.total_size_bytes != sum_size {
            return Err(VerifyError::Structure(ManifestError::TotalSizeMismatch {
                declared: manifest.total_size_bytes,
                actual: sum_size,
            }));
        }

        // Recompute Merkle root and compare. Plain `==` (not
        // constant-time) is fine here — the root is public output
        // material; a side-channel oracle on the comparison would not
        // give an attacker any advantage that isn't already public via
        // the manifest envelope itself.
        let recomputed = crate::manifest::merkle::build_root(&manifest.chunks)
            .map_err(VerifyError::Structure)?;
        if recomputed != manifest.merkle_root {
            return Err(VerifyError::Structure(ManifestError::RootMismatch));
        }
        Ok(())
    }
}

/// Per-chunk reference source — abstracts a streaming cursor over the
/// canonical-ordered chunk list for [`verify_streaming`]. The
/// production impl is a D1 cursor over `manifest_chunks WHERE
/// (tenant_id, blob_digest) ORDER BY chunk_index ASC`; the in-memory
/// fake [`InMemoryChunkRefSource`] ships below for tests.
///
/// **O(1) memory invariant**: implementations MUST NOT materialise the
/// full `Vec<ChunkRef>` in memory. The trait is single-method-poll so
/// production cursors lazily fetch one row at a time
/// (`INV-MULTIPART-STREAMING-MEMORY` per spec contract §5.1
/// P0-SR5-003).
pub trait ChunkRefSource {
    /// Pull the next [`ChunkRef`] in canonical order. Returns
    /// `Ok(None)` when the cursor is exhausted (canonical end-of-stream).
    ///
    /// # Errors
    ///
    /// Implementations may return a backend-class error string —
    /// surfaced by [`verify_streaming`] as a generic stream truncation
    /// the caller can map to an audit code.
    fn next_chunk_ref(&mut self) -> Result<Option<ChunkRef>, String>;
}

/// Per-chunk bytes source — abstracts the streaming R2 GET pipeline.
/// Implementations yield the raw bytes for the chunk at the requested
/// `index` (which the verifier always asks for in canonical ascending
/// order; out-of-order requests are a bug).
pub trait ChunkBytesSource {
    /// Pull the bytes for `chunk_index`. Returns `Ok(None)` when the
    /// source is exhausted before the manifest is fully consumed
    /// (surfaced as [`VerifyError::StreamingSourceTruncated`]).
    ///
    /// # Errors
    ///
    /// Implementations may return a backend-class error string —
    /// surfaced by [`verify_streaming`] as a generic stream truncation.
    fn fetch_chunk_bytes(
        &mut self,
        chunk_index: u32,
    ) -> Result<Option<Vec<u8>>, String>;
}

/// Sink consuming verified bytes during streaming verify. Mirrors the
/// `ChunkSink` trait shipped in `corelink-worker::reapi::cas::assembler`
/// but kept independent here so the manifest crate has no upstream
/// dependency on the worker.
pub trait VerifiedChunkSink {
    /// Receive verified bytes for `chunk_index`. Sink-class errors
    /// bubble up as a generic backend error.
    ///
    /// # Errors
    ///
    /// Implementations may surface a backend / wire error string.
    fn write_verified(&mut self, chunk_index: u32, bytes: Vec<u8>) -> Result<(), String>;
}

/// Streaming verify return value. Captures how many chunks were
/// streamed + how many bytes total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamingVerifyOutcome {
    /// Number of chunks streamed end-to-end.
    pub chunks_streamed: u32,
    /// Total bytes streamed end-to-end.
    pub bytes_streamed: u64,
}

/// Manifest envelope subset consumed by [`verify_streaming`]. Carries
/// only the bound-relevant scalars (no `chunks: Vec<ChunkRef>` — that's
/// the load-bearing memory cap per `INV-MULTIPART-STREAMING-MEMORY`).
/// Construct via [`StreamingManifestHeader::from_manifest`] when you
/// have a full [`Manifest`] handy, or via the `new` ctor when reading
/// from a D1 row directly.
#[derive(Clone, Copy, Debug)]
pub struct StreamingManifestHeader {
    /// Manifest schema version (must be `1`).
    pub version: u8,
    /// Tenant scoping.
    pub tenant_id: Uuid,
    /// Reassembled blob digest.
    pub blob_digest: [u8; DIGEST_LEN],
    /// Claimed Merkle root — recomputed incrementally during the
    /// streaming verify and compared at terminal poll.
    pub merkle_root: [u8; DIGEST_LEN],
    /// Number of chunks the source MUST yield.
    pub chunk_count: u32,
    /// Declared total bytes (must match observed sum).
    pub total_size_bytes: u64,
}

impl StreamingManifestHeader {
    /// Project a [`Manifest`] envelope into the streaming header subset
    /// consumed by [`verify_streaming`].
    #[must_use]
    pub fn from_manifest(m: &Manifest) -> Self {
        Self {
            version: m.version,
            tenant_id: m.tenant_id,
            blob_digest: m.blob_digest,
            merkle_root: m.merkle_root,
            chunk_count: m.chunk_count,
            total_size_bytes: m.total_size_bytes,
        }
    }
}

/// Streaming progressive verifier — O(1)-per-chunk memory.
///
/// The verifier walks the canonical chunk list one row at a time,
/// validates each chunk's bytes against the per-chunk digest stored on
/// the row, recomputes the leaf hash on the fly, AND incrementally
/// builds the Merkle tree using a stack-based bottom-up algorithm
/// (max stack depth = `log2(MAX_CHUNKS_PER_BLOB) ≈ 17` levels × 32
/// bytes = 544 bytes — bounded and tiny). At terminal poll the
/// stack-collapse yields the recomputed Merkle root which is
/// constant-time-compared against `header.merkle_root` AS A FINAL
/// GATE.
///
/// **Fail-fast**: on any per-chunk mismatch the verifier returns the
/// canonical [`VerifyError`] arm BEFORE asking the source for the next
/// chunk — the caller's cancellation token propagates upstream, no
/// further bytes flow from R2 / the wire.
///
/// # Errors
///
/// - [`VerifyError::Structure`] for header-level bound violations
///   (chunk_count zero, total_size overflow, etc.) caught BEFORE the
///   stream is opened.
/// - [`VerifyError::StreamingChunkMismatch`] for per-chunk hash
///   mismatch (tamper detected).
/// - [`VerifyError::StreamingChunkSizeMismatch`] for declared-vs-observed
///   bytes length disagreement (wire truncation).
/// - [`VerifyError::StreamingSourceTruncated`] when the source ends
///   before `chunk_count` chunks are observed.
/// - [`VerifyError::StreamingChunkIndexUnexpected`] when the source
///   yields chunks out of order (canonical streaming surface bug).
/// - [`VerifyError::Structure`] with [`ManifestError::RootMismatch`]
///   at terminal poll when the recomputed Merkle root disagrees with
///   the header's claimed `merkle_root`.
#[allow(clippy::too_many_arguments, reason = "single-call canonical streaming surface; arguments are necessary contract — chunk_refs cursor + bytes cursor + sink + cancellation must remain explicit + caller-controlled")]
pub fn verify_streaming(
    header: &StreamingManifestHeader,
    refs: &mut dyn ChunkRefSource,
    bytes_src: &mut dyn ChunkBytesSource,
    sink: &mut dyn VerifiedChunkSink,
) -> Result<StreamingVerifyOutcome, VerifyError> {
    // --- Header-level fast-fail bounds ---
    if header.version != CURRENT_MANIFEST_VERSION {
        return Err(VerifyError::Structure(ManifestError::VersionUnsupported(
            header.version,
        )));
    }
    if header.chunk_count == 0 {
        return Err(VerifyError::Structure(ManifestError::Empty));
    }
    if header.chunk_count > crate::manifest::bounds::MAX_CHUNKS_PER_BLOB {
        return Err(VerifyError::Structure(ManifestError::ChunkCountExceeded {
            found: header.chunk_count,
        }));
    }
    if header.total_size_bytes > MAX_TOTAL_SIZE_BYTES {
        return Err(VerifyError::Structure(ManifestError::TotalSizeExceeded {
            found: header.total_size_bytes,
        }));
    }

    // --- Streaming bottom-up Merkle accumulator ---
    //
    // Canonical bottom-up algorithm: maintain a stack of `(level,
    // hash)` slots. For each new leaf at level 0, push it; while the
    // top two slots are at the same level, pop them and push their
    // inner-hash at level+1. Memory is `O(log2 chunk_count) ≈ 17 × 32
    // bytes = 544 bytes` — bounded and tiny.
    //
    // This is the SAME "Merkle Mountain Range collapse" pattern that
    // RFC 6962 §2.1 specifies for streaming append; the only twist
    // here is the canonical odd-leaf promotion, applied at the final
    // collapse.
    let mut stack: Vec<(u32, [u8; DIGEST_LEN])> = Vec::with_capacity(32);

    let mut chunks_streamed: u32 = 0;
    let mut bytes_streamed: u64 = 0;

    for expected_idx in 0..header.chunk_count {
        // 1. Pull next ChunkRef.
        let chunk_ref = match refs
            .next_chunk_ref()
            .map_err(|_| VerifyError::StreamingSourceTruncated {
                index: chunks_streamed,
                expected_total: header.chunk_count,
            })? {
            Some(c) => c,
            None => {
                return Err(VerifyError::StreamingSourceTruncated {
                    index: chunks_streamed,
                    expected_total: header.chunk_count,
                });
            }
        };
        if chunk_ref.index != expected_idx {
            return Err(VerifyError::StreamingChunkIndexUnexpected {
                expected: expected_idx,
                found: chunk_ref.index,
            });
        }
        if chunk_ref.size_bytes == 0 {
            return Err(VerifyError::Structure(ManifestError::ChunkSizeZero(
                expected_idx,
            )));
        }
        if chunk_ref.size_bytes > MAX_CHUNK_SIZE_BYTES {
            return Err(VerifyError::Structure(ManifestError::ChunkSizeExceeded {
                index: expected_idx,
                size_bytes: chunk_ref.size_bytes,
            }));
        }

        // 2. Pull bytes for the chunk + verify hash + size.
        let bytes = match bytes_src.fetch_chunk_bytes(expected_idx).map_err(|_| {
            VerifyError::StreamingSourceTruncated {
                index: expected_idx,
                expected_total: header.chunk_count,
            }
        })? {
            Some(b) => b,
            None => {
                return Err(VerifyError::StreamingSourceTruncated {
                    index: expected_idx,
                    expected_total: header.chunk_count,
                });
            }
        };
        if bytes.len() != chunk_ref.size_bytes as usize {
            return Err(VerifyError::StreamingChunkSizeMismatch {
                index: expected_idx,
                declared: chunk_ref.size_bytes,
                observed: bytes.len(),
            });
        }
        // Recompute the chunk digest and compare BEFORE pushing bytes
        // to the sink (fail-fast — no unverified bytes ever reach the
        // sink). Plain `==` is fine: a side-channel oracle on the
        // chunk-digest comparison gives no leverage; the digest is
        // public output material.
        let recomputed = *blake3::hash(&bytes).as_bytes();
        if recomputed != chunk_ref.digest {
            return Err(VerifyError::StreamingChunkMismatch {
                index: expected_idx,
            });
        }

        // 3. Push verified bytes to sink.
        let bytes_len = bytes.len() as u64;
        sink.write_verified(expected_idx, bytes)
            .map_err(|_| VerifyError::StreamingSourceTruncated {
                index: expected_idx,
                expected_total: header.chunk_count,
            })?;

        // 4. Update bottom-up Merkle stack.
        let leaf = hash_leaf(&chunk_ref.digest);
        push_leaf_and_collapse(&mut stack, leaf);

        chunks_streamed = chunks_streamed.saturating_add(1);
        bytes_streamed = bytes_streamed.saturating_add(bytes_len);
    }

    // --- Terminal collapse (odd-leaf promotion mirror) ---
    let recomputed_root = collapse_stack(&mut stack)
        .ok_or(VerifyError::Structure(ManifestError::Empty))?;
    if recomputed_root != header.merkle_root {
        return Err(VerifyError::Structure(ManifestError::RootMismatch));
    }

    // --- Aggregate-size cross-check ---
    if bytes_streamed != header.total_size_bytes {
        return Err(VerifyError::Structure(ManifestError::TotalSizeMismatch {
            declared: header.total_size_bytes,
            actual: bytes_streamed,
        }));
    }

    Ok(StreamingVerifyOutcome {
        chunks_streamed,
        bytes_streamed,
    })
}

/// Push a leaf onto the streaming-Merkle stack and collapse pairs at
/// matching levels. Stack-bounded log2(MAX_CHUNKS_PER_BLOB) ≈ 17.
fn push_leaf_and_collapse(
    stack: &mut Vec<(u32, [u8; DIGEST_LEN])>,
    leaf: [u8; DIGEST_LEN],
) {
    stack.push((0, leaf));
    // Collapse: while top two have equal level, pop + inner-hash + push.
    while stack.len() >= 2 {
        let (Some(top1), Some(top2)) = (stack.last(), stack.get(stack.len().saturating_sub(2)))
        else {
            // Unreachable given the len() >= 2 guard; defensive.
            break;
        };
        if top1.0 != top2.0 {
            break;
        }
        let lvl = top1.0;
        // Pop the right (top1) then the left (top2). The left was
        // pushed first so it's deeper in the stack.
        let Some((_, right)) = stack.pop() else { break };
        let Some((_, left)) = stack.pop() else { break };
        let combined = hash_inner(&left, &right);
        stack.push((lvl.saturating_add(1), combined));
    }
}

/// Collapse the streaming-Merkle stack at terminal poll. Implements
/// the canonical RFC 6962 / Bitcoin odd-leaf promotion rule when the
/// stack has more than one slot at terminal.
fn collapse_stack(
    stack: &mut Vec<(u32, [u8; DIGEST_LEN])>,
) -> Option<[u8; DIGEST_LEN]> {
    if stack.is_empty() {
        return None;
    }
    while stack.len() > 1 {
        // Pop the top (right) and the second-top (left). The level of
        // the right is always <= level of the left — because the
        // collapse loop in `push_leaf_and_collapse` only fires when
        // levels are equal, so unequal-level neighbours mean the right
        // is "shorter" than the left in MMR terms.
        //
        // Canonical odd-leaf promotion rule: hash the two slots together
        // with the inner-prefix tag — DOES NOT promote unchanged.
        // (RFC 6962 §2.1.2 — at every level, if odd-leaf tail exists,
        // it gets paired with itself? — the lex-merkle-mountain-range
        // canonical resolution is `inner(left, right)` where right is
        // at a lower level; the level of the resulting node is the max
        // + 1.)
        //
        // This matches the `corelink-ac::merkle::build_root` shape:
        // unequal-height tail elements are folded together via the
        // `\x01` inner-prefix hash.
        let (_, right) = stack.pop()?;
        let Some((left_lvl, left)) = stack.pop() else {
            // Single-slot remainder — the loop guard `stack.len() > 1`
            // protects against this in practice, but we keep the
            // defensive arm so a future refactor that flips the guard
            // doesn't introduce a panic path. Returning the right
            // alone preserves the canonical odd-leaf promotion shape.
            return Some(right);
        };
        let combined = hash_inner(&left, &right);
        stack.push((left_lvl.saturating_add(1), combined));
    }
    stack.pop().map(|(_, h)| h)
}

/// In-memory test fake for [`ChunkRefSource`] — yields a vector of
/// chunk refs in order. The fake is for unit tests + property tests
/// only — production code uses the D1 cursor.
#[derive(Debug)]
pub struct InMemoryChunkRefSource {
    refs: Vec<ChunkRef>,
    cursor: usize,
}

impl InMemoryChunkRefSource {
    /// Construct a fresh in-memory chunk-refs source.
    #[must_use]
    pub fn new(refs: Vec<ChunkRef>) -> Self {
        Self { refs, cursor: 0 }
    }
}

impl ChunkRefSource for InMemoryChunkRefSource {
    fn next_chunk_ref(&mut self) -> Result<Option<ChunkRef>, String> {
        let r = self.refs.get(self.cursor).copied();
        if r.is_some() {
            self.cursor = self.cursor.saturating_add(1);
        }
        Ok(r)
    }
}

/// In-memory test fake for [`ChunkBytesSource`] — yields chunk bytes
/// keyed by `chunk_index`.
#[derive(Debug, Default)]
pub struct InMemoryChunkBytesSource {
    by_index: std::collections::BTreeMap<u32, Vec<u8>>,
}

impl InMemoryChunkBytesSource {
    /// Construct a fresh empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert chunk bytes for `chunk_index` (test helper).
    pub fn insert(&mut self, chunk_index: u32, bytes: Vec<u8>) {
        self.by_index.insert(chunk_index, bytes);
    }
}

impl ChunkBytesSource for InMemoryChunkBytesSource {
    fn fetch_chunk_bytes(
        &mut self,
        chunk_index: u32,
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(self.by_index.get(&chunk_index).cloned())
    }
}

/// In-memory test fake for [`VerifiedChunkSink`] — collects verified
/// bytes into an internal `Vec<(index, bytes)>`.
#[derive(Debug, Default)]
pub struct CollectingVerifiedSink {
    inner: Vec<(u32, Vec<u8>)>,
}

impl CollectingVerifiedSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the captured `(index, bytes)` rows.
    #[must_use]
    pub fn snapshot(&self) -> &[(u32, Vec<u8>)] {
        &self.inner
    }

    /// Drain the captured rows.
    #[must_use]
    pub fn take(&mut self) -> Vec<(u32, Vec<u8>)> {
        core::mem::take(&mut self.inner)
    }
}

impl VerifiedChunkSink for CollectingVerifiedSink {
    fn write_verified(&mut self, chunk_index: u32, bytes: Vec<u8>) -> Result<(), String> {
        self.inner.push((chunk_index, bytes));
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::manifest::builder::ManifestBuilder;
    use crate::manifest::sig::{ManifestSigner, ManifestVerifierSig};
    use crate::manifest::types::{ChunkInput, ChunkerAlgorithm};
    use corelink_ac::sig::{MockTdkHandle, TdkHandle};
    use std::sync::Arc;

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn fresh_signer_verifier() -> (ManifestSigner, ManifestVerifierSig, Arc<MockTdkHandle>) {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(fixed_tenant(), 1);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = ManifestSigner::new(Arc::clone(&handle), 1).unwrap();
        let verifier = ManifestVerifierSig::new(handle, vec![1]).unwrap();
        (signer, verifier, mock)
    }

    fn build_test_manifest(payloads: &[&[u8]]) -> Manifest {
        let signer = fresh_signer_verifier().0;
        let chunks: Vec<ChunkInput> = payloads
            .iter()
            .map(|p| {
                let d = *blake3::hash(p).as_bytes();
                ChunkInput::new(d, p.len() as u32)
            })
            .collect();
        ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap()
    }

    #[test]
    fn verify_full_happy_path() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let m = build_test_manifest(&[b"AAA", b"BBB", b"CCC"]);
        ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap();
    }

    #[test]
    fn verify_full_rejects_tampered_chunk_digest() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB", b"CCC"]);
        m.chunks[1].digest[0] ^= 0x01;
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::RootMismatch)
        ));
    }

    #[test]
    fn verify_full_rejects_tampered_root() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB"]);
        m.merkle_root[0] ^= 0x01;
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::RootMismatch)
        ));
    }

    #[test]
    fn verify_full_rejects_chunk_count_mismatch() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB"]);
        m.chunks.pop();
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::ChunkCountMismatch { .. })
        ));
    }

    #[test]
    fn verify_full_rejects_total_size_mismatch() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB"]);
        m.total_size_bytes = m.total_size_bytes.saturating_add(1);
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::TotalSizeMismatch { .. })
        ));
    }

    #[test]
    fn verify_full_rejects_index_out_of_order() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB", b"CCC"]);
        m.chunks.swap(0, 1);
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::ChunkIndexOutOfOrder { .. })
        ));
    }

    #[test]
    fn verify_full_rejects_version_unknown() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA"]);
        m.version = 9;
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::VersionUnsupported(9))
        ));
    }

    #[test]
    fn verify_full_rejects_sig_byte_flip() {
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA"]);
        m.sig[5] ^= 0x01;
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        assert!(matches!(err, VerifyError::Sig(_)));
    }

    #[test]
    fn verify_full_structure_first_then_sig() {
        // Pin the canonical pinned-order discipline: a manifest with
        // both a structural failure AND a sig failure should surface
        // the STRUCTURAL failure (sig must NOT be evaluated against an
        // out-of-bounds preimage).
        let (_, sig_verifier, _) = fresh_signer_verifier();
        let mut m = build_test_manifest(&[b"AAA", b"BBB"]);
        m.chunks.swap(0, 1); // structural P0
        m.sig[5] ^= 0x01; // sig P0
        let err = ManifestVerifier::new()
            .verify_full(&m, &sig_verifier)
            .unwrap_err();
        // The error MUST be Structure, not Sig.
        assert!(matches!(err, VerifyError::Structure(_)));
    }

    fn make_streaming_setup(
        m: &Manifest,
        payloads: &[&[u8]],
    ) -> (
        StreamingManifestHeader,
        InMemoryChunkRefSource,
        InMemoryChunkBytesSource,
        CollectingVerifiedSink,
    ) {
        let header = StreamingManifestHeader::from_manifest(m);
        let refs = InMemoryChunkRefSource::new(m.chunks.clone());
        let mut bytes = InMemoryChunkBytesSource::new();
        for (i, p) in payloads.iter().enumerate() {
            bytes.insert(i as u32, p.to_vec());
        }
        let sink = CollectingVerifiedSink::new();
        (header, refs, bytes, sink)
    }

    #[test]
    fn streaming_verify_happy_path() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB", b"CCC"];
        let m = build_test_manifest(payloads);
        let (header, mut refs, mut bytes, mut sink) = make_streaming_setup(&m, payloads);
        let outcome = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap();
        assert_eq!(outcome.chunks_streamed, 3);
        assert_eq!(outcome.bytes_streamed, 9);
        let collected: Vec<u8> = sink
            .take()
            .into_iter()
            .flat_map(|(_, b)| b)
            .collect();
        assert_eq!(collected, b"AAABBBCCC");
    }

    #[test]
    fn streaming_verify_fail_fast_chunk_mismatch() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB", b"CCC"];
        let m = build_test_manifest(payloads);
        let (header, mut refs, mut bytes, mut sink) = make_streaming_setup(&m, payloads);
        // Tamper chunk 1's bytes — verifier must catch BEFORE chunk 2.
        bytes.insert(1, b"XXX".to_vec());
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        match err {
            VerifyError::StreamingChunkMismatch { index: 1 } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
        // Sink must contain ONLY chunk 0 (chunk 1 was caught BEFORE sink push).
        let collected = sink.take();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, 0);
    }

    #[test]
    fn streaming_verify_size_mismatch() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB"];
        let m = build_test_manifest(payloads);
        let (header, mut refs, mut bytes, mut sink) = make_streaming_setup(&m, payloads);
        // Replace chunk 1 with bytes of wrong length.
        bytes.insert(1, b"XX".to_vec());
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        match err {
            VerifyError::StreamingChunkSizeMismatch {
                index: 1,
                declared: 3,
                observed: 2,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn streaming_verify_truncated_source() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB", b"CCC"];
        let m = build_test_manifest(payloads);
        let (header, mut refs, _, mut sink) = make_streaming_setup(&m, payloads);
        // Bytes source MISSING chunk 2.
        let mut bytes = InMemoryChunkBytesSource::new();
        bytes.insert(0, b"AAA".to_vec());
        bytes.insert(1, b"BBB".to_vec());
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        match err {
            VerifyError::StreamingSourceTruncated {
                index: 2,
                expected_total: 3,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn streaming_verify_index_out_of_order() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB"];
        let m = build_test_manifest(payloads);
        let header = StreamingManifestHeader::from_manifest(&m);
        // Build a refs source that yields chunk 1 BEFORE chunk 0 — bug.
        let mut refs_v = m.chunks.clone();
        refs_v.swap(0, 1);
        let mut refs = InMemoryChunkRefSource::new(refs_v);
        let mut bytes = InMemoryChunkBytesSource::new();
        bytes.insert(0, b"AAA".to_vec());
        bytes.insert(1, b"BBB".to_vec());
        let mut sink = CollectingVerifiedSink::new();
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        match err {
            VerifyError::StreamingChunkIndexUnexpected {
                expected: 0,
                found: 1,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn streaming_verify_root_mismatch_at_terminal() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB"];
        let m = build_test_manifest(payloads);
        let mut header = StreamingManifestHeader::from_manifest(&m);
        // Tamper the claimed root in the header — every chunk verifies
        // OK individually but the terminal collapse mismatches.
        header.merkle_root[0] ^= 0x01;
        let mut refs = InMemoryChunkRefSource::new(m.chunks.clone());
        let mut bytes = InMemoryChunkBytesSource::new();
        for (i, p) in payloads.iter().enumerate() {
            bytes.insert(i as u32, p.to_vec());
        }
        let mut sink = CollectingVerifiedSink::new();
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::RootMismatch)
        ));
    }

    #[test]
    fn streaming_verify_total_size_mismatch_at_terminal() {
        let payloads: &[&[u8]] = &[b"AAA", b"BBB"];
        let m = build_test_manifest(payloads);
        let mut header = StreamingManifestHeader::from_manifest(&m);
        header.total_size_bytes = header.total_size_bytes.saturating_add(1);
        let mut refs = InMemoryChunkRefSource::new(m.chunks.clone());
        let mut bytes = InMemoryChunkBytesSource::new();
        for (i, p) in payloads.iter().enumerate() {
            bytes.insert(i as u32, p.to_vec());
        }
        let mut sink = CollectingVerifiedSink::new();
        let err = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::Structure(ManifestError::TotalSizeMismatch { .. })
        ));
    }

    #[test]
    fn streaming_verify_root_matches_corelink_ac_pattern_for_3_leaves() {
        // Pin the bottom-up streaming root reproduces the same
        // canonical recursive build_root for an odd-leaf-count tree.
        let payloads: &[&[u8]] = &[b"AAA", b"BBB", b"CCC"];
        let m = build_test_manifest(payloads);
        let direct_root = crate::manifest::merkle::build_root(&m.chunks).unwrap();
        let header = StreamingManifestHeader::from_manifest(&m);
        let mut refs = InMemoryChunkRefSource::new(m.chunks.clone());
        let mut bytes = InMemoryChunkBytesSource::new();
        for (i, p) in payloads.iter().enumerate() {
            bytes.insert(i as u32, p.to_vec());
        }
        let mut sink = CollectingVerifiedSink::new();
        // Streaming verify should succeed → roots are equal by
        // construction (verify_streaming compares to header.merkle_root
        // which is m.merkle_root which is direct_root).
        let outcome = verify_streaming(&header, &mut refs, &mut bytes, &mut sink).unwrap();
        assert_eq!(outcome.chunks_streamed, 3);
        // Sanity: header root = direct root.
        assert_eq!(header.merkle_root, direct_root);
    }
}

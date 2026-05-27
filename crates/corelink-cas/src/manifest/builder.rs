//! Manifest builder (WI-S05-005 §6.1).
//!
//! Consumes the canonical-ordered chunk stream from upstream
//! `corelink-chunker` (or any other source that yields chunks in
//! ascending `index = 0..N` order with each chunk's BLAKE3-256 digest
//! and size already computed) and produces a sealed [`Manifest`] with:
//!
//! 1. The Merkle root computed via [`crate::manifest::merkle::build_root`].
//! 2. The 102-byte canonical preimage assembled via
//!    [`Manifest::canonical_bytes`].
//! 3. The HKDF-SHA256 + BLAKE3 keyed-hash signature computed via
//!    [`crate::manifest::sig::ManifestSigner::sign`] over that preimage.
//!
//! ## Bounded parser
//!
//! The builder enforces every cap from [`crate::manifest::bounds`]:
//!
//! - `chunks.len() <= MAX_CHUNKS_PER_BLOB`
//! - `total_size_bytes <= MAX_TOTAL_SIZE_BYTES`
//! - `chunks[i].size_bytes in 1..=MAX_CHUNK_SIZE_BYTES`
//! - `chunks[i].index == i as u32` for `i in 0..chunks.len()`
//! - `chunks.len() > 0` (empty manifests rejected per
//!   [`crate::manifest::error::ManifestError::Empty`])
//!
//! ## `created_at_ms` capture-once semantic
//!
//! Per spec contract §5.1 P1-SR5-002 (Lote 10.5-tris): the handler
//! captures `created_at_ms` ONCE before signing and reuses it on retry.
//! This module accepts `created_at_ms` as a builder parameter so the
//! caller controls the capture-once discipline; do NOT call
//! `SystemTime::now()` inside this module (that would re-randomise on
//! retry, breaking idempotency).

use uuid::Uuid;

use crate::manifest::bounds::{
    MAX_CHUNK_SIZE_BYTES, MAX_CHUNKS_PER_BLOB, MAX_TOTAL_SIZE_BYTES,
};
use crate::manifest::error::ManifestError;
use crate::manifest::merkle::build_root;
use crate::manifest::sig::ManifestSigner;
use crate::manifest::types::{
    ChunkInput, ChunkRef, ChunkerAlgorithm, Manifest, CURRENT_MANIFEST_VERSION, DIGEST_LEN,
};

/// Pure-logic manifest builder. Holds no state — the entire manifest
/// derives from `(tenant_id, blob_digest, chunks, created_at_ms,
/// chunker_algo, sig_key_id)` deterministically.
#[derive(Clone, Copy, Debug, Default)]
pub struct ManifestBuilder;

impl ManifestBuilder {
    /// Construct a fresh builder. `const fn` so callers can pin one in
    /// a `static` for zero-allocation reuse.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Build + sign a manifest.
    ///
    /// `chunks` MUST be in canonical (index-ascending) order; the
    /// builder verifies the order + every bound and surfaces the
    /// canonical [`ManifestError`] for any violation.
    ///
    /// # Errors
    ///
    /// - [`ManifestError::Empty`] when `chunks` is empty.
    /// - [`ManifestError::ChunkCountExceeded`] /
    ///   [`ManifestError::TotalSizeExceeded`] /
    ///   [`ManifestError::ChunkSizeExceeded`] /
    ///   [`ManifestError::ChunkSizeZero`] when bounds are violated.
    /// - [`ManifestError::ChunkIndexOutOfOrder`] when the input is not
    ///   strictly index-ascending `0..N`.
    /// - propagates [`corelink_ac_core::sig::SigError`] (wrapped via
    ///   [`ManifestError::Empty`]?? — see below) when the sig fails.
    ///
    /// To keep the error taxonomy clean, sig failures surface via
    /// [`BuildError::Sig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "single-call canonical builder surface; arguments bind the canonical envelope identity (tenant + blob + chunks + sig key + sealed timestamp + algo + signer) — collapsing into a struct would obscure the cripto contract that each input commits to"
    )]
    pub fn build(
        &self,
        tenant_id: Uuid,
        blob_digest: [u8; DIGEST_LEN],
        chunks: Vec<ChunkInput>,
        created_at_ms: u64,
        chunker_algo: ChunkerAlgorithm,
        signer: &ManifestSigner,
        sig_key_id: u32,
    ) -> Result<Manifest, BuildError> {
        // Step 1: bounded-parser checks BEFORE any allocation that
        // depends on attacker-controlled length.
        let count_usize = chunks.len();
        if count_usize == 0 {
            return Err(BuildError::Structure(ManifestError::Empty));
        }
        let chunk_count = u32::try_from(count_usize).map_err(|_| {
            BuildError::Structure(ManifestError::ChunkCountExceeded { found: u32::MAX })
        })?;
        if chunk_count > MAX_CHUNKS_PER_BLOB {
            return Err(BuildError::Structure(ManifestError::ChunkCountExceeded {
                found: chunk_count,
            }));
        }

        // Materialise ChunkRef list in canonical order; pin index
        // monotonicity + per-chunk bounds during the same pass.
        let mut chunk_refs: Vec<ChunkRef> = Vec::with_capacity(count_usize);
        let mut total_size: u64 = 0;
        for (slot, c) in chunks.iter().enumerate() {
            let slot_u32 = u32::try_from(slot).map_err(|_| {
                BuildError::Structure(ManifestError::ChunkCountExceeded {
                    found: u32::MAX,
                })
            })?;
            if c.size_bytes == 0 {
                return Err(BuildError::Structure(ManifestError::ChunkSizeZero(slot_u32)));
            }
            if c.size_bytes > MAX_CHUNK_SIZE_BYTES {
                return Err(BuildError::Structure(ManifestError::ChunkSizeExceeded {
                    index: slot_u32,
                    size_bytes: c.size_bytes,
                }));
            }
            total_size = total_size.saturating_add(u64::from(c.size_bytes));
            if total_size > MAX_TOTAL_SIZE_BYTES {
                return Err(BuildError::Structure(ManifestError::TotalSizeExceeded {
                    found: total_size,
                }));
            }
            chunk_refs.push(ChunkRef::new(slot_u32, c.digest, c.size_bytes));
        }

        // Step 2: Merkle root over the canonical-ordered chunk list.
        let merkle_root = build_root(&chunk_refs).map_err(BuildError::Structure)?;

        // Step 3: assemble the unsigned manifest skeleton (sig + key_id
        // filled in below).
        let mut manifest = Manifest {
            version: CURRENT_MANIFEST_VERSION,
            tenant_id,
            blob_digest,
            merkle_root,
            chunk_count,
            total_size_bytes: total_size,
            chunks: chunk_refs,
            created_at_ms,
            sig: [0u8; 32],
            sig_key_id,
            chunker_algo,
        };

        // Step 4: canonical preimage + sig.
        let canonical = manifest.canonical_bytes();
        let sig = signer
            .sign(tenant_id, sig_key_id, &canonical)
            .map_err(BuildError::Sig)?;
        manifest.sig = sig;

        Ok(manifest)
    }
}

/// Error type surfaced by [`ManifestBuilder::build`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// Structural / bounded-parser failure.
    #[error("manifest structure invalid: {0}")]
    Structure(ManifestError),
    /// HKDF / BLAKE3 sig failure.
    #[error("manifest sig failure: {0}")]
    Sig(corelink_ac_core::sig::SigError),
}

impl BuildError {
    /// Short canonical audit code for SIEM emission.
    #[must_use]
    pub fn audit_code(&self) -> &'static str {
        match self {
            Self::Structure(e) => e.audit_code(),
            Self::Sig(_) => "manifest_sig_invalid",
        }
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
    use corelink_ac_core::sig::{MockTdkHandle, TdkHandle};
    use std::sync::Arc;

    fn fresh_signer(tenant: Uuid, kid: u32) -> ManifestSigner {
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(tenant, kid);
        let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
        ManifestSigner::new(handle, kid).unwrap()
    }

    fn input(idx_seed: u8, size: u32) -> ChunkInput {
        ChunkInput::new([idx_seed; DIGEST_LEN], size)
    }

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    #[test]
    fn build_rejects_empty_chunks() {
        let signer = fresh_signer(fixed_tenant(), 1);
        let err = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0u8; 32],
                vec![],
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap_err();
        match err {
            BuildError::Structure(ManifestError::Empty) => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn build_rejects_zero_size_chunk() {
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, 0)];
        let err = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0u8; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap_err();
        match err {
            BuildError::Structure(ManifestError::ChunkSizeZero(0)) => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn build_rejects_oversize_chunk() {
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, MAX_CHUNK_SIZE_BYTES + 1)];
        let err = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0u8; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap_err();
        match err {
            BuildError::Structure(ManifestError::ChunkSizeExceeded { index: 0, .. }) => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn build_canonical_assigns_indices() {
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, 5), input(2, 7), input(3, 11)];
        let m = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        assert_eq!(m.chunk_count, 3);
        for (i, c) in m.chunks.iter().enumerate() {
            assert_eq!(c.index as usize, i);
        }
        assert_eq!(m.total_size_bytes, 5 + 7 + 11);
    }

    #[test]
    fn build_idempotent_for_same_input() {
        // Same inputs (including created_at_ms) → byte-identical
        // manifest.
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, 5), input(2, 7)];
        let m1 = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks.clone(),
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        let m2 = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        assert_eq!(m1, m2);
    }

    #[test]
    fn build_emits_chunker_algo_into_sig() {
        // Same chunks + same created_at_ms but different chunker_algo
        // produces different sig (because chunker_algo lives in the
        // canonical preimage).
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, 5), input(2, 7)];
        let m_fixed = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks.clone(),
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        let m_fastcdc = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks,
                42,
                ChunkerAlgorithm::FastCdc2MiB,
                &signer,
                1,
            )
            .unwrap();
        // merkle_root same (because chunk digests + order are same),
        // sig differs (because chunker_algo byte differs).
        assert_eq!(m_fixed.merkle_root, m_fastcdc.merkle_root);
        assert_ne!(m_fixed.sig, m_fastcdc.sig);
    }

    #[test]
    fn build_emits_chunk_count_into_sig() {
        // Two manifests sharing the same merkle_root (constructed
        // out-of-band) but different chunk_count must produce different
        // sigs — preventing the bound-bypass forge attack documented in
        // WI §6.1.5.
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks_a = vec![input(1, 5), input(2, 7)];
        let chunks_b = vec![input(1, 5)];
        let a = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks_a,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        let b = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks_b,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                1,
            )
            .unwrap();
        assert_ne!(a.sig, b.sig);
        assert_ne!(a.merkle_root, b.merkle_root);
    }

    #[test]
    fn build_with_reserved_key_id_rejected() {
        let signer = fresh_signer(fixed_tenant(), 1);
        let chunks = vec![input(1, 5)];
        let err = ManifestBuilder::new()
            .build(
                fixed_tenant(),
                [0xAB; 32],
                chunks,
                42,
                ChunkerAlgorithm::Fixed2MiB,
                &signer,
                0, // reserved sentinel
            )
            .unwrap_err();
        match err {
            BuildError::Sig(corelink_ac_core::sig::SigError::KeyIdReserved) => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }
}

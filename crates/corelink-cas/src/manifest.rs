//! `corelink-manifest` — canonical Merkle manifest builder + dual-side
//! verifier (WI-S05-005).
//!
//! Implements the cripto-load-bearing surface that the Cloudflare
//! Worker SplitBlob handler (`corelink-worker::reapi::cas`) and the
//! Rust client SDK (forward; consumed via `corelink-client-verify`)
//! BOTH consume — the worker invokes [`ManifestBuilder::build`] +
//! [`ManifestVerifier::verify_full`] on every SplitBlob *pre-persist*,
//! and the SDK invokes the same verifier *post-download* on every
//! SpliceBlob completion. Two independent code paths converging on the
//! same canonical algorithm = defense-in-depth against any
//! single-tier compromise (per WI §2 partial-compromise threat
//! analysis).
//!
//! # Cripto algorithm summary (WI-S05-005 §6.1, §9.x)
//!
//! - **Hash**: BLAKE3-256 (matches CAS S-01 + S-04 — single hash
//!   family; SIMD-accelerated; collision-resistant 2^128).
//! - **Domain separation**: RFC 6962-style. `\x00`-prefix for leaf
//!   inputs, `\x01`-prefix for inner-node inputs — prevents the
//!   second-preimage tree-shape attack where a leaf could otherwise
//!   substitute for an inner node. Mirrors `corelink-ac::merkle::{LEAF_PREFIX, INNER_PREFIX}`
//!   byte-equal.
//! - **Order is semantic, NOT canonical**: chunks are kept in
//!   ascending `index = 0..N` order (the concatenation order that
//!   produces the blob bytes); shuffling chunks → different
//!   `merkle_root` → bind detected. (Contrast `corelink-ac::merkle`
//!   which lex-sorts because the AC input is an unordered set.)
//! - **HKDF-SHA256 sig with `info = b"manifest-sig"`**: domain-separated
//!   from `b"ac-sig"` (WI-S04-004) and `b"meta-manifest-sig"` (WI-S05-006
//!   forward). The CI test `sig::tests::canonical_info_string`
//!   asserts byte-equal AND non-prefix relationships across all three.
//! - **Per-tenant TDK** lookup goes through
//!   `corelink_ac::sig::TdkHandle` reused verbatim — single TDK
//!   surface across the whole stack.
//!
//! # Bounded parser (WI §6.1.5 + [`bounds`])
//!
//! | Bound | Value |
//! |---|---|
//! | `MAX_CHUNKS_PER_BLOB` | 81 920 |
//! | `MAX_TOTAL_SIZE_BYTES` | 160 GiB |
//! | `MAX_CHUNK_SIZE_BYTES` | 4 MiB |
//! | `MANIFEST_PREIMAGE_LEN` | 102 bytes |
//! | `MANIFEST_SIG_LEN` | 32 bytes |
//!
//! Bounds reject at decode / build / verify time, BEFORE any allocation
//! that depends on attacker-controlled length, so a crafted oversized
//! envelope cannot exhaust the 128 MiB Worker isolate budget.
//!
//! # Memory discipline (`INV-MULTIPART-STREAMING-MEMORY`; spec contract §5.1 P0-SR5-003)
//!
//! - **`verify_full` / `verify_structure`**: hold the full
//!   `Vec<ChunkRef>` in memory; ~3.28 MB worst case at
//!   `MAX_CHUNKS_PER_BLOB`. Used by the SplitBlob pre-persist gate
//!   exactly once per upload.
//! - **`verify_streaming`** (Lote 10.5-tris P0-SR5-003 redesign): does
//!   NOT hold the full `Vec<ChunkRef>`. The verifier reads chunk refs
//!   one at a time from a [`verifier::ChunkRefSource`] (the production
//!   D1 cursor) AND maintains a bottom-up streaming Merkle stack
//!   (max ≈ 17 levels × 32 bytes = 544 bytes). Total streaming
//!   memory ≈ 32 KiB stack + 1 chunk's bytes (≤ 4 MiB) + 1 chunk-ref
//!   row + the streaming Merkle stack — independent of `chunk_count`.
//!
//! # Quickstart
//!
//! ```
//! use corelink_cas::manifest::{
//!     ChunkInput, ChunkerAlgorithm, ManifestBuilder,
//!     ManifestSigner, ManifestVerifier, ManifestVerifierSig,
//! };
//! use corelink_ac::sig::{MockTdkHandle, TdkHandle};
//! use std::sync::Arc;
//! use uuid::Uuid;
//!
//! let tenant = Uuid::nil();
//! let mock = Arc::new(MockTdkHandle::new());
//! mock.install_default(tenant, 1);
//! let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
//! let signer = ManifestSigner::new(Arc::clone(&handle), 1).unwrap();
//! let sig_verifier = ManifestVerifierSig::new(handle, vec![1]).unwrap();
//!
//! let chunks = vec![
//!     ChunkInput::new([1u8; 32], 5),
//!     ChunkInput::new([2u8; 32], 7),
//! ];
//! let manifest = ManifestBuilder::new()
//!     .build(tenant, [0xABu8; 32], chunks, 42, ChunkerAlgorithm::Fixed2MiB, &signer, 1)
//!     .unwrap();
//! ManifestVerifier::new()
//!     .verify_full(&manifest, &sig_verifier)
//!     .unwrap();
//! ```

#![forbid(unsafe_code)]

pub mod bounds;
pub mod builder;
pub mod error;
pub mod merkle;
pub mod sig;
pub mod types;
pub mod verifier;

pub use bounds::{
    MANIFEST_PREIMAGE_LEN, MANIFEST_SIG_LEN, MAX_CHUNKS_PER_BLOB, MAX_CHUNK_SIZE_BYTES,
    MAX_TOTAL_SIZE_BYTES,
};
pub use builder::{BuildError, ManifestBuilder};
pub use error::{ManifestError, VerifyError};
pub use merkle::{build_root, hash_inner, hash_leaf, INNER_PREFIX, LEAF_PREFIX};
pub use sig::{
    compute_signature, ManifestSigner, ManifestVerifierSig, HKDF_INFO_MANIFEST_SIG,
    HKDF_INFO_META_MANIFEST_SIG_RESERVED,
};
pub use types::{
    ChunkInput, ChunkRef, ChunkerAlgorithm, Manifest, CURRENT_MANIFEST_VERSION, DIGEST_LEN,
    MANIFEST_VERSION_V1,
};
pub use verifier::{
    verify_streaming, ChunkBytesSource, ChunkRefSource, CollectingVerifiedSink,
    InMemoryChunkBytesSource, InMemoryChunkRefSource, ManifestVerifier,
    StreamingManifestHeader, StreamingVerifyOutcome, VerifiedChunkSink,
};

/// Crate version sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

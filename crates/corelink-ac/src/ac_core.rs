//! `corelink-ac` — canonical Action Cache Merkle codec + dual-side
//! verifier (WI-S04-003).
//!
//! Implements the cripto-load-bearing surface that the Cloudflare
//! Worker AC handler (`corelink-worker::reapi::ac`) and the Rust
//! client SDK (`corelink-client-verify`) BOTH consume — the worker
//! invokes [`merkle::CanonicalMerkleVerifier`] on every
//! `UpdateActionResult` *pre-persist*, and a future SDK release will
//! invoke the same verifier *post-download* on every
//! `GetActionResult`. Two independent code paths converging on the
//! same canonical algorithm = defense-in-depth against any
//! single-tier compromise (per WI §2 partial-compromise threat
//! analysis).
//!
//! # Cripto algorithm summary
//!
//! - **Hash**: BLAKE3-256 (matches CAS S-01 — single hash family
//!   across the whole stack; SIMD-accelerated; collision-resistant
//!   2^128).
//! - **Domain separation**: RFC 6962-style. `\x00`-prefix for leaf
//!   inputs, `\x01`-prefix for inner-node inputs — prevents the
//!   second-preimage tree-shape attack where a leaf could otherwise
//!   substitute for an inner node.
//! - **Determinism**: leaves are lex-sorted by digest bytes prior to
//!   the build, so the same `(output_files, output_directories)`
//!   *set* (regardless of insertion order) yields the same
//!   `merkle_root` byte-for-byte.
//! - **`result_hash` index column**: `BLAKE3(merkle_root)` per
//!   ADR-0037 — D1 INDEX column only, NOT a cripto authority. The
//!   binding authority is the HKDF-SHA256 envelope sig over the
//!   canonical 121-byte preimage layout — see [`sig`] (WI-S04-004).
//!
//! # Sig surface (WI-S04-004)
//!
//! The [`sig`] module ships the canonical HKDF-SHA256 + BLAKE3-keyed
//! signer + verifier that satisfies CTRL-AC-002. Real-impl traits
//! [`sig::SignatureSigner`] / [`sig::SignatureVerifier`] are consumed
//! by:
//!
//! - the worker handler (`corelink-worker::reapi::ac::sig::CanonicalAcSigner`
//!   adapter wraps `Arc<HkdfSigner>` + `Arc<HkdfVerifier>`);
//! - the future Rust client SDK dual-side post-download verifier (see
//!   [`sig::compute_signature`]).
//!
//! Per-tenant TDK lookup goes through [`sig::TdkHandle`]; the test
//! fixture [`sig::MockTdkHandle`] is deterministic so canonical
//! vectors + property tests are reproducible without any external
//! secret store. The production `CfSecretsTdkHandle` Cloudflare
//! Secrets shim ships alongside WI-S04-006 per the charter
//! trait-abstraction-defer pattern.
//!
//! # Bounded parser
//!
//! Every public surface enforces ([`bounds`]):
//!
//! | Bound | Value |
//! |---|---|
//! | `MAX_TREE_DEPTH` | 32 |
//! | `MAX_TREE_FANOUT` | 4096 |
//! | `MAX_NODE_COUNT` | 100 000 |
//! | `MAX_PAYLOAD_BYTES` | 1 MiB |
//! | `MAX_OUTPUT_FILES` | 4096 |
//! | `MAX_OUTPUT_DIRECTORIES` | 4096 |
//!
//! Bounds reject at decode time (before any allocation that depends
//! on attacker-controlled length) so a crafted oversized envelope
//! cannot exhaust the 128 MiB Worker isolate budget.
//!
//! # Forward-compat with the worker handler trait
//!
//! `corelink-worker::reapi::ac::merkle` ships a [`MerkleVerifier`]
//! trait + an `InMemoryMerkleVerifier` test fake (WI-S04-001). The
//! handler is generic over `V: MerkleVerifier`, so both the worker's
//! local fake AND this crate's [`merkle::CanonicalMerkleVerifier`]
//! satisfy the bound and can be slotted into the handler at wiring
//! time. The bounds + error variants of the two crates are
//! deliberately compatible (`audit_code()` short identifiers match
//! 1:1 where the variants overlap).
//!
//! # Threat model
//!
//! See WI-S04-003 §2 + ADR-0037 §Rationale for the full
//! adversarial scenarios. Short version:
//!
//! - **Storage-tier insider** (R2 access, no HKDF key): caught by
//!   the HKDF sig (WI-S04-004); the Merkle root is redundant defense.
//! - **Wire tampering** (handler ↔ R2): caught by HKDF sig +
//!   Merkle root mismatch on read.
//! - **Compromised writer mid-pipeline** (sig generated for v1,
//!   stored bytes are v2): Merkle root + canonical preimage layout
//!   diverges, sig invalid.
//! - **Full HKDF key compromise**: defense-in-depth FAILS — both
//!   layers verify "ok" against attacker-controlled material.
//!   Externalized mitigation = TDK rotation + S-09 audit chain
//!   anomaly detection.
//!
//! # Quickstart
//!
//! ```
//! use corelink_ac::merkle::{build_root, verify_root};
//! use corelink_ac::types::{ActionResult, OutputFileDigest};
//! use corelink_hash::Digest;
//!
//! let r = ActionResult::new(
//!     vec![
//!         OutputFileDigest::new(Digest::compute(b"out1"), 7),
//!         OutputFileDigest::new(Digest::compute(b"out2"), 11),
//!     ],
//!     Vec::new(),
//!     0,
//!     b"raw-proto-bytes".to_vec(),
//! );
//! let root = build_root(&r).expect("well-formed result");
//! verify_root(&r, &root).expect("recomputes to the same root");
//! ```

#![forbid(unsafe_code)]

pub mod bounds;
pub mod codec;
pub mod error;
pub mod merkle;
pub mod outputs;
pub mod sig;
pub mod types;

pub use bounds::{
    MAX_NODE_COUNT, MAX_OUTPUT_DIRECTORIES, MAX_OUTPUT_FILES, MAX_PAYLOAD_BYTES, MAX_TREE_DEPTH,
    MAX_TREE_FANOUT,
};
pub use codec::{decode, encode};
pub use error::{BuildError, MerkleError, OutputsCheckError, VerifyError};
pub use merkle::{
    build_root, compute_result_hash, verify_root, CanonicalMerkleVerifier, MerkleVerifier,
    INNER_PREFIX, LEAF_PREFIX,
};
pub use outputs::{BlobMetaReader, OutputsValidator, StrictOutputsValidator};
pub use types::{
    AcEnvelope, ActionDigest, ActionResult, OutputDirectoryDigest, OutputFileDigest,
    AC_ENVELOPE_VERSION, MERKLE_ROOT_LEN, RESULT_HASH_LEN,
};

/// Crate version sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

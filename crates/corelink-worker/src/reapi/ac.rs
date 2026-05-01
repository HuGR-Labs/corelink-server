//! REAPI v2 ActionCache handler module (WI-S04-001).
//!
//! Re-exports the canonical handler trait + impl + per-WI trait
//! abstractions for the upstream integration tier (gRPC tonic + REST
//! axum surfaces in `corelink-reapi`).
//!
//! ## Public surface
//!
//! - [`ActionCacheHandler`] / [`ActionCacheHandlerImpl`] — the canonical
//!   handler trait + the canonical impl with the 7-step GET / 10-step
//!   UPDATE sequences from WI §1.
//! - [`ActionDigest`] / [`ActionResult`] / [`AcEnvelope`] / [`AcMetaRow`] —
//!   the typed shapes that travel across the trait boundary.
//! - [`AcError`] — the canonical error taxonomy (10 variants) mapped 1:1
//!   to the `COR_AC_*` error codes per WI §23.
//! - [`AcEventType`] — the 5 canonical audit event types per WI brief
//!   (`ac.get.ok` / `ac.get.miss` / `ac.update.ok` /
//!   `ac.update.merkle_invalid` / `ac.update.outputs_missing`) plus the
//!   sig_invalid / result_mismatch / outputs_check warning shapes.
//! - [`meta::AcMetaStore`] — `(tenant_id, action_digest)`-keyed metadata
//!   row store (D1 backend in production; in-memory fake here).
//! - [`sig::Signer`] / [`sig::SigError`] — HKDF signer trait surface
//!   (real impl lands in WI-S04-004).
//! - [`merkle::MerkleVerifier`] / [`merkle::MerkleError`] — Merkle
//!   tree codec verifier surface (real impl lands in WI-S04-003).
//! - [`outputs::OutputsCheck`] — `output_files`/`output_directories`
//!   aliveness check against `blob_meta` (delegates to
//!   [`corelink_meta::MetaStore`] in production wiring).
//! - [`audit::AuditSink`] — the audit emit seam used by the handler.
//!
//! ## What is NOT here
//!
//! - Tonic gRPC + axum REST wrappers: deferred to a follow-up
//!   `corelink-reapi`-level WI (charter trait-abstraction-defer
//!   pattern). The handler trait is the integration seam.
//! - The 4 ActionCache examples (basic GET, basic UPDATE, idempotency
//!   demo, batch UPDATE) — also deferred until the gRPC surface lands;
//!   the property tests + e2e tests in `tests/` exercise the same code
//!   paths.

pub mod audit;
pub mod handler;
pub mod merkle;
pub mod meta;
pub mod neg_cache;
pub mod outputs;
pub mod sig;
pub mod types;

pub use audit::{AcEventType, AuditSink, InMemoryAuditSink};
pub use handler::{ActionCacheHandler, ActionCacheHandlerImpl, AcError};
pub use merkle::{InMemoryMerkleVerifier, MerkleError, MerkleVerifier};
pub use meta::{AcKey, AcMetaRow, AcMetaStore, AcMetaUpsertOutcome, InMemoryAcMetaStore};
pub use neg_cache::AcNegCache;
pub use outputs::{InMemoryOutputsCheck, OutputsCheck, OutputsCheckError, OutputsCheckOutcome};
pub use sig::{AcEnvelope, InMemoryFakeSigner, SigError, Signer};
pub use types::{
    ActionDigest, ActionResult, OutputDirectoryDigest, OutputFileDigest, ResultHash,
};

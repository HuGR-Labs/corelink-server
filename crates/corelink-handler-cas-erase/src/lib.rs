//! `corelink-handler-cas-erase` — per-hash CAS erase + 410-Gone tombstone
//! (hugit-P2 seam B, WP-B).
//!
//! Pure decision logic for an **operator/internal write-side** endpoint that
//! deletes a single content-addressed blob from R2 and leaves a durable
//! **410-Gone tombstone** so subsequent GETs of an erased hash answer HTTP 410
//! Gone — never 404 (which would imply "never existed") and never 200 (which
//! would resurrect erased bytes).
//!
//! # Why a separate pure crate
//!
//! Mirrors the sibling handler crates (`corelink-handler-ac`,
//! `corelink-handler-cas`): the decision logic carries **no I/O** and is
//! exhaustively unit/proptest-tested in isolation. The container's
//! `routes::cas_erase` composes it with the real transports:
//!
//! - **R2 delete** reuses the DSR Wave 1 R2 CAS primitives —
//!   `R2S3Client::{delete, list_objects_v2}` + `R2S3Client::blob_key` —
//!   added on branch `feat/dsr-account-deletion` (PR #254, DSR increment 3).
//!   WP-B does **not** duplicate those primitives; it composes them. Until
//!   #254 lands, the container delete site is gated behind that adapter.
//! - **Tombstone persistence** is a row in the `cas_tombstone` D1 table
//!   (migration `migrations/d1/0067_cas_tombstone.sql`), keyed
//!   `(tenant_id, digest)`.
//!
//! # 410 semantics (the contract)
//!
//! - `POST /_internal/cas/:tenant/:hash/erase` (internal-auth gated) →
//!   delete the R2 object(s) + upsert the tombstone → 200 OK.
//! - A re-erase of an already-tombstoned hash → idempotent no-op, still 200 OK
//!   ([`EraseOutcome::AlreadyErased`]).
//! - `GET /v1/cas/:tenant/:hash` consults [`read_gate`]; a present tombstone →
//!   [`ReadGate::Gone`] → HTTP **410 Gone** (off the hot path: a single keyed
//!   D1 lookup that short-circuits before the R2 GET).
//!
//! # Crate contents
//!
//! - [`error`] — [`CasEraseError`] `#[non_exhaustive]` taxonomy.
//! - [`handler`] — request/marker/outcome types + the pure decision functions
//!   ([`prepare_erase`], [`read_gate`], [`erase_outcome`], [`validate_digest`]).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod error;
pub mod handler;

pub use error::CasEraseError;
pub use handler::{
    erase_outcome, prepare_erase, read_gate, validate_digest, CasEraseRequest, EraseOutcome,
    ReadGate, TombstoneMarker,
};

//! `corelink-core` — apex cross-cutting types for the CoreLink Rust
//! workspace.
//!
//! Wave-33 Stage 0 (per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md`
//! §3 + §6) lands this crate as the apex of the dependency graph: it
//! depends on NO other `corelink-*` crate, so every context crate can
//! freely import it without creating cycles. Stage 3 will enforce this
//! via cargo-deny.
//!
//! ## What lives here
//!
//! - `TenantId` — UUIDv7 newtype, canonical D1 tenant key.
//! - `Digest` — 32-byte content-address newtype. The BLAKE3 hashing
//!   implementation lives in `corelink-crypto::blake3` (sub-step 2).
//! - `Region` — 4-valued canonical region enum (`Wnam`/`Enam`/`Weur`/
//!   `Sam`). Mirrors `corelink_region::Region` shape exactly.
//! - `SecretWrap` — domain-typed wrapper over `secrecy::SecretString`.
//! - `CoreError` / `DigestParseError` — workspace-cross-cutting errors
//!   (chokepoint-only; per-context errors stay in their owning crate).
//! - `Clock` trait — wall + monotonic clock surface. Impls live in
//!   `apps/server` (preserved in-place until Stage 2).
//!
//! ## What does NOT live here
//!
//! - Hashing primitives → `corelink-crypto::blake3` (Stage 0 sub-step 2).
//! - Audit event types + the `AuditEmitter` trait → `corelink-audit`
//!   (Stage 0 sub-step 4; consumes `corelink-core::TenantId` /
//!   `corelink-core::CoreError`).
//! - Concrete `WallClock` impls → `apps/server::wall_clock`.
//!
//! ## Migration story (Stage 0 → Stage 1)
//!
//! Stage 0 creates the canonical types here WITHOUT updating
//! consumers — existing scattered copies (`corelink_meta::TenantId`,
//! `corelink_region::Region`, `corelink_hash::Digest`, etc.) stay in
//! place. Stage 1 streams will migrate their context-owned consumers
//! via `pub use corelink_core::*` and drop the scattered duplicates.
//!
//! ## INV pin map (W36-PROPTEST-FU-001 closure)
//!
//! This crate is the apex types crate; its single INV reference
//! (`INV-DATA-RESIDENCY` in `types::region`) is a **policy pin**
//! documenting how `Region` participates in the data-residency
//! invariant, not a load-bearing property test. Per
//! WI-PROPTEST-FU-W33-001 closure
//! (`specs/_audits/sealed/2026-05-26-w36-proptest-fu-001-seal.md`), the
//! property test lives in the owning crate listed below; this crate
//! is listed in `scripts/proptest-density-allowlist.txt` as an
//! "inv-pin documentation" exemption:
//!
//! | INV ref pinned here   | Property-test owner crate                       |
//! |-----------------------|-------------------------------------------------|
//! | `INV-DATA-RESIDENCY`  | `corelink-signup` (regional-pin at signup path) |

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod errors;
pub mod time;
pub mod types;

// Top-level convenience re-exports so consumers can
// `use corelink_core::{TenantId, Digest, Region, SecretWrap};`.
pub use errors::{CoreError, DigestParseError};
pub use time::Clock;
pub use types::{Digest, Region, SecretWrap, TenantId, DIGEST_LEN};

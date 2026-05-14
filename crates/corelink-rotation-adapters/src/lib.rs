//! `corelink-rotation-adapters` — per-asset rotation adapters implementing
//! the [`RotationAdapter`] trait (WI-S13-003).
//!
//! # What this crate ships
//!
//! Per the CoreLink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the rotation-adapter primitive: trait surfaces every
//! production KMS + D1 + Cloudflare Workers Secrets binding will
//! satisfy, plus in-memory adapters that exercise every load-bearing
//! invariant the production wiring relies on. Property tests pinned at
//! 10k iter (PR gate; nightly 100k via `PROPTEST_CASES` env var) against
//! the orchestrator cover INV-KEY-OVERLAP (old + new both valid for reads
//! during overlap; writes only on new), INV-KEY-NO-SKIP (writes never in
//! invalid state: pending / retired / destroyed), INV-KEY-AUDIT (every
//! state transition emits an audit record).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`error`] module ships [`RotationError`] `#[non_exhaustive]`
//!    taxonomy: `DownstreamErrorThreshold` / `InvalidTransition` /
//!    `OverlapExceedsHardUpper` / `RotationInFlight` / `Kms` /
//!    `Storage` / `Audit`.
//! 2. The [`types`] module ships [`AssetClass`] (5-element taxonomy:
//!    Tdk / PatSigning / AuditChain / AdminSigning / Byok) with
//!    `overlap_seconds()` + `hard_upper_bound_seconds()` canonical per
//!    `key_management.md §3.2.1` + ADR-0018; [`KeyState`]
//!    `#[non_exhaustive]` 6-state taxonomy (Pending / Active / Overlap /
//!    Retired / Destroyed / RolledBack); [`KeyHandle`] (key_id / asset
//!    class / state / timestamps).
//! 3. The [`adapter`] module ships the [`RotationAdapter`] trait (7
//!    async methods: generate / promote / rekey_downstream / retire /
//!    destroy / rollback / downstream_error_rate) + the canonical valid
//!    write predicate [`is_valid_write_state`] (only Active; rejects
//!    Pending / Retired / Destroyed / RolledBack per INV-KEY-NO-SKIP) +
//!    the canonical read predicate [`is_valid_read_state`] (Active OR
//!    Overlap; per INV-KEY-OVERLAP).
//! 4. The [`tdk`] module ships [`TdkRotationAdapter`] (7d overlap;
//!    S-01 envelope re-wrap; in-memory fake for CI).
//! 5. The [`pat_signing`] module ships [`PatSigningRotationAdapter`]
//!    (24h overlap; multi-key signing_key_id column; S-03).
//! 6. The [`audit_chain`] module ships [`AuditChainRotationAdapter`]
//!    (24h overlap; per-region; S-09 daily verifier compatible).
//! 7. The [`admin_signing`] module ships [`AdminSigningRotationAdapter`]
//!    (24h overlap; per-region HMAC-SHA256; dual-approval; WI-S13-002).
//! 8. The [`byok`] module ships [`ByokRotationAdapter`] stub (7d
//!    overlap; customer-trigger; S-14 forward).
//! 9. The [`erasure_attestation`] module ships
//!    [`ErasureAttestationRotationAdapter`] (30d overlap; per-region;
//!    Ed25519 attestation key rotation; WI-S14-007).
//!
//! # INV-KEY-OVERLAP enforcement
//!
//! During the overlap window both the previous Active key (now Overlap)
//! and the new Active key are accepted for reads. Only the new Active
//! key accepts writes. This is enforced at the trait surface via
//! [`is_valid_read_state`] and [`is_valid_write_state`].
//!
//! # INV-KEY-NO-SKIP enforcement
//!
//! Writes are only accepted for keys in `Active` state. Any attempt to
//! write with a key in `Pending`, `Retired`, `Destroyed`, or
//! `RolledBack` state returns `RotationError::InvalidTransition`.
//!
//! # Hard upper bound 30d (key_management.md §3.2.1)
//!
//! Every adapter validates `overlap_seconds ≤ 30d` before accepting a
//! promote call. `RotationError::OverlapExceedsHardUpper` is returned on
//! violation; an ADR + Security lead sign-off is required to override.
//!
//! # State machine canonical (INV-KEY-NO-SKIP)
//!
//! ```text
//! pending --[promote]--> active --[next rotation]--> overlap
//!                                                        |
//!                                                   [overlap window expired]
//!                                                        |
//!                                                     retired --[grace 90d]--> destroyed
//!          active --[downstream errors > 1%]--> rolled_back
//! ```
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is wasm32-clean: no `std::time`, no `tokio`, no OS
//! syscalls. All timestamps are passed in as `u64` milliseconds by the
//! caller (Cloudflare Workers uses `Date.now()` at the binding layer).
//!
//! # Production wiring (deferred to WI-S13-006 PRR ship gate)
//!
//! - Cloudflare Workers Secrets KMS binding.
//! - D1 `rotation_state` table + UNIQUE index per (asset_class, region).
//! - D1 atomic batch [state transition UPDATE + audit_outbox INSERT]
//!   (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//! - CloudEvent emission per state transition (INV-KEY-AUDIT).
//! - Admin signing key push to dual-approval DO cache (WI-S13-002).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod adapter;
pub mod admin_signing;
pub mod audit_chain;
pub mod byok;
pub mod erasure_attestation;
pub mod error;
pub mod pat_signing;
pub mod tdk;
pub mod types;

pub use adapter::{is_valid_read_state, is_valid_write_state, RotationAdapter};
pub use admin_signing::AdminSigningRotationAdapter;
pub use audit_chain::AuditChainRotationAdapter;
pub use byok::ByokRotationAdapter;
pub use erasure_attestation::ErasureAttestationRotationAdapter;
pub use error::RotationError;
pub use pat_signing::PatSigningRotationAdapter;
pub use tdk::TdkRotationAdapter;
pub use types::{AssetClass, KeyHandle, KeyState};

/// Crate canonical schema version.
#[must_use]
pub const fn rotation_adapters_schema_version() -> u32 {
    1
}

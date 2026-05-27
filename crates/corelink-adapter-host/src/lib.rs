//! `corelink-adapter-host` — Wave 35 bridge crate.
//!
//! Bridges the adapter-local port traits declared by each Wave-34 adapter
//! (`corelink-adapter-{cargo,npm,pip,brew,oci}`) to the canonical workspace
//! Stage-1 SPI traits (`CasReadHandler`, `CasWriteHandler`, `PatValidator`,
//! `KvBackend`, `AuditEmitter`). Production binaries compose these bridges
//! at boot; the adapters themselves remain free of workspace SPI imports.
//!
//! # Module map
//!
//! | Module | Adapter bridged | Bridges implemented |
//! |---|---|---|
//! | [`cargo`] | `corelink-adapter-cargo` | `CasStore → CasReadHandler+CasWriteHandler`, `TenantResolver → PatValidator` |
//! | [`brew`] | `corelink-adapter-brew` | `CasStore → CasReadHandler+CasWriteHandler`, `TenantResolver → PatValidator` |
//! | [`npm`] | `corelink-adapter-npm` | `CasStore`, `KvStore`, `TenantResolver` |
//! | [`pip`] | `corelink-adapter-pip` | `CasStore`, `KvStore`, `TenantResolver` |
//! | [`oci`] | `corelink-adapter-oci` | `BlobStore → CasReadHandler+CasWriteHandler`, `ManifestKvStore`, `TenantResolver` |
//!
//! # Bridging approach
//!
//! The adapter port traits are `async`; the workspace SPI traits
//! (`CasReadHandler`, `CasWriteHandler`, `AuditEmitter`) are sync.
//! Bridges call sync workspace handlers from async adapter ports by
//! spawning the sync call onto `tokio::task::spawn_blocking` so the
//! async runtime thread is never blocked.
//!
//! [`KvBackend`] uses RPITIT (`impl Future`) and is therefore not
//! object-safe. Bridges that target `KvBackend` are generic over
//! `K: KvBackend + Send + Sync + fmt::Debug + 'static`.
//!
//! # Wave 35 Phase 2 absorption
//!
//! Per `specs/_audits/2026-05-26-w35-p2-adapter-host-absorption.md`
//! (SEALED 2026-05-26), the 5 Wave-34 adapter crates were physically
//! consolidated into this umbrella as inline `mod` submodules
//! (9,873 LOC + 192 tests moved in-tree); the per-adapter
//! `corelink-adapter-{brew,cargo,npm,oci,pip}` crates were dropped
//! from `workspace.members`. The pre-absorption Wave-35 bridge surface
//! is preserved 1:1 — each submodule now hosts BOTH the absorbed
//! adapter source AND the SPI bridge. Public-API paths
//! (`corelink_adapter_host::<adapter>::*`) remain stable; charter
//! `#![forbid(unsafe_code)]` + `#![deny(missing_docs)]` +
//! `#[non_exhaustive]` discipline preserved by reference.
//!
//! Absorbed crates (5):
//!
//! - `corelink-adapter-brew` → [`brew`] — Homebrew bottle proxy
//!   (1,157 LOC, 20 tests).
//! - `corelink-adapter-cargo` → [`cargo`] — cargo registry proxy
//!   (1,028 LOC, 28 tests).
//! - `corelink-adapter-npm` → [`npm`] — npm registry proxy
//!   (1,801 LOC, 46 tests).
//! - `corelink-adapter-oci` → [`oci`] — OCI distribution registry
//!   (3,551 LOC, 45 tests).
//! - `corelink-adapter-pip` → [`pip`] — PyPI registry proxy
//!   (2,336 LOC, 53 tests).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod brew;
pub mod cargo;
pub mod npm;
pub mod oci;
pub mod pip;

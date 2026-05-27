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
//! `KvBackend` uses RPITIT (`impl Future`) and is therefore not
//! object-safe. Bridges that target `KvBackend` are generic over
//! `K: KvBackend + Send + Sync + fmt::Debug + 'static`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod brew;
pub mod cargo;
pub mod npm;
pub mod oci;
pub mod pip;

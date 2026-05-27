//! Absorbed `corelink-adapter-cargo` — sccache HTTP build cache adapter.
//!
//! Wave-35 Phase 2 absorbed the former `corelink-adapter-cargo` crate into
//! this module. The adapter bridges sccache's HTTP storage backend and
//! CoreLink's per-tenant content-addressable storage. With
//! `SCCACHE_ENDPOINT=http://corelink-cargo-adapter` set, every `rustc`
//! invocation wrapped by sccache (via `RUSTC_WRAPPER=sccache`) issues
//! `GET`/`PUT`/`HEAD` requests against this adapter. The adapter:
//!
//! 1. authenticates the caller via PAT (`Authorization: Bearer
//!    hugr-pat_<token>`; constant-time prefix verified via `subtle`);
//! 2. resolves the PAT to a `TenantId` via the adapter-local
//!    [`ports::TenantResolver`] (see [`ports`]);
//! 3. translates the sccache BLAKE3 key (URL path segment) to a
//!    CoreLink CAS digest — zero translation cost as both use BLAKE3;
//! 4. on `GET`/`HEAD`: serves from CAS on hit; 404 on miss (sccache
//!    falls back to local compilation);
//! 5. on `PUT`: emits an audit row BEFORE the CAS put (fail-CLOSED
//!    invariant); stores the artifact.
//!
//! ## Bridges
//!
//! See [`bridge`] for `CargoCasBridge` and `CargoTenantBridge`.

pub mod audit;
pub mod auth;
pub mod bridge;
pub mod config;
pub mod error;
pub mod ports;
pub mod server;
pub mod translate;

pub use bridge::{CargoCasBridge, CargoTenantBridge};
pub use config::CargoAdapterConfig;
pub use error::CargoAdapterError;
pub use server::run_cargo_adapter;

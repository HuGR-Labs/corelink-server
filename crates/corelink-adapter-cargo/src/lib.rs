//! `corelink-adapter-cargo` — sccache HTTP build cache adapter.
//!
//! Wave-34 lands this crate as the bridge between sccache's HTTP
//! storage backend and CoreLink's per-tenant content-addressable storage.
//! With `SCCACHE_ENDPOINT=http://corelink-cargo-adapter` set, every
//! `rustc` invocation wrapped by sccache (via `RUSTC_WRAPPER=sccache`)
//! issues `GET`/`PUT`/`HEAD` requests against this adapter. The adapter:
//!
//! 1. authenticates the caller via PAT (`Authorization: Bearer
//!    hugr-pat_<token>`; constant-time prefix verified via `subtle`);
//! 2. resolves the PAT to a `TenantId` via the adapter-local
//!    `TenantResolver` port (see `ports.rs`);
//! 3. translates the sccache BLAKE3 key (URL path segment) to a
//!    CoreLink CAS digest — zero translation cost as both use BLAKE3;
//! 4. on `GET`/`HEAD`: serves from CAS on hit; 404 on miss (sccache
//!    falls back to local compilation);
//! 5. on `PUT`: emits an audit row BEFORE the CAS put (fail-CLOSED
//!    invariant); stores the artifact.
//!
//! ## Scope (per `specs/_proposals/adapters/cargo.md` §1)
//!
//! **In scope:** binary artifact caching for `rustc` invocations.
//! **Out of scope:** cargo registry mirror, source-level caching,
//! cargo-lock pinning, dependency resolution.
//!
//! ## Charter constraints enforced
//!
//! - `#![forbid(unsafe_code)]`: enforced at crate root.
//! - `#[non_exhaustive]`: every `pub` type below carries the attribute
//!   so future fields can be added without breaking call sites.
//! - No `unwrap()`/`expect()`/`panic!()` in `src/` (clippy `deny`).
//! - PAT compared via `subtle::ConstantTimeEq`; secrets boxed in
//!   `secrecy::SecretString`.
//! - Audit emit BEFORE CAS state mutation — see `server::handle_put`.

#![forbid(unsafe_code)]

pub mod audit;
pub mod auth;
pub mod config;
pub mod error;
pub mod ports;
pub mod server;
pub mod translate;

pub use config::CargoAdapterConfig;
pub use error::CargoAdapterError;
pub use server::run_cargo_adapter;

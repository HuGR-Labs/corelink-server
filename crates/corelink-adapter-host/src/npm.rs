//! Absorbed `corelink-adapter-npm` — npm registry caching proxy.
//!
//! Wave-35 Phase 2 absorbed the former `corelink-adapter-npm` crate into
//! this module. Implements the npm registry API subset used by
//! `npm install` (and `pnpm` / `yarn` / `bun`) so that clients consult
//! CoreLink as a transparent caching mirror in front of
//! `registry.npmjs.org`. Package metadata (JSON) is cached in a
//! tenant-scoped key-value store with a configurable TTL; tarballs
//! (`.tgz`) are stored in the CoreLink content-addressable store (CAS)
//! keyed by SHA256 of the tarball URL. Read-only mirror (no
//! `npm publish`).
//!
//! The adapter performs a mandatory pre-CAS-store integrity check
//! that verifies the downloaded tarball's SHA1 against the
//! `dist.shasum` field published in the npm metadata for that
//! version. On mismatch it emits `corelink.npm.tarball.integrity_mismatch.v1`
//! (fail-CLOSED) and rejects the store.
//!
//! ## Bridges
//!
//! See [`bridge`] for `NpmCasBridge`, `NpmKvBridge<K>`, and
//! `NpmTenantBridge` — these map the adapter-local
//! [`ports::CasStore`] / [`ports::KvStore`] / [`ports::TenantResolver`]
//! onto the workspace `CasReadHandler`/`CasWriteHandler`/`KvBackend`
//! /`PatValidator` SPI traits.

pub mod audit;
pub mod auth;
pub mod bridge;
pub mod config;
pub mod error;
pub mod metadata;
pub mod ports;
pub mod search;
pub mod server;
pub mod tarball;
pub mod upstream;

pub use bridge::{NpmCasBridge, NpmKvBridge, NpmTenantBridge};
pub use config::NpmAdapterConfig;
pub use error::NpmAdapterError;
pub use server::run_npm_adapter;

//! Wave-34 — CoreLink npm registry caching proxy adapter.
//!
//! Implements the npm registry API subset used by `npm install`
//! (and `pnpm` / `yarn` / `bun`) so that clients consult CoreLink
//! as a transparent caching mirror in front of `registry.npmjs.org`.
//! Package metadata (JSON) is cached in a tenant-scoped key-value
//! store with a configurable TTL; tarballs (`.tgz`) are stored in
//! the CoreLink content-addressable store (CAS) keyed by SHA256 of
//! the tarball URL. Read-only mirror (no `npm publish`).
//!
//! The adapter performs a mandatory pre-CAS-store integrity check
//! that verifies the downloaded tarball's SHA1 against the
//! `dist.shasum` field published in the npm metadata for that
//! version. On mismatch it emits `corelink.npm.tarball.integrity_mismatch.v1`
//! (fail-CLOSED) and rejects the store.
//!
//! See the dispatch packet at `specs/_proposals/adapters/npm.md` and
//! the SEAL audit at `specs/_audits/2026-05-26-w34-adapter-npm-v2.md`
//! for the contract, acceptance criteria, and parallel-safety
//! analysis with the sibling adapter campaigns (cargo / pip / brew /
//! oci).
//!
//! ## Module layout
//!
//! - [`config`]: `NpmAdapterConfig` + env-var loader.
//! - [`error`]: `NpmAdapterError` (terminal error enum, `#[non_exhaustive]`).
//! - [`ports`]: locally-defined port traits (`CasStore`, `KvStore`,
//!   `TenantResolver`) the adapter consumes. Per the Wave-34 charter
//!   "Concurrent agents (parallel-safe per Section 0.6)" decision,
//!   each per-package-manager adapter declares its own minimal port
//!   surface inline; consolidation into a workspace-level
//!   `corelink-adapter-ports` crate is deferred to a follow-up
//!   sequential wave.
//! - [`auth`]: PAT → tenant resolution; `Bearer` token extraction;
//!   `SecretString` for PAT bytes.
//! - [`audit`]: thin adapter audit emit helper; always emits BEFORE
//!   the state-mutating CAS / KV write returns.
//! - [`upstream`]: `reqwest` client for `registry.npmjs.org`
//!   (metadata JSON + tarball bytes).
//! - [`metadata`]: `GET /<pkg>` metadata cache logic.
//! - [`tarball`]: `GET /<pkg>/-/<x>.tgz` CAS-backed tarball cache
//!   with mandatory integrity check.
//! - [`server`]: `axum` router wiring + the `run_npm_adapter` entry
//!   point.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod audit;
pub mod auth;
pub mod config;
pub mod error;
pub mod metadata;
pub mod ports;
pub mod server;
pub mod tarball;
pub mod upstream;

pub use config::NpmAdapterConfig;
pub use error::NpmAdapterError;
pub use server::run_npm_adapter;

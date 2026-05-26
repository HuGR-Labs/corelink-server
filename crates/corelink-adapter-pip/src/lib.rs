//! Wave-34 — CoreLink pip (PyPI) caching proxy adapter.
//!
//! Implements PEP 691 (JSON Simple Index, preferred) + PEP 503 (HTML
//! Simple Repository API, fallback) so that `pip install`, `uv`,
//! `poetry`, and `pdm` consult CoreLink as a transparent caching
//! mirror in front of `pypi.org`. Wheels (`.whl`) and source dists
//! (`.tar.gz`) are stored in the CoreLink content-addressable store
//! (CAS) keyed by their `#sha256=` URL-fragment digest; index JSON is
//! cached in a tenant-scoped key-value store with a configurable TTL.
//!
//! Read-only mirror (no `twine` / upload, no private indexes). The
//! adapter performs a mandatory pre-CAS-store integrity check that
//! rejects wheels whose actual downloaded SHA256 does not match the
//! upstream URL `#sha256=` fragment and emits a fail-CLOSED audit
//! event on mismatch.
//!
//! See the dispatch packet at `specs/_proposals/adapters/pip.md` and
//! the SEAL audit at `specs/_audits/2026-05-26-w34-adapter-pip.md`
//! for the contract, acceptance criteria, and parallel-safety
//! analysis with the sibling adapter campaigns (cargo / npm / brew /
//! oci).
//!
//! ## Module layout
//!
//! - [`config`]: `PipAdapterConfig` + env-var loader.
//! - [`error`]: `PipAdapterError` (terminal error enum, `#[non_exhaustive]`).
//! - [`ports`]: locally-defined port traits (`CasStore`, `KvStore`,
//!   `TenantResolver`) the adapter consumes. Per the Wave-34 charter
//!   "Concurrent agents (parallel-safe per Section 0.6)" decision,
//!   each per-package-manager adapter declares its own minimal port
//!   surface inline so the five sibling campaigns can land without
//!   editing any shared crate; consolidation into a workspace-level
//!   `corelink-adapter-ports` crate is deferred to a follow-up
//!   sequential wave.
//! - [`auth`]: PAT → tenant resolution; constant-time PAT compare via
//!   [`subtle::ConstantTimeEq`]; `SecretString` for the PAT bytes.
//! - [`audit`]: thin adapter audit emit helper; always emits BEFORE
//!   the state-mutating CAS / KV write returns, per the
//!   audit-fail-CLOSED contract (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//! - [`upstream`]: `reqwest` client for `pypi.org` (index JSON +
//!   wheel/sdist bytes).
//! - [`pep503_html`]: bidirectional JSON ↔ PEP 503 HTML re-encoder
//!   for clients that do not negotiate `application/vnd.pypi.simple.v1+json`.
//! - [`index`]: `GET /simple/<project>/` index logic (JSON + HTML).
//! - [`wheel`]: `GET /<wheel_url>` CAS-backed wheel/sdist cache.
//! - [`server`]: `axum` router wiring + the `run_pip_adapter` entry
//!   point.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod audit;
pub mod auth;
pub mod config;
pub mod error;
pub mod index;
pub mod pep503_html;
pub mod ports;
pub mod server;
pub mod upstream;
pub mod wheel;

pub use config::PipAdapterConfig;
pub use error::PipAdapterError;
pub use server::run_pip_adapter;

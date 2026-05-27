//! Absorbed `corelink-adapter-pip` — PyPI caching proxy.
//!
//! Wave-35 Phase 2 absorbed the former `corelink-adapter-pip` crate into
//! this module. Implements PEP 691 (JSON Simple Index, preferred) +
//! PEP 503 (HTML Simple Repository API, fallback) so that
//! `pip install`, `uv`, `poetry`, and `pdm` consult CoreLink as a
//! transparent caching mirror in front of `pypi.org`. Wheels (`.whl`)
//! and source dists (`.tar.gz`) are stored in the CoreLink
//! content-addressable store (CAS) keyed by their `#sha256=`
//! URL-fragment digest; index JSON is cached in a tenant-scoped
//! key-value store with a configurable TTL.
//!
//! Read-only mirror (no `twine` / upload, no private indexes). The
//! adapter performs a mandatory pre-CAS-store integrity check that
//! rejects wheels whose actual downloaded SHA256 does not match the
//! upstream URL `#sha256=` fragment and emits a fail-CLOSED audit
//! event on mismatch.
//!
//! ## Bridges
//!
//! See [`bridge`] for `PipCasBridge`, `PipKvBridge<K>`, and
//! `PipTenantBridge`.

pub mod audit;
pub mod auth;
pub mod bridge;
pub mod config;
pub mod error;
pub mod index;
pub mod pep503_html;
pub mod ports;
pub mod server;
pub mod upstream;
pub mod wheel;

pub use bridge::{PipCasBridge, PipKvBridge, PipTenantBridge};
pub use config::PipAdapterConfig;
pub use error::PipAdapterError;
pub use server::run_pip_adapter;

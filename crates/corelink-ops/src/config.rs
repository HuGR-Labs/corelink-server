//! Config — wave-33 canonical ops aggregator (W35-P2 update).
//!
//! Two config-context crates folded under the canonical `config`
//! submodule:
//!
//! - [`api`] — physically absorbed (W35-P2-OPS): config REST API.
//!   Was `corelink-config-api`. Lives at
//!   `crates/corelink-ops/src/config/api.rs` plus its sibling files
//!   under `crates/corelink-ops/src/config/api/`.
//! - [`durable_object`] — `corelink-config-do` (still external):
//!   config Durable Object (CF Worker DO-backed storage).

pub mod api;

/// Config Durable Object (CF Worker DO-backed storage).
///
/// Re-exports the entire public API of `corelink-config-do`.
pub mod durable_object {
    pub use corelink_config_do::*;
}

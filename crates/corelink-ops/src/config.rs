//! Config — wave-33 canonical ops aggregator.
//!
//! Two config-context crates folded under the canonical `config`
//! submodule as two sub-submodules (Stage 1 Stream C sub-step C.2
//! Option-A aggregator pattern):
//!
//! - [`api`] — `corelink-config-api`: config REST API.
//! - [`durable_object`] — `corelink-config-do`: config Durable Object
//!   (CF Worker DO-backed storage).

/// Config REST API.
///
/// Re-exports the entire public API of `corelink-config-api`.
pub mod api {
    pub use corelink_config_api::*;
}

/// Config Durable Object (CF Worker DO-backed storage).
///
/// Re-exports the entire public API of `corelink-config-do`.
pub mod durable_object {
    pub use corelink_config_do::*;
}

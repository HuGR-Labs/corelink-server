//! DPA acceptance + versioning — wave-33 canonical surface.
//!
//! Two DPA-related submodules:
//!
//! - [`acceptance`] — `corelink-dpa-acceptance` (still external; not in
//!   the W35-P2 absorb scope): DPA click-through 6-field consent +
//!   RS256 JWT receipt + 3 locales; re-exported here as a thin
//!   façade.
//! - [`versioning`] — physically absorbed in W35-P2-PRIVACY (was
//!   `corelink-dpa-versioning`): DPA versioning + 30-day grace +
//!   read-only degrade middleware. Lives at
//!   `crates/corelink-privacy/src/dpa/versioning.rs` plus its sibling
//!   files under `crates/corelink-privacy/src/dpa/versioning/`.

/// DPA click-through 6-field consent + RS256 JWT receipt + 3 locales.
///
/// Re-exports the entire public API of the external
/// `corelink-dpa-acceptance` crate (NOT absorbed in W35-P2; remains a
/// standalone workspace member).
pub mod acceptance {
    pub use corelink_dpa_acceptance::*;
}

pub mod versioning;

//! DPA acceptance + versioning — wave-33 canonical surface.
//!
//! Two absorbed DPA-related crates folded under the canonical `dpa`
//! submodule as two sub-submodules so consumers can target the exact
//! granularity they need (Stage 1 Stream B sub-step B.5 Option-A
//! aggregator pattern):
//!
//! - [`acceptance`] — `corelink-dpa-acceptance`: DPA click-through
//!   6-field consent + RS256 JWT receipt + 3 locales.
//! - [`versioning`] — `corelink-dpa-versioning`: DPA versioning +
//!   30-day grace + read-only degrade middleware.

/// DPA click-through 6-field consent + RS256 JWT receipt + 3 locales.
///
/// Re-exports the entire public API of `corelink-dpa-acceptance`.
pub mod acceptance {
    pub use corelink_dpa_acceptance::*;
}

/// DPA versioning + 30-day grace + read-only degrade middleware.
///
/// Re-exports the entire public API of `corelink-dpa-versioning`.
pub mod versioning {
    pub use corelink_dpa_versioning::*;
}

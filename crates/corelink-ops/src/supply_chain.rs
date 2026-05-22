//! Supply chain — wave-33 canonical ops aggregator.
//!
//! Two supply-chain context crates folded under the canonical
//! `supply_chain` submodule as two sub-submodules (Stage 1 Stream C
//! sub-step C.2 Option-A aggregator pattern):
//!
//! - [`policy`] — `corelink-supply-chain-policy`: supply-chain policy
//!   engine (rules + allow-list / deny-list).
//! - [`verify`] — `corelink-supply-verify`: supply-chain verification
//!   (SBOM + cosign attestation).

/// Supply-chain policy engine (rules + allow-list / deny-list).
///
/// Re-exports the entire public API of `corelink-supply-chain-policy`.
pub mod policy {
    pub use corelink_supply_chain_policy::*;
}

/// Supply-chain verification (SBOM + cosign attestation).
///
/// Re-exports the entire public API of `corelink-supply-verify`.
pub mod verify {
    pub use corelink_supply_verify::*;
}

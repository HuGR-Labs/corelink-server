//! Supply chain — wave-33 canonical ops aggregator (W35-P2 update).
//!
//! Both supply-chain context crates physically absorbed (W35-P2-OPS):
//!
//! - [`policy`] — was `corelink-supply-chain-policy`: supply-chain
//!   policy engine (rules + allow-list / deny-list).
//! - [`verify`] — was `corelink-supply-verify`: supply-chain
//!   verification (SBOM + cosign attestation).

pub mod policy;
pub mod verify;

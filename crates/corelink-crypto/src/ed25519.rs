//! Ed25519 (FIPS 186-5 EdDSA) primitives — wave-33 canonical surface.
//!
//! Currently surfaces the [`attestation`] submodule (BYOK DSR erasure
//! attestation). Future Stage 1 work may extend this with additional
//! Ed25519-rooted primitives; the namespace is reserved for that
//! growth.

pub mod attestation {
    //! Per-region Ed25519 erasure attestation — wave-33 canonical
    //! surface.
    //!
    //! Re-exports the entire public API of
    //! `corelink-erasure-attestation`. The actual implementation lives
    //! in `crates/corelink-erasure-attestation/` (Stage 0 sub-step 2
    //! Option-A aggregator pattern; see crate-level rustdoc).

    pub use corelink_erasure_attestation::*;
}

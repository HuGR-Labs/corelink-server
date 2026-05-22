//! 12-backend erasure worker + Ed25519 signed report — wave-33 canonical surface.
//!
//! Re-exports the entire public API of
//! `corelink-privacy-erasure-worker`. The actual implementation lives
//! in `crates/corelink-privacy-erasure-worker/` (Stage 1 Stream B
//! sub-step B.5 Option-A aggregator pattern).
//!
//! The Ed25519 signing primitive used to sign completion reports
//! lives at `corelink_crypto::ed25519::attestation` (absorbed in
//! Stage 0); the erasure worker consumes it unchanged — charter
//! "Ed25519 erasure attestation signing path preserved" guarantee
//! holds by reference.

pub use corelink_privacy_erasure_worker::*;

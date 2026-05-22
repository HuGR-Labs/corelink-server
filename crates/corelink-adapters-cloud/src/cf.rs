//! CF Worker R2 / D1 / KV / DO binding adapters — wave-33 canonical
//! cloud-adapter surface.
//!
//! Re-exports the entire public API of `corelink-cf-bindings`. The
//! actual implementation lives in `crates/corelink-cf-bindings/`
//! (Stage 1 Stream C sub-step C.3 Option-A aggregator pattern).
//!
//! **wasm32-only:** per-module `#[cfg(target_arch = "wasm32")]` gates
//! in `corelink-cf-bindings` keep the wasm32-only types out of the
//! native rlib. On native targets this re-export resolves to an empty
//! surface.

pub use corelink_cf_bindings::*;

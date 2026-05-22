//! Clerk JWKS on CF KV / Fetch — wave-33 canonical cloud-adapter
//! surface.
//!
//! Re-exports the entire public API of `corelink-clerk-cf` —
//! `CfKvJwksCache`, `CfJwksFetcher`, the GET /health proof-of-concept
//! handler, and the `prod_wiring` orchestration glue. Compiles on
//! native (pure-logic) and on wasm32 (the `#[durable_object]` actor
//! class is gated wasm32-only inside the absorbed crate). The actual
//! implementation lives in `crates/corelink-clerk-cf/` (Stage 1
//! Stream C sub-step C.3 Option-A aggregator pattern).

pub use corelink_clerk_cf::*;

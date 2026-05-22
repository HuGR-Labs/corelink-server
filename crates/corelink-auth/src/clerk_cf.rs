//! Clerk CF Worker bindings — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-clerk-cf`. The actual
//! implementation lives in `crates/corelink-clerk-cf/` and includes a
//! wasm32-only `#[durable_object]` actor wire-up; the wasm32-gated
//! portion remains physically in the absorbed crate (Stage 2 binding
//! consolidation owns the wasm-bridged move).
//!
//! Native consumers see the pure-logic surface; wasm32 consumers see
//! the full Cloudflare Worker actor surface. Both reachable via this
//! re-export.

pub use corelink_clerk_cf::*;

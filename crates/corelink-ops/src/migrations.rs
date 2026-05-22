//! D1 migration replay harness — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-d1-migrations` (R2-13
//! rusqlite-backed CI gate verifying that every `migrations/d1/*.sql`
//! applies cleanly in numeric order against a fresh in-memory SQLite;
//! catches ordering bugs + schema dependency hazards). The actual
//! implementation lives in `crates/corelink-d1-migrations/` (Stage 1
//! Stream C sub-step C.2 Option-A aggregator pattern).

pub use corelink_d1_migrations::*;

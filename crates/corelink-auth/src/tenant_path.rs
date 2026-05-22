//! CTRL-CAS-001 tenant-path prefix enforcer — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-tenant-path` (the
//! package whose directory is `crates/tenant-path/` but whose package
//! name is `corelink-tenant-path`). The actual implementation lives
//! at that path (Stage 1 Stream B sub-step B.3 Option-A aggregator
//! pattern).
//!
//! The crate ships derive-prefix benches (`derive`,
//! `derive_prefix_v2`, `derive_prefix_cached`) under `benches/` that
//! exercise the prefix-enforcement hot path; benches are not part of
//! the re-export surface (they execute under `cargo bench` against
//! the absorbed crate directly).

pub use corelink_tenant_path::*;

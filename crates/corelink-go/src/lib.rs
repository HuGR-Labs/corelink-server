//! CoreLink Go cgo bridge — Rust side.
//!
//! This crate produces a `cdylib` + `staticlib` consumed by the
//! `corelink-go` Go module via cgo. It re-exports the C-ABI surface of
//! `corelink-client-verify` (feature `ffi`) and adds Go-specific helpers
//! for:
//!
//! - Construction with explicit `client_verify_enabled` flag.
//! - An `IsClientVerifyEnabled` accessor (maps to Go method on the cgo
//!   handle). Test inspection: `assert client.IsClientVerifyEnabled() == true`.
//! - `Put` — BLAKE3 digest computation (single Rust truth; Go wrapper
//!   calls this for local digest generation before upload).
//!
//! All C-ABI symbols live in the `ffi` sub-module, gated behind
//! `unsafe_code` locally (same pattern as `corelink-client-verify/ffi.rs`).

#![deny(unsafe_code)]

/// Re-export the client-verify C-ABI surface so Go cgo only needs to link
/// one shared library.
pub use corelink_client_verify::ffi::*;

pub mod go_bridge;

pub use go_bridge::{
    corelink_go_client_free, corelink_go_client_is_verify_enabled, corelink_go_client_new,
    corelink_go_client_put, corelink_go_client_verify_get,
};

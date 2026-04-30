//! CoreLink client-side digest verify SDK helper.
//!
//! Stand-alone Rust crate that SDK clients (Python pyO3, Go cgo, JS/TS
//! WASM) consume to verify integrity post-download. Implements
//! CTRL-CAS-002 / INV-CAS-INTEGRITY at the client edge: the body that
//! the server returned MUST match the digest the SDK was asked to
//! fetch, every time, by default.
//!
//! See `specs/04_sprints/S02/work_items/WI-S02-003-corelink-client-verify-crate.md`
//! for the canonical scope; this crate is the Rust implementation.
//!
//! # Two-layer API surface
//!
//! - **Rust-native** ([`ClientVerifier`], [`VerifyConfig`], [`VerifyError`]) —
//!   idiomatic Result, optional async stream behind feature `stream`,
//!   used directly by `corelink-server` and `corelink-cli`.
//! - **C-ABI** (module [`ffi`], gated behind feature `ffi`) — opaque
//!   handle + `extern "C"` functions + `i32` error codes; consumed via
//!   cbindgen by Python pyO3 + Go cgo wrappers in S-15. JS/WASM uses a
//!   separate wasm-bindgen pipeline, NOT cbindgen.
//!
//! # Default-on policy
//!
//! [`VerifyConfig::default`] is `enabled=true, warn_on_optout=true`.
//! Opt-out requires the explicit [`VerifyConfig::disabled`] constructor,
//! which causes [`ClientVerifier::new`] to emit a `tracing::warn!`
//! event + bump the [`opt_out_total`] counter (forwarded to Grafana
//! `corelink_client_verify_optout_total{lang}` in S-15).
//!
//! # Quickstart
//!
//! ```rust
//! use corelink_client_verify::{ClientVerifier, Digest, VerifyConfig};
//!
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let v = ClientVerifier::new(VerifyConfig::default());
//! let body = b"hello world";
//! // In real SDK code, `expected` comes from the digest the caller
//! // asked the server for (e.g. as the cache key).
//! let expected = Digest::compute(body);
//! v.verify(body, &expected)?;
//! # Ok(())
//! # }
//! ```

// Crate-wide unsafe is denied; the FFI module re-enables it locally
// with a documented `#![allow(unsafe_code)]` because the cbindgen-
// stable surface uses raw pointers + opaque handles. Every other
// module sits under `#![deny(unsafe_code)]` (stricter than `forbid`
// per WI-S02-003 design — `forbid` cannot be locally overridden,
// which would prevent the intentional FFI exception).
#![deny(unsafe_code)]

#[deny(unsafe_code)]
pub mod config;
#[deny(unsafe_code)]
pub mod digest;
#[deny(unsafe_code)]
pub mod error;
#[deny(unsafe_code)]
pub mod verifier;

#[cfg(feature = "stream")]
#[deny(unsafe_code)]
pub mod stream;

// FFI module is gated behind `--features ffi` so the default workspace
// build neither emits the `extern "C"` symbols nor pays the cdylib
// linker cost. The `corelink-client-verify` SDK distribution build
// flips this feature on. WI-S02-003 §6.1 + §10.3.5.
#[cfg(feature = "ffi")]
pub mod ffi;

pub use config::VerifyConfig;
pub use digest::{Digest, COR_CAS_DIGEST_MISMATCH, COR_CAS_VERIFY_DISABLED, DIGEST_LEN};
pub use error::VerifyError;
pub use verifier::{opt_out_total, ClientVerifier};

#[cfg(feature = "stream")]
pub use error::StreamVerifyError;

#[cfg(feature = "stream")]
pub use stream::STREAM_CHUNK_BYTES;

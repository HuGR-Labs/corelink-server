//! Client-side SDK verifier — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-client-verify`. The
//! actual implementation (including the `extern "C"` FFI surface
//! consumed by Go cgo + Python pyO3 SDK wrappers via cbindgen) lives
//! in `crates/corelink-client-verify/` (Stage 0 sub-step 2 Option-A
//! aggregator pattern; see crate-level rustdoc). Physical absorption
//! deferred to the FFI-aware Stage 1 stream that owns SDK paths
//! atomically — see `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md`
//! §7 hard-pause-trigger-1 partial activation.

pub use corelink_client_verify::*;

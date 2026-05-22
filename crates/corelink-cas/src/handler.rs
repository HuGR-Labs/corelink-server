//! CAS handler (write + read state machines) — wave-33 canonical CAS
//! surface.
//!
//! Re-exports the entire public API of `corelink-handler-cas`. The
//! actual implementation lives in `crates/corelink-handler-cas/`
//! (Stage 1 Stream A sub-step A.1 Option-A aggregator pattern).

pub use corelink_handler_cas::*;

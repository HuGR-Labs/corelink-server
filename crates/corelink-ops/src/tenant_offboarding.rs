//! Tenant offboarding 5-state machine — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-tenant-offboarding`
//! (ACTIVE → CANCEL_REQUESTED → GRACE_PERIOD → READ_ONLY → SUSPENDED
//! → ERASED) + 30d grace export + 90d cryptographic erasure. The
//! actual implementation lives in `crates/corelink-tenant-offboarding/`
//! (Stage 1 Stream C sub-step C.2 Option-A aggregator pattern).

pub use corelink_tenant_offboarding::*;

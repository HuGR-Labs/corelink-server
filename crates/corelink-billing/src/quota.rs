//! Quota surface — wave-33 canonical aggregator.
//!
//! Three absorbed quota crates folded under the canonical `quota`
//! submodule as three sub-submodules so consumers can target the
//! exact granularity they need (Stage 1 Stream B sub-step B.1
//! Option-A aggregator pattern):
//!
//! - [`core`] — `corelink-quota`: the quota trait + accounting
//!   primitives shared by every quota enforcement path.
//! - [`cas`] — `corelink-quota-cas`: CAS write-time enforcement
//!   binding (`QuotaCheck` for content-addressable storage).
//! - [`fsm`] — `corelink-quota-fsm`: soft / hard quota state
//!   machine + audit envelope for quota-breach events.

/// Quota trait + accounting primitives.
///
/// Re-exports the entire public API of `corelink-quota`.
pub mod core {
    pub use corelink_quota::*;
}

/// CAS write-time quota enforcement.
///
/// Re-exports the entire public API of `corelink-quota-cas`.
pub mod cas {
    pub use corelink_quota_cas::*;
}

/// Soft / hard quota state machine.
///
/// Re-exports the entire public API of `corelink-quota-fsm`.
pub mod fsm {
    pub use corelink_quota_fsm::*;
}

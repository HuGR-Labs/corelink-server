//! Quota surface — wave-33 canonical aggregator.
//!
//! Three absorbed quota crates folded under the canonical `quota`
//! submodule as three sub-submodules so consumers can target the
//! exact granularity they need (W35 Phase 2 physical absorption):
//!
//! - [`core`] — `corelink-quota`: the quota trait + accounting
//!   primitives shared by every quota enforcement path.
//! - [`cas`] — `corelink-quota-cas`: CAS write-time enforcement
//!   binding (`QuotaCheck` for content-addressable storage).
//! - [`fsm`] — `corelink-quota-fsm`: soft / hard quota state
//!   machine + audit envelope for quota-breach events.

/// Quota trait + accounting primitives.
pub mod core;

/// CAS write-time quota enforcement.
pub mod cas;

/// Soft / hard quota state machine.
pub mod fsm;

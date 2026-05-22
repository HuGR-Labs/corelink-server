//! Disaster recovery — wave-33 canonical ops aggregator.
//!
//! Two DR-context crates folded under the canonical `dr` submodule as
//! two sub-submodules (Stage 1 Stream C sub-step C.2 Option-A
//! aggregator pattern):
//!
//! - [`drill`] — `corelink-dr-drill`: DR drill scheduler + CF region
//!   outage simulator (WI-S17-002).
//! - [`backup_verify`] — `corelink-backup-verify`: continuous (daily)
//!   backup verification harness (complements GAP-15 cold-restore drill;
//!   catches silent backup corruption / freshness regressions / restore
//!   failures between drill cycles).

/// DR drill scheduler + CF region outage simulator (WI-S17-002).
///
/// Re-exports the entire public API of `corelink-dr-drill`.
pub mod drill {
    pub use corelink_dr_drill::*;
}

/// Continuous (daily) backup verification harness.
///
/// Re-exports the entire public API of `corelink-backup-verify`.
pub mod backup_verify {
    pub use corelink_backup_verify::*;
}

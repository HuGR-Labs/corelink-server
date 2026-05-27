//! Disaster recovery — wave-33 canonical ops aggregator (W35-P2 update).
//!
//! Two DR-context crates physically absorbed (W35-P2-OPS):
//!
//! - [`drill`] — was `corelink-dr-drill`: DR drill scheduler + CF
//!   region outage simulator (WI-S17-002).
//! - [`backup_verify`] — was `corelink-backup-verify`: continuous
//!   (daily) backup verification harness (complements GAP-15
//!   cold-restore drill; catches silent backup corruption / freshness
//!   regressions / restore failures between drill cycles).

pub mod drill;
pub mod backup_verify;

//! `corelink-region` — CoreLink multi-region types and primitives.
//!
//! WI-S14-001 foundation crate. wasm32-clean (no tokio in src/).
//!
//! # Modules
//!
//! - [`region`] — [`Region`] enum + [`DoJurisdiction`] enum.
//! - [`event`] — [`ProvisioningEvent`] + [`MigrationEvent`] CloudEvent types.
//! - [`metrics`] — [`RegionMetrics`] struct (5 canonical metrics §6.1.6).
//! - [`migration`] — [`MigrationReport`] + [`MigrationDecision`] taxonomy.
//! - [`error`] — [`RegionError`] taxonomy.
//! - [`audit`] — [`RegionAuditSink`] trait + [`InMemoryRegionAuditSink`].
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::format_in_format_args)]

pub mod audit;
pub mod error;
pub mod event;
pub mod metrics;
pub mod migration;
pub mod region;

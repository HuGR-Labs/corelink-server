//! Admin surface — wave-33 canonical ops aggregator (W35-P2 update).
//!
//! Four admin-context crates folded under the canonical `admin`
//! submodule as four sub-submodules so consumers can target the exact
//! granularity they need:
//!
//! - [`api`] — physically absorbed (W35-P2-OPS): admin REST API surface
//!   (was `corelink-admin-api`). Lives at
//!   `crates/corelink-ops/src/admin/api.rs` plus its sibling files under
//!   `crates/corelink-ops/src/admin/api/`.
//! - [`dry_run`] — physically absorbed (W35-P2-OPS): admin dry-run
//!   preview (diff-only mode for mutating ops). Was
//!   `corelink-admin-dry-run`.
//! - [`handler`] — `corelink-handler-admin` (still external):
//!   admin handler trait + InMemoryFake + per-handler SliObserver.
//! - [`dual_approval`] — `corelink-dual-approval` (still external):
//!   2-of-N approval gate (`require_two_distinct_approvers`).

pub mod api;
pub mod dry_run;

/// Admin handler trait + InMemoryFake + per-handler SliObserver.
///
/// Re-exports the entire public API of `corelink-handler-admin`.
pub mod handler {
    pub use corelink_handler_admin::*;
}

/// Admin 2-of-N approval gate (`require_two_distinct_approvers`).
///
/// Re-exports the entire public API of `corelink-dual-approval`.
pub mod dual_approval {
    pub use corelink_dual_approval::*;
}

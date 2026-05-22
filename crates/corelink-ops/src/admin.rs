//! Admin surface — wave-33 canonical ops aggregator.
//!
//! Four admin-context crates folded under the canonical `admin`
//! submodule as four sub-submodules so consumers can target the exact
//! granularity they need (Stage 1 Stream C sub-step C.2 Option-A
//! aggregator pattern):
//!
//! - [`api`] — `corelink-admin-api`: admin REST API surface.
//! - [`dry_run`] — `corelink-admin-dry-run`: admin dry-run preview
//!   (diff-only mode for mutating ops).
//! - [`handler`] — `corelink-handler-admin`: admin handler trait +
//!   InMemoryFake + per-handler SliObserver.
//! - [`dual_approval`] — `corelink-dual-approval`: 2-of-N approval
//!   gate (`require_two_distinct_approvers`).

/// Admin REST API surface.
///
/// Re-exports the entire public API of `corelink-admin-api`.
pub mod api {
    pub use corelink_admin_api::*;
}

/// Admin dry-run preview (diff-only mode for mutating ops).
///
/// Re-exports the entire public API of `corelink-admin-dry-run`.
pub mod dry_run {
    pub use corelink_admin_dry_run::*;
}

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

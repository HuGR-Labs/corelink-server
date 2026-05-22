//! `corelink-replication` — canonical replication-context surface for
//! the CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream C sub-step C.1 lands this crate as the
//! **single import target** for every replication-context primitive
//! that previously lived across 5 separate crates:
//!
//! ```text
//! use corelink_replication::region::*;       // multi-region topology + region pinning
//! use corelink_replication::replica::*;      // replica worker (per-region apply lane)
//! use corelink_replication::coordinator::*;  // R-PREP wave-15 coordinator (3-state)
//! use corelink_replication::failover::*;     // DR-16 warm-failover router
//! use corelink_replication::rollout::*;      // staged rollout controller (canary lanes)
//! ```
//!
//! ## Stage 1 Stream C absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream C and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" 5 existing crates by
//! re-exporting them at canonical submodule paths. The absorbed
//! crates remain the canonical sources of truth — their src/, tests/,
//! benches/ harnesses are unchanged. Consumer migration
//! (apps/server replication wiring, multi-region migration runner)
//! proceeds incrementally.
//!
//! ### Absorbed crates (5)
//!
//! - `corelink-region` — multi-region topology + region pinning;
//!   re-exported at [`region`].
//! - `corelink-replica-worker` — per-region apply lane (replica side
//!   of the replication pipeline); re-exported at [`replica`].
//! - `corelink-replication-coordinator` — R-PREP wave-15 multi-region
//!   replication coordinator: singleton DO-lock-modeled orchestrator,
//!   per-region health (heartbeat + 4-domain lag), 3-state failover
//!   decision tree (PRIMARY / HOT_STANDBY / REPLICA), audit-emit-
//!   BEFORE-mutation fail-CLOSED, 24h failback cool-down,
//!   replication_status() API; re-exported at [`coordinator`].
//! - `corelink-failover-router` — DR-16 warm-failover router (5
//!   canonical scenarios: primary-up, primary-degraded, split-brain
//!   prevention, failback after recovery, partial-region); re-exported
//!   at [`failover`].
//! - `corelink-rollout-controller` — staged rollout controller (canary
//!   lanes, version-pinned rollout gates); re-exported at [`rollout`].
//!
//! ### Why aggregator rather than physical move
//!
//! 1. **Per-region DO-lock coupling** — the replication-coordinator
//!    singleton lock + replica-worker apply lane + region topology are
//!    structurally coupled via the per-region `replication_status()`
//!    composition; physical relocation requires atomic consumer-side
//!    migration of apps/server `/health` + customer-dashboard wiring
//!    (Stage 2 territory per spec §7).
//! 2. **DR-16 warm-failover lifecycle** — the failover-router 5-state
//!    canonical scenarios are pinned by the
//!    `tests/e2e-failover-router` harness + the
//!    `tests/e2e-replication-failover` cross-coordinator harness;
//!    moving src/ across crate boundaries risks breaking the
//!    deterministic logical-clock + write-lease-ledger split-brain
//!    proof (charter Hard Pause Trigger 4).
//! 3. **24h failback cool-down timer** — the coordinator's failback
//!    cool-down timer is a wall-clock-coupled invariant; physical
//!    relocation requires deterministic-clock injection rewiring
//!    across both the coordinator + the replica-worker apply lane.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 5 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken.
//!
//! ## Charter compliance (preserved by reference)
//!
//! - Audit-emit-BEFORE-mutation fail-CLOSED envelopes across every
//!   absorbed crate — preserved by reference.
//! - Split-brain rejection invariant (failover-router + coordinator)
//!   — preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — inherited via
//!   re-export.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod coordinator;
pub mod failover;
pub mod region;
pub mod replica;
pub mod rollout;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new state is introduced.

    #[test]
    fn region_path_resolves() {
        #[allow(unused_imports)]
        use crate::region as _r;
    }

    #[test]
    fn replica_path_resolves() {
        #[allow(unused_imports)]
        use crate::replica as _rw;
    }

    #[test]
    fn coordinator_path_resolves() {
        #[allow(unused_imports)]
        use crate::coordinator as _c;
    }

    #[test]
    fn failover_path_resolves() {
        #[allow(unused_imports)]
        use crate::failover as _f;
    }

    #[test]
    fn rollout_path_resolves() {
        #[allow(unused_imports)]
        use crate::rollout as _ro;
    }
}

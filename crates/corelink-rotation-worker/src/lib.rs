//! `corelink-rotation-worker` — secret rotation orchestrator + state
//! machine + PAT-ROLL-FORWARD-001 auto-rollback (WI-S13-003).
//!
//! # What this crate ships
//!
//! Per the CoreLink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the rotation orchestrator: the state machine driver,
//! the auto-rollback driver (PAT-ROLL-FORWARD-001), and the metrics
//! taxonomy. All production Cloudflare Workers Cron bindings + D1
//! atomic batches + KMS calls are deferred to WI-S13-006 (PRR ship gate).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`state_machine`] module ships [`RotationStateMachine`]
//!    (D1-backed state machine driver: per-asset atomic transitions;
//!    INV-KEY-OVERLAP + INV-KEY-NO-SKIP + INV-KEY-AUDIT enforced) +
//!    [`InMemoryRotationStateMachine`] (CI fake) + [`RotationRecord`]
//!    + [`RotationPhase`] `#[non_exhaustive]` taxonomy.
//! 2. The [`rollback`] module ships [`RollbackDriver`] (PAT-ROLL-FORWARD-001
//!    auto-rollback: probes `downstream_error_rate` every 60s; if >1%
//!    sustained 5 min → trigger rollback via adapter; SEV-2 audit emit)
//!    + [`RollbackConfig`] (threshold + probe interval + sustain window).
//! 3. The [`metrics`] module ships the 6 canonical Prometheus metric
//!    descriptors (`corelink_admin_rotation_*`) + [`RotationMetrics`]
//!    struct (in-memory fake for CI; production wiring emits Prometheus
//!    counters/gauges/histograms via Cloudflare Workers Analytics Engine
//!    at the cron binding layer).
//! 4. The [`orchestrator`] module ships [`RotationOrchestrator`]
//!    (top-level coordinator: compose adapter + state machine + rollback
//!    driver + metrics; exposes `run_rotation(asset_class, now_ms)`)
//!    + the per-asset cron schedule constants.
//!
//! # INV-KEY-OVERLAP enforcement
//!
//! During the overlap window both the previous `Overlap` key and the
//! new `Active` key are accepted for reads. Only the new `Active` key
//! accepts writes. Enforced at the adapter trait surface
//! (`is_valid_read_state` / `is_valid_write_state`) + verified by
//! property tests (10k iter PR gate; 100k iter nightly).
//!
//! # INV-KEY-NO-SKIP enforcement
//!
//! Writes are only accepted for keys in `Active` state. Any attempt to
//! write with a key in `Pending`, `Retired`, `Destroyed`, or `RolledBack`
//! state returns `RotationError::InvalidTransition`. Pinned by
//! `prop_rotation_inv_key_no_skip` (10k iter).
//!
//! # PAT-ROLL-FORWARD-001 auto-rollback
//!
//! The rollback driver probes `adapter.downstream_error_rate()` every
//! 60 seconds (configurable). If the rate exceeds 1% for 5 consecutive
//! minutes, the rollback is triggered: `new` → `RolledBack`; `previous`
//! re-promoted to `Active`. SEV-2 audit emitted.
//!
//! # Hard upper bound 30d
//!
//! All adapters enforce `overlap_seconds ≤ 30d`
//! (`key_management.md §3.2.1`). `RotationError::OverlapExceedsHardUpper`
//! is returned on violation. Pinned by `prop_rotation_hard_upper_bound_30d`.
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is wasm32-clean: no `std::time`, no `tokio`, no OS
//! syscalls. Timestamps passed as `u64` milliseconds by the caller.
//!
//! # Production wiring (deferred to WI-S13-006 PRR ship gate)
//!
//! - Cloudflare Workers Cron trigger (4 cron schedules per §6.1.1).
//! - D1 `rotation_state` table + UNIQUE index.
//! - D1 atomic batch [state transition UPDATE + audit_outbox INSERT]
//!   (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//! - CloudEvent emission per state transition (INV-KEY-AUDIT).
//! - Prometheus metrics via Cloudflare Workers Analytics Engine.
//! - OTel trace spans per phase.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod metrics;
pub mod orchestrator;
pub mod rollback;
pub mod state_machine;

pub use corelink_rotation_adapters::{
    is_valid_read_state, is_valid_write_state, AdminSigningRotationAdapter,
    AuditChainRotationAdapter, AssetClass, ByokRotationAdapter, KeyHandle, KeyState,
    PatSigningRotationAdapter, RotationAdapter, RotationError, TdkRotationAdapter,
};
pub use metrics::{RotationMetrics, RotationMetricsSink};
pub use orchestrator::{RotationOrchestrator, RotationOutcome};
pub use rollback::{ProbeContext, RollbackConfig, RollbackDriver, RollbackOutcome};
pub use state_machine::{InMemoryRotationStateMachine, RotationPhase, RotationRecord, RotationStateMachine};

/// Crate canonical schema version.
#[must_use]
pub const fn rotation_worker_schema_version() -> u32 {
    1
}

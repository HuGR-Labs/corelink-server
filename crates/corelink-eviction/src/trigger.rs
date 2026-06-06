//! 95% quota-trigger boundary check + size-proportional reservation TTL
//! integration + async-spawn fire-and-forget envelope (Lote 10.7bis R5
//! P0-3).
//!
//! ## Why fire-and-forget
//!
//! Per WI-S07-002 §6.1.8 + WI-S07-003 §6 design decision (Lote
//! 10.7-tris cycle 6 alignment), the quota middleware MUST NOT block
//! the hot-path write on the eviction phase. Instead, when the
//! middleware detects `bytes_used / bytes_quota >= 0.95`, it spawns
//! the eviction phase via the canonical Cloudflare Workers Rust runtime
//! primitive `worker::send_future()` — fire-and-forget; the response
//! returns to the client immediately.
//!
//! ## Why NOT `tokio::spawn` or `wasm_bindgen_futures::spawn_local`
//!
//! Per Lote 10.7bis R5 P0-3:
//! - `tokio::spawn` doesn't exist in CF Workers Rust runtime (no
//!   tokio reactor in the worker isolate).
//! - `wasm_bindgen_futures::spawn_local` is a wasm-bindgen JS-runtime
//!   primitive; CF Workers Rust uses a different (worker-rs) runtime
//!   shim where `worker::send_future()` is the canonical
//!   fire-and-forget seam.
//!
//! The in-memory simulator in this module provides a synchronous
//! capture sink that mirrors the contract: a closure is enqueued and
//! returns immediately; the test harness invokes the closure
//! synchronously to inspect the resulting eviction phase outcome.

use uuid::Uuid;

use crate::phase::{QUOTA_TARGET_HEADROOM_PCT, QUOTA_TRIGGER_THRESHOLD_PCT};
use crate::storage_state::TenantStorageStateRow;

/// Outcome of [`should_fire_quota_trigger`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QuotaTriggerOutcome {
    /// `bytes_used / bytes_quota < 95%` — trigger does NOT fire.
    BelowThreshold,
    /// `bytes_used / bytes_quota >= 95%` — trigger fires; carries
    /// `target_bytes_to_reclaim`.
    Fire {
        /// Bytes the eviction phase MUST reclaim to bring the tenant
        /// back to ≤ 90% (= `bytes_used - 0.90 × bytes_quota`).
        target_bytes_to_reclaim: u64,
    },
}

/// Decide whether the 95% trigger arm fires for a given storage-state
/// row.
///
/// Boundary semantics:
/// - `bytes_quota == 0` → defensive — returns `BelowThreshold` (no
///   division by zero; misconfigured tenant would otherwise pin the
///   trigger ON forever).
/// - `bytes_used / bytes_quota < 0.95` → `BelowThreshold`.
/// - `bytes_used / bytes_quota == 0.95` → `Fire` (boundary inclusive).
/// - `bytes_used / bytes_quota > 0.95` → `Fire`.
#[must_use]
pub fn should_fire_quota_trigger(row: &TenantStorageStateRow) -> QuotaTriggerOutcome {
    if row.bytes_quota == 0 {
        return QuotaTriggerOutcome::BelowThreshold;
    }
    let pct = row.utilization_pct();
    if pct < QUOTA_TRIGGER_THRESHOLD_PCT {
        return QuotaTriggerOutcome::BelowThreshold;
    }
    let target = target_bytes_to_reclaim(row);
    QuotaTriggerOutcome::Fire {
        target_bytes_to_reclaim: target,
    }
}

/// Compute `target_bytes_to_reclaim = bytes_used - 0.90 × bytes_quota`
/// (saturating at 0).
///
/// When the tenant is exactly at 90% quota, the target is 0 (no
/// reclaim needed); when over 95% the target is ~5% of `bytes_quota`
/// (canonical reclaim-to-90% headroom per WI §6.1.8).
#[must_use]
pub fn target_bytes_to_reclaim(row: &TenantStorageStateRow) -> u64 {
    let quota_f = row.bytes_quota as f64;
    let target_floor_f = quota_f * QUOTA_TARGET_HEADROOM_PCT;
    // Round UP to ensure we reclaim at least enough to get below 90%.
    let target_floor = target_floor_f.ceil() as u64;
    row.bytes_used.saturating_sub(target_floor)
}

/// Synchronous in-memory simulator of the production
/// `worker::send_future(execute_quota_trigger(...))` envelope. Returns
/// an [`InMemorySpawnHandle`] whose `run_now` method invokes the
/// captured closure and returns its result.
///
/// The contract pinned by this surface:
/// 1. `spawn_quota_trigger_in_memory` returns IMMEDIATELY (the
///    closure is captured but NOT yet invoked).
/// 2. The handle records that `invoked == false` post-spawn.
/// 3. The test harness calls `handle.run_now()` to drive the closure
///    synchronously (mirrors the production runtime invoking the
///    enqueued future).
///
/// Production wiring substitutes `worker::send_future()` for this
/// surface; the response returns to the client at step 1 regardless
/// of whether the eviction phase succeeded.
pub fn spawn_quota_trigger_in_memory<R, F>(
    _tenant_id: Uuid,
    _target_bytes_to_reclaim: u64,
    f: F,
) -> InMemorySpawnHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    InMemorySpawnHandle {
        closure: Some(Box::new(f)),
        result: None,
    }
}

/// Spawn handle returned by [`spawn_quota_trigger_in_memory`].
pub struct InMemorySpawnHandle<R> {
    closure: Option<Box<dyn FnOnce() -> R + Send>>,
    result: Option<R>,
}

impl<R> core::fmt::Debug for InMemorySpawnHandle<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemorySpawnHandle")
            .field("closure_pending", &self.closure.is_some())
            .field("result_pending", &self.result.is_some())
            .finish()
    }
}

impl<R> InMemorySpawnHandle<R> {
    /// Whether the closure has been invoked.
    #[must_use]
    pub const fn invoked(&self) -> bool {
        self.closure.is_none()
    }

    /// Drive the captured closure synchronously. Returns the result.
    /// Calling twice surfaces the cached result.
    pub fn run_now(&mut self) -> Option<&R> {
        if let Some(c) = self.closure.take() {
            self.result = Some(c());
        }
        self.result.as_ref()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::region::EvictionRegion;

    fn ten() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn row(used: u64, quota: u64) -> TenantStorageStateRow {
        TenantStorageStateRow {
            tenant_id: ten(),
            region: EvictionRegion::Sam,
            bytes_used: used,
            bytes_quota: quota,
            bytes_used_updated_at_ms: 100,
            last_synced_at_ms: 100,
            last_evict_at_ms: None,
            bytes_reclaimed_lifetime: 0,
            created_at_ms: 100,
            updated_at_ms: 100,
        }
    }

    #[test]
    fn zero_quota_returns_below_threshold() {
        let r = row(50, 0);
        assert_eq!(
            should_fire_quota_trigger(&r),
            QuotaTriggerOutcome::BelowThreshold
        );
    }

    #[test]
    fn at_94_percent_below_threshold() {
        let r = row(94, 100);
        assert_eq!(
            should_fire_quota_trigger(&r),
            QuotaTriggerOutcome::BelowThreshold
        );
    }

    #[test]
    fn exactly_at_95_percent_fires() {
        let r = row(95, 100);
        match should_fire_quota_trigger(&r) {
            QuotaTriggerOutcome::Fire {
                target_bytes_to_reclaim,
            } => {
                // target = 95 - ceil(0.90 × 100) = 95 - 90 = 5.
                assert_eq!(target_bytes_to_reclaim, 5);
            }
            other => panic!("expected Fire, got {other:?}"),
        }
    }

    #[test]
    fn at_96_percent_fires_with_higher_target() {
        let r = row(96, 100);
        match should_fire_quota_trigger(&r) {
            QuotaTriggerOutcome::Fire {
                target_bytes_to_reclaim,
            } => {
                assert_eq!(target_bytes_to_reclaim, 6);
            }
            other => panic!("expected Fire, got {other:?}"),
        }
    }

    #[test]
    fn at_100_percent_fires_with_max_target() {
        let r = row(100, 100);
        match should_fire_quota_trigger(&r) {
            QuotaTriggerOutcome::Fire {
                target_bytes_to_reclaim,
            } => {
                // target = 100 - ceil(0.90 × 100) = 100 - 90 = 10.
                assert_eq!(target_bytes_to_reclaim, 10);
            }
            other => panic!("expected Fire, got {other:?}"),
        }
    }

    #[test]
    fn target_bytes_saturates_at_zero_below_floor() {
        // 80% utilization < 90% target floor → target saturates 0.
        let r = row(80, 100);
        assert_eq!(target_bytes_to_reclaim(&r), 0);
    }

    #[test]
    fn target_bytes_with_large_quota() {
        // 1 GiB quota at 96% → target = ceil(96% - 90%) = 6% of 1 GiB.
        let one_gib = 1_073_741_824_u64;
        let r = row(one_gib * 96 / 100, one_gib);
        let target = target_bytes_to_reclaim(&r);
        // 6% of 1 GiB ≈ 64 MiB; allow ±1 byte for ceil rounding.
        let expected_floor = ((one_gib as f64) * QUOTA_TARGET_HEADROOM_PCT).ceil() as u64;
        assert_eq!(target, (one_gib * 96 / 100) - expected_floor);
    }

    #[test]
    fn spawn_handle_invoked_starts_false() {
        let h: InMemorySpawnHandle<u32> = spawn_quota_trigger_in_memory(ten(), 100, || 42);
        assert!(!h.invoked());
    }

    #[test]
    fn spawn_handle_run_now_invokes_closure() {
        let mut h: InMemorySpawnHandle<u32> = spawn_quota_trigger_in_memory(ten(), 100, || 42);
        let r = h.run_now().copied();
        assert_eq!(r, Some(42));
        assert!(h.invoked());
    }

    #[test]
    fn spawn_handle_run_now_idempotent() {
        let mut h: InMemorySpawnHandle<u32> = spawn_quota_trigger_in_memory(ten(), 100, || 7);
        h.run_now();
        // Second call surfaces the cached result.
        let r = h.run_now().copied();
        assert_eq!(r, Some(7));
    }

    #[test]
    fn spawn_handle_take_result_after_run() {
        // (sanity test — note this exercises the closure capture
        // contract, NOT the production async semantics).
        let mut h: InMemorySpawnHandle<&'static str> =
            spawn_quota_trigger_in_memory(ten(), 100, || "ok");
        h.run_now();
        let v: Option<&&'static str> = h.result.as_ref();
        assert_eq!(v.copied(), Some("ok"));
    }
}

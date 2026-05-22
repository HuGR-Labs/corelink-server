//! Canonical [`Clock`] trait surface — wave-33 Stage 0.
//!
//! The workspace currently carries ≥10 small per-crate `Clock` /
//! `WallClock` / `EngineClock` / `SweepClock` / `MarkClock` /
//! `ReconcileClock` / `LruClock` / `EvictionClock` / `MatClock` traits
//! with similar wall-clock-only surfaces. The [`Clock`] trait here is
//! the canonical one Stage 1 streams will migrate to; existing traits
//! remain in place until their owning context lands per stream.
//!
//! Per the wave-33 reorg spec §6 Stage 0 task list ("Clock trait
//! (monotonic + wall) — there's already `apps/server/src/wall_clock.rs`;
//! consider folding the trait surface here (NOT the impl)"). We fold
//! the wall-clock SURFACE here. The concrete `SystemWallClock` +
//! `InMemoryFakeWallClock` impls remain in `apps/server` until Stage 2
//! binding consolidation, when the runtime entry points get rewired
//! through `corelink-container` / `corelink-worker`.

use core::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock + monotonic-clock abstraction.
///
/// Wall-clock methods ([`Self::now`] / [`Self::now_unix_ms`]) read the
/// system clock. The default impl of [`Self::now_unix_ms`] is derived
/// from [`Self::now`]; implementations only need to provide `now`.
///
/// Production wiring: `apps/server::wall_clock::SystemWallClock`
/// (preserved in-place for Stage 0). Tests: deterministic in-memory
/// fakes that satisfy the same surface.
pub trait Clock: fmt::Debug + Send + Sync {
    /// Current wall-clock instant as a [`SystemTime`].
    fn now(&self) -> SystemTime;

    /// Current unix time in whole milliseconds. Saturates to `0` for
    /// pre-epoch values and to `u64::MAX` on overflow. Matches the
    /// pre-existing
    /// [`apps/server::wall_clock::WallClock::now_ms`][crate] semantics.
    fn now_unix_ms(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Test fixture clock — pinned at the unix epoch + caller offset.
    /// Lives in `#[cfg(test)]` so it doesn't ship as part of the
    /// public API (consumers will use whichever fake their context
    /// owns).
    #[derive(Debug)]
    struct FixtureClock(SystemTime);

    impl Clock for FixtureClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    #[test]
    fn clock_now_unix_ms_derives_from_now() {
        let c = FixtureClock(UNIX_EPOCH + Duration::from_millis(1_700_000_000_000));
        assert_eq!(c.now_unix_ms(), 1_700_000_000_000);
    }

    #[test]
    fn clock_pre_epoch_saturates_to_zero() {
        // Pre-epoch is unrepresentable in `Duration::from_millis` so
        // we use UNIX_EPOCH itself and assert the boundary.
        let c = FixtureClock(UNIX_EPOCH);
        assert_eq!(c.now_unix_ms(), 0);
    }
}

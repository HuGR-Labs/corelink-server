//! Cross-route wall-clock abstraction — wave-21 closure of the
//! `A-P2-05` (audit-export) + `B-P2-03` (audit-analytics) findings.
//!
//! # Why
//!
//! Both `audit_export` and `audit_analytics` previously anchored their
//! rate-limit `now_ms` to the request's query-window upper bound
//! (`until_ms` / `to_ms`). This made the bucket clock window-derived
//! rather than wall-clock-derived, so a customer querying a stationary
//! 1970-epoch window every 60ms could keep refilling the token bucket
//! without ever tripping the per-tenant cap (see audit docs
//! `2026-05-16-wave18-adversarial-review-streamA-audit-export.md
//! §A-P2-05` + `…-streamB-neon-shadow.md §B-P2-03`).
//!
//! Wave-21 lifts that residual trait by introducing a single shared
//! [`WallClock`] trait that both route states consume via
//! `Arc<dyn WallClock>`. Production wiring injects [`SystemWallClock`];
//! tests inject [`InMemoryFakeWallClock`] for deterministic time.
//!
//! # Why a separate trait from `corelink_stripe_real::clock::Clock`?
//!
//! The Stripe `Clock` trait lives inside the Stripe crate and carries
//! webhook-specific SLI helpers (`start_marker`, `observe_latency_seconds`).
//! The audit routes only need the wall-clock surface — coupling them to
//! the Stripe crate would create an unwanted cross-crate dependency
//! (`apps/server::routes::audit_*` does not depend on
//! `corelink-stripe-real`). The two traits are intentional siblings,
//! both rooted on the same `SystemTime`-shaped contract.
//!
//! # Example — deterministic time in tests
//!
//! ```
//! use corelink_server::wall_clock::{WallClock, InMemoryFakeWallClock};
//! use std::time::{Duration, UNIX_EPOCH};
//!
//! let clock = InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000);
//! assert_eq!(clock.now_ms(), 1_700_000_000_000);
//!
//! clock.advance(Duration::from_millis(60_000));
//! assert_eq!(clock.now_ms(), 1_700_000_060_000);
//! ```

use core::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock abstraction shared across server routes.
///
/// Production: [`SystemWallClock`] (wraps `SystemTime::now`).
/// Tests: [`InMemoryFakeWallClock`] (deterministic, advance()-able).
///
/// The canonical method is [`WallClock::now`]; [`WallClock::now_ms`]
/// is a default impl derived from `now()`.
pub trait WallClock: fmt::Debug + Send + Sync {
    /// Current wall-clock instant as a [`SystemTime`].
    fn now(&self) -> SystemTime;

    /// Current unix time in whole milliseconds. Saturates to `0` for
    /// pre-epoch values and to `u64::MAX` on overflow.
    fn now_ms(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

// =========================================================================
// SystemWallClock — production wall clock on native targets.
// =========================================================================

/// Native production wall clock. Wraps [`std::time::SystemTime::now`].
///
/// On `wasm32-unknown-unknown` the host runtime (CF Workers) panics on
/// `SystemTime::now()`; the audit routes are not wired into the wasm32
/// build path today (see `build_state` `compile_error!`), so this
/// trait deliberately only supplies the native impl.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemWallClock;

impl SystemWallClock {
    /// Construct the canonical native wall clock.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl WallClock for SystemWallClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

// =========================================================================
// InMemoryFakeWallClock — deterministic test clock.
// =========================================================================

/// Deterministic test wall clock. Pinned at a caller-supplied unix-ms
/// instant; supports `advance()` to step time forward without `&mut`.
#[derive(Debug)]
pub struct InMemoryFakeWallClock {
    // Current wall-clock instant as ms since the unix epoch. `Mutex`
    // so `advance()` works without `&mut self` (parallels the other
    // in-memory test fakes in this workspace).
    ms_since_epoch: Mutex<u64>,
}

impl InMemoryFakeWallClock {
    /// Construct pinned at `unix_ms` past epoch.
    #[must_use]
    pub fn at_unix_ms(unix_ms: u64) -> Self {
        Self {
            ms_since_epoch: Mutex::new(unix_ms),
        }
    }

    /// Construct pinned at `unix_seconds` past epoch.
    #[must_use]
    pub fn at_unix_seconds(unix_seconds: u64) -> Self {
        Self {
            ms_since_epoch: Mutex::new(unix_seconds.saturating_mul(1_000)),
        }
    }

    /// Advance the clock by `delta`. Saturates at `u64::MAX` ms on
    /// overflow.
    pub fn advance(&self, delta: Duration) {
        let add_ms = u64::try_from(delta.as_millis()).unwrap_or(u64::MAX);
        if let Ok(mut g) = self.ms_since_epoch.lock() {
            *g = g.saturating_add(add_ms);
        }
    }

    /// Set the clock to an absolute unix-ms value.
    pub fn set_unix_ms(&self, unix_ms: u64) {
        if let Ok(mut g) = self.ms_since_epoch.lock() {
            *g = unix_ms;
        }
    }
}

impl WallClock for InMemoryFakeWallClock {
    fn now(&self) -> SystemTime {
        let ms = match self.ms_since_epoch.lock() {
            Ok(g) => *g,
            // Poisoned mutex → return the inner value deterministically
            // rather than propagating a panic (charter: no panics in
            // src). Mirrors the Stripe `InMemoryFakeClock` policy.
            Err(p) => *p.into_inner(),
        };
        UNIX_EPOCH + Duration::from_millis(ms)
    }
}

// =========================================================================
// default_wall_clock() — production wall-clock factory.
// =========================================================================

/// Construct the canonical production [`WallClock`] wrapped in an
/// `Arc<dyn WallClock>` so it can be stored directly in route state.
#[must_use]
pub fn default_wall_clock() -> Arc<dyn WallClock> {
    Arc::new(SystemWallClock::new())
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_returns_injected_time_and_advances() {
        let clock = InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000);

        clock.advance(Duration::from_millis(60_000));
        assert_eq!(clock.now_ms(), 1_700_000_060_000);

        clock.set_unix_ms(2_000_000_000_500);
        assert_eq!(clock.now_ms(), 2_000_000_000_500);
    }

    #[test]
    fn at_unix_seconds_matches_ms_conversion() {
        let clock = InMemoryFakeWallClock::at_unix_seconds(1_700_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000);
        let st = clock.now();
        assert_eq!(
            st.duration_since(UNIX_EPOCH).unwrap().as_millis(),
            1_700_000_000_000_u128
        );
    }

    #[test]
    fn system_clock_is_non_decreasing() {
        let clock = SystemWallClock::new();
        let a = clock.now_ms();
        let mut b = clock.now_ms();
        let mut spins = 0u64;
        while b == a && spins < 10_000_000 {
            b = clock.now_ms();
            spins = spins.saturating_add(1);
        }
        assert!(
            b >= a,
            "SystemWallClock must be non-decreasing across consecutive reads (a={a}, b={b})"
        );
    }

    #[test]
    fn default_wall_clock_is_arc_dyn() {
        let c = default_wall_clock();
        // Smoke: now_ms() works through the trait-object indirection.
        let _ = c.now_ms();
    }
}

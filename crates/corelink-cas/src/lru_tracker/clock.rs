//! Wall-clock seam for the LRU tracker.
//!
//! Mirrors `corelink-eviction::EvictionClock` + `corelink-gc::SweepClock`
//! — deterministic test seam without binding directly to `Date.now()` /
//! tokio time. Production wiring substitutes a CF-Workers `Date.now()`
//! reader; the in-memory fake exposes a counter-driven `LruClock`.

use std::sync::Mutex;

/// Wall-clock seam for the LRU tracker.
pub trait LruClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;
}

/// Counter-driven [`LruClock`] used by tests.
#[derive(Debug)]
pub struct CountingLruClock {
    inner: Mutex<u64>,
}

impl CountingLruClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }

    /// Advance the clock to a specific instant (test wiring).
    pub fn set_to(&self, ms: u64) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = ms;
    }

    /// Advance the clock by a delta (test wiring).
    pub fn advance(&self, delta_ms: u64) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = g.saturating_add(delta_ms);
    }
}

impl LruClock for CountingLruClock {
    fn now_ms(&self) -> u64 {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g
    }
}

/// Frozen [`LruClock`] returning a fixed instant. Useful for tests
/// where the clock must NOT advance between calls (otherwise the
/// tracker would observe artificial drift).
#[derive(Debug)]
pub struct FrozenLruClock {
    at_ms: u64,
}

impl FrozenLruClock {
    /// Construct frozen at the given instant.
    #[must_use]
    pub const fn new(at_ms: u64) -> Self {
        Self { at_ms }
    }
}

impl LruClock for FrozenLruClock {
    fn now_ms(&self) -> u64 {
        self.at_ms
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

    #[test]
    fn counting_clock_returns_current_value() {
        let c = CountingLruClock::new(100);
        assert_eq!(c.now_ms(), 100);
        assert_eq!(c.now_ms(), 100);
    }

    #[test]
    fn counting_clock_advance_bumps_now() {
        let c = CountingLruClock::new(100);
        c.advance(50);
        assert_eq!(c.now_ms(), 150);
    }

    #[test]
    fn counting_clock_set_to_overrides_value() {
        let c = CountingLruClock::new(100);
        c.set_to(500);
        assert_eq!(c.now_ms(), 500);
    }

    #[test]
    fn counting_clock_advance_saturates_on_overflow() {
        let c = CountingLruClock::new(u64::MAX);
        c.advance(1);
        assert_eq!(c.now_ms(), u64::MAX);
    }

    #[test]
    fn frozen_clock_returns_constant() {
        let c = FrozenLruClock::new(42);
        assert_eq!(c.now_ms(), 42);
        assert_eq!(c.now_ms(), 42);
        assert_eq!(c.now_ms(), 42);
    }
}

//! Pluggable wall-clock abstraction — wave-20 closure of the wave-19
//! `wasm32-unknown-unknown` runtime-panic caveat.
//!
//! # Why
//!
//! `std::time::SystemTime::now()` compiles on `wasm32-unknown-unknown`
//! but panics at runtime ("time not implemented on this platform"). The
//! Cloudflare Worker target (`wasm32-unknown-unknown`) ships
//! production stripe webhook + portal code paths that previously called
//! `SystemTime::now()` directly (see wave-19 audit
//! `specs/_audits/2026-05-16-stripe-wasm32-gate-lift.md §8`). Wave-20
//! introduces this [`Clock`] trait so:
//!
//! - **native** callers inject [`SystemClock`] (wraps `SystemTime::now`)
//! - **wasm32** callers inject `WasmWorkerClock` (reads
//!   `js_sys::Date::now()` — Workers expose `Date` per the
//!   Web-Platform `globalThis` contract)
//! - **tests** inject [`InMemoryFakeClock`] for deterministic timestamps
//!
//! The trait's canonical method is [`Clock::now`] returning a
//! [`std::time::SystemTime`] for ergonomic compat with existing call
//! sites; convenience methods [`Clock::now_seconds`] / [`Clock::now_ms`]
//! have default impls that derive from `now()`.
//!
//! # Example — deterministic time in tests
//!
//! ```
//! use corelink_stripe_real::clock::{Clock, InMemoryFakeClock};
//! use std::time::{Duration, UNIX_EPOCH};
//!
//! let clock = InMemoryFakeClock::at_unix_seconds(1_700_000_000);
//! assert_eq!(clock.now_seconds(), 1_700_000_000);
//! assert_eq!(clock.now_ms(), 1_700_000_000_000);
//!
//! // Advance time deterministically.
//! clock.advance(Duration::from_secs(30));
//! assert_eq!(clock.now_seconds(), 1_700_000_030);
//!
//! // `now()` returns a `SystemTime` for compat with stdlib call sites.
//! let st = clock.now();
//! assert_eq!(
//!     st.duration_since(UNIX_EPOCH).unwrap().as_secs(),
//!     1_700_000_030,
//! );
//! ```

use core::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock abstraction.
///
/// Production impls: [`SystemClock`] (native) or `WasmWorkerClock`
/// (wasm32). Test impls: [`InMemoryFakeClock`] (deterministic).
///
/// The canonical method is [`Clock::now`]; the seconds / millis / SLI
/// observe methods are derived defaults so most impls only need to
/// override `now()`.
pub trait Clock: fmt::Debug + Send + Sync {
    /// Current wall-clock instant as a [`SystemTime`].
    ///
    /// Implementations MUST NOT panic on wasm32. The wave-19 caveat
    /// (`SystemTime::now()` panics on `wasm32-unknown-unknown`) is
    /// closed by routing wasm32 callers through `WasmWorkerClock`
    /// which reads `js_sys::Date::now()` instead.
    fn now(&self) -> SystemTime;

    /// Current unix time in whole seconds (saturates to `0` on
    /// pre-epoch values).
    fn now_seconds(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Current unix time in whole milliseconds (saturates to `0` on
    /// pre-epoch values; clamps to `u64::MAX` on overflow ~ year
    /// 584_942_417 AD).
    fn now_ms(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }

    /// Capture an opaque start marker for [`Clock::observe_latency_seconds`].
    /// Default impl returns `now_ms()`.
    fn start_marker(&self) -> u64 {
        self.now_ms()
    }

    /// Compute the elapsed time (seconds) between `start_marker` and now.
    /// Default impl: `(now_ms - start) / 1000.0` (saturating).
    fn observe_latency_seconds(&self, start_marker: u64) -> f64 {
        let delta_ms = self.now_ms().saturating_sub(start_marker);
        // Cast: ms<u64> → f64 is lossy only beyond 2^53 ms (~285k years).
        #[allow(
            clippy::cast_precision_loss,
            reason = "ms→seconds, 53-bit precision is far beyond webhook latency range"
        )]
        let s = delta_ms as f64 / 1000.0;
        s
    }
}

// =========================================================================
// SystemClock — native wall clock.
// =========================================================================

/// Native system-clock impl. Wraps [`std::time::SystemTime::now`].
///
/// On `wasm32-unknown-unknown` this impl is **not available** — wasm32
/// callers MUST use `WasmWorkerClock` instead. Mis-use is caught at
/// compile time via the `cfg` gate.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

// =========================================================================
// WasmWorkerClock — wasm32 wall clock via js_sys::Date.
// =========================================================================

/// Cloudflare-Worker / browser-runtime clock impl.
///
/// Reads `js_sys::Date::now()` (milliseconds since epoch as `f64`) and
/// converts to a [`SystemTime`]. Available only on
/// `target_arch = "wasm32"`.
///
/// # Panics
///
/// Never. Negative or non-finite values from `Date::now()` (impossible
/// per the Web Platform contract) saturate to `UNIX_EPOCH`.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct WasmWorkerClock;

#[cfg(target_arch = "wasm32")]
impl Clock for WasmWorkerClock {
    fn now(&self) -> SystemTime {
        // `Date::now()` returns ms since unix epoch as f64. The Web
        // Platform guarantees a non-negative finite value. We defend
        // against malformed runtimes by clamping.
        let ms_f64 = js_sys::Date::now();
        if !ms_f64.is_finite() || ms_f64 < 0.0 {
            return UNIX_EPOCH;
        }
        // Clamp to u64 range. f64 → u64 is well-defined for finite
        // non-negative values within range.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Date::now is non-negative and we clamp above u64::MAX"
        )]
        let ms = if ms_f64 >= u64::MAX as f64 {
            u64::MAX
        } else {
            ms_f64 as u64
        };
        UNIX_EPOCH + Duration::from_millis(ms)
    }
}

// =========================================================================
// InMemoryFakeClock — deterministic test clock.
// =========================================================================

/// Deterministic test clock. Returns a caller-supplied unix instant
/// and supports `advance()` to step time forward.
///
/// Cheap to clone via inner `Arc` (the [`Mutex`] state is shared).
#[derive(Debug)]
pub struct InMemoryFakeClock {
    // Current wall-clock instant, as ms since unix epoch. `Mutex` so
    // `advance()` can mutate without `&mut self` (parallels the
    // in-memory test fakes elsewhere in this crate).
    ms_since_epoch: Mutex<u64>,
}

impl InMemoryFakeClock {
    /// Construct pinned at `unix_seconds` past epoch.
    #[must_use]
    pub fn at_unix_seconds(unix_seconds: u64) -> Self {
        Self {
            ms_since_epoch: Mutex::new(unix_seconds.saturating_mul(1_000)),
        }
    }

    /// Construct pinned at `unix_ms` past epoch.
    #[must_use]
    pub fn at_unix_ms(unix_ms: u64) -> Self {
        Self {
            ms_since_epoch: Mutex::new(unix_ms),
        }
    }

    /// Advance the clock by `delta`.
    ///
    /// Saturates at `u64::MAX` ms on overflow.
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

impl Clock for InMemoryFakeClock {
    fn now(&self) -> SystemTime {
        let ms = match self.ms_since_epoch.lock() {
            Ok(g) => *g,
            // Poisoned mutex → return epoch deterministically rather
            // than propagate panic (charter: no panics in src).
            Err(p) => *p.into_inner(),
        };
        UNIX_EPOCH + Duration::from_millis(ms)
    }
}

// =========================================================================
// default_clock() — target-conditional production clock factory.
// =========================================================================

/// Construct the production wall-clock for the current target:
///
/// - native (`cfg(not(target_arch = "wasm32"))`) → [`SystemClock`]
/// - wasm32 (`cfg(target_arch = "wasm32")`)     → `WasmWorkerClock`
///
/// Wraps in `Arc<dyn Clock + Send + Sync>` so callers can store the
/// clock in shared state without leaking the concrete type.
#[must_use]
pub fn default_clock() -> Arc<dyn Clock + Send + Sync> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Arc::new(SystemClock)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(WasmWorkerClock)
    }
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
    fn fake_clock_returns_injected_time() {
        let clock = InMemoryFakeClock::at_unix_seconds(1_700_000_000);
        assert_eq!(clock.now_seconds(), 1_700_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000);
        let st = clock.now();
        assert_eq!(
            st.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            1_700_000_000
        );

        clock.advance(Duration::from_secs(42));
        assert_eq!(clock.now_seconds(), 1_700_000_042);

        clock.set_unix_ms(2_000_000_000_500);
        assert_eq!(clock.now_ms(), 2_000_000_000_500);
        assert_eq!(clock.now_seconds(), 2_000_000_000);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn system_clock_returns_monotonic() {
        let clock = SystemClock;
        let a = clock.now_ms();
        // Spin briefly to guarantee a non-zero delta on every supported
        // platform (the OS clock has at most µs granularity).
        let mut b = clock.now_ms();
        let mut spins = 0u64;
        while b == a && spins < 10_000_000 {
            b = clock.now_ms();
            spins = spins.saturating_add(1);
        }
        assert!(
            b >= a,
            "SystemClock must be non-decreasing across consecutive reads (a={a}, b={b})"
        );
        // SLI helper: start marker → observe → non-negative latency.
        let marker = clock.start_marker();
        let lat = clock.observe_latency_seconds(marker);
        assert!(lat >= 0.0, "latency must be non-negative, got {lat}");
    }

    #[test]
    fn fake_clock_observe_latency_uses_now_ms_delta() {
        let clock = InMemoryFakeClock::at_unix_ms(1_000);
        let marker = clock.start_marker();
        assert_eq!(marker, 1_000);
        clock.advance(Duration::from_millis(2_500));
        let lat = clock.observe_latency_seconds(marker);
        assert!((lat - 2.5).abs() < 1e-9, "expected 2.5s latency, got {lat}");
    }
}

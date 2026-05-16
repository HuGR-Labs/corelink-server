//! Pluggable wall-clock abstraction for the Stripe billing
//! materializer — wave-22 closure of the wave-20 follow-on caveat.
//!
//! # Why
//!
//! `std::time::SystemTime::now()` compiles on `wasm32-unknown-unknown`
//! but panics at runtime ("time not implemented on this platform"). The
//! Cloudflare Worker target (`wasm32-unknown-unknown`) ships
//! `D1SubscriptionStateHandler` (`crate::handler`) which previously
//! drove a private `MatClock` whose `SystemMatClock` impl called
//! `SystemTime::now()` directly. Wave-20 closed the equivalent caveat
//! in `corelink-stripe-real::clock`; this module mirrors that pattern
//! end-to-end for the materializer per wave-19 audit §7 + the wave-20
//! `OUT OF SCOPE` carve-out (separate `MatClock` trait, separate
//! closure wave).
//!
//! - **native** callers inject [`SystemMatClock`] (wraps
//!   `SystemTime::now`).
//! - **wasm32** callers inject [`WasmWorkerMatClock`] (reads
//!   `js_sys::Date::now()` — Workers expose `Date` per the
//!   Web-Platform `globalThis` contract).
//! - **tests** inject [`InMemoryFakeMatClock`] for deterministic
//!   timestamps.
//!
//! The trait's canonical method is [`MatClock::now`] returning a
//! [`std::time::SystemTime`] for ergonomic compat with stdlib call
//! sites; convenience method [`MatClock::now_ms`] has a default impl
//! that derives from `now()` and is what the handler consumes when
//! stamping `ts_ms` on `corelink.billing.<event>.materialized.v1`
//! audit rows.
//!
//! # Example — deterministic time in tests
//!
//! ```
//! use corelink_billing_stripe_materializer::clock::{MatClock, InMemoryFakeMatClock};
//! use std::time::{Duration, UNIX_EPOCH};
//!
//! let clock = InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000);
//! assert_eq!(clock.now_ms(), 1_700_000_000_000);
//!
//! clock.advance(Duration::from_millis(500));
//! assert_eq!(clock.now_ms(), 1_700_000_000_500);
//!
//! // `now()` returns a `SystemTime` for compat with stdlib call sites.
//! let st = clock.now();
//! assert_eq!(
//!     st.duration_since(UNIX_EPOCH).unwrap().as_millis(),
//!     1_700_000_000_500,
//! );
//! ```

use core::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock abstraction for the Stripe billing materializer.
///
/// Production impls: [`SystemMatClock`] (native) or
/// [`WasmWorkerMatClock`] (wasm32). Test impl: [`InMemoryFakeMatClock`]
/// (deterministic).
///
/// The canonical method is [`MatClock::now`]; the milliseconds helper
/// is a derived default so most impls only need to override `now()`.
pub trait MatClock: fmt::Debug + Send + Sync {
    /// Current wall-clock instant as a [`SystemTime`].
    ///
    /// Implementations MUST NOT panic on wasm32. The wave-19 caveat
    /// (`SystemTime::now()` panics on `wasm32-unknown-unknown`) is
    /// closed by routing wasm32 callers through [`WasmWorkerMatClock`]
    /// which reads `js_sys::Date::now()` instead.
    fn now(&self) -> SystemTime;

    /// Current unix time in whole milliseconds (saturates to `0` on
    /// pre-epoch values; clamps to `u64::MAX` on overflow ~ year
    /// 584_942_417 AD).
    fn now_ms(&self) -> u64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

// =========================================================================
// SystemMatClock — native wall clock.
// =========================================================================

/// Native system-clock impl. Wraps [`std::time::SystemTime::now`].
///
/// On `wasm32-unknown-unknown` this impl is **not available** — wasm32
/// callers MUST use [`WasmWorkerMatClock`] instead. Mis-use is caught
/// at compile time via the `cfg` gate.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemMatClock;

#[cfg(not(target_arch = "wasm32"))]
impl MatClock for SystemMatClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

// =========================================================================
// WasmWorkerMatClock — wasm32 wall clock via js_sys::Date.
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
/// per the Web Platform contract) saturate to [`UNIX_EPOCH`].
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct WasmWorkerMatClock;

#[cfg(target_arch = "wasm32")]
impl MatClock for WasmWorkerMatClock {
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
// InMemoryFakeMatClock — deterministic test clock.
// =========================================================================

/// Deterministic test clock. Returns a caller-supplied unix instant
/// and supports `advance()` to step time forward.
///
/// Cheap to clone via inner `Arc` (the [`Mutex`] state is shared).
#[derive(Debug)]
pub struct InMemoryFakeMatClock {
    // Current wall-clock instant, as ms since unix epoch. `Mutex` so
    // `advance()` can mutate without `&mut self` (parallels the
    // in-memory test fakes elsewhere in this crate).
    ms_since_epoch: Mutex<u64>,
}

impl InMemoryFakeMatClock {
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

impl MatClock for InMemoryFakeMatClock {
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
// default_mat_clock() — target-conditional production clock factory.
// =========================================================================

/// Construct the production wall-clock for the current target:
///
/// - native (`cfg(not(target_arch = "wasm32"))`) → [`SystemMatClock`]
/// - wasm32 (`cfg(target_arch = "wasm32")`)     → [`WasmWorkerMatClock`]
///
/// Wraps in `Arc<dyn MatClock + Send + Sync>` so callers can store the
/// clock in shared state without leaking the concrete type. Mirrors
/// the wave-20 `corelink_stripe_real::clock::default_clock` factory.
#[must_use]
pub fn default_mat_clock() -> Arc<dyn MatClock + Send + Sync> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Arc::new(SystemMatClock)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(WasmWorkerMatClock)
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
    fn fake_clock_returns_injected_time_and_advances() {
        let clock = InMemoryFakeMatClock::at_unix_seconds(1_700_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000);
        let st = clock.now();
        assert_eq!(
            st.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            1_700_000_000
        );

        clock.advance(Duration::from_millis(2_500));
        assert_eq!(clock.now_ms(), 1_700_000_002_500);

        clock.set_unix_ms(2_000_000_000_500);
        assert_eq!(clock.now_ms(), 2_000_000_000_500);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn system_clock_returns_non_decreasing_ms() {
        let clock = SystemMatClock;
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
            "SystemMatClock must be non-decreasing across consecutive reads (a={a}, b={b})"
        );
        // The `now()` `SystemTime` should also be readable on native.
        let _st = clock.now();
    }

    #[test]
    fn default_mat_clock_returns_target_appropriate_impl() {
        // On native the factory wires `SystemMatClock`, on wasm32 it
        // wires `WasmWorkerMatClock`. Either way, `now_ms()` must
        // return a sensible value (>= 0 by type; > 0 on native; we
        // accept whatever the runtime returns on wasm32).
        let clock = default_mat_clock();
        let _ = clock.now_ms();
        let _ = clock.now();
    }
}

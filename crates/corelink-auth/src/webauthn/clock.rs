//! Monotonic clock abstraction.
//!
//! WebAuthn ceremonies are time-sensitive (challenge TTL, recovery
//! OTP TTL, step-up token TTL). A trait-injected clock lets property
//! tests drive deterministic time forward without `std::thread::sleep`,
//! and lets the production shim swap in a real `SystemTime`-backed
//! source.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Wall-clock reading expressed as milliseconds-since-epoch.
///
/// Using `u64` (≈ 584M years horizon) avoids `SystemTime` `Result`
/// noise and keeps the API trivially serializable for the in-memory
/// stores.
pub trait EngineClock: Send + Sync + std::fmt::Debug {
    /// Current monotonic timestamp in milliseconds since the unix
    /// epoch.
    fn now_ms(&self) -> u64;
}

/// Test-only clock with explicit cursor control.
#[derive(Debug, Clone)]
pub struct FixedClock {
    cursor: Arc<AtomicU64>,
}

impl FixedClock {
    /// Construct a clock anchored at the unix epoch (`0 ms`).
    #[must_use]
    pub fn epoch() -> Self {
        Self {
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Construct a clock anchored at an explicit timestamp.
    #[must_use]
    pub fn at_ms(ms: u64) -> Self {
        Self {
            cursor: Arc::new(AtomicU64::new(ms)),
        }
    }

    /// Advance the cursor by `delta_ms`. Saturating add — never wraps.
    pub fn advance_ms(&self, delta_ms: u64) {
        let mut current = self.cursor.load(Ordering::SeqCst);
        loop {
            let next = current.saturating_add(delta_ms);
            match self
                .cursor
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    /// Set the cursor to an explicit value (test helper).
    pub fn set_ms(&self, ms: u64) {
        self.cursor.store(ms, Ordering::SeqCst);
    }
}

impl EngineClock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.cursor.load(Ordering::SeqCst)
    }
}

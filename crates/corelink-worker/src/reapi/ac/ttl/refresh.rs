//! Refresh-on-hit threshold logic (WI-S04-005 §6.1.3).
//!
//! ## Why a threshold gate
//!
//! Without a gate, every GET on a hot digest fires a `D1 UPDATE
//! ac_meta SET last_hit_at = ?, expires_at = ?`. At 1 k req/s on a
//! hot digest = 1 k UPDATE/s ⇒ D1 lock contention + cost spike.
//! Under the canonical 60 s gate, at most 1 UPDATE/min per
//! `(tenant_id, action_digest)`; latency for the cache HIT path is
//! unchanged (the gate is pure-logic — no D1 round-trip when the
//! threshold is not met).
//!
//! ## Pure logic — no I/O, no clock, no state
//!
//! [`refresh_if_needed`] is a deterministic predicate driven by the
//! caller's wall-clock + the row's `last_hit_at`. The handler
//! consults it once per GET; the function has no side effects, so
//! property tests can drive 10 k iterations without spinning up a
//! tokio runtime.

use core::time::Duration;

/// Canonical refresh-on-hit threshold — **60 s** (`60_000` ms) per
/// WI-S04-005 §6.1.3. Tunable in production via the
/// `CORELINK_AC_REFRESH_THRESHOLD_MS` env var; the constant pins the
/// default that the handler property tests assert.
pub const DEFAULT_REFRESH_THRESHOLD_MS: u64 = 60_000;

/// Pure-logic refresh-on-hit threshold gate.
///
/// Returns `true` iff the wall-clock has advanced **at least**
/// `threshold` past `last_hit_at`. The handler issues the D1 UPDATE
/// only when this returns `true`; otherwise the GET path returns the
/// cached row without touching D1 (per WI §6.1.3 / §9.4 + Gherkin
/// "Refresh-on-hit threshold skip").
///
/// **Monotonic guard**: if `now_ms < last_hit_at_ms` (clock skew /
/// NTP rollback), the function returns `false` — never roll back
/// `last_hit_at` (per `INV-AC-TTL-MONOTONIC` + WI §6.1.3). The
/// existing row's `last_hit_at` is preserved by the meta store's
/// `refresh_on_hit` impl which clamps via `max(prev, now)`.
#[must_use]
pub const fn refresh_if_needed(now_ms: u64, last_hit_at_ms: u64, threshold: Duration) -> bool {
    // saturating_sub: if now_ms < last_hit_at_ms, returns 0 ⇒ false
    // for any positive threshold.
    let elapsed_ms = now_ms.saturating_sub(last_hit_at_ms);
    let threshold_ms = threshold_as_u64_ms(threshold);
    elapsed_ms >= threshold_ms
}

#[must_use]
const fn threshold_as_u64_ms(threshold: Duration) -> u64 {
    // Saturate at u64::MAX so a Duration::MAX threshold is treated as
    // "never refresh" (the usual handler value is 60 s; saturation is
    // belt-and-suspenders for adversarial fixtures).
    let secs = threshold.as_secs();
    let millis = threshold.subsec_millis() as u64;
    secs.saturating_mul(1000).saturating_add(millis)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn refresh_triggered_when_elapsed_meets_threshold() {
        let threshold = Duration::from_secs(60);
        // Exactly at threshold ⇒ refresh.
        assert!(refresh_if_needed(60_000, 0, threshold));
        // Past threshold ⇒ refresh.
        assert!(refresh_if_needed(60_001, 0, threshold));
    }

    #[test]
    fn refresh_skipped_when_below_threshold() {
        let threshold = Duration::from_secs(60);
        assert!(!refresh_if_needed(59_999, 0, threshold));
        assert!(!refresh_if_needed(30_000, 0, threshold));
    }

    #[test]
    fn refresh_handles_clock_rollback_safely() {
        // Wall clock rolled back (NTP correction); we must NOT refresh.
        let threshold = Duration::from_secs(60);
        assert!(!refresh_if_needed(1_000, 60_000, threshold));
        assert!(!refresh_if_needed(0, 60_000, threshold));
    }

    #[test]
    fn refresh_zero_threshold_always_triggers() {
        // A 0 s threshold means "always refresh" — useful for tests
        // and for an emergency env override forcing fresh writes.
        let threshold = Duration::from_secs(0);
        assert!(refresh_if_needed(0, 0, threshold));
        assert!(refresh_if_needed(1, 0, threshold));
        assert!(refresh_if_needed(u64::MAX, 0, threshold));
    }

    #[test]
    fn refresh_max_threshold_never_triggers_under_finite_clock() {
        // Belt-and-suspenders: a `Duration::MAX` threshold (saturating
        // to `u64::MAX` ms) MUST NOT report `true` for any finite
        // wall-clock value. (`u64::MAX >= u64::MAX` is mathematically
        // true, so the only way this test could fail is if we ever
        // see `now_ms == u64::MAX` — wall-clock in ms; year ~ 5×10^8
        // CE — bounded out of any realistic deployment lifetime.)
        let threshold = Duration::MAX;
        assert!(!refresh_if_needed(0, 0, threshold));
        assert!(!refresh_if_needed(86_400_000_000_u64, 0, threshold)); // ~2.7M years
    }

    #[test]
    fn refresh_default_threshold_is_60s() {
        assert_eq!(DEFAULT_REFRESH_THRESHOLD_MS, 60_000);
    }
}

//! Padding-target arithmetic + per-request seed mixing + splitmix
//! helpers.
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

use core::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

use http::Request;
use rand::{rngs::OsRng, Rng, SeedableRng, TryRngCore};
use rand_chacha::ChaCha20Rng;

use super::config::TimingPaddingConfig;
use super::policy::JitterPolicy;

/// Compute the canonical pad target (per ADR-0023 §"Decision" 1).
///
/// Returns the **total** wall-clock duration that should elapse from
/// the request entry point until the padded response is released.
/// Callers compose this with the captured `started` instant via
/// `tokio::time::sleep_until(started + canonical_pad_target(...))`.
///
/// Handler resolution that already exceeds the padded window is
/// returned **unchanged** — i.e. the function returns the larger of
/// `(target ± jitter, observed_elapsed)` so `sleep_until` never
/// produces a negative delay (per WI-S02-004 §10.4.8 — long-resolution
/// anomaly is logged separately by the caller; the middleware itself
/// never **shortens** a response).
#[must_use]
pub fn canonical_pad_target(
    config: TimingPaddingConfig,
    policy: &JitterPolicy,
    request_id_seed: u64,
    elapsed: Duration,
) -> Duration {
    let target_ms = config.target_p99_ms();
    let jitter_pct = config.jitter_pct();

    let signed_pct = match policy {
        JitterPolicy::Seeded => {
            let mut rng = ChaCha20Rng::seed_from_u64(request_id_seed);
            let signed = i32::from(jitter_pct);
            if signed == 0 {
                0i32
            } else {
                // Range is inclusive on both ends so the boundary
                // cases (`-jitter_pct` and `+jitter_pct`) appear in
                // the test set — flushing out off-by-one regressions.
                rng.random_range(-signed..=signed)
            }
        }
        JitterPolicy::FixedForTests { signed_pct } => i32::from(*signed_pct),
    };

    let delta_ms = (i64::try_from(target_ms).unwrap_or(i64::MAX) * i64::from(signed_pct)) / 100;
    let padded_ms = i64::try_from(target_ms).unwrap_or(i64::MAX).saturating_add(delta_ms);
    let padded_ms_u = u64::try_from(padded_ms.max(0)).unwrap_or(0);
    let padded = Duration::from_millis(padded_ms_u);
    if elapsed > padded {
        elapsed
    } else {
        padded
    }
}

/// Compute the per-request seed by mixing:
/// - The `x-request-id` header hash (when present).
/// - The per-layer server-side secret (`OsRng`-generated at layer
///   construction; opaque to the caller).
/// - A monotonic per-call counter (cross-request independence).
///
/// Returns a u64 suitable for `ChaCha20Rng::seed_from_u64`.
pub(super) fn compute_request_seed<B>(
    req: &Request<B>,
    server_secret: u64,
    call_counter: &AtomicU64,
) -> u64 {
    let header_seed = if let Some(value) = req.headers().get("x-request-id") {
        if let Ok(s) = value.to_str() {
            splitmix_str(s)
        } else {
            0
        }
    } else {
        0
    };
    let counter = call_counter.fetch_add(1, Ordering::Relaxed);
    // Mix all three: server_secret defeats client-controlled seeds;
    // counter defeats id-collision (same id, two probes); header_seed
    // gives correlation across ostensibly-paired logs / traces.
    splitmix_u64(server_secret ^ counter ^ header_seed)
}

/// Generate a per-layer server-side secret via `OsRng`. Falls back to
/// a deterministic seed mixed with the system epoch when `OsRng`
/// itself fails (treated as non-fatal because the per-call counter
/// still provides cross-request independence).
pub(super) fn generate_server_secret() -> u64 {
    let mut buf = [0u8; 8];
    if OsRng.try_fill_bytes(&mut buf).is_ok() {
        u64::from_le_bytes(buf)
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        splitmix_u64(now)
    }
}

/// SplitMix64 finalizer — fast, well-distributed, deterministic.
pub(super) fn splitmix_u64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub(super) fn splitmix_str(s: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    splitmix_u64(h)
}

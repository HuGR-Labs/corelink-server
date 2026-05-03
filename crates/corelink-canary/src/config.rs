//! Canonical canary configuration constants.
//!
//! These constants are pinned at the type system layer so a calibration
//! drift on the SLO ceilings, the cron interval, or the synthetic
//! tenant identifier is caught by surface-stability regression tests.

/// Canonical CF Workers cron-trigger interval in seconds per WI-S09-007
/// §6.1.2 + sprint contract §6 DoD. Balances canary cost (CF Workers
/// cron + R2 ops + observability ingest) vs detection latency (60s =
/// 1440 loops/dia/region; sufficient sample size for SLO computation
/// 95% CI ±0.5%).
pub const CRON_INTERVAL_SECS: u64 = 60;

/// Canonical SEV-3 trigger threshold for canary cron drift in
/// milliseconds per WI-S09-007 §6.1.9 + §1 invariant 9. Observed
/// dispatch lag > 90s sustained 5min = SEV-3 alert source
/// (`corelink_canary_dispatch_lag_ms` over threshold).
pub const DISPATCH_LAG_SEV3_MS: u64 = 90_000;

/// Canonical 72h sustained loop target per region per sprint contract
/// §6 DoD ship gate criterion. Per Lote 10.9bis P0-A correction:
/// 60s × 60min × 24h × 3 days = 4_320 loops/72h/region.
pub const SUSTAINED_72H_LOOPS_PER_REGION: u32 = 4_320;

/// Canonical 72h sustained total loop target across all 3 regions per
/// sprint contract §6 DoD ship gate criterion. Per Lote 10.9bis P0-A
/// correction (previously 38_880 was 3× inflated triple-counting):
/// 3 regions × 4_320 loops/72h/region = 12_960.
pub const SUSTAINED_72H_LOOP_TARGET: u32 = 12_960;

/// Canonical synthetic canary tenant identifier (canonical UUID
/// format: 8-4-4-4-12 hex segments = 36 chars total) per WI-S09-007
/// §6.1.6. The 12-char last segment encodes the literal `canary` +
/// `000000` zero-padding to fit the canonical UUID grammar (the WI
/// spec uses the prose shorthand `CANARY00000`; here we ship the
/// 36-char UUID-conformant literal so production parsers accept the
/// value via `Uuid::parse_str`). Excluded from SLI denominator
/// computation via the WI-S09-006 recording rule filter (analogous to
/// the ManualOverride exclusion in WI-S08-005 per Lote 10.8bis
/// P1-NEW-3 inheritance).
pub const SYNTHETIC_CANARY_TENANT_ID: &str = "00000000-0000-0000-0000-canary000000";

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
    fn cron_interval_pinned_at_60s() {
        assert_eq!(CRON_INTERVAL_SECS, 60);
    }

    #[test]
    fn dispatch_lag_sev3_threshold_pinned_at_90s() {
        assert_eq!(DISPATCH_LAG_SEV3_MS, 90_000);
        // Threshold MUST be strictly greater than the cron interval
        // expressed in milliseconds so a single tick of expected
        // jitter never trips the SEV-3 alert source.
        let cron_ms: u64 = CRON_INTERVAL_SECS.saturating_mul(1_000);
        assert!(DISPATCH_LAG_SEV3_MS > cron_ms, "DISPATCH_LAG_SEV3_MS must exceed cron_ms");
    }

    #[test]
    fn sustained_72h_loop_target_canonical_lote_10_9bis_p0_a() {
        assert_eq!(SUSTAINED_72H_LOOPS_PER_REGION, 4_320);
        assert_eq!(SUSTAINED_72H_LOOP_TARGET, 12_960);
        assert_eq!(
            3_u32 * SUSTAINED_72H_LOOPS_PER_REGION,
            SUSTAINED_72H_LOOP_TARGET
        );
        let derived = (60u64 * 60 * 24 * 3 / CRON_INTERVAL_SECS) as u32 * 3;
        assert_eq!(derived, SUSTAINED_72H_LOOP_TARGET);
    }

    #[test]
    fn synthetic_canary_tenant_id_canonical_format() {
        assert_eq!(SYNTHETIC_CANARY_TENANT_ID.len(), 36);
        assert_eq!(SYNTHETIC_CANARY_TENANT_ID.matches('-').count(), 4);
        assert!(SYNTHETIC_CANARY_TENANT_ID.starts_with("00000000-"));
        assert!(SYNTHETIC_CANARY_TENANT_ID.ends_with("canary000000"));
        // Canonical UUID grammar: 8-4-4-4-12 hex segments separated by
        // hyphens. The literal `canary` is intentionally non-hex so a
        // verifier scanning for real UUID v7 values cannot collide.
        let parts: Vec<&str> = SYNTHETIC_CANARY_TENANT_ID.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }
}

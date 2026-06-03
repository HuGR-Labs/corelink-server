//! SLO definition + canonical SLI taxonomy per `slo_catalog.md`.
//!
//! ## Why an enum
//!
//! Closed taxonomy at compile-time per WI §9.2 + Lote 10.8bis
//! discipline: a `String` slot here would let a typo'd SLI bypass
//! cardinality + alert coverage CI gates. The `#[non_exhaustive]`
//! marker reserves additive growth for follow-on WIs without
//! breaking downstream sinks.

use crate::error::SloError;

/// Canonical SLI taxonomy from `slo_catalog.md` §4.x. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. S-13 admin-plane SLIs forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Sli {
    /// `SLI-AVAIL-CAS-GET` per `slo_catalog.md §4.2`. CAS read path
    /// is the hot path of product value.
    AvailCasGet,
    /// `SLI-AVAIL-CAS-PUT` per `slo_catalog.md §4.3`.
    AvailCasPut,
    /// `SLI-AVAIL-AC-LOOKUP` per `slo_catalog.md §4` — Action Cache
    /// lookup availability.
    AvailAcLookup,
    /// `SLI-AVAIL-AUTH` per `slo_catalog.md §4` — auth control plane
    /// availability.
    AvailAuth,
    /// `SLI-LATENCY-CAS-GET-P99` per `slo_catalog.md §4` — p99
    /// latency target for CAS GET.
    LatencyCasGetP99,
    /// `SLO-DEDUP-RATIO` per `slo_catalog.md §4` — deduplication
    /// ratio (S-07 inheritance; quality SLI).
    DedupRatio,
    /// `SLI-RATE-LIMIT-WITHIN-QUOTA` per `slo_catalog.md §4` —
    /// fraction of within-quota requests that are NOT rate-limited
    /// (S-08 inheritance; rate-limiter false-positive bound).
    RateLimitWithinQuota,
    /// `SLI-AVAIL-CP` per `slo_catalog.md §4.1` — control-plane
    /// (auth + admin + non-storage) availability. Closure from
    /// `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md` P0-1.
    AvailControlPlane,
    /// `SLI-LATENCY-CAS-PUT-P99` per `slo_catalog.md §4.7` — CAS PUT
    /// p99 latency target (1s team / 600ms enterprise, blobs ≤ 16 MiB).
    /// Closure from audit P0-2.
    LatencyCasPutP99,
    /// `SLI-LATENCY-AC-HIT-P99` per `slo_catalog.md §4.8` — Action
    /// Cache hit p99 latency target (150ms). Closure from audit P0-3.
    LatencyAcHitP99,
    /// `SLO-CORRECT-CAS` per `slo_catalog.md §4.9` — CAS integrity
    /// correctness (100% target, zero-budget; any client-side
    /// verify mismatch = SEV-1). Closure from audit P0-4.
    CorrectnessCas,
    /// `SLO-CORRECT-ISO` per `slo_catalog.md §4.10` — tenant isolation
    /// correctness (100% target, zero-budget; any violation = SEV-1
    /// + TLA+ re-check). Closure from audit P0-5.
    CorrectnessTenantIsolation,
    /// `SLO-BACKUP-VERIFICATION` per `slo_catalog.md §4.22` — daily
    /// backup verification pass rate (≥ 99.5 % rolling 30d; daily cron
    /// cycle covers R2 / D1 / KV tiers; complements GAP-15 cold-restore
    /// drill). Closure from DR-16 wave-14 audit (pre-existing DEBT-011
    /// gap).
    BackupVerification,
    /// `SLO-REPLICATION-LAG-R2` per `slo_catalog.md §4.23` — cross-region
    /// R2 replication lag (p99 ≤ 60 s hot blobs / p99 ≤ 24 h CRR). Closure
    /// from DR-16 wave-14 audit.
    ReplicationLagR2,
    /// `SLO-REPLICATION-LAG-D1` per `slo_catalog.md §4.24` — D1
    /// read-replica lag (p99 ≤ 60 s). Closure from DR-16 wave-14 audit.
    ReplicationLagD1,
    /// `SLO-REPLICATION-LAG-KV` per `slo_catalog.md §4.25` — KV global
    /// eventual propagation lag (p99 ≤ 60 s typical / ≤ 300 s pessimistic).
    /// Closure from DR-16 wave-14 audit.
    ReplicationLagKv,
    /// `SLO-REPLICATION-LAG-NEON` per `slo_catalog.md §4.27` — Neon
    /// read-replica lag (soft / informational; p99 ≤ 5 s, no paging at
    /// GA). Closure from DR-16 wave-14 audit.
    ReplicationLagNeon,
    /// `SLO-FRESH-DSR-ERASURE` per `slo_catalog.md §4.12` — DSR
    /// erasure resolution freshness target (≥ 99 % of erasure tickets
    /// resolve within 30 days; LGPD Art. 19 + GDPR Art. 12.3 + CCPA
    /// §1798.130 SLA). Bound from
    /// `specs/_audits/sealed/2026-05-15-dsr-worker-production.md §3`
    /// closure of WI-S11-002 SLI binding gap. The emit point is the
    /// `corelink-privacy-erasure-worker` 24h verification job which
    /// observes `corelink_dsr_resolution_hours` once per completed
    /// DSR ticket (S-11 / WI-S11-002 §10.4 O-4.1 metric).
    FreshDsrErasure,
}

impl Sli {
    /// Canonical SLI slug used in alert `labels.slo`, audit records,
    /// and PagerDuty dedup-key construction. Mirrors the `slo_catalog`
    /// SLI ID exactly.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AvailCasGet => "SLI-AVAIL-CAS-GET",
            Self::AvailCasPut => "SLI-AVAIL-CAS-PUT",
            Self::AvailAcLookup => "SLI-AVAIL-AC-LOOKUP",
            Self::AvailAuth => "SLI-AVAIL-AUTH",
            Self::LatencyCasGetP99 => "SLI-LATENCY-CAS-GET-P99",
            Self::DedupRatio => "SLO-DEDUP-RATIO",
            Self::RateLimitWithinQuota => "SLI-RATE-LIMIT-WITHIN-QUOTA",
            Self::AvailControlPlane => "SLI-AVAIL-CP",
            Self::LatencyCasPutP99 => "SLI-LATENCY-CAS-PUT-P99",
            Self::LatencyAcHitP99 => "SLI-LATENCY-AC-HIT-P99",
            Self::CorrectnessCas => "SLO-CORRECT-CAS",
            Self::CorrectnessTenantIsolation => "SLO-CORRECT-ISO",
            Self::BackupVerification => "SLO-BACKUP-VERIFICATION",
            Self::ReplicationLagR2 => "SLO-REPLICATION-LAG-R2",
            Self::ReplicationLagD1 => "SLO-REPLICATION-LAG-D1",
            Self::ReplicationLagKv => "SLO-REPLICATION-LAG-KV",
            Self::ReplicationLagNeon => "SLO-REPLICATION-LAG-NEON",
            Self::FreshDsrErasure => "SLO-FRESH-DSR-ERASURE",
        }
    }

    /// Lower-case Prometheus-compatible metric base name (per
    /// OpenMetrics 1.0 naming convention).
    #[must_use]
    pub const fn prometheus_metric_base(self) -> &'static str {
        match self {
            Self::AvailCasGet => "corelink_cas_get",
            Self::AvailCasPut => "corelink_cas_put",
            Self::AvailAcLookup => "corelink_ac_lookup",
            Self::AvailAuth => "corelink_auth_attempts",
            Self::LatencyCasGetP99 => "corelink_cas_get_latency",
            Self::DedupRatio => "corelink_dedup_ratio",
            Self::RateLimitWithinQuota => "corelink_rate_limited_within_quota",
            Self::AvailControlPlane => "corelink_cp_requests",
            Self::LatencyCasPutP99 => "corelink_cas_put_latency",
            Self::LatencyAcHitP99 => "corelink_ac_get_latency",
            Self::CorrectnessCas => "corelink_cas_client_verify",
            Self::CorrectnessTenantIsolation => "corelink_isolation_assertion",
            Self::BackupVerification => "corelink_backup_verification_status",
            Self::ReplicationLagR2 => "corelink_replication_lag_seconds",
            Self::ReplicationLagD1 => "corelink_d1_replica_lag_seconds",
            Self::ReplicationLagKv => "corelink_kv_propagation_lag_seconds",
            Self::ReplicationLagNeon => "corelink_neon_replica_lag_seconds",
            Self::FreshDsrErasure => "corelink_dsr_resolution_hours",
        }
    }
}

impl core::fmt::Display for Sli {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 18-element SLI list per `slo_catalog.md §4.x` and WI
/// §6.1.1, extended by audit 2026-05-14 P0 closures (+5 SLIs:
/// AvailControlPlane, LatencyCasPutP99, LatencyAcHitP99,
/// CorrectnessCas, CorrectnessTenantIsolation), audit DR-16 wave-14
/// closures (+5 SLIs: BackupVerification, ReplicationLagR2,
/// ReplicationLagD1, ReplicationLagKv, ReplicationLagNeon —
/// pre-existing DEBT-011 gaps now bound), and audit
/// `2026-05-15-dsr-worker-production.md` closure (+1 SLI:
/// FreshDsrErasure — DSR erasure SLA freshness target per
/// `slo_catalog.md §4.12` / WI-S11-002 §10.4 O-4.1). Pinned at the
/// type system layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_slis() -> &'static [Sli; 18] {
    &[
        Sli::AvailCasGet,
        Sli::AvailCasPut,
        Sli::AvailAcLookup,
        Sli::AvailAuth,
        Sli::LatencyCasGetP99,
        Sli::DedupRatio,
        Sli::RateLimitWithinQuota,
        Sli::AvailControlPlane,
        Sli::LatencyCasPutP99,
        Sli::LatencyAcHitP99,
        Sli::CorrectnessCas,
        Sli::CorrectnessTenantIsolation,
        Sli::BackupVerification,
        Sli::ReplicationLagR2,
        Sli::ReplicationLagD1,
        Sli::ReplicationLagKv,
        Sli::ReplicationLagNeon,
        Sli::FreshDsrErasure,
    ]
}

/// Canonical SLO definition: the SLI being measured + the target
/// availability percent + the derived error budget percent. Used by
/// [`crate::calculator::BurnRateCalculator`] to compute the
/// per-window burn-rate decision per Google SRE Workbook Ch 5
/// Table 4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SloDefinition {
    /// The SLI being measured.
    pub sli: Sli,
    /// Target availability as a fraction in `(0.0, 1.0]` (e.g. 0.999
    /// for 99.9 %). Strict upper bound 1.0 inclusive (a 100 %
    /// target with zero budget is allowed; some correctness SLIs are
    /// 100 % per `slo_catalog.md §3.1` — they treat any error as
    /// a budget breach which translates to a `PageSev0` arm at any
    /// non-zero error rate).
    pub target_pct: f64,
    /// Derived error budget percent: `1.0 - target_pct`. Stored
    /// pre-computed so the calculator does not pay a subtraction per
    /// evaluation hot-path.
    pub error_budget_pct: f64,
    /// Rolling SLO window in days (canonical 30 per `slo_catalog.md`).
    pub window_days: u32,
}

impl SloDefinition {
    /// Construct a canonical 30-day-window SLO from `(sli, target_pct)`.
    ///
    /// # Errors
    ///
    /// Returns [`SloError::InvalidSloDefinition`] when:
    /// - `target_pct` is NaN, infinite, ≤ 0.0, or > 1.0.
    pub fn new(sli: Sli, target_pct: f64) -> Result<Self, SloError> {
        Self::with_window_days(sli, target_pct, 30)
    }

    /// Construct an SLO with an explicit rolling window in days.
    ///
    /// # Errors
    ///
    /// Returns [`SloError::InvalidSloDefinition`] when:
    /// - `target_pct` is NaN, infinite, ≤ 0.0, or > 1.0;
    /// - `window_days` is 0.
    pub fn with_window_days(sli: Sli, target_pct: f64, window_days: u32) -> Result<Self, SloError> {
        if !target_pct.is_finite() || target_pct <= 0.0 || target_pct > 1.0 {
            return Err(SloError::InvalidSloDefinition(format!(
                "target_pct must be in (0.0, 1.0]; got {target_pct}"
            )));
        }
        if window_days == 0 {
            return Err(SloError::InvalidSloDefinition(
                "window_days must be ≥ 1".to_string(),
            ));
        }
        let error_budget_pct = 1.0 - target_pct;
        Ok(Self {
            sli,
            target_pct,
            error_budget_pct,
            window_days,
        })
    }

    /// Return the canonical SLI slug for audit records + alert
    /// annotations.
    #[must_use]
    pub const fn sli_slug(&self) -> &'static str {
        self.sli.slug()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_slis_unique_slugs_pinned() {
        let v = canonical_slis();
        assert_eq!(v.len(), 18);
        let mut set = std::collections::HashSet::new();
        for s in v {
            assert!(s.slug().starts_with("SLI-") || s.slug().starts_with("SLO-"));
            assert!(set.insert(s.slug()), "duplicate sli slug: {s}");
        }
    }

    #[test]
    fn audit_dr_16_wave_14_closures_present() {
        // Five DEBT-011-vintage gaps closed by DR-16 wave-14:
        // backup-verification + 4 replication-lag domains. Pre-existing
        // SLOs that lacked Sli enum binding; now bound with canonical
        // slugs matching `slo_catalog.md §4.22, §4.23, §4.24, §4.25, §4.27`.
        let v = canonical_slis();
        let slugs: std::collections::HashSet<&str> = v.iter().map(|s| s.slug()).collect();
        assert!(slugs.contains("SLO-BACKUP-VERIFICATION"));
        assert!(slugs.contains("SLO-REPLICATION-LAG-R2"));
        assert!(slugs.contains("SLO-REPLICATION-LAG-D1"));
        assert!(slugs.contains("SLO-REPLICATION-LAG-KV"));
        assert!(slugs.contains("SLO-REPLICATION-LAG-NEON"));
    }

    #[test]
    fn audit_dr_16_wave_14_closures_prometheus_bases_pinned() {
        // Prometheus base names are LOAD-BEARING: they must match the
        // canonical metric-name constants in the emit-site crates
        // (corelink-backup-verify, corelink-region, corelink-replica-worker).
        // Cross-crate alignment tests live in each emit-site crate's
        // `tests/sli_binding.rs`.
        assert_eq!(
            Sli::BackupVerification.prometheus_metric_base(),
            "corelink_backup_verification_status"
        );
        assert_eq!(
            Sli::ReplicationLagR2.prometheus_metric_base(),
            "corelink_replication_lag_seconds"
        );
        assert_eq!(
            Sli::ReplicationLagD1.prometheus_metric_base(),
            "corelink_d1_replica_lag_seconds"
        );
        assert_eq!(
            Sli::ReplicationLagKv.prometheus_metric_base(),
            "corelink_kv_propagation_lag_seconds"
        );
        assert_eq!(
            Sli::ReplicationLagNeon.prometheus_metric_base(),
            "corelink_neon_replica_lag_seconds"
        );
    }

    #[test]
    fn audit_dr_16_wave_14_closures_display_matches_slug() {
        assert_eq!(
            format!("{}", Sli::BackupVerification),
            "SLO-BACKUP-VERIFICATION"
        );
        assert_eq!(
            format!("{}", Sli::ReplicationLagR2),
            "SLO-REPLICATION-LAG-R2"
        );
        assert_eq!(
            format!("{}", Sli::ReplicationLagD1),
            "SLO-REPLICATION-LAG-D1"
        );
        assert_eq!(
            format!("{}", Sli::ReplicationLagKv),
            "SLO-REPLICATION-LAG-KV"
        );
        assert_eq!(
            format!("{}", Sli::ReplicationLagNeon),
            "SLO-REPLICATION-LAG-NEON"
        );
    }

    #[test]
    fn audit_2026_05_15_dsr_worker_closure_present() {
        // S-11 / WI-S11-002 SLI binding closure shipped per
        // `specs/_audits/sealed/2026-05-15-dsr-worker-production.md §3`.
        let v = canonical_slis();
        let slugs: std::collections::HashSet<&str> = v.iter().map(|s| s.slug()).collect();
        assert!(slugs.contains("SLO-FRESH-DSR-ERASURE"));
        assert_eq!(
            Sli::FreshDsrErasure.prometheus_metric_base(),
            "corelink_dsr_resolution_hours"
        );
        assert_eq!(format!("{}", Sli::FreshDsrErasure), "SLO-FRESH-DSR-ERASURE");
    }

    #[test]
    fn audit_2026_05_14_p0_closures_present() {
        // Five P0 closures shipped per
        // `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §5`.
        let v = canonical_slis();
        let slugs: std::collections::HashSet<&str> = v.iter().map(|s| s.slug()).collect();
        assert!(slugs.contains("SLI-AVAIL-CP"));
        assert!(slugs.contains("SLI-LATENCY-CAS-PUT-P99"));
        assert!(slugs.contains("SLI-LATENCY-AC-HIT-P99"));
        assert!(slugs.contains("SLO-CORRECT-CAS"));
        assert!(slugs.contains("SLO-CORRECT-ISO"));
    }

    #[test]
    fn audit_2026_05_14_p0_closures_prometheus_bases_pinned() {
        // Prometheus base names canonical per audit §5 closure list;
        // these slugs flow into `corelink-slo::alert` PromQL rule
        // construction in the multi-burn-rate alert path.
        assert_eq!(
            Sli::AvailControlPlane.prometheus_metric_base(),
            "corelink_cp_requests"
        );
        assert_eq!(
            Sli::LatencyCasPutP99.prometheus_metric_base(),
            "corelink_cas_put_latency"
        );
        assert_eq!(
            Sli::LatencyAcHitP99.prometheus_metric_base(),
            "corelink_ac_get_latency"
        );
        assert_eq!(
            Sli::CorrectnessCas.prometheus_metric_base(),
            "corelink_cas_client_verify"
        );
        assert_eq!(
            Sli::CorrectnessTenantIsolation.prometheus_metric_base(),
            "corelink_isolation_assertion"
        );
    }

    #[test]
    fn correctness_slos_accept_100pct_target() {
        // SLO-CORRECT-CAS and SLO-CORRECT-ISO are 100% / zero-budget
        // SLOs per `slo_catalog.md §4.9` + §4.10. Constructing the
        // SloDefinition at target_pct=1.0 must succeed (per §3.1
        // correctness SLI semantics).
        let s_cas = SloDefinition::new(Sli::CorrectnessCas, 1.0).unwrap();
        assert_eq!(s_cas.target_pct, 1.0);
        assert_eq!(s_cas.error_budget_pct, 0.0);
        let s_iso = SloDefinition::new(Sli::CorrectnessTenantIsolation, 1.0).unwrap();
        assert_eq!(s_iso.target_pct, 1.0);
        assert_eq!(s_iso.error_budget_pct, 0.0);
    }

    #[test]
    fn audit_p0_slis_display_matches_slug() {
        // Display path is used in audit records + alert
        // annotations + PagerDuty dedup-key construction; pinning
        // here so a typo in `slug()` would break Display too.
        assert_eq!(format!("{}", Sli::AvailControlPlane), "SLI-AVAIL-CP");
        assert_eq!(
            format!("{}", Sli::LatencyCasPutP99),
            "SLI-LATENCY-CAS-PUT-P99"
        );
        assert_eq!(
            format!("{}", Sli::LatencyAcHitP99),
            "SLI-LATENCY-AC-HIT-P99"
        );
        assert_eq!(format!("{}", Sli::CorrectnessCas), "SLO-CORRECT-CAS");
        assert_eq!(
            format!("{}", Sli::CorrectnessTenantIsolation),
            "SLO-CORRECT-ISO"
        );
    }

    #[test]
    fn prometheus_metric_base_lowercase_underscore() {
        // Prometheus / OpenMetrics 1.0 naming permits `[a-zA-Z_:][a-zA-Z0-9_:]*`.
        // We restrict the first byte to lowercase + underscore (no colons —
        // colons are reserved for recording-rule outputs), and the remainder
        // to lowercase / underscore / ASCII digits. Digits are required by
        // the DR-16 wave-14 closures (`corelink_d1_replica_lag_seconds` etc.).
        for s in canonical_slis() {
            let base = s.prometheus_metric_base();
            assert!(base.starts_with("corelink_"));
            let mut bytes = base.bytes();
            let first = bytes.next().expect("non-empty base");
            assert!(
                first == b'_' || first.is_ascii_lowercase(),
                "first byte must be lowercase or _: {base}"
            );
            assert!(
                bytes.all(|b| b == b'_' || b.is_ascii_lowercase() || b.is_ascii_digit()),
                "non-canonical char in {base}"
            );
        }
    }

    #[test]
    fn slo_new_rejects_invalid_target_pct() {
        assert!(SloDefinition::new(Sli::AvailCasGet, 0.0).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, -0.1).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, 1.1).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, f64::NAN).is_err());
        assert!(SloDefinition::new(Sli::AvailCasGet, f64::INFINITY).is_err());
    }

    #[test]
    fn slo_new_accepts_canonical_targets() {
        let s = SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap();
        assert_eq!(s.sli, Sli::AvailCasGet);
        assert_eq!(s.target_pct, 0.999);
        assert!((s.error_budget_pct - 0.001).abs() < 1e-12);
        assert_eq!(s.window_days, 30);
    }

    #[test]
    fn slo_with_window_days_rejects_zero() {
        assert!(SloDefinition::with_window_days(Sli::AvailCasGet, 0.999, 0).is_err());
    }

    #[test]
    fn slo_target_100pct_yields_zero_budget() {
        let s = SloDefinition::new(Sli::DedupRatio, 1.0).unwrap();
        assert_eq!(s.target_pct, 1.0);
        assert_eq!(s.error_budget_pct, 0.0);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Sli::AvailCasGet), "SLI-AVAIL-CAS-GET");
    }
}

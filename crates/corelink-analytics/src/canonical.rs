//! Canonical RED + USE metric taxonomy — `#[non_exhaustive]` enum
//! pinned to `observability_model.md §4.2` 9-RED + 6-USE list.
//!
//! Lote 10.8bis discipline: enum exhaustive at compile-time; no
//! string-typed metric names accepted in the hot-path. NEW metric
//! requires PR amending this enum + `observability_model.md §4.2` +
//! re-running `scripts/cardinality_check.py`.
//!
//! ## Why an enum, not a string
//!
//! Per WI-S09-001 §9.2 design decision: dynamic metric registration
//! (string-typed name) opens a cardinality blow-up vector — bad-PR
//! adding `format!("custom_{tenant_id}_total", …)` would silently
//! create one metric per tenant. The enum bound forces every NEW
//! metric to land in this file, which CODEOWNERS routes to the
//! observability-discipline reviewer (sprint contract §6 DoD).

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
use core::fmt;

/// Canonical 9-RED + 6-USE metric kind enum (15 total).
///
/// Each variant maps 1:1 to a Prometheus-compatible metric name slug
/// (snake_case underscore-prefixed `corelink_*` per OpenMetrics 1.0 +
/// Prom remote-write naming convention; the dotted CloudEvents
/// `corelink.cas.put.requests_total` shape is reserved for the audit
/// taxonomy in [`crate::audit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RedMetricKind {
    // ---- 9 RED metrics (sprint contract §5.1 R-S09-1) ---------------
    /// `corelink_cas_put_requests_total{tenant_tier, region, result}` —
    /// counter; CAS PUT request rate.
    CasPutRequestsTotal,
    /// `corelink_cas_put_duration_seconds{tenant_tier, region}` —
    /// histogram; CAS PUT duration distribution.
    CasPutDurationSeconds,
    /// `corelink_cas_get_bytes_total{tenant_tier, region}` — counter;
    /// CAS GET bytes egress.
    CasGetBytesTotal,
    /// `corelink_ac_lookup_requests_total{tenant_tier, region, hit_miss}`
    /// — counter; ActionCache lookup rate.
    AcLookupRequestsTotal,
    /// `corelink_gc_runs_total{phase, result}` — counter; GC runs by
    /// phase (mark / sweep / soft_delete / phys_delete).
    GcRunsTotal,
    /// `corelink_dedup_ratio{tenant_tier, region}` — gauge; dedup
    /// ratio (S-07).
    DedupRatio,
    /// `corelink_rate_limit_rejects_total{layer, tenant_tier, reason}`
    /// — counter; rate-limit rejections (S-08).
    RateLimitRejectsTotal,
    /// `corelink_privacy_dsr_active_total{dsr_type}` — gauge; active
    /// privacy DSR requests by type (S-11).
    PrivacyDsrActiveTotal,
    /// `corelink_billing_events_emitted_total{event_type, region}` —
    /// counter; billing events emitted (S-10).
    BillingEventsEmittedTotal,

    // ---- 6 USE metrics (sprint contract §5.1 R-S09-3) ---------------
    /// `corelink_cf_cpu_time_us{region}` — gauge; CF Worker CPU time
    /// per invocation (microseconds).
    CfCpuTimeUs,
    /// `corelink_r2_ops_total{bucket, op_type}` — counter; R2 ops/sec
    /// per bucket.
    R2OpsTotal,
    /// `corelink_d1_row_scans_total{database}` — counter; D1 row scans
    /// per database (cost signal).
    D1RowScansTotal,
    /// `corelink_kv_read_quota_used{namespace}` — gauge; KV read quota
    /// utilization per namespace (0.0–1.0 fraction).
    KvReadQuotaUsed,
    /// `corelink_kv_write_quota_used{namespace}` — gauge; KV write
    /// quota utilization per namespace (0.0–1.0 fraction).
    KvWriteQuotaUsed,
    /// `corelink_do_storage_size_bytes{do_class}` — gauge; Durable
    /// Object storage size per DO class.
    DoStorageSizeBytes,
}

/// Total count of canonical metric kinds (9 RED + 6 USE = 15).
pub const CANONICAL_METRIC_COUNT: usize = 15;

impl RedMetricKind {
    /// Canonical Prom-compatible metric-name slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CasPutRequestsTotal => {
                "corelink_cas_put_requests_total"
            }
            Self::CasPutDurationSeconds => {
                "corelink_cas_put_duration_seconds"
            }
            Self::CasGetBytesTotal => "corelink_cas_get_bytes_total",
            Self::AcLookupRequestsTotal => {
                "corelink_ac_lookup_requests_total"
            }
            Self::GcRunsTotal => "corelink_gc_runs_total",
            Self::DedupRatio => "corelink_dedup_ratio",
            Self::RateLimitRejectsTotal => {
                "corelink_rate_limit_rejects_total"
            }
            Self::PrivacyDsrActiveTotal => {
                "corelink_privacy_dsr_active_total"
            }
            Self::BillingEventsEmittedTotal => {
                "corelink_billing_events_emitted_total"
            }
            Self::CfCpuTimeUs => "corelink_cf_cpu_time_us",
            Self::R2OpsTotal => "corelink_r2_ops_total",
            Self::D1RowScansTotal => "corelink_d1_row_scans_total",
            Self::KvReadQuotaUsed => "corelink_kv_read_quota_used",
            Self::KvWriteQuotaUsed => "corelink_kv_write_quota_used",
            Self::DoStorageSizeBytes => "corelink_do_storage_size_bytes",
        }
    }

    /// Whether this metric is a histogram (subject to bucket-multiplier
    /// cardinality estimation per Lote 10.9bis P0-I).
    #[must_use]
    pub const fn is_histogram(self) -> bool {
        matches!(self, Self::CasPutDurationSeconds)
    }

    /// Whether this metric is a counter (monotonic; subject to
    /// `prop_red_rate_monotone` + `prop_red_errors_monotone` invariants).
    #[must_use]
    pub const fn is_counter(self) -> bool {
        matches!(
            self,
            Self::CasPutRequestsTotal
                | Self::CasGetBytesTotal
                | Self::AcLookupRequestsTotal
                | Self::GcRunsTotal
                | Self::RateLimitRejectsTotal
                | Self::BillingEventsEmittedTotal
                | Self::R2OpsTotal
                | Self::D1RowScansTotal
        )
    }

    /// Whether this metric is a gauge (overwrite semantics).
    #[must_use]
    pub const fn is_gauge(self) -> bool {
        matches!(
            self,
            Self::DedupRatio
                | Self::PrivacyDsrActiveTotal
                | Self::CfCpuTimeUs
                | Self::KvReadQuotaUsed
                | Self::KvWriteQuotaUsed
                | Self::DoStorageSizeBytes
        )
    }
}

impl fmt::Display for RedMetricKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 15-element metric-name list for cross-component
/// regression tests + dashboard widget configuration.
///
/// Ordering mirrors the [`RedMetricKind`] enum declaration.
#[must_use]
pub const fn canonical_metric_names() -> &'static [&'static str;
       CANONICAL_METRIC_COUNT]
{
    &[
        "corelink_cas_put_requests_total",
        "corelink_cas_put_duration_seconds",
        "corelink_cas_get_bytes_total",
        "corelink_ac_lookup_requests_total",
        "corelink_gc_runs_total",
        "corelink_dedup_ratio",
        "corelink_rate_limit_rejects_total",
        "corelink_privacy_dsr_active_total",
        "corelink_billing_events_emitted_total",
        "corelink_cf_cpu_time_us",
        "corelink_r2_ops_total",
        "corelink_d1_row_scans_total",
        "corelink_kv_read_quota_used",
        "corelink_kv_write_quota_used",
        "corelink_do_storage_size_bytes",
    ]
}

/// Canonical 15-element [`RedMetricKind`] enumeration list — pinned
/// for cross-component regression tests + property-test seed sources.
#[must_use]
pub const fn canonical_metric_kinds(
) -> &'static [RedMetricKind; CANONICAL_METRIC_COUNT] {
    &[
        RedMetricKind::CasPutRequestsTotal,
        RedMetricKind::CasPutDurationSeconds,
        RedMetricKind::CasGetBytesTotal,
        RedMetricKind::AcLookupRequestsTotal,
        RedMetricKind::GcRunsTotal,
        RedMetricKind::DedupRatio,
        RedMetricKind::RateLimitRejectsTotal,
        RedMetricKind::PrivacyDsrActiveTotal,
        RedMetricKind::BillingEventsEmittedTotal,
        RedMetricKind::CfCpuTimeUs,
        RedMetricKind::R2OpsTotal,
        RedMetricKind::D1RowScansTotal,
        RedMetricKind::KvReadQuotaUsed,
        RedMetricKind::KvWriteQuotaUsed,
        RedMetricKind::DoStorageSizeBytes,
    ]
}

/// Type alias for ergonomic property-test seed: a `Vec<RedMetricKind>`
/// passed through proptest strategies via [`canonical_metric_kinds`].
pub type RedMetricKindSet = [RedMetricKind; CANONICAL_METRIC_COUNT];

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
    fn canonical_count_is_15() {
        assert_eq!(CANONICAL_METRIC_COUNT, 15);
        assert_eq!(canonical_metric_names().len(), 15);
        assert_eq!(canonical_metric_kinds().len(), 15);
    }

    #[test]
    fn every_kind_has_unique_canonical_name() {
        let mut set = std::collections::HashSet::new();
        for k in canonical_metric_kinds() {
            assert!(k.as_str().starts_with("corelink_"));
            assert!(set.insert(k.as_str()), "duplicate name: {}", k);
        }
        assert_eq!(set.len(), CANONICAL_METRIC_COUNT);
    }

    #[test]
    fn name_list_aligns_with_enum_list() {
        let names = canonical_metric_names();
        let kinds = canonical_metric_kinds();
        for (idx, name) in names.iter().enumerate() {
            assert_eq!(*name, kinds[idx].as_str(), "drift at idx {idx}");
        }
    }

    #[test]
    fn red_9_partition_uses_6() {
        let kinds = canonical_metric_kinds();
        let red_set: std::collections::HashSet<&'static str> = kinds[..9]
            .iter()
            .map(|k| k.as_str())
            .collect();
        let use_set: std::collections::HashSet<&'static str> = kinds[9..]
            .iter()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(red_set.len(), 9);
        assert_eq!(use_set.len(), 6);
        assert!(red_set.is_disjoint(&use_set));
    }

    #[test]
    fn histogram_classification_only_cas_put_duration() {
        for k in canonical_metric_kinds() {
            let expected = matches!(k, RedMetricKind::CasPutDurationSeconds);
            assert_eq!(
                k.is_histogram(),
                expected,
                "{} histogram mis-classified",
                k
            );
        }
    }

    #[test]
    fn each_kind_is_one_of_counter_gauge_histogram() {
        for k in canonical_metric_kinds() {
            let n = u32::from(k.is_counter())
                + u32::from(k.is_gauge())
                + u32::from(k.is_histogram());
            assert_eq!(
                n, 1,
                "{} must be exactly one of counter/gauge/histogram \
                 (got n={n})",
                k
            );
        }
    }

    #[test]
    fn display_matches_as_str() {
        let k = RedMetricKind::CasPutRequestsTotal;
        assert_eq!(format!("{}", k), "corelink_cas_put_requests_total");
    }

    #[test]
    fn no_dotted_names_in_canonical_list() {
        // Lote 10.9bis P0-E lesson: Prom remote-write expects
        // underscored names (NOT dotted CloudEvents shape). Audit
        // taxonomy uses dotted names; metric taxonomy uses
        // underscored.
        for n in canonical_metric_names() {
            assert!(!n.contains('.'), "dotted metric: {n}");
            assert!(n.starts_with("corelink_"));
        }
    }
}

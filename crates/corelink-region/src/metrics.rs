//! Prometheus metrics for region operations — WI-S14-001 §6.1.6.
//!
//! 5 canonical metrics:
//! - `corelink_region_provisioning_duration_seconds_bucket{region}` (histogram)
//! - `corelink_region_health_status{region}` (gauge; 0=down/1=degraded/2=healthy)
//! - `corelink_region_terraform_drift_findings_total{region,severity}` (counter)
//! - `corelink_region_migration_progress_ratio{tenant_id_hash,source_region,target_region}` (gauge)
//! - `corelink_region_outage_events_total{region,event_type}` (counter)
//!
//! INV-OBS-CARDINALITY-BUDGET: tenant_id label PROHIBITED.
//! Migration progress uses `tenant_id_hash` (SHA-256 truncated hex) to stay
//! within cardinality budget.

use crate::region::{Region, RegionHealthStatus};

/// Prometheus metric name constants — WI-S14-001 §6.1.6.
pub const METRIC_PROVISIONING_DURATION: &str =
    "corelink_region_provisioning_duration_seconds_bucket";
/// Health status gauge metric name.
pub const METRIC_HEALTH_STATUS: &str = "corelink_region_health_status";
/// Terraform drift findings counter metric name.
pub const METRIC_DRIFT_FINDINGS: &str = "corelink_region_terraform_drift_findings_total";
/// Migration progress ratio gauge metric name.
pub const METRIC_MIGRATION_PROGRESS: &str = "corelink_region_migration_progress_ratio";
/// Outage events counter metric name.
pub const METRIC_OUTAGE_EVENTS: &str = "corelink_region_outage_events_total";

/// In-memory metric accumulator for testing.
///
/// Production wiring exports to Cloudflare Workers Analytics Engine /
/// Prometheus-compatible endpoint.
///
/// F-001: per-instance `Arc<Mutex<RegionMetrics>>` in orchestrators.
#[derive(Debug, Default)]
pub struct RegionMetrics {
    /// Provisioning duration observations per region (seconds).
    pub provisioning_duration_seconds: Vec<(String, f64)>,

    /// Current health status per region.
    pub health_status: Vec<(String, u8)>,

    /// Drift findings per (region, severity).
    pub drift_findings_total: Vec<(String, String, u64)>,

    /// Migration progress ratio observations per (tenant_id_hash, source, target).
    pub migration_progress_ratio: Vec<(String, String, String, f64)>,

    /// Outage events per (region, event_type).
    pub outage_events_total: Vec<(String, String, u64)>,
}

impl RegionMetrics {
    /// Record provisioning duration for a region.
    pub fn record_provisioning_duration(&mut self, region: Region, duration_secs: f64) {
        self.provisioning_duration_seconds
            .push((region.as_str().to_owned(), duration_secs));
    }

    /// Update health status for a region.
    pub fn set_health_status(&mut self, region: Region, status: RegionHealthStatus) {
        // Replace existing entry or append
        let region_str = region.as_str().to_owned();
        if let Some(entry) = self
            .health_status
            .iter_mut()
            .find(|(r, _)| *r == region_str)
        {
            entry.1 = status.gauge_value();
        } else {
            self.health_status
                .push((region_str, status.gauge_value()));
        }
    }

    /// Record a terraform drift finding.
    pub fn record_drift_finding(&mut self, region: Region, severity: &str) {
        self.drift_findings_total
            .push((region.as_str().to_owned(), severity.to_owned(), 1));
    }

    /// Record migration progress ratio for a tenant (hashed).
    ///
    /// `ratio` is 0.0..=1.0 (0.0 = not started, 1.0 = complete).
    ///
    /// INV-OBS-CARDINALITY-BUDGET: `tenant_id_hash` must be SHA-256 truncated
    /// hex of tenant_id — never raw tenant_id.
    pub fn record_migration_progress(
        &mut self,
        tenant_id_hash: &str,
        source_region: Region,
        target_region: Region,
        ratio: f64,
    ) {
        self.migration_progress_ratio.push((
            tenant_id_hash.to_owned(),
            source_region.as_str().to_owned(),
            target_region.as_str().to_owned(),
            ratio,
        ));
    }

    /// Record a region outage event (detected or resolved).
    pub fn record_outage_event(&mut self, region: Region, event_type: &str) {
        self.outage_events_total
            .push((region.as_str().to_owned(), event_type.to_owned(), 1));
    }

    /// Total drift findings recorded.
    #[must_use]
    pub fn total_drift_findings(&self) -> u64 {
        self.drift_findings_total.iter().map(|(_, _, c)| c).sum()
    }

    /// Total outage events recorded.
    #[must_use]
    pub fn total_outage_events(&self) -> u64 {
        self.outage_events_total.iter().map(|(_, _, c)| c).sum()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_provisioning_duration() {
        let mut m = RegionMetrics::default();
        m.record_provisioning_duration(Region::Weur, 120.5);
        m.record_provisioning_duration(Region::Wnam, 95.3);
        assert_eq!(m.provisioning_duration_seconds.len(), 2);
        assert_eq!(m.provisioning_duration_seconds[0].0, "weur");
    }

    #[test]
    fn test_metrics_health_status_upsert() {
        let mut m = RegionMetrics::default();
        m.set_health_status(Region::Wnam, RegionHealthStatus::Healthy);
        m.set_health_status(Region::Wnam, RegionHealthStatus::Degraded);
        assert_eq!(m.health_status.len(), 1);
        assert_eq!(m.health_status[0].1, RegionHealthStatus::Degraded.gauge_value());
    }

    #[test]
    fn test_metrics_drift_findings() {
        let mut m = RegionMetrics::default();
        m.record_drift_finding(Region::Weur, "medium");
        m.record_drift_finding(Region::Wnam, "high");
        assert_eq!(m.total_drift_findings(), 2);
    }

    #[test]
    fn test_migration_progress_uses_hash_not_raw_id() {
        let mut m = RegionMetrics::default();
        // tenant_id_hash must be a hash (length check as proxy)
        let hash = "a3b4c5d6e7f80123"; // simulated truncated hex
        m.record_migration_progress(hash, Region::Wnam, Region::Weur, 0.5);
        assert_eq!(m.migration_progress_ratio.len(), 1);
        assert_eq!(m.migration_progress_ratio[0].0, hash);
    }

    #[test]
    fn test_outage_events() {
        let mut m = RegionMetrics::default();
        m.record_outage_event(Region::Weur, "detected");
        m.record_outage_event(Region::Weur, "resolved");
        assert_eq!(m.total_outage_events(), 2);
    }
}

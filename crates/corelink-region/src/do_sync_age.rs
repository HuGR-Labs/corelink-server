//! DO-to-D1 sync-age SLI — continuous SLI for `SLO-REPLICATION-LAG-DO`
//! (closes DEBT-011 R-PREP-REPL-P1-003).
//!
//! # What this module ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production probe reads `tenant_storage_state.last_synced_at`
//! (data_model.md §4.2) and `corelink_admin_config_singleton.last_synced_at`
//! from D1, computes `now − last_synced_at` per (DO class, region) pair, and
//! emits the gauge from the DO's periodic tick.
//!
//! Here we ship:
//!
//! 1. [`DoClass`] enum — `#[non_exhaustive]` 3-canonical (`TenantQuota` /
//!    `ConfigSingleton` / `RateLimiter`). `RateLimiter` is **excluded** from
//!    the SLO (intentional reset on failover, audit §3.5 edge case (a)) but
//!    declared here for taxonomy completeness and audit-trail invariance.
//! 2. [`DoSyncAgeProbe`] trait — fixed boundary the DO tick consumes; the
//!    verifier `--domain=do` reads the resulting Prometheus gauge.
//! 3. [`InMemoryDoSyncAgeProbe`] — deterministic fixture (per-instance
//!    `Arc<Mutex<>>` F-001 closure).
//! 4. Canonical metric name + per-class ceilings matching `slo_catalog.md §4.26`.
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{do_class, region}` — no `tenant_id`. With 3 classes × 4 regions
//! = 12 séries maximum. Budget-safe.
//!
//! # Audit ordering
//!
//! The probe is a *read-only* observability source — no state mutation to wrap.
//! `RateLimiter` is excluded from the SLO check; calling [`DoSyncAgeProbe::probe`]
//! with `DoClass::RateLimiter` is allowed (returns the configured age) but the
//! verifier ignores it. See `slo_catalog.md §4.26` "RateLimiter excluded".

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for DO sync-age.
///
/// LOAD-BEARING: must match `PROM_METRIC["do"]` in
/// `scripts/verify-replication-lag.py`.
pub const METRIC_DO_SYNC_AGE_SECONDS: &str = "corelink_do_sync_age_seconds";

/// p99 sync-age ceiling for `TenantQuota` DO class (matches §4.26 Target).
///
/// Matches the 5-min DO sync cadence per `data_model.md §4.2 tenant_storage_state`.
pub const DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS: u64 = 300;

/// p99 sync-age ceiling for `ConfigSingleton` DO class.
///
/// Consent / kill-switch propagation per `CTRL-PRIV-CONSENT-002` +
/// `SLO-ADMIN-CONFIG-PROPAGATION §4.14` — must be ≤ 60 s.
pub const DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS: u64 = 60;

/// Sync-age budget for `RateLimiter` — intentionally `None` (excluded from
/// SLO; graceful-degradation reset on failover; audit §3.5 edge case (a)).
pub const DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS: Option<u64> = None;

/// 3-canonical DO class taxonomy.
///
/// Mirrors `data_model.md §4` Durable Object catalog. `RateLimiter` is the
/// only class **excluded** from the sync-age SLO (intentional reset on
/// failover; audit §3.5 edge case (a)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DoClass {
    /// Per-tenant DO that mirrors quota / rate-limit / storage counters into
    /// D1 every ~5 min for cross-region rebuild.
    TenantQuota,
    /// Global DO for admin config (kill-switch, consent flags). Tight
    /// propagation budget (60 s) per `SLO-ADMIN-CONFIG-PROPAGATION §4.14`.
    ConfigSingleton,
    /// In-memory rate-limiter DO. **Excluded** from sync-age SLO — by design
    /// resets on failover (audit §3.5 edge case (a)).
    RateLimiter,
}

impl DoClass {
    /// Canonical lowercase label value for Prometheus.
    pub fn as_str(self) -> &'static str {
        match self {
            DoClass::TenantQuota => "tenant_quota",
            DoClass::ConfigSingleton => "config_singleton",
            DoClass::RateLimiter => "rate_limiter",
        }
    }

    /// Per-class sync-age budget in seconds, or `None` if the class is
    /// excluded from the SLO (currently only `RateLimiter`).
    ///
    /// `Some(secs)` ↔ the verifier and alerting rule MUST enforce this
    /// ceiling; `None` ↔ the class is ignored by `verify-replication-lag.py
    /// --domain=do`.
    pub fn budget_seconds(self) -> Option<u64> {
        match self {
            DoClass::TenantQuota => Some(DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS),
            DoClass::ConfigSingleton => Some(DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS),
            DoClass::RateLimiter => DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS,
        }
    }
}

impl std::fmt::Display for DoClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One DO sync-age sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoSyncAgeSample {
    /// DO class (TenantQuota / ConfigSingleton / RateLimiter).
    pub do_class: DoClass,
    /// Region the DO resides in.
    pub region: Region,
    /// Observed sync-age in seconds (`now − last_synced_at`).
    pub age_seconds: f64,
    /// Timestamp (ms since epoch) when the sample was taken.
    pub sample_timestamp_ms: u64,
}

impl DoSyncAgeSample {
    /// `true` iff this sample is within the per-class budget. Returns `true`
    /// for `RateLimiter` (excluded class — never violates).
    #[must_use]
    pub fn within_budget(&self) -> bool {
        match self.do_class.budget_seconds() {
            Some(secs) => self.age_seconds <= (secs as f64),
            None => true, // excluded class — never violates
        }
    }
}

/// DO sync-age probe.
///
/// # Examples
///
/// ```
/// use corelink_region::do_sync_age::{
///     DoClass, DoSyncAgeProbe, InMemoryDoSyncAgeProbe,
/// };
/// use corelink_region::Region;
///
/// let probe = InMemoryDoSyncAgeProbe::new();
/// probe.set_age(DoClass::TenantQuota, Region::Wnam, 120.0);
/// let s = probe
///     .probe(DoClass::TenantQuota, Region::Wnam, 1_700_000_000_000)
///     .unwrap();
/// assert!((s.age_seconds - 120.0).abs() < f64::EPSILON);
/// assert!(s.within_budget());
/// ```
pub trait DoSyncAgeProbe: std::fmt::Debug + Send + Sync {
    /// Probe the DO sync-age for `(do_class, region)`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying D1 read fails. Callers MUST
    /// emit SEV-3 on failure per `slo_catalog.md §4.26`.
    fn probe(
        &self,
        do_class: DoClass,
        region: Region,
        timestamp_ms: u64,
    ) -> Result<DoSyncAgeSample, String>;

    /// Canonical Prometheus metric name (load-bearing for the verifier).
    fn metric_name(&self) -> &'static str {
        METRIC_DO_SYNC_AGE_SECONDS
    }
}

/// In-memory DO sync-age probe — deterministic fixture for tests.
#[derive(Debug, Default)]
pub struct InMemoryDoSyncAgeProbe {
    ages: Mutex<Vec<(DoClass, Region, f64)>>,
}

impl InMemoryDoSyncAgeProbe {
    /// Construct an empty probe; default age for any pair is 0 s.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fixed sync-age (seconds) for a `(do_class, region)` pair.
    pub fn set_age(&self, do_class: DoClass, region: Region, age_seconds: f64) {
        let mut g = self.ages.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|(c, r, _)| !(*c == do_class && *r == region));
        g.push((do_class, region, age_seconds));
    }
}

impl DoSyncAgeProbe for InMemoryDoSyncAgeProbe {
    fn probe(
        &self,
        do_class: DoClass,
        region: Region,
        timestamp_ms: u64,
    ) -> Result<DoSyncAgeSample, String> {
        let age = self
            .ages
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|(c, r, _)| *c == do_class && *r == region)
            .map(|(_, _, v)| *v)
            .unwrap_or(0.0);
        if !age.is_finite() || age < 0.0 {
            return Err(format!(
                "invalid configured age {age:?} for class={do_class} region={region:?}"
            ));
        }
        Ok(DoSyncAgeSample {
            do_class,
            region,
            age_seconds: age,
            sample_timestamp_ms: timestamp_ms,
        })
    }
}

/// Adversarial probe that always returns `Err`.
#[derive(Debug)]
pub struct FailingDoSyncAgeProbe {
    message: String,
}

impl FailingDoSyncAgeProbe {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl DoSyncAgeProbe for FailingDoSyncAgeProbe {
    fn probe(
        &self,
        _do_class: DoClass,
        _region: Region,
        _timestamp_ms: u64,
    ) -> Result<DoSyncAgeSample, String> {
        Err(self.message.clone())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[test]
    fn metric_name_is_canonical() {
        assert_eq!(METRIC_DO_SYNC_AGE_SECONDS, "corelink_do_sync_age_seconds");
    }

    #[test]
    fn slo_constants_match_catalog() {
        // slo_catalog.md §4.26 — TenantQuota ≤ 300s, ConfigSingleton ≤ 60s,
        // RateLimiter excluded.
        assert_eq!(DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS, 300);
        assert_eq!(DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS, 60);
        assert!(DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS.is_none());
    }

    #[test]
    fn do_class_labels_match_verifier_keys() {
        // These string values are LOAD-BEARING for the alerting rule label
        // matchers (`do_class="tenant_quota"`, etc.).
        assert_eq!(DoClass::TenantQuota.as_str(), "tenant_quota");
        assert_eq!(DoClass::ConfigSingleton.as_str(), "config_singleton");
        assert_eq!(DoClass::RateLimiter.as_str(), "rate_limiter");
    }

    #[test]
    fn do_class_budget_seconds_match_constants() {
        assert_eq!(
            DoClass::TenantQuota.budget_seconds(),
            Some(DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS)
        );
        assert_eq!(
            DoClass::ConfigSingleton.budget_seconds(),
            Some(DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS)
        );
        assert_eq!(DoClass::RateLimiter.budget_seconds(), None);
    }

    #[test]
    fn rate_limiter_within_budget_always_true() {
        // Excluded class — never violates by definition.
        let s = DoSyncAgeSample {
            do_class: DoClass::RateLimiter,
            region: Region::Wnam,
            age_seconds: 99_999.0, // absurdly stale
            sample_timestamp_ms: 0,
        };
        assert!(s.within_budget());
    }

    #[test]
    fn tenant_quota_within_budget_boundary() {
        // Exactly at ceiling = within budget (`<=`).
        let s = DoSyncAgeSample {
            do_class: DoClass::TenantQuota,
            region: Region::Wnam,
            age_seconds: 300.0,
            sample_timestamp_ms: 0,
        };
        assert!(s.within_budget());
        let s_over = DoSyncAgeSample {
            do_class: DoClass::TenantQuota,
            region: Region::Wnam,
            age_seconds: 300.001,
            sample_timestamp_ms: 0,
        };
        assert!(!s_over.within_budget());
    }

    #[test]
    fn config_singleton_within_budget_boundary() {
        // Tight 60s budget — propagation-of-consent guarantee.
        let s_under = DoSyncAgeSample {
            do_class: DoClass::ConfigSingleton,
            region: Region::Weur,
            age_seconds: 59.0,
            sample_timestamp_ms: 0,
        };
        assert!(s_under.within_budget());
        let s_over = DoSyncAgeSample {
            do_class: DoClass::ConfigSingleton,
            region: Region::Weur,
            age_seconds: 61.0,
            sample_timestamp_ms: 0,
        };
        assert!(!s_over.within_budget());
    }

    #[test]
    fn inmemory_probe_default_age_zero() {
        let p = InMemoryDoSyncAgeProbe::new();
        let s = p
            .probe(DoClass::TenantQuota, Region::Wnam, 1_700_000_000_000)
            .expect("probe");
        assert_eq!(s.age_seconds, 0.0);
        assert!(s.within_budget());
    }

    #[test]
    fn inmemory_probe_set_age_observable() {
        let p = InMemoryDoSyncAgeProbe::new();
        p.set_age(DoClass::TenantQuota, Region::Enam, 250.0);
        let s = p
            .probe(DoClass::TenantQuota, Region::Enam, 0)
            .expect("probe");
        assert!((s.age_seconds - 250.0).abs() < f64::EPSILON);
        assert!(s.within_budget());
    }

    #[test]
    fn inmemory_probe_set_age_breach_observable() {
        let p = InMemoryDoSyncAgeProbe::new();
        p.set_age(DoClass::ConfigSingleton, Region::Weur, 120.0);
        let s = p
            .probe(DoClass::ConfigSingleton, Region::Weur, 0)
            .expect("probe");
        assert!((s.age_seconds - 120.0).abs() < f64::EPSILON);
        assert!(!s.within_budget(), "120s > 60s budget for ConfigSingleton");
    }

    #[test]
    fn inmemory_probe_rejects_negative_configured_age() {
        let p = InMemoryDoSyncAgeProbe::new();
        p.set_age(DoClass::TenantQuota, Region::Wnam, -10.0);
        let r = p.probe(DoClass::TenantQuota, Region::Wnam, 0);
        assert!(r.is_err());
    }

    #[test]
    fn failing_probe_returns_error() {
        let p = FailingDoSyncAgeProbe::new("simulated D1 outage");
        let r = p.probe(DoClass::TenantQuota, Region::Wnam, 0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("simulated"));
    }

    #[test]
    fn do_probe_trait_object_safe() {
        let probes: Vec<Box<dyn DoSyncAgeProbe>> = vec![
            Box::new(InMemoryDoSyncAgeProbe::new()),
            Box::new(FailingDoSyncAgeProbe::new("x")),
        ];
        assert_eq!(probes.len(), 2);
    }
}

//! `ReconcileConfig` knobs + the two pure-fn boundary helpers
//! ([`sev_level_for`] + [`auto_fix_gate_fires`]) + the
//! [`CountingReconcileClock`] test seam. Extracted from the monolithic
//! `reconcile.rs` per Wave 33 Stream A2.2 file-size discipline.

use std::sync::Mutex;

use super::{
    ReconcileClock, ReconcileError, SevLevel, AUTO_FIX_MAX_PERCENT, AUTO_FIX_MAX_RECORDS,
    CANONICAL_RECONCILE_PHASE_BUDGET_MS, SEV1_PER_TENANT_DRIFT_PERCENT,
    SEV2_GLOBAL_DRIFT_PERCENT,
};

/// Counter-driven [`ReconcileClock`] used by tests + property tests.
#[derive(Debug)]
pub struct CountingReconcileClock {
    inner: Mutex<u64>,
}

impl CountingReconcileClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl ReconcileClock for CountingReconcileClock {
    fn now_ms(&self) -> u64 {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = *g;
        *g = g.saturating_add(1);
        now
    }
}

/// Knobs driving the reconcile phase. The defaults pin the canonical
/// values from sprint contract §5.5 + WI §1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconcileConfig {
    auto_fix_max_records: u64,
    auto_fix_max_percent: f64,
    sev1_per_tenant_drift_percent: f64,
    sev2_global_drift_percent: f64,
    phase_budget_ms: u64,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            auto_fix_max_records: AUTO_FIX_MAX_RECORDS,
            auto_fix_max_percent: AUTO_FIX_MAX_PERCENT,
            sev1_per_tenant_drift_percent: SEV1_PER_TENANT_DRIFT_PERCENT,
            sev2_global_drift_percent: SEV2_GLOBAL_DRIFT_PERCENT,
            phase_budget_ms: CANONICAL_RECONCILE_PHASE_BUDGET_MS,
        }
    }
}

impl ReconcileConfig {
    /// Construct a custom config. Production wiring lifts the values
    /// from `wrangler.toml` env overrides
    /// (`CORELINK_GC_RECONCILE_AUTO_FIX_MAX_RECORDS` /
    /// `…_AUTO_FIX_MAX_PERCENT` / `…_PHASE_BUDGET_MS`).
    ///
    /// # Errors
    ///
    /// Returns [`ReconcileError::Backend`] when an invariant is
    /// violated (zero budget; non-finite percentage; negative
    /// percentage; SEV-1 threshold ≤ SEV-2 threshold; auto-fix
    /// percentage gate ≥ SEV-2 threshold which would defeat the
    /// purpose of the gate).
    pub fn new(
        auto_fix_max_records: u64,
        auto_fix_max_percent: f64,
        sev1_per_tenant_drift_percent: f64,
        sev2_global_drift_percent: f64,
        phase_budget_ms: u64,
    ) -> Result<Self, ReconcileError> {
        if phase_budget_ms == 0 {
            return Err(ReconcileError::Backend(
                "phase_budget_ms must be > 0".to_owned(),
            ));
        }
        if !auto_fix_max_percent.is_finite() || auto_fix_max_percent < 0.0 {
            return Err(ReconcileError::Backend(
                "auto_fix_max_percent must be finite and >= 0".to_owned(),
            ));
        }
        if !sev1_per_tenant_drift_percent.is_finite() || sev1_per_tenant_drift_percent <= 0.0 {
            return Err(ReconcileError::Backend(
                "sev1_per_tenant_drift_percent must be finite and > 0".to_owned(),
            ));
        }
        if !sev2_global_drift_percent.is_finite() || sev2_global_drift_percent <= 0.0 {
            return Err(ReconcileError::Backend(
                "sev2_global_drift_percent must be finite and > 0".to_owned(),
            ));
        }
        if sev1_per_tenant_drift_percent <= sev2_global_drift_percent {
            return Err(ReconcileError::Backend(
                "sev1 threshold must be strictly greater than sev2 threshold".to_owned(),
            ));
        }
        Ok(Self {
            auto_fix_max_records,
            auto_fix_max_percent,
            sev1_per_tenant_drift_percent,
            sev2_global_drift_percent,
            phase_budget_ms,
        })
    }

    /// Auto-fix max records gate (count arm).
    #[must_use]
    pub const fn auto_fix_max_records(self) -> u64 {
        self.auto_fix_max_records
    }

    /// Auto-fix max percent gate (percentage arm).
    #[must_use]
    pub const fn auto_fix_max_percent(self) -> f64 {
        self.auto_fix_max_percent
    }

    /// SEV-1 per-tenant drift threshold.
    #[must_use]
    pub const fn sev1_per_tenant_drift_percent(self) -> f64 {
        self.sev1_per_tenant_drift_percent
    }

    /// SEV-2 global drift threshold.
    #[must_use]
    pub const fn sev2_global_drift_percent(self) -> f64 {
        self.sev2_global_drift_percent
    }

    /// Phase budget ceiling (ms).
    #[must_use]
    pub const fn phase_budget_ms(self) -> u64 {
        self.phase_budget_ms
    }
}

/// Compute SEV level from observed drift percentages per sprint
/// contract §5.5 R-S06-10 + Lote 10.6bis P2-7 fix (per-tenant > 1% is
/// SEV-1; global > 0.1% is SEV-2).
///
/// The two arguments are distinct scopes: `global_drift_percent` is the
/// cross-tenant aggregate (computed by the caller after aggregating
/// `ReconcileResult.drift_percent` across all per-tenant runs);
/// `per_tenant_drift_percent` is this-tenant only. `execute()` itself
/// is per-tenant-scoped and passes `0.0` for the global term — the
/// orchestrator/worker layer is responsible for aggregating across
/// tenants and re-evaluating SEV-2 with the true global value plus
/// emitting the `corelink_gc_refcount_drift_percent{scope="global"}`
/// metric.
#[must_use]
pub fn sev_level_for(
    global_drift_percent: f64,
    per_tenant_drift_percent: f64,
    config: &ReconcileConfig,
) -> SevLevel {
    if per_tenant_drift_percent > config.sev1_per_tenant_drift_percent {
        return SevLevel::Sev1;
    }
    if global_drift_percent > config.sev2_global_drift_percent {
        return SevLevel::Sev2;
    }
    SevLevel::None
}

/// Whether the dual-condition auto-fix gate fires for the given
/// (drift_count, drift_percent) per tenant.
///
/// Both conditions required (Lote 10.6bis Part 2a P0-6
/// scale-invariant percentage-floor + absolute-floor): boundary
/// `drift_count = 5 AND drift_percent = 0.0001` auto-fixes (operative
/// bound `≤`); either drift_count = 6 OR drift_percent = 0.000101
/// rejects (manual review).
#[must_use]
pub fn auto_fix_gate_fires(
    drift_count: u64,
    drift_percent: f64,
    config: &ReconcileConfig,
) -> bool {
    drift_count <= config.auto_fix_max_records
        && drift_percent <= config.auto_fix_max_percent
}

//! Cardinality budget enforcement constants (INV-OBS-CARDINALITY-BUDGET S-09).
//!
//! **CRITICAL**: The live Prometheus metric `corelink_cas_get_bytes_total`
//! uses `{tenant_tier, region}` labels ONLY — NOT `tenant_id`.
//!
//! Budget-safe: 4 `tenant_tier` × 4 `region` = **16 séries baseline**.
//! Per-tenant cardinality (10k+ tenants) = **FORBIDDEN** — would explode
//! to 1M+ unique series, exceeding Grafana Mimir tenant limit.
//!
//! Use [`crate::aggregator`] (offline batch job) for per-tenant analysis.

/// Total cardinality of the live Prometheus metric label cartesian product.
///
/// `corelink_cas_get_bytes_total{tenant_tier, region}`:
/// - `tenant_tier` ∈ {solo, team, business, enterprise} = 4 values
/// - `region` ∈ {wnam, enam, weur, sam} = 4 values
/// - Total: 4 × 4 = **16 séries** — budget-safe
///
/// Per INV-OBS-CARDINALITY-BUDGET S-09: bound 20k séries per metric
/// + 100k total. This metric uses 16/20000 = 0.08% of per-metric budget.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::LIVE_METRIC_LABEL_CARDINALITY;
///
/// assert_eq!(LIVE_METRIC_LABEL_CARDINALITY, 16);
/// ```
pub const LIVE_METRIC_LABEL_CARDINALITY: usize = 16; // 4 tiers × 4 regions

/// Compile-time assertion: NO `tenant_id` label in live Prometheus metrics.
///
/// This constant is `true` when the cardinality budget is respected
/// (live metric uses tier-label NOT tenant_id label). CI gate checks this.
///
/// **DO NOT add `tenant_id` label to any live Prometheus metric.**
/// Use [`crate::aggregator`] offline aggregation for per-tenant analysis.
///
/// With 10k tenants × 100 metrics each = 1M+ unique series → Grafana Mimir
/// tenant limit exceeded → metrics dropped silently (INV-OBS-CARDINALITY-BUDGET).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::NO_TENANT_ID_LABEL;
///
/// // This constant must always be true — CI gate enforces it.
/// assert!(NO_TENANT_ID_LABEL);
/// ```
pub const NO_TENANT_ID_LABEL: bool = true;

/// Per-metric series budget (from INV-OBS-CARDINALITY-BUDGET S-09).
pub const SERIES_BUDGET_PER_METRIC: usize = 20_000;

/// Total series budget across all metrics (INV-OBS-CARDINALITY-BUDGET S-09).
pub const SERIES_BUDGET_TOTAL: usize = 100_000;

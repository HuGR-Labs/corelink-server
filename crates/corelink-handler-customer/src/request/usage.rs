// ─── Usage ───────────────────────────────────────────────────────────────────

/// Usage request — `GET /v1/customer/usage?period=` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UsageRequest {
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Optional billing period filter (e.g. `"2026-05"`). `None`
    /// means the current period.
    pub period: Option<String>,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl UsageRequest {
    /// Construct a [`UsageRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        period: Option<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            period,
            at_unix_ms,
        }
    }

    /// Target a tenant explicitly while retaining the authenticated caller.
    #[must_use]
    pub fn for_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.requested_tenant = Some(tenant.into());
        self
    }

    /// Alias for [`Self::for_tenant`].
    #[must_use]
    pub fn with_requested_tenant(self, tenant: impl Into<String>) -> Self {
        self.for_tenant(tenant)
    }
}

/// One daily bucket in the usage time-series.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DailyUsageBucket {
    /// Calendar day in `"YYYY-MM-DD"` format.
    pub day: String,
    /// CAS reads on this day.
    pub reads: u64,
    /// CAS writes on this day.
    pub writes: u64,
    /// CAS bytes consumed on this day.
    pub cas_bytes: u64,
}

impl DailyUsageBucket {
    /// Construct a [`DailyUsageBucket`] from its fields.
    #[must_use]
    pub fn new(day: impl Into<String>, reads: u64, writes: u64, cas_bytes: u64) -> Self {
        Self {
            day: day.into(),
            reads,
            writes,
            cas_bytes,
        }
    }
}

/// Usage response — period-level + daily time-series.
///
/// Not `Eq` because [`Self::hit_rate`] is a floating-point fraction.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UsageResponse {
    /// Billing period (e.g. `"2026-05"`).
    pub period: String,
    /// CAS bytes consumed this period.
    pub cas_bytes: u64,
    /// Total CAS reads this period.
    pub reads: u64,
    /// Total CAS writes this period.
    pub writes: u64,
    /// Total billable requests this period, from `monthly_request_counts`
    /// (migration 0071) — the running counter the quota gate already increments
    /// per request. Compared against the tier's `requestsPerMonthMax` on the
    /// client for a usage-vs-quota gauge. `0` when no counter row exists yet.
    pub request_count: u64,
    /// Quota ceiling in bytes.
    pub quota_bytes: u64,
    /// Cache hit-rate for the period as a fraction `0.0..=1.0`
    /// (`hits / (hits + misses)`), sourced from `usage_daily` (migration 0089).
    /// `None` when there were no cache reads at all (`hits + misses == 0`) — an
    /// honest "no data" rather than a fabricated rate. Serializes as JSON `null`.
    pub hit_rate: Option<f64>,
    /// Estimated build-time saved this period, in seconds: `hits *
    /// SECONDS_SAVED_PER_HIT` — a cache hit avoids re-executing ~one build
    /// action. Displayed as an estimate.
    pub time_saved_seconds: u64,
    /// Estimated compute-cost saved this period, in USD cents:
    /// `round(time_saved_seconds * USD_PER_COMPUTE_SECOND * 100)`. Conservative;
    /// displayed as an estimate.
    pub dollars_saved_cents: u64,
    /// Daily breakdown.
    pub daily: Vec<DailyUsageBucket>,
}

impl UsageResponse {
    /// Construct a [`UsageResponse`] from its fields.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "flat DTO constructor")]
    pub fn new(
        period: impl Into<String>,
        cas_bytes: u64,
        reads: u64,
        writes: u64,
        quota_bytes: u64,
        daily: Vec<DailyUsageBucket>,
        request_count: u64,
        hit_rate: Option<f64>,
        time_saved_seconds: u64,
        dollars_saved_cents: u64,
    ) -> Self {
        Self {
            period: period.into(),
            cas_bytes,
            reads,
            writes,
            request_count,
            quota_bytes,
            hit_rate,
            time_saved_seconds,
            dollars_saved_cents,
            daily,
        }
    }
}

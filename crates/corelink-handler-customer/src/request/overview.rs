// ─── Overview ────────────────────────────────────────────────────────────────

/// Overview request — `GET /v1/customer/overview` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OverviewRequest {
    /// Tenant named by the request path or route context.
    ///
    /// `None` preserves the historical self-serve shape and resolves to
    /// [`Self::caller_tenant`]. A distinct value is used by trusted adapters
    /// to exercise the handler's cross-tenant boundary.
    pub requested_tenant: Option<String>,
    /// Caller's authenticated tenant (inferred from session upstream).
    pub caller_tenant: String,
    /// Caller principal (already-authenticated upstream).
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl OverviewRequest {
    /// Construct an [`OverviewRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }

    /// Target a tenant explicitly while retaining the authenticated caller.
    #[must_use]
    pub fn for_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.requested_tenant = Some(tenant.into());
        self
    }

    /// Alias for [`Self::for_tenant`] used by route-adapter tests.
    #[must_use]
    pub fn with_requested_tenant(self, tenant: impl Into<String>) -> Self {
        self.for_tenant(tenant)
    }
}

/// BYOK status embedded in the overview and keys-list responses.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ByokStatus {
    /// One of `"none"` / `"active"` / `"rotation_pending"`.
    pub status: String,
    /// CMK identifier when `status != "none"`.
    pub cmk_id: Option<String>,
    /// ISO-8601 timestamp of the last rotation, when present.
    pub last_rotated_at: Option<String>,
}

impl ByokStatus {
    /// Construct a [`ByokStatus`] from its fields.
    #[must_use]
    pub fn new(
        status: impl Into<String>,
        cmk_id: Option<String>,
        last_rotated_at: Option<String>,
    ) -> Self {
        Self {
            status: status.into(),
            cmk_id,
            last_rotated_at,
        }
    }
}

/// Inline usage snapshot embedded in the overview response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OverviewUsage {
    /// Billing period (e.g. `"2026-05"`).
    pub period: String,
    /// CAS bytes consumed this period.
    pub cas_bytes: u64,
    /// Quota ceiling in bytes.
    pub quota_bytes: u64,
    /// Total CAS reads this period.
    pub reads: u64,
    /// Total CAS writes this period.
    pub writes: u64,
}

impl OverviewUsage {
    /// Construct an [`OverviewUsage`] from its fields.
    #[must_use]
    pub fn new(
        period: impl Into<String>,
        cas_bytes: u64,
        quota_bytes: u64,
        reads: u64,
        writes: u64,
    ) -> Self {
        Self {
            period: period.into(),
            cas_bytes,
            quota_bytes,
            reads,
            writes,
        }
    }
}

/// Inline billing snapshot embedded in the overview response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OverviewBilling {
    /// One of `"trialing"` / `"active"` / `"past_due"` / `"canceled"`.
    pub status: String,
    /// ISO-8601 timestamp of the next invoice.
    pub next_invoice_at: String,
    /// Amount due in smallest currency unit (e.g. cents).
    pub amount_due_cents: i64,
    /// One of `"usd"` / `"eur"` / `"brl"`.
    pub currency: String,
}

impl OverviewBilling {
    /// Construct an [`OverviewBilling`] from its fields.
    #[must_use]
    pub fn new(
        status: impl Into<String>,
        next_invoice_at: impl Into<String>,
        amount_due_cents: i64,
        currency: impl Into<String>,
    ) -> Self {
        Self {
            status: status.into(),
            next_invoice_at: next_invoice_at.into(),
            amount_due_cents,
            currency: currency.into(),
        }
    }
}

/// A single customer audit event embedded in overview / audit-query
/// responses.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CustomerAuditEventRow {
    /// Opaque event identifier.
    pub event_id: String,
    /// ISO-8601 timestamp.
    pub ts: String,
    /// Dotted event type (e.g. `"corelink.customer.keys.create.committed"`).
    pub event_type: String,
    /// One of `"info"` / `"warn"` / `"critical"`.
    pub severity: String,
    /// Principal that performed the action.
    pub actor: String,
    /// Human-readable summary.
    pub summary: String,
}

impl CustomerAuditEventRow {
    /// Construct a [`CustomerAuditEventRow`] from its fields.
    #[must_use]
    pub fn new(
        event_id: impl Into<String>,
        ts: impl Into<String>,
        event_type: impl Into<String>,
        severity: impl Into<String>,
        actor: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            ts: ts.into(),
            event_type: event_type.into(),
            severity: severity.into(),
            actor: actor.into(),
            summary: summary.into(),
        }
    }
}

/// Overview response — full customer dashboard snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OverviewResponse {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Tenant display name.
    pub tenant_name: String,
    /// One of `"free"` / `"starter"` / `"team"` / `"enterprise"`.
    pub plan: String,
    /// Current-period usage snapshot.
    pub usage: OverviewUsage,
    /// Billing snapshot.
    pub billing: OverviewBilling,
    /// BYOK status.
    pub byok: ByokStatus,
    /// Recent activity rows (last N audit events).
    pub recent_activity: Vec<CustomerAuditEventRow>,
}

impl OverviewResponse {
    /// Construct an [`OverviewResponse`] from its fields.
    #[must_use]
    pub fn new(
        tenant_id: impl Into<String>,
        tenant_name: impl Into<String>,
        plan: impl Into<String>,
        usage: OverviewUsage,
        billing: OverviewBilling,
        byok: ByokStatus,
        recent_activity: Vec<CustomerAuditEventRow>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            tenant_name: tenant_name.into(),
            plan: plan.into(),
            usage,
            billing,
            byok,
            recent_activity,
        }
    }
}

//! Request / response envelopes for the customer handler surface.
//!
//! All shapes are `#[non_exhaustive]` + expose a `pub fn new()`
//! constructor so out-of-crate callers can construct them without
//! struct-literal syntax. Field names mirror the `customer-types.ts`
//! TypeScript types exactly (snake_case).

// ─── Overview ────────────────────────────────────────────────────────────────

/// Overview request — `GET /v1/customer/overview` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OverviewRequest {
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
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
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

// ─── Usage ───────────────────────────────────────────────────────────────────

/// Usage request — `GET /v1/customer/usage?period=` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UsageRequest {
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
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            period,
            at_unix_ms,
        }
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Daily breakdown.
    pub daily: Vec<DailyUsageBucket>,
}

impl UsageResponse {
    /// Construct a [`UsageResponse`] from its fields.
    #[must_use]
    pub fn new(
        period: impl Into<String>,
        cas_bytes: u64,
        reads: u64,
        writes: u64,
        quota_bytes: u64,
        daily: Vec<DailyUsageBucket>,
        request_count: u64,
    ) -> Self {
        Self {
            period: period.into(),
            cas_bytes,
            reads,
            writes,
            request_count,
            quota_bytes,
            daily,
        }
    }
}

// ─── Billing ─────────────────────────────────────────────────────────────────

/// Billing request — `GET /v1/customer/billing` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BillingRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl BillingRequest {
    /// Construct a [`BillingRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// One invoice row in the billing response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InvoiceRow {
    /// Opaque invoice identifier.
    pub invoice_id: String,
    /// ISO-8601 issue timestamp.
    pub issued_at: String,
    /// Invoice amount in smallest currency unit.
    pub amount_cents: i64,
    /// One of `"paid"` / `"open"` / `"void"`.
    pub status: String,
    /// Stripe-hosted invoice URL.
    pub hosted_url: String,
}

impl InvoiceRow {
    /// Construct an [`InvoiceRow`] from its fields.
    #[must_use]
    pub fn new(
        invoice_id: impl Into<String>,
        issued_at: impl Into<String>,
        amount_cents: i64,
        status: impl Into<String>,
        hosted_url: impl Into<String>,
    ) -> Self {
        Self {
            invoice_id: invoice_id.into(),
            issued_at: issued_at.into(),
            amount_cents,
            status: status.into(),
            hosted_url: hosted_url.into(),
        }
    }
}

/// Billing response — full billing dashboard snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BillingResponse {
    /// Subscription status.
    pub status: String,
    /// Plan slug.
    pub plan: String,
    /// ISO-8601 start of the current billing period.
    pub current_period_start: String,
    /// ISO-8601 end of the current billing period.
    pub current_period_end: String,
    /// Amount due in smallest currency unit.
    pub amount_due_cents: i64,
    /// Currency code.
    pub currency: String,
    /// Invoice history (most-recent first).
    pub invoices: Vec<InvoiceRow>,
}

impl BillingResponse {
    /// Construct a [`BillingResponse`] from its fields.
    #[must_use]
    pub fn new(
        status: impl Into<String>,
        plan: impl Into<String>,
        current_period_start: impl Into<String>,
        current_period_end: impl Into<String>,
        amount_due_cents: i64,
        currency: impl Into<String>,
        invoices: Vec<InvoiceRow>,
    ) -> Self {
        Self {
            status: status.into(),
            plan: plan.into(),
            current_period_start: current_period_start.into(),
            current_period_end: current_period_end.into(),
            amount_due_cents,
            currency: currency.into(),
            invoices,
        }
    }
}

/// Billing portal request — `POST /v1/customer/billing/portal`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortalRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl PortalRequest {
    /// Construct a [`PortalRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Billing portal response — short-lived Stripe portal URL.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortalResponse {
    /// Stripe billing portal URL (expires in ~5 min per Stripe docs).
    pub portal_url: String,
}

impl PortalResponse {
    /// Construct a [`PortalResponse`] from its fields.
    #[must_use]
    pub fn new(portal_url: impl Into<String>) -> Self {
        Self {
            portal_url: portal_url.into(),
        }
    }
}

// ─── Keys (PAT) ──────────────────────────────────────────────────────────────

/// One PAT row returned by list / create / revoke responses.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PatRow {
    /// Opaque PAT identifier.
    pub pat_id: String,
    /// Human-readable name.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of last use, if any.
    pub last_used_at: Option<String>,
    /// ISO-8601 revocation timestamp, if revoked.
    pub revoked_at: Option<String>,
}

impl PatRow {
    /// Construct a [`PatRow`] from its fields.
    #[must_use]
    pub fn new(
        pat_id: impl Into<String>,
        name: impl Into<String>,
        scopes: Vec<String>,
        created_at: impl Into<String>,
        last_used_at: Option<String>,
        revoked_at: Option<String>,
    ) -> Self {
        Self {
            pat_id: pat_id.into(),
            name: name.into(),
            scopes,
            created_at: created_at.into(),
            last_used_at,
            revoked_at,
        }
    }
}

/// Keys list request — `GET /v1/customer/keys`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeysListRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl KeysListRequest {
    /// Construct a [`KeysListRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Keys list response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeysListResponse {
    /// All non-deleted PATs for this tenant.
    pub pats: Vec<PatRow>,
    /// Current BYOK status for the tenant.
    pub byok: ByokStatus,
}

impl KeysListResponse {
    /// Construct a [`KeysListResponse`] from its fields.
    #[must_use]
    pub fn new(pats: Vec<PatRow>, byok: ByokStatus) -> Self {
        Self { pats, byok }
    }
}

/// PAT create request — `POST /v1/customer/keys`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyCreateRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Human-readable name for the new PAT.
    pub name: String,
    /// Scopes to grant.
    pub scopes: Vec<String>,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl KeyCreateRequest {
    /// Construct a [`KeyCreateRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        name: impl Into<String>,
        scopes: Vec<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            name: name.into(),
            scopes,
            at_unix_ms,
        }
    }
}

/// PAT create response — includes the raw token (shown once only).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyCreateResponse {
    /// The newly created PAT metadata row.
    pub pat: PatRow,
    /// Raw bearer token (shown once; not stored in cleartext).
    pub token: String,
}

impl KeyCreateResponse {
    /// Construct a [`KeyCreateResponse`] from its fields.
    #[must_use]
    pub fn new(pat: PatRow, token: impl Into<String>) -> Self {
        Self {
            pat,
            token: token.into(),
        }
    }
}

/// PAT revoke request — `POST /v1/customer/keys/{pat_id}/revoke`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyRevokeRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// PAT to revoke.
    pub pat_id: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl KeyRevokeRequest {
    /// Construct a [`KeyRevokeRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        pat_id: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            pat_id: pat_id.into(),
            at_unix_ms,
        }
    }
}

/// PAT revoke response — the updated PAT row with `revoked_at` set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyRevokeResponse {
    /// The PAT row after revocation.
    pub pat: PatRow,
}

impl KeyRevokeResponse {
    /// Construct a [`KeyRevokeResponse`] from its fields.
    #[must_use]
    pub fn new(pat: PatRow) -> Self {
        Self { pat }
    }
}

// ─── Team ────────────────────────────────────────────────────────────────────

/// One team member row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamMemberRow {
    /// Opaque user identifier.
    pub user_id: String,
    /// User's email address.
    pub email: String,
    /// One of `"Owner"` / `"Admin"` / `"Developer"` / `"Viewer"`.
    pub role: String,
    /// ISO-8601 timestamp when the user joined (accepted invite).
    pub joined_at: String,
    /// One of `"active"` / `"invited"` / `"suspended"`.
    pub status: String,
}

impl TeamMemberRow {
    /// Construct a [`TeamMemberRow`] from its fields.
    #[must_use]
    pub fn new(
        user_id: impl Into<String>,
        email: impl Into<String>,
        role: impl Into<String>,
        joined_at: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            email: email.into(),
            role: role.into(),
            joined_at: joined_at.into(),
            status: status.into(),
        }
    }
}

/// Team list request — `GET /v1/customer/team`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamListRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TeamListRequest {
    /// Construct a [`TeamListRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Team list response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamListResponse {
    /// All members (active + invited + suspended) for this tenant.
    pub members: Vec<TeamMemberRow>,
}

impl TeamListResponse {
    /// Construct a [`TeamListResponse`] from its fields.
    #[must_use]
    pub fn new(members: Vec<TeamMemberRow>) -> Self {
        Self { members }
    }
}

/// Team invite request — `POST /v1/customer/team/invite`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamInviteRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Email address to invite.
    pub email: String,
    /// Role to assign.
    pub role: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TeamInviteRequest {
    /// Construct a [`TeamInviteRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        email: impl Into<String>,
        role: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            email: email.into(),
            role: role.into(),
            at_unix_ms,
        }
    }
}

/// Team invite response — the new member row (status `"invited"`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamInviteResponse {
    /// The newly created member row with `status = "invited"`.
    pub member: TeamMemberRow,
}

impl TeamInviteResponse {
    /// Construct a [`TeamInviteResponse`] from its fields.
    #[must_use]
    pub fn new(member: TeamMemberRow) -> Self {
        Self { member }
    }
}

/// Team seat-removal request — `DELETE /v1/customer/team/{user_id}`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamRemoveRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal (must be owner/admin; never self-removal of the owner).
    pub principal: String,
    /// The member `user_id` whose seat is being removed.
    pub target_user_id: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TeamRemoveRequest {
    /// Construct a [`TeamRemoveRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        target_user_id: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            target_user_id: target_user_id.into(),
            at_unix_ms,
        }
    }
}

/// Team seat-removal response — the removed member + how many PATs were revoked.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamRemoveResponse {
    /// The removed member row (now `status = "removed"`).
    pub member: TeamMemberRow,
    /// How many of the member's PATs were revoked as part of the removal — the
    /// load-bearing security effect (a removed seat must lose data-plane access).
    pub revoked_pats: u32,
}

impl TeamRemoveResponse {
    /// Construct a [`TeamRemoveResponse`] from its fields.
    #[must_use]
    pub fn new(member: TeamMemberRow, revoked_pats: u32) -> Self {
        Self {
            member,
            revoked_pats,
        }
    }
}

// ─── Audit query ─────────────────────────────────────────────────────────────

/// Audit query request — `GET /v1/customer/audit` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditQueryRequest {
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Optional ISO-8601 `since` filter.
    pub since: Option<String>,
    /// Optional event-type allowlist filter (empty = all types).
    pub event_types: Vec<String>,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl AuditQueryRequest {
    /// Construct an [`AuditQueryRequest`] from its fields.
    #[must_use]
    pub fn new(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        since: Option<String>,
        event_types: Vec<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            since,
            event_types,
            at_unix_ms,
        }
    }
}

/// Audit query response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditQueryResponse {
    /// Matching audit event rows (newest first).
    pub rows: Vec<CustomerAuditEventRow>,
}

impl AuditQueryResponse {
    /// Construct an [`AuditQueryResponse`] from its fields.
    #[must_use]
    pub fn new(rows: Vec<CustomerAuditEventRow>) -> Self {
        Self { rows }
    }
}

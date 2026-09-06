use super::CustomerAuditEventRow;

// ─── Audit query ─────────────────────────────────────────────────────────────

/// Audit query request — `GET /v1/customer/audit` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditQueryRequest {
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            since,
            event_types,
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

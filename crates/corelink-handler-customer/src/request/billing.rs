// ─── Billing ─────────────────────────────────────────────────────────────────

/// Billing request — `GET /v1/customer/billing` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BillingRequest {
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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

    /// Alias for [`Self::for_tenant`].
    #[must_use]
    pub fn with_requested_tenant(self, tenant: impl Into<String>) -> Self {
        self.for_tenant(tenant)
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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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

    /// Alias for [`Self::for_tenant`].
    #[must_use]
    pub fn with_requested_tenant(self, tenant: impl Into<String>) -> Self {
        self.for_tenant(tenant)
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

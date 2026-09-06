// ─── Keys (PAT) ──────────────────────────────────────────────────────────────

use super::overview::ByokStatus;

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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            name: name.into(),
            scopes,
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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Server-trusted team role of the caller. Empty means the legacy
    /// handler-only API (and is rejected by the HTTP route before dispatch).
    pub caller_role: String,
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
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            caller_role: String::new(),
            pat_id: pat_id.into(),
            at_unix_ms,
        }
    }

    /// Construct a revoke request with the Worker's server-trusted team role.
    #[must_use]
    pub fn with_role(
        caller_tenant: impl Into<String>,
        principal: impl Into<String>,
        caller_role: impl Into<String>,
        pat_id: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            caller_role: caller_role.into(),
            pat_id: pat_id.into(),
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

/// PAT revoke response — the updated PAT row with `revoked_at` set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyRevokeResponse {
    /// The PAT row after revocation.
    pub pat: PatRow,
    /// Non-secret cache handle returned only by the D1-backed implementation.
    /// The edge uses it to evict the positive auth row immediately; the
    /// in-memory implementation leaves it `None`.
    pub cache_token_id: Option<String>,
}

impl KeyRevokeResponse {
    /// Construct a [`KeyRevokeResponse`] from its fields.
    #[must_use]
    pub fn new(pat: PatRow) -> Self {
        Self {
            pat,
            cache_token_id: None,
        }
    }

    /// Attach the D1 token id without exposing token material.
    #[must_use]
    pub fn with_cache_token_id(mut self, token_id: impl Into<String>) -> Self {
        self.cache_token_id = Some(token_id.into());
        self
    }
}

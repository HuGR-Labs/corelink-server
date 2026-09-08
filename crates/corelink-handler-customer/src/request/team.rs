// ─── Team ────────────────────────────────────────────────────────────────────

/// Normalize an invite role to the frozen `team_member.role` CHECK domain.
/// Unknown and non-grantable roles are rejected instead of silently downgraded.
#[must_use]
pub fn canonical_invite_role(role: &str) -> Option<&'static str> {
    match role.trim().to_ascii_lowercase().as_str() {
        "admin" => Some("admin"),
        "member" => Some("member"),
        "viewer" => Some("viewer"),
        _ => None,
    }
}

/// One team member row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamMemberRow {
    /// Opaque user identifier.
    pub user_id: String,
    /// User's email address.
    pub email: String,
    /// One of the persisted values `"owner"` / `"admin"` / `"member"` / `"viewer"`.
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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            email: email.into(),
            role: role.into(),
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

/// Team invite response — the new member row (status `"invited"`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamInviteResponse {
    /// The newly created member row with `status = "invited"`.
    pub member: TeamMemberRow,
    /// One-time cryptographic invitation capability. It is returned only when
    /// an invitation is created and is never persisted or returned by list.
    pub invitation_token: Option<String>,
}

impl TeamInviteResponse {
    /// Construct a [`TeamInviteResponse`] from its fields.
    #[must_use]
    pub fn new(member: TeamMemberRow) -> Self {
        Self {
            member,
            invitation_token: None,
        }
    }

    /// Construct a response carrying the one-time invitation capability.
    #[must_use]
    pub fn with_token(member: TeamMemberRow, invitation_token: impl Into<String>) -> Self {
        Self {
            member,
            invitation_token: Some(invitation_token.into()),
        }
    }
}

/// Team seat-removal request — `DELETE /v1/customer/team/{user_id}`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamRemoveRequest {
    /// Tenant named by the request route; defaults to the caller tenant.
    pub requested_tenant: Option<String>,
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
            requested_tenant: None,
            caller_tenant: caller_tenant.into(),
            principal: principal.into(),
            target_user_id: target_user_id.into(),
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

/// Canonical `tier_selections.tier` enum (migration 0039). The D1
/// `UPDATE tier_selections SET tier = ?1` write is gated by a CHECK
/// constraint accepting EXACTLY these lower-case labels.
///
/// NOTE on divergence: migration 0057's `tenant.tier` CHECK additionally
/// allows `org` (a legacy / quota-class alias). That is NOT valid in
/// `tier_selections`, so the operator admin path rejects it here with a
/// clean 400 rather than letting the value reach the D1 CHECK (which would
/// surface as an opaque 500). Reconciling the two enums is an additive-only
/// auth-migration follow-up (do NOT widen 0039 destructively) — tracked in
/// the PR for #35.
// CAA-360 #26: this is the narrower **settable** ladder (6, current product
// tiers). The **resolve** set is `auth_introspect.rs::VALID_TIERS` (8 — also
// accepts the back-compat `team`/`org` that the `tenant.tier` CHECK still allows
// for existing rows but are deliberately NOT settable here). Invariant:
// settable ⊂ resolvable ⊂ D1-CHECK — keep that subset relationship if changing.
const TIER_SELECTIONS_TIERS: [&str; 6] = ["free", "solo", "starter", "pro", "max", "enterprise"];

/// Validate + normalize an operator-supplied `tier` string against the
/// `tier_selections.tier` enum BEFORE it is flowed into a
/// [`MutateOp::SetTenantTier`] and on to the D1 write.
///
/// Trims surrounding whitespace and lower-cases (so the codebase's own
/// admin callers that send `"Pro"` normalize to `"pro"`), then rejects
/// anything outside [`TIER_SELECTIONS_TIERS`] with a static error string —
/// surfaced as a `400 invalid_tier` at the route boundary instead of an
/// opaque D1 CHECK-violation 500.
///
/// # Errors
///
/// Returns `Err("invalid_tier")` when the normalized value is not one of
/// the canonical `tier_selections` tiers (e.g. `org`, typos).
fn normalize_tier_selection(raw: &str) -> Result<String, &'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    if TIER_SELECTIONS_TIERS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err("invalid_tier")
    }
}

impl AdminMutateBody {
    /// Parse the body into an [`AdminMutateRequest`].
    ///
    /// # Errors
    ///
    /// Returns a static error string when the op-kind / required
    /// fields combination is invalid (e.g. `set_tenant_tier` without
    /// `tenant` + `tier`).
    pub fn into_request(self, at_unix_ms: u64) -> Result<AdminMutateRequest, &'static str> {
        let op = match self.op_kind.as_str() {
            "set_tenant_tier" => {
                let tenant = self.tenant.ok_or("set_tenant_tier requires tenant")?;
                let tier = self.tier.ok_or("set_tenant_tier requires tier")?;
                // Validate + normalize against the tier_selections enum
                // BEFORE the value can reach the D1 CHECK (#35).
                let tier = normalize_tier_selection(&tier)?;
                MutateOp::set_tenant_tier(tenant, tier)
            }
            "rotate_admin_token" => {
                let token_id = self
                    .token_id
                    .ok_or("rotate_admin_token requires token_id")?;
                MutateOp::rotate_admin_token(token_id)
            }
            _ => return Err("unknown op_kind"),
        };
        let approval = match (self.approval_id, self.approver) {
            (Some(id), Some(approver)) => Some(DualApprovalToken::new(id, approver)),
            (None, None) => None,
            _ => return Err("approval_id + approver must be set together"),
        };
        Ok(AdminMutateRequest::new(
            op,
            self.initiator,
            self.initiator_is_admin,
            approval,
            at_unix_ms,
        ))
    }

    /// Parse the body into an [`AdminMutateRequest`] for the
    /// **operator-gated** route path.
    ///
    /// Unlike [`into_request`](Self::into_request), the admin assertion
    /// is NOT taken from the client JSON: the caller already cleared the
    /// `x-corelink-internal-auth` gate, so the initiator principal is the
    /// supplied trusted `operator` and `is_admin` is forced `true`. The
    /// body's `initiator` / `initiator_is_admin` fields are IGNORED for
    /// the auth decision (kept on the struct only for wire-compat).
    ///
    /// Dual-approval (`approval_id` + `approver`) is still parsed and
    /// enforced downstream; the handler rejects self-approval, so the
    /// `approver` must differ from `operator`.
    ///
    /// # Errors
    ///
    /// Returns a static error string when the op-kind / required-field
    /// combination is invalid (same rules as [`into_request`](Self::into_request)).
    pub fn into_request_gated(
        self,
        operator: &str,
        at_unix_ms: u64,
    ) -> Result<AdminMutateRequest, &'static str> {
        let op = match self.op_kind.as_str() {
            "set_tenant_tier" => {
                let tenant = self.tenant.ok_or("set_tenant_tier requires tenant")?;
                let tier = self.tier.ok_or("set_tenant_tier requires tier")?;
                // Validate + normalize against the tier_selections enum
                // BEFORE the value can reach the D1 CHECK (#35).
                let tier = normalize_tier_selection(&tier)?;
                MutateOp::set_tenant_tier(tenant, tier)
            }
            "rotate_admin_token" => {
                let token_id = self
                    .token_id
                    .ok_or("rotate_admin_token requires token_id")?;
                MutateOp::rotate_admin_token(token_id)
            }
            _ => return Err("unknown op_kind"),
        };
        let approval = match (self.approval_id, self.approver) {
            (Some(id), Some(approver)) => Some(DualApprovalToken::new(id, approver)),
            (None, None) => None,
            _ => return Err("approval_id + approver must be set together"),
        };
        Ok(AdminMutateRequest::new(
            op,
            operator.to_owned(),
            // is_admin is derived from the internal-auth gate, NOT the body.
            true,
            approval,
            at_unix_ms,
        ))
    }
}

/// `GET /v1/admin/read/:resource` handler.
async fn handle_read(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    Path(resource): Path<String>,
) -> impl IntoResponse {
    // Operator-only gate (fail-CLOSED). The admin control plane is NOT
    // reachable by tenant PATs: only a caller holding the
    // `CORELINK_INTERNAL_AUTH_KEY` shared secret (the operator, via the
    // Worker→DO→container hop) may read admin records. Absent/wrong
    // secret, or unconfigured key → 403 BEFORE any handler logic. The
    // handler's own ReadAttempted audit row is only emitted once the
    // gate passes; on the gate-fail path the SEC signal is the structured
    // warning below (no per-tenant audit sink exists at this boundary).
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "AdminReadUnauthorized",
            "admin read rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    // Real wall-clock timestamp for the audit record (was hardcoded
    // epoch-zero, which made admin audit rows un-orderable). Matches the
    // CAS/AC/Bazel/Turbo audit path (`SystemWallClock.now_ms()`).
    let now_ms = SystemWallClock.now_ms();
    // The caller is the trusted operator (it cleared the internal-auth
    // gate above), so we derive the admin principal from the gated
    // context — NOT from any client-supplied field.
    let req = AdminReadRequest::new(resource, "admin@root", true, now_ms);
    match state.read.read(req) {
        Ok(resp) => (StatusCode::OK, resp.body).into_response(),
        Err(e) => map_err(e),
    }
}

/// Principal recorded for operator-gated admin mutations. The admin
/// assertion is derived from the internal-auth gate, NEVER from the
/// client JSON body.
const ADMIN_OPERATOR_PRINCIPAL: &str = "operator@internal";

/// `POST /v1/admin/mutate` handler.
///
/// M3 pattern (F16 fix): the body is accepted as raw [`Bytes`] so the
/// `HeaderMap` `FromRequestParts` extractor resolves BEFORE the body is
/// buffered into memory. The internal-auth gate is evaluated FIRST; an
/// unauthenticated caller is rejected with 403 WITHOUT the body ever
/// being JSON-parsed — denying a pre-auth caller the CPU/heap cost of
/// parsing an arbitrarily-large body. JSON deserialisation runs only AFTER
/// the gate passes, matching the M3 pattern already used by
/// `dsr.rs` and `internal_pat.rs`.
async fn handle_mutate(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Operator-only gate (fail-CLOSED). Auth is checked BEFORE the body is
    // parsed (M3 / F16): `headers` is a `FromRequestParts` extractor, so
    // this gate runs before the body buffer is consumed. The previous version
    // used `Json(body): Json<AdminMutateBody>` which caused body-parse to
    // run before the gate — that is corrected here.
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "AdminMutateUnauthorized",
            "admin mutate rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    // Body parsed ONLY after the auth gate passes (M3).
    let body: AdminMutateBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "admin mutate: invalid request body");
            return (StatusCode::BAD_REQUEST, "invalid_body").into_response();
        }
    };
    // Real wall-clock timestamp for the audit record (was hardcoded
    // epoch-zero, which made admin audit rows un-orderable). Matches the
    // CAS/AC/Bazel/Turbo audit path (`SystemWallClock.now_ms()`).
    let now_ms = SystemWallClock.now_ms();
    // Build the request from the body's OPERATION fields only. The
    // initiator principal + is_admin flag are derived from the gated
    // operator context (is_admin = true), NOT from the body. Dual-
    // approval (approval_id + approver) is still honoured from the body
    // and is enforced by the handler — the approver must differ from the
    // operator principal.
    let req = match body.into_request_gated(ADMIN_OPERATOR_PRINCIPAL, now_ms) {
        Ok(r) => r,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    match state.mutate.mutate(req) {
        Ok(resp) => (StatusCode::OK, resp.resource).into_response(),
        Err(e) => map_err(e),
    }
}

/// Map an [`AdminHandlerError`] to the canonical HTTP response.
fn map_err(e: AdminHandlerError) -> axum::response::Response {
    match e {
        AdminHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "admin not found").into_response()
        }
        AdminHandlerError::Forbidden { .. } => (StatusCode::FORBIDDEN, "forbidden").into_response(),
        AdminHandlerError::DualApprovalMissing => {
            (StatusCode::FORBIDDEN, "dual-approval required").into_response()
        }
        AdminHandlerError::DualApprovalSelfApproval { .. } => {
            (StatusCode::FORBIDDEN, "self-approval rejected").into_response()
        }
        AdminHandlerError::DualApprovalUnknown { .. } => {
            // No ledger record for the supplied approval_id — the forged /
            // absent-approver reject that kills the H5 free-text bypass.
            (StatusCode::FORBIDDEN, "dual-approval not found").into_response()
        }
        AdminHandlerError::DualApprovalScopeMismatch { .. } => {
            (StatusCode::FORBIDDEN, "dual-approval scope mismatch").into_response()
        }
        AdminHandlerError::DualApprovalConsumed { .. } => {
            // Single-use replay.
            (StatusCode::CONFLICT, "dual-approval already used").into_response()
        }
        AdminHandlerError::ApprovalLedgerUnavailable(_) => {
            // Fail-CLOSED: cannot authoritatively consult+consume the ledger.
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "approval ledger unavailable",
            )
                .into_response()
        }
        AdminHandlerError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never mutate.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

/// Principal recorded as the second approver for an operator-gated approval.
/// Deliberately DISTINCT from [`ADMIN_OPERATOR_PRINCIPAL`] so a recorded
/// approval is never a self-approval at the identity level; the real
/// two-person guarantee is the separate [`approver_auth_key_from_env`]
/// credential the approve gate requires.
const ADMIN_APPROVER_PRINCIPAL: &str = "approver@internal";

/// JSON shape for `POST /v1/admin/approve` (finding H5).
///
/// Records a second-approver approval for a specific pending mutation. The
/// `approval_id` the operator later presents on `POST /v1/admin/mutate` MUST
/// match the one recorded here, and the `resource` is derived identically to
/// [`MutateOp::resource`] so scope-binding lines up.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct AdminApproveBody {
    /// Opaque approval id (the operator generates a fresh one, e.g. a UUID).
    pub approval_id: String,
    /// Operation kind being approved (`set_tenant_tier` / `rotate_admin_token`).
    pub op_kind: String,
    /// Tenant id (required for `set_tenant_tier`).
    pub tenant: Option<String>,
    /// Token id (required for `rotate_admin_token`).
    pub token_id: Option<String>,
}

impl AdminApproveBody {
    /// Derive the scope `resource` string this approval binds to, identical to
    /// [`MutateOp::resource`] so the mutate-time scope check matches.
    ///
    /// # Errors
    ///
    /// Returns a static error when the op-kind / required fields are invalid.
    fn resource(&self) -> Result<String, &'static str> {
        match self.op_kind.as_str() {
            "set_tenant_tier" => {
                let tenant = self
                    .tenant
                    .as_deref()
                    .ok_or("set_tenant_tier requires tenant")?;
                Ok(format!("tenant:{tenant}"))
            }
            "rotate_admin_token" => {
                let token_id = self
                    .token_id
                    .as_deref()
                    .ok_or("rotate_admin_token requires token_id")?;
                Ok(format!("admin_token:{token_id}"))
            }
            _ => Err("unknown op_kind"),
        }
    }
}

/// `POST /v1/admin/approve` handler (finding H5).
///
/// Records the second approver into the durable approval ledger. Gated by the
/// dedicated `CORELINK_ADMIN_APPROVER_AUTH_KEY` (approve gate), a DIFFERENT
/// credential from the mutate gate — so a single admin-key holder cannot both
/// approve and mutate. Auth is checked BEFORE the body is parsed (M3 pattern).
async fn handle_approve(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Approve gate (fail-CLOSED), evaluated before body parse (M3).
    if !internal_auth_ok(state.approver_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "AdminApproveUnauthorized",
            "admin approve rejected: missing/invalid approver auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let body: AdminApproveBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "admin approve: invalid request body");
            return (StatusCode::BAD_REQUEST, "invalid_body").into_response();
        }
    };
    let resource = match body.resource() {
        Ok(r) => r,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    // The approver identity is derived from the (dedicated) approve gate, NEVER
    // from the client body — mirrors the mutate path's operator-principal rule.
    match state.approval_writer.record_approval(
        &body.approval_id,
        ADMIN_APPROVER_PRINCIPAL,
        &resource,
    ) {
        Ok(()) => {
            tracing::info!(
                event = "AdminApprovalRecorded",
                approval_id = %body.approval_id,
                resource = %resource,
                durable = state.approvals_durable,
                "admin dual-approval recorded"
            );
            (StatusCode::OK, "approved").into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin approve: ledger record failed");
            // A re-record of a consumed approval, or a backend error.
            if e.contains("already consumed") {
                (StatusCode::CONFLICT, "approval already used").into_response()
            } else {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "approval ledger unavailable",
                )
                    .into_response()
            }
        }
    }
}

/// Resolve a **dedicated-only** internal-auth key: reads `dedicated_env` and
/// NEVER falls back to the shared `CORELINK_INTERNAL_AUTH_KEY` (finding H4).
///
/// This is the fail-CLOSED sibling of [`resolve_internal_auth_key`], for the
/// two authorities whose entire security value is that they are held by a
/// DIFFERENT party than the broad shared secret: the irreversible **erase**
/// authority (`CORELINK_ERASE_AUTH_KEY`) and the DSR legitimacy **anchor**
/// authority (`CORELINK_DSR_ANCHOR_AUTH_KEY`). If either fell back to the
/// shared key, a single `CORELINK_INTERNAL_AUTH_KEY` leak would collapse the
/// "eraser ≠ requester" two-authority split. Mirrors the dedicated-key-only
/// PAT-mint gate (`internal_pat::resolve_mint_auth_key`).
///
/// Fail-CLOSED: an absent / blank / `< INTERNAL_AUTH_KEY_MIN_LEN`-char value
/// yields `None`, so the caller declines to mount the surface (route
/// unavailable / 403) rather than silently widening to the shared key.
#[must_use]
pub(crate) fn resolve_dedicated_auth_key(dedicated_env: &str) -> Option<Arc<str>> {
    match std::env::var(dedicated_env) {
        Ok(key) if key.len() >= INTERNAL_AUTH_KEY_MIN_LEN => Some(Arc::from(key.as_str())),
        Ok(key) if !key.is_empty() => {
            tracing::warn!(
                env = dedicated_env,
                "dedicated internal-auth key set but < 32 chars; this surface will \
                 fail CLOSED (403) — NO fallback to the shared CORELINK_INTERNAL_AUTH_KEY \
                 (finding H4; use `openssl rand -hex 32`)"
            );
            None
        }
        _ => None,
    }
}

/// Resolve the DSR legitimacy-anchor register key (`POST /_internal/dsr/anchor`),
/// from the dedicated `CORELINK_DSR_ANCHOR_AUTH_KEY` ONLY — NO shared-key
/// fallback (finding H4). This key MUST be held by the erasure-REQUEST authority
/// (e.g. githugr), a DIFFERENT party than the eraser holding
/// `CORELINK_ERASE_AUTH_KEY` — the anti-forge basis of the legitimacy gate; a
/// shared fallback would let one `CORELINK_INTERNAL_AUTH_KEY` holder forge both.
/// `None` ⇒ the anchor route is not mounted (fail-CLOSED). See
/// [`crate::routes::dsr_anchor`]. (Placed at the end of the module, after the
/// OKF-cited items above, to keep anti-drift line-anchors stable.)
#[must_use]
pub fn dsr_anchor_auth_key_from_env() -> Option<Arc<str>> {
    resolve_dedicated_auth_key("CORELINK_DSR_ANCHOR_AUTH_KEY")
}

// Route-level auth-resolution tests mutate process environment variables. A
// single crate-wide lock keeps those tests deterministic across the admin,
// tier-select, and DPA-accept modules.
#[cfg(test)]
pub(crate) static INTERNAL_AUTH_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

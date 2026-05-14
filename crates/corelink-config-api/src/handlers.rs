//! HTTP handler logic for the config admin endpoints (WI-S13-001).
//!
//! # Endpoints
//!
//! | Method | Path | Handler |
//! |--------|------|---------|
//! | GET | `/v1/admin/config/current` | [`handle_get_current`] |
//! | PUT | `/v1/admin/config` | [`handle_put`] |
//! | POST | `/v1/admin/config/rollback?to_version=X` | [`handle_rollback`] |
//! | GET | `/v1/admin/config/history?limit=N` | [`handle_get_history`] |
//!
//! All mutating endpoints enforce:
//! 1. Admin role (via [`AdminContext::is_admin`]).
//! 2. MFA freshness ≤ 30 min (via [`middleware::mfa_freshness::check_mfa_freshness`]).
//! 3. Rollback additionally requires dual-approval (X-Dual-Approver header).
//!
//! # Audit fail-CLOSED
//!
//! Handlers do not directly call the audit sink; the sink is wired inside
//! [`ConfigSingletonStore::update`] / [`rollback_to`]. Handlers only build
//! the request and propagate errors.

use serde::{Deserialize, Serialize};

use corelink_config_do::{
    AdminActor, ConfigPayload, ConfigVersionEntry,
    store::ConfigSingletonStore,
};

use crate::{
    error::ApiError,
    middleware::mfa_freshness::check_mfa_freshness,
};

/// Admin caller context extracted from the session token by the auth middleware.
///
/// In production this is deserialized from the JWT claims + MFA attestation.
/// In tests it is constructed directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminContext {
    /// Admin actor (user_id + pseudonymized email_hash).
    pub actor: AdminActor,
    /// MFA completion timestamp (Unix ms).
    pub mfa_ts_ms: u64,
    /// Whether the caller has the admin role (CTRL-AUTHZ-001).
    pub is_admin: bool,
    /// Optional dual-approver UUID (required for rollback).
    pub dual_approver_user_id: Option<uuid::Uuid>,
}

/// Request body for `PUT /v1/admin/config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutConfigRequest {
    /// Expected current version (CAS guard).
    pub expected_version: u64,
    /// New config payload.
    pub new_payload: ConfigPayload,
}

/// Response for `PUT /v1/admin/config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutConfigResponse {
    /// New version after successful update.
    pub new_version: u64,
}

/// Response for `GET /v1/admin/config/current`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentResponse {
    /// Current version.
    pub version: u64,
    /// Current payload.
    pub payload: ConfigPayload,
}

/// Response for `POST /v1/admin/config/rollback`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    /// New version created by the rollback (payload = payload of `to_version`).
    pub new_version: u64,
}

/// Response for `GET /v1/admin/config/history`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    /// History entries (newest first, up to `limit`).
    pub entries: Vec<ConfigVersionEntry>,
}

/// Handle `GET /v1/admin/config/current`.
///
/// No admin check required for read-only current endpoint (config is
/// pseudo-public operational metadata; secrets are in a separate path).
///
/// # Errors
///
/// Returns [`ApiError::Config`] on store failure.
pub async fn handle_get_current(
    store: &impl ConfigSingletonStore,
) -> Result<GetCurrentResponse, ApiError> {
    let (version, payload) = store.current().await?;
    Ok(GetCurrentResponse { version, payload })
}

/// Handle `PUT /v1/admin/config`.
///
/// Enforces admin role + MFA freshness, then performs CAS update.
///
/// # Errors
///
/// - [`ApiError::NotAdmin`] if caller lacks admin role.
/// - [`ApiError::MfaStale`] if MFA session > 30 min old.
/// - [`ApiError::Config`] (`VersionConflict`, `SchemaInvalid`, `Backend`).
pub async fn handle_put(
    store: &impl ConfigSingletonStore,
    ctx: &AdminContext,
    request: PutConfigRequest,
    now_ms: u64,
) -> Result<PutConfigResponse, ApiError> {
    if !ctx.is_admin {
        return Err(ApiError::NotAdmin);
    }
    check_mfa_freshness(ctx.mfa_ts_ms, now_ms)?;

    let new_version = store
        .update(
            request.expected_version,
            request.new_payload,
            &ctx.actor,
            now_ms,
        )
        .await?;

    Ok(PutConfigResponse { new_version })
}

/// Handle `POST /v1/admin/config/rollback?to_version=X`.
///
/// Enforces admin role + MFA freshness + dual-approval header.
///
/// # Errors
///
/// - [`ApiError::NotAdmin`] if caller lacks admin role.
/// - [`ApiError::MfaStale`] if MFA session > 30 min old.
/// - [`ApiError::DualApprovalMissing`] if `dual_approver_user_id` not set.
/// - [`ApiError::Config`] (`VersionUnknown`, `VersionExpired`, `Backend`).
pub async fn handle_rollback(
    store: &impl ConfigSingletonStore,
    ctx: &AdminContext,
    to_version: u64,
    now_ms: u64,
) -> Result<RollbackResponse, ApiError> {
    if !ctx.is_admin {
        return Err(ApiError::NotAdmin);
    }
    check_mfa_freshness(ctx.mfa_ts_ms, now_ms)?;

    if ctx.dual_approver_user_id.is_none() {
        return Err(ApiError::DualApprovalMissing);
    }

    let new_version = store.rollback_to(to_version, &ctx.actor, now_ms).await?;

    Ok(RollbackResponse { new_version })
}

/// Handle `GET /v1/admin/config/history?limit=N`.
///
/// # Errors
///
/// Returns [`ApiError::Config`] on store failure.
pub async fn handle_get_history(
    store: &impl ConfigSingletonStore,
    limit: u32,
) -> Result<HistoryResponse, ApiError> {
    let entries = store.history(limit).await?;
    Ok(HistoryResponse { entries })
}

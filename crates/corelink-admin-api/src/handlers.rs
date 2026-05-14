//! Admin op handler dispatcher (WI-S13-002).
//!
//! Maps `AdminOpType` → foundational handler stub. Each handler is a
//! composition point; the real implementation is in the respective WI:
//! - `ConfigRollback` / `RetentionPolicyReduce` / `FeatureFlagDisable` → WI-S13-001.
//! - `SecretRotationStart` → WI-S13-003.
//! - `TenantTombstone` → S-11 erasure pipeline.
//! - `FeatureFlagToggleSafe` / `RateLimitAdjustUp` → config update (WI-S13-001).
//!
//! All handlers are called AFTER dual-approval gate passes.

use corelink_dual_approval::{AdminOpType, VerifiedApproval};

use crate::error::AdminApiError;

/// Dispatch a verified admin op to the appropriate handler stub.
///
/// Returns `Ok(OpResult)` on success; `Err(AdminApiError::DispatchError)`
/// on handler failure.
pub fn dispatch_op(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    match &approval.op_type {
        AdminOpType::ConfigRollback => handle_config_rollback(approval),
        AdminOpType::RetentionPolicyReduce => handle_retention_reduce(approval),
        AdminOpType::FeatureFlagDisable => handle_feature_flag_disable(approval),
        AdminOpType::SecretRotationStart => handle_secret_rotation_start(approval),
        AdminOpType::TenantTombstone => handle_tenant_tombstone(approval),
        AdminOpType::FeatureFlagToggleSafe => handle_feature_flag_toggle_safe(approval),
        AdminOpType::RateLimitAdjustUp => handle_rate_limit_adjust_up(approval),
        _ => Err(AdminApiError::DispatchError(
            "unknown op_type variant".to_owned(),
        )),
    }
}

/// Result of a dispatched admin op.
#[derive(Debug, Clone)]
pub struct OpResult {
    /// Op type executed.
    pub op_type: String,
    /// Confirmation message.
    pub message: String,
}

fn handle_config_rollback(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    // Production: forwards to WI-S13-001 rollback API.
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "config rollback scheduled (WI-S13-001 composed)".to_owned(),
    })
}

fn handle_retention_reduce(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "retention policy reduction scheduled (WI-S13-001 composed)".to_owned(),
    })
}

fn handle_feature_flag_disable(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "feature flag disable scheduled (WI-S13-001 composed)".to_owned(),
    })
}

fn handle_secret_rotation_start(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "secret rotation start forwarded (WI-S13-003 composed)".to_owned(),
    })
}

fn handle_tenant_tombstone(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "tenant tombstone queued (S-11 erasure pipeline composed)".to_owned(),
    })
}

fn handle_feature_flag_toggle_safe(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "feature flag safe toggle applied (WI-S13-001 composed)".to_owned(),
    })
}

fn handle_rate_limit_adjust_up(approval: &VerifiedApproval) -> Result<OpResult, AdminApiError> {
    Ok(OpResult {
        op_type: format!("{:?}", approval.op_type),
        message: "rate limit increase applied (WI-S13-001 composed)".to_owned(),
    })
}

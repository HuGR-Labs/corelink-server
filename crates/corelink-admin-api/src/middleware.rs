//! Tower middleware composition sketch for admin API (WI-S13-002).
//!
//! Production: Tower `ServiceBuilder` chain:
//! `auth → mfa_freshness → dual_approval → op_dispatch → audit_emit`.
//!
//! Here we ship the logical composition as a pure-Rust pipeline struct
//! (no Tower dep to avoid pulling in heavy deps for a CF Workers target).
//! The CI integration test (`e2e_admin_dual_approval.rs`) exercises the
//! full pipeline end-to-end.

use std::sync::Arc;

use corelink_dual_approval::{AdminOpRequest, DualApprovalGate, VerifiedApproval};

use crate::error::AdminApiError;
use crate::handlers::{dispatch_op, OpResult};

/// `AdminApiPipeline` — logical middleware composition for admin op
/// request processing.
///
/// Pipeline steps (in order):
/// 1. Schema validation (pre-dispatch).
/// 2. Dual-approval gate (clock-skew → MFA → caller≠approver → role →
///    HMAC → collusion-rotation → nonce → audit fail-CLOSED).
/// 3. Op dispatch → handler.
///
/// Production wiring: each step maps to a Tower middleware layer; here
/// we compose them as a method chain for CI testability.
#[derive(Debug, Clone)]
pub struct AdminApiPipeline {
    gate: Arc<dyn DualApprovalGate>,
}

impl AdminApiPipeline {
    /// Construct the pipeline with an injected gate.
    pub fn new(gate: Arc<dyn DualApprovalGate>) -> Self {
        Self { gate }
    }

    /// Run the full pipeline for a single admin op request.
    ///
    /// # Parameters
    /// - `req`: parsed admin op request.
    /// - `caller_mfa_ts_ms`: caller Clerk JWT `auth_time` claim (ms epoch).
    /// - `now_ms`: server-side current time (ms epoch).
    ///
    /// # Returns
    /// `Ok(PipelineResult)` if all steps pass; `Err(AdminApiError)` on
    /// any failure (HTTP status from `AdminApiError::http_status()`).
    pub fn process(
        &self,
        req: &AdminOpRequest,
        caller_mfa_ts_ms: u64,
        now_ms: u64,
    ) -> Result<PipelineResult, AdminApiError> {
        // Step 1: schema validation (basic sanity — production: full JSON
        // schema validator).
        self.validate_schema(req)?;

        // Step 2: dual-approval gate.
        let approval: VerifiedApproval = self
            .gate
            .verify(req, caller_mfa_ts_ms, now_ms)
            .map_err(AdminApiError::DualApprovalRejected)?;

        // Step 3: op dispatch.
        let op_result = dispatch_op(&approval)?;

        Ok(PipelineResult {
            approval,
            op_result,
        })
    }

    fn validate_schema(&self, req: &AdminOpRequest) -> Result<(), AdminApiError> {
        if req.op_payload.is_empty() {
            return Err(AdminApiError::SchemaValidation(
                "op_payload must not be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Result of a successful pipeline run.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Verified dual-approval context.
    pub approval: VerifiedApproval,
    /// Op dispatch result.
    pub op_result: OpResult,
}

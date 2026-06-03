//! `DualApprovalGate` trait + `DualApprovalGateImpl` (WI-S13-002).
//!
//! The gate runs the full defense-in-depth pipeline:
//!
//! 1. Clock-skew check (≤ 60s).
//! 2. MFA freshness check (≤ 30 min).
//! 3. Caller ≠ approver (separation of duties).
//! 4. HMAC-SHA256 signature verify (constant-time).
//! 5. Collusion-rotation 3-cycle check (NIST AC-2(7) oracle).
//! 6. Nonce replay guard.
//! 7. Audit emit (fail-CLOSED — before recording result).
//!
//! All checks are hard-fail: any error → deny, audit emitted.
//! No advisory mode; no environment-gated bypass.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::audit::{AdminOpAuditSink, AdminOpCloudEvent, AdminOpCloudEventBuilder};
use crate::collusion::InMemoryCollusionStore;
use crate::error::DualApprovalError;
use crate::hmac_verify::{verify_hmac, AdminSigningKey};
use crate::nonce::InMemoryNonceStore;
use crate::types::{ActorIdentity, AdminOpRequest, ApprovalOutcome, VerifiedApproval};

/// MFA freshness hard limit (ms).
const MFA_MAX_AGE_MS: u64 = 30 * 60 * 1_000; // 30 min

/// Clock-skew tolerance (ms).
const CLOCK_SKEW_MAX_MS: u64 = 60 * 1_000; // 60 s

/// Admin role store — abstracts the D1 `admin_role` table check.
///
/// Production: D1 `SELECT 1 FROM admin_role WHERE user_id=$1 AND active=true
/// AND revoked_at IS NULL`. In-memory for CI.
pub trait AdminRoleStore: Send + Sync + std::fmt::Debug {
    /// Returns `true` if the user holds an active admin role.
    fn is_admin(&self, user_id: Uuid) -> bool;
}

/// Simple in-memory role store (test/CI fixture).
#[derive(Debug, Clone)]
pub struct InMemoryAdminRoleStore {
    admins: Vec<Uuid>,
}

impl InMemoryAdminRoleStore {
    /// Construct with a list of active admin user IDs.
    pub fn new(admins: Vec<Uuid>) -> Self {
        Self { admins }
    }
}

impl AdminRoleStore for InMemoryAdminRoleStore {
    fn is_admin(&self, user_id: Uuid) -> bool {
        self.admins.contains(&user_id)
    }
}

/// `DualApprovalGate` — the core enforcement trait.
///
/// Callers invoke [`DualApprovalGate::verify`] before dispatching any
/// destructive admin op. Hard-fail on any violation.
pub trait DualApprovalGate: Send + Sync + std::fmt::Debug {
    /// Verify dual-approval pre-handler dispatch.
    ///
    /// # Parameters
    /// - `req`: fully-parsed admin op request.
    /// - `caller_mfa_ts_ms`: Clerk JWT `auth_time` claim (ms epoch).
    /// - `now_ms`: server-side current time (ms epoch); injected for
    ///   deterministic testing.
    ///
    /// # Returns
    /// `Ok(VerifiedApproval)` if all checks pass; `Err(DualApprovalError)`
    /// on any failure (hard-fail; no advisory path).
    fn verify(
        &self,
        req: &AdminOpRequest,
        caller_mfa_ts_ms: u64,
        now_ms: u64,
    ) -> Result<VerifiedApproval, DualApprovalError>;
}

/// Production-equivalent implementation of [`DualApprovalGate`].
///
/// Uses injected stores for all D1 queries (production: real D1 binding;
/// CI: in-memory fakes). Per-instance `Arc<Mutex<>>` (F-001 closure).
#[derive(Debug, Clone)]
pub struct DualApprovalGateImpl {
    signing_key: AdminSigningKey,
    role_store: Arc<dyn AdminRoleStore>,
    collusion_store: InMemoryCollusionStore,
    nonce_store: InMemoryNonceStore,
    audit_sink: Arc<dyn AdminOpAuditSink>,
    region: String,
}

impl DualApprovalGateImpl {
    /// Construct a new gate impl.
    pub fn new(
        signing_key: AdminSigningKey,
        role_store: Arc<dyn AdminRoleStore>,
        collusion_store: InMemoryCollusionStore,
        nonce_store: InMemoryNonceStore,
        audit_sink: Arc<dyn AdminOpAuditSink>,
        region: impl Into<String>,
    ) -> Self {
        Self {
            signing_key,
            role_store,
            collusion_store,
            nonce_store,
            audit_sink,
            region: region.into(),
        }
    }
}

/// Build and emit a denial audit event. Ignores emit errors (denial is
/// already propagated by the caller).
fn emit_denial(
    gate: &DualApprovalGateImpl,
    req: &AdminOpRequest,
    outcome: ApprovalOutcome,
    now_ms: u64,
    mfa_ts_ms: u64,
) {
    let op_id = Uuid::now_v7();
    let payload_hash = sha256_32(&req.op_payload);
    let event: AdminOpCloudEvent = AdminOpCloudEventBuilder {
        op_id,
        region: gate.region.clone(),
        outcome,
        caller: ActorIdentity::new(req.caller_user_id, ""),
        approver: ActorIdentity::new(req.approver_user_id, ""),
        mfa_ts_ms,
        op_type: req.op_type.clone(),
        op_payload_hash: payload_hash,
        prev_state_hash: [0u8; 32],
        nonce: req.nonce,
        hmac_chain_sig: [0u8; 32],
        now_ms,
    }
    .build();
    let _ = gate.audit_sink.emit(event);
}

/// SHA-256 of a byte slice → 32-byte array.
fn sha256_32(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

impl DualApprovalGate for DualApprovalGateImpl {
    fn verify(
        &self,
        req: &AdminOpRequest,
        caller_mfa_ts_ms: u64,
        now_ms: u64,
    ) -> Result<VerifiedApproval, DualApprovalError> {
        // ── 1. Clock-skew check ─────────────────────────────────────────
        let skew = now_ms.abs_diff(req.ts_ms);
        if skew > CLOCK_SKEW_MAX_MS {
            emit_denial(
                self,
                req,
                ApprovalOutcome::DeniedClockSkew,
                now_ms,
                caller_mfa_ts_ms,
            );
            return Err(DualApprovalError::ClockSkew {
                req_ms: req.ts_ms,
                srv_ms: now_ms,
            });
        }

        // ── 2. MFA freshness check ──────────────────────────────────────
        let mfa_age_ms = now_ms.saturating_sub(caller_mfa_ts_ms);
        if mfa_age_ms > MFA_MAX_AGE_MS {
            emit_denial(
                self,
                req,
                ApprovalOutcome::DeniedMfaStale,
                now_ms,
                caller_mfa_ts_ms,
            );
            let age_min = (mfa_age_ms / 60_000) as u32;
            return Err(DualApprovalError::MfaStale { age_min });
        }

        // ── 3. Separation of duties: caller ≠ approver ─────────────────
        // INV-ADMIN-DUAL-APPROVAL CRITICAL — enforced unconditionally.
        if req.caller_user_id == req.approver_user_id {
            emit_denial(
                self,
                req,
                ApprovalOutcome::DeniedCallerEq,
                now_ms,
                caller_mfa_ts_ms,
            );
            return Err(DualApprovalError::CallerEqualsApprover);
        }

        // ── 4. Approver admin role check ────────────────────────────────
        if !self.role_store.is_admin(req.approver_user_id) {
            emit_denial(
                self,
                req,
                ApprovalOutcome::DeniedApproverNotAdmin,
                now_ms,
                caller_mfa_ts_ms,
            );
            return Err(DualApprovalError::ApproverNotAdmin);
        }

        // ── 5. HMAC signature verify (constant-time) ────────────────────
        verify_hmac(
            &self.signing_key,
            &req.op_payload,
            &req.nonce,
            req.ts_ms,
            &req.approver_signature,
        )
        .inspect_err(|_| {
            emit_denial(
                self,
                req,
                ApprovalOutcome::DeniedSig,
                now_ms,
                caller_mfa_ts_ms,
            );
        })?;

        // ── 6. Collusion-rotation check (destructive ops only) ──────────
        if req.op_type.is_destructive() {
            self.collusion_store
                .check_collusion(req.tenant_id, req.approver_user_id, now_ms)
                .inspect_err(|_| {
                    emit_denial(
                        self,
                        req,
                        ApprovalOutcome::DeniedCollusion,
                        now_ms,
                        caller_mfa_ts_ms,
                    );
                })?;
        }

        // ── 7. Nonce replay protection ──────────────────────────────────
        self.nonce_store
            .check_and_record(req.caller_user_id, req.nonce, now_ms)
            .inspect_err(|_| {
                emit_denial(
                    self,
                    req,
                    ApprovalOutcome::DeniedNonceReplay,
                    now_ms,
                    caller_mfa_ts_ms,
                );
            })?;

        // ── 8. Success: compute prev_state_hash + emit audit ────────────
        let op_payload_hash = sha256_32(&req.op_payload);
        let prev_state_hash = sha256_32(&op_payload_hash);
        let hmac_chain_sig = [0u8; 32]; // Production: HMAC of canonical audit bytes.

        let op_id = Uuid::now_v7();
        let event: AdminOpCloudEvent = AdminOpCloudEventBuilder {
            op_id,
            region: self.region.clone(),
            outcome: ApprovalOutcome::Approved,
            caller: ActorIdentity::new(req.caller_user_id, ""),
            approver: ActorIdentity::new(req.approver_user_id, ""),
            mfa_ts_ms: caller_mfa_ts_ms,
            op_type: req.op_type.clone(),
            op_payload_hash,
            prev_state_hash,
            nonce: req.nonce,
            hmac_chain_sig,
            now_ms,
        }
        .build();

        // Fail-CLOSED: if audit emit fails, deny the op.
        self.audit_sink
            .emit(event)
            .map_err(|e| DualApprovalError::Internal(e.to_string()))?;

        // Record approval for collusion-rotation tracking (only destructive).
        if req.op_type.is_destructive() {
            self.collusion_store.record_approval(
                req.tenant_id,
                req.approver_user_id,
                &req.op_type,
                now_ms,
            )?;
        }

        Ok(VerifiedApproval {
            caller_user_id: req.caller_user_id,
            approver_user_id: req.approver_user_id,
            mfa_ts_ms: caller_mfa_ts_ms,
            op_type: req.op_type.clone(),
            prev_state_hash,
            op_payload: req.op_payload.clone(),
        })
    }
}

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k for
/// PR gate; nightly job overrides to 100k.
pub fn proptest_cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

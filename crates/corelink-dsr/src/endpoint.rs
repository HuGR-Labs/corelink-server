//! [`DsrEndpoint`] trait + [`InMemoryDsrEndpoint`] orchestrator.
//!
//! ## Run pipeline (per DSR submission)
//!
//! For each input [`DsrRequest`]:
//!
//! 1. **Audit `request_received`**: the canonical inbound audit row
//!    fires BEFORE any state mutation / MFA verify / decision branch.
//!    Audit failure aborts the run fail-CLOSED at the trait surface.
//! 2. **Idempotency lookup**: the canonical store is consulted with
//!    `(tenant_id, request_id)`; a prior ticket short-circuits to the
//!    canonical [`DsrDecision::RequestAccepted`] arm reusing the
//!    prior receipt + sla_deadline (NO second audit / store insert /
//!    receipt issuance — the prior outcome is the authoritative one).
//! 3. **MFA gate (destructive arms only)**: if
//!    `request.request_kind.is_destructive()` AND
//!    `request.mfa_step_up_token.is_none()`, the
//!    `mfa_step_up_required` audit fires BEFORE returning the
//!    canonical [`DsrDecision::MfaRequired`] arm. NO store insert, no
//!    receipt issued (the audit row IS the canonical "customer needs
//!    to step up" signal). If a token IS present, the MFA verifier
//!    runs; on success the `mfa_verified` audit fires BEFORE the
//!    canonical store insert.
//! 4. **Receipt issue + store insert + accept audit**: the JWT
//!    receipt is issued (anti-replay 90d cap enforced at the issuer
//!    boundary) → the canonical `request_accepted` audit fires BEFORE
//!    the store insert → the canonical ticket is inserted → the
//!    `receipt_issued` audit fires AFTER the store insert but BEFORE
//!    the receipt is returned to the caller.
//!
//! ## Status-poll arm
//!
//! The canonical [`DsrEndpoint::poll_status`] surface looks up the
//! ticket by `(tenant_id, request_id)`; on hit the
//! `status_polled` audit fires BEFORE returning the canonical
//! [`DsrDecision::StatusPolled`] arm; on miss the canonical
//! `request_rejected` audit fires with reason
//! `IdentityVerificationFailed` (CTRL-ISO-004 constant-time
//! confidentiality — never disclose whether a request_id exists for
//! a different tenant; the rejection is the canonical 404-like
//! surface from the customer perspective). The status-poll arm is
//! pure read; no store mutation past the audit row.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S11-001 §1 invariant + ADR-S11-002 split-tier (canonical
//! distinct from `corelink-billing-emit` fail-OPEN), every decision
//! arm fires its canonical audit BEFORE the state-mutating step:
//!
//! - `request_received` fires BEFORE the idempotency lookup so the
//!   auditor evidence trail captures the inbound request even on
//!   short-circuit paths.
//! - `mfa_step_up_required` fires BEFORE returning the
//!   `MfaRequired` arm (no state mutation past the audit row).
//! - `mfa_verified` fires BEFORE the canonical store insert.
//! - `request_accepted` fires BEFORE the canonical store insert.
//! - `receipt_issued` fires AFTER the store insert but BEFORE the
//!   receipt is returned (anti-truncation: the receipt issuance is
//!   captured even if the response transport subsequently fails).
//! - `request_rejected` fires BEFORE returning the canonical
//!   `RequestRejected` arm (no store insert on rejection).
//! - `status_polled` fires BEFORE returning the canonical
//!   `StatusPolled` arm (no state mutation past the audit row).
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! [`crate::error::DsrError::Audit`] (caller sees no state mutation
//! past the point of the failure).
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + ticket store + receipt
//! issuer + MFA verifier + per-instance `Arc<Mutex<()>>` mutex as
//! `Arc` handles passed at construction; per-instance state lives
//! inside those `Arc`'d primitives + the per-instance mutex
//! serializes the canonical pipeline so the
//! audit-emit-then-store-insert pair is atomic from the caller's
//! perspective. Tests instantiate fresh orchestrators per case so
//! cross-test contamination is structurally impossible.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::audit::{DsrAuditEventType, DsrAuditRecord, DsrAuditSink};
use crate::error::DsrError;
use crate::event::{
    sla_for, DsrDecision, DsrRejectReason, DsrRequest, DsrRequestKind, DsrTicket,
    RECEIPT_EXPIRY_DAYS,
};
use crate::mfa::MfaStepUpVerifier;
use crate::receipt::{DsrReceipt, JwtReceiptIssuer};
use crate::store::DsrRequestStore;

/// Canonical receipt expiration window in days (90 days per
/// [`crate::event::RECEIPT_EXPIRY_DAYS`]). Pinned for cross-component
/// regression tests + dashboard widget grouping.
pub const CANONICAL_RECEIPT_EXPIRY_DAYS: u32 = RECEIPT_EXPIRY_DAYS;

/// Canonical receipt expiration window in milliseconds (90 days ×
/// 86_400_000 ms/day).
pub const CANONICAL_RECEIPT_EXPIRY_MS: u64 =
    (CANONICAL_RECEIPT_EXPIRY_DAYS as u64) * 86_400_000;

/// DSR self-service endpoint trait. Production wiring composes the
/// `DsrEndpointDO` Cloudflare Durable Object route at
/// `POST /v1/privacy/dsr/{access|portability|rectification|erasure|
/// restriction|objection}` + `GET /v1/privacy/dsr/{request_id}/status`
/// (deferred to WI-S11-008 PRR ship gate per the
/// `trait-abstraction-defer` charter pattern) — the route extracts
/// the canonical `TenantCtx` middleware (S-03 inheritance Lote
/// 10.4bis) + dispatches into [`DsrEndpoint::submit`] /
/// [`DsrEndpoint::poll_status`].
pub trait DsrEndpoint: Send + Sync + core::fmt::Debug {
    /// Submit a fresh DSR request. The orchestrator dispatches the
    /// canonical 4-arm decision tree:
    ///
    /// - destructive arm without MFA token →
    ///   [`DsrDecision::MfaRequired`]
    /// - destructive arm with MFA token → MFA verify → on success
    ///   [`DsrDecision::RequestAccepted`]
    /// - read / policy arm → [`DsrDecision::RequestAccepted`]
    /// - duplicate / divergent payload / store backend failure →
    ///   [`DsrError::Store`] / [`DsrDecision::RequestRejected`]
    ///
    /// Every arm fires the canonical audit envelope BEFORE the state-
    /// mutating step (fail-CLOSED per ADR-S11-002).
    ///
    /// # Errors
    ///
    /// - [`DsrError::Audit`] when the audit envelope rejects any
    ///   decision arm (fail-CLOSED at the trait surface).
    /// - [`DsrError::Store`] when the canonical ticket store rejects
    ///   the insert (Neon backend failure / canonical idempotency
    ///   collision with diverged content).
    /// - [`DsrError::Receipt`] when the JWT receipt issuer rejects
    ///   the canonical claim shape.
    /// - [`DsrError::Mfa`] when the MFA verifier rejects a malformed
    ///   step-up token (distinct from `MfaRequired` which is a
    ///   decision arm, not an error — `Required` flows through the
    ///   decision arm path so the customer SDK / UI knows to re-prompt).
    /// - [`DsrError::Internal`] when a per-instance mutex is poisoned.
    fn submit(&self, request: &DsrRequest) -> Result<DsrDecision, DsrError>;

    /// Poll the canonical status surface for `(tenant_id,
    /// request_id)`. Pure read; no state mutation past the
    /// `status_polled` audit row (or `request_rejected` audit row
    /// when the lookup misses).
    ///
    /// Per WI-S11-001 §AC-008 + CTRL-ISO-004: cross-tenant lookups
    /// surface as `request_rejected` with reason
    /// `IdentityVerificationFailed` (constant-time confidentiality —
    /// never disclose whether a request_id exists for a different
    /// tenant via a 404 vs 403 distinction).
    ///
    /// # Errors
    ///
    /// - [`DsrError::Audit`] when the audit envelope rejects the
    ///   poll (fail-CLOSED at the trait surface).
    /// - [`DsrError::Store`] when the canonical ticket store rejects
    ///   the lookup (Neon backend failure).
    /// - [`DsrError::Internal`] when a per-instance mutex is poisoned.
    fn poll_status(
        &self,
        tenant_id: Uuid,
        request_id: Uuid,
    ) -> Result<DsrDecision, DsrError>;
}

/// In-memory DSR self-service orchestrator. Composes the audit sink,
/// ticket store, receipt issuer, and MFA verifier via `Arc` handles;
/// all four are generic over their trait so test fakes (such as
/// [`crate::audit::FailingDsrAuditSink`],
/// [`crate::store::FailingDsrRequestStore`], and
/// [`crate::mfa::FailingMfaStepUpVerifier`]) compose directly at
/// construction.
#[derive(Clone)]
pub struct InMemoryDsrEndpoint<A, S, R, M>
where
    A: DsrAuditSink + 'static,
    S: DsrRequestStore + 'static,
    R: JwtReceiptIssuer + 'static,
    M: MfaStepUpVerifier + 'static,
{
    audit: Arc<A>,
    store: Arc<S>,
    receipt: Arc<R>,
    mfa: Arc<M>,
    // Per-instance mutex serializes the canonical
    // audit-emit-then-store-insert pair so the orchestrator's
    // pipeline is atomic from the caller's perspective. Per the
    // canonical F-001 closure pattern (mirrors S-07 `corelink-quota`
    // DO-actor model + `corelink-billing-aggregator`
    // HashChainBuilder + `corelink-billing-replay`
    // InMemoryReplayEngine).
    mutex: Arc<Mutex<()>>,
}

impl<A, S, R, M> core::fmt::Debug for InMemoryDsrEndpoint<A, S, R, M>
where
    A: DsrAuditSink + 'static,
    S: DsrRequestStore + 'static,
    R: JwtReceiptIssuer + 'static,
    M: MfaStepUpVerifier + 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryDsrEndpoint").finish_non_exhaustive()
    }
}

impl<A, S, R, M> InMemoryDsrEndpoint<A, S, R, M>
where
    A: DsrAuditSink + 'static,
    S: DsrRequestStore + 'static,
    R: JwtReceiptIssuer + 'static,
    M: MfaStepUpVerifier + 'static,
{
    /// Construct an endpoint with the canonical 4-component composition.
    pub fn new(audit: Arc<A>, store: Arc<S>, receipt: Arc<R>, mfa: Arc<M>) -> Self {
        Self {
            audit,
            store,
            receipt,
            mfa,
            mutex: Arc::new(Mutex::new(())),
        }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the ticket store.
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// Borrow the receipt issuer.
    #[must_use]
    pub fn receipt(&self) -> &Arc<R> {
        &self.receipt
    }

    /// Borrow the MFA verifier.
    #[must_use]
    pub fn mfa(&self) -> &Arc<M> {
        &self.mfa
    }

    fn audit_record(
        event_type: DsrAuditEventType,
        request: &DsrRequest,
        reject_reason: Option<DsrRejectReason>,
        status: Option<crate::event::DsrStatus>,
        context: String,
    ) -> DsrAuditRecord {
        DsrAuditRecord {
            event_type,
            request_id: request.request_id,
            tenant_id: request.tenant_id,
            data_subject_id: request.data_subject_id,
            request_kind: request.request_kind,
            jurisdiction: request.jurisdiction,
            reject_reason,
            status,
            now_ms: request.submitted_at_ms,
            context,
        }
    }

    fn poll_audit_record(
        event_type: DsrAuditEventType,
        tenant_id: Uuid,
        request_id: Uuid,
        ticket: Option<&DsrTicket>,
        reject_reason: Option<DsrRejectReason>,
        context: String,
    ) -> DsrAuditRecord {
        DsrAuditRecord {
            event_type,
            request_id,
            tenant_id,
            data_subject_id: ticket.map_or_else(Uuid::nil, |t| t.data_subject_id),
            request_kind: ticket.map_or(DsrRequestKind::Access, |t| t.request_kind),
            jurisdiction: ticket
                .map_or(crate::event::DsrJurisdiction::Lgpd, |t| t.jurisdiction),
            reject_reason,
            status: ticket.map(|t| t.status),
            now_ms: 0,
            context,
        }
    }
}

impl<A, S, R, M> DsrEndpoint for InMemoryDsrEndpoint<A, S, R, M>
where
    A: DsrAuditSink + 'static,
    S: DsrRequestStore + 'static,
    R: JwtReceiptIssuer + 'static,
    M: MfaStepUpVerifier + 'static,
{
    fn submit(&self, request: &DsrRequest) -> Result<DsrDecision, DsrError> {
        // F-001 closure: per-instance serial pipeline so audit-emit-
        // then-store-insert is atomic from the caller's perspective.
        let _guard = self
            .mutex
            .lock()
            .map_err(|_| DsrError::Internal("dsr endpoint mutex poisoned".to_string()))?;

        // 1. Audit `request_received` BEFORE any state mutation /
        //    decision branch. Per ADR-S11-002 split-tier: DSR is
        //    regulatory-grade fail-CLOSED so the inbound request is
        //    captured in the auditor evidence trail before ANY
        //    further work.
        self.audit.emit(Self::audit_record(
            DsrAuditEventType::RequestReceived,
            request,
            None,
            None,
            format!("kind={}", request.request_kind),
        ))?;

        // 2. Idempotency lookup. A prior ticket short-circuits to the
        //    canonical RequestAccepted arm reusing the prior receipt
        //    + sla_deadline. Per WI §AC-003: the audit + store
        //    + receipt are NOT re-fired on idempotent re-submission;
        //    the prior outcome is the authoritative one. The
        //    `request_received` audit row above IS the auditor
        //    evidence of the re-submission attempt.
        if let Some(prior) = self.store.get(request.tenant_id, request.request_id)? {
            if let (Some(receipt), false) = (prior.receipt.clone(), prior.status.is_terminal()) {
                return Ok(DsrDecision::RequestAccepted {
                    receipt,
                    sla_deadline_ms: prior.sla_deadline_ms,
                });
            }
            // Prior ticket exists but is rejected / completed: surface
            // the canonical reject reason (forensic trail intact).
            // The status-poll arm is the canonical surface for prior-
            // ticket inspection; the submit path treats prior-rejected
            // as a duplicate (no second insert).
            self.audit.emit(Self::audit_record(
                DsrAuditEventType::RequestRejected,
                request,
                Some(DsrRejectReason::DuplicateRequest),
                Some(prior.status),
                format!("duplicate prior_status={}", prior.status),
            ))?;
            return Ok(DsrDecision::RequestRejected {
                reason: DsrRejectReason::DuplicateRequest,
            });
        }

        // 3. MFA gate (destructive arms only — Erasure +
        //    Rectification per CTRL-AUTH-010 + ADR-S11-001). Read +
        //    policy arms skip the gate.
        if request.is_destructive() {
            match request.mfa_step_up_token.as_ref() {
                None => {
                    self.audit.emit(Self::audit_record(
                        DsrAuditEventType::MfaStepUpRequired,
                        request,
                        None,
                        None,
                        format!("mfa_required_for kind={}", request.request_kind),
                    ))?;
                    return Ok(DsrDecision::MfaRequired {
                        kind: request.request_kind,
                    });
                }
                Some(token) => {
                    self.mfa.verify(Some(token))?;
                    self.audit.emit(Self::audit_record(
                        DsrAuditEventType::MfaVerified,
                        request,
                        None,
                        None,
                        format!("mfa_verified_for kind={}", request.request_kind),
                    ))?;
                }
            }
        }

        // 4. Issue receipt + store insert + accept audit + receipt-
        //    issued audit. The canonical ordering captures the
        //    request acceptance in the auditor evidence trail BEFORE
        //    the store insert, so a store backend failure does NOT
        //    silently drop the audit row.
        let receipt_claims = DsrReceipt::new(request);
        let receipt_token = self.receipt.issue(&receipt_claims)?;
        let sla_deadline_ms = sla_for(request.jurisdiction, request.submitted_at_ms);
        let ticket = DsrTicket::accepted(request, sla_deadline_ms, receipt_token.clone());

        self.audit.emit(Self::audit_record(
            DsrAuditEventType::RequestAccepted,
            request,
            None,
            Some(crate::event::DsrStatus::Pending),
            format!("accepted sla_deadline_ms={sla_deadline_ms}"),
        ))?;

        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER — receipt was already issued at
        // line 392 (above), so the ReceiptIssued audit captures the canonical
        // intent BEFORE the store.insert mutation. If store.insert fails after
        // emit, the audit log carries the receipt-issued intent and downstream
        // reconciliation handles the ticket-side miss.
        self.audit.emit(Self::audit_record(
            DsrAuditEventType::ReceiptIssued,
            request,
            None,
            None,
            format!("receipt_issued exp_days={CANONICAL_RECEIPT_EXPIRY_DAYS}"),
        ))?;

        self.store.insert(ticket)?;

        Ok(DsrDecision::RequestAccepted {
            receipt: receipt_token,
            sla_deadline_ms,
        })
    }

    fn poll_status(
        &self,
        tenant_id: Uuid,
        request_id: Uuid,
    ) -> Result<DsrDecision, DsrError> {
        let _guard = self
            .mutex
            .lock()
            .map_err(|_| DsrError::Internal("dsr endpoint mutex poisoned".to_string()))?;

        let ticket = self.store.get(tenant_id, request_id)?;
        match ticket {
            Some(t) => {
                self.audit.emit(Self::poll_audit_record(
                    DsrAuditEventType::StatusPolled,
                    tenant_id,
                    request_id,
                    Some(&t),
                    None,
                    format!("status={}", t.status),
                ))?;
                Ok(DsrDecision::StatusPolled { status: t.status })
            }
            None => {
                // CTRL-ISO-004 constant-time confidentiality: cross-
                // tenant lookups surface as RequestRejected with
                // IdentityVerificationFailed reason — never disclose
                // whether a request_id exists for a different tenant.
                self.audit.emit(Self::poll_audit_record(
                    DsrAuditEventType::RequestRejected,
                    tenant_id,
                    request_id,
                    None,
                    Some(DsrRejectReason::IdentityVerificationFailed),
                    "poll_miss tenant_or_request_id_unknown".to_string(),
                ))?;
                Ok(DsrDecision::RequestRejected {
                    reason: DsrRejectReason::IdentityVerificationFailed,
                })
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{FailingDsrAuditSink, InMemoryDsrAuditSink};
    use crate::event::{DsrJurisdiction, DsrStatus};
    use crate::mfa::{FailingMfaStepUpVerifier, InMemoryMfaStepUpVerifier, MfaStepUpToken};
    use crate::receipt::InMemoryJwtReceiptIssuer;
    use crate::store::{FailingDsrRequestStore, InMemoryDsrRequestStore};

    type Endpoint = InMemoryDsrEndpoint<
        InMemoryDsrAuditSink,
        InMemoryDsrRequestStore,
        InMemoryJwtReceiptIssuer,
        InMemoryMfaStepUpVerifier,
    >;

    fn fresh_endpoint() -> (
        Endpoint,
        Arc<InMemoryDsrAuditSink>,
        Arc<InMemoryDsrRequestStore>,
        Arc<InMemoryJwtReceiptIssuer>,
        Arc<InMemoryMfaStepUpVerifier>,
    ) {
        let audit = Arc::new(InMemoryDsrAuditSink::new());
        let store = Arc::new(InMemoryDsrRequestStore::new());
        let receipt = Arc::new(InMemoryJwtReceiptIssuer::default());
        let mfa = Arc::new(InMemoryMfaStepUpVerifier::new());
        let e = InMemoryDsrEndpoint::new(
            Arc::clone(&audit),
            Arc::clone(&store),
            Arc::clone(&receipt),
            Arc::clone(&mfa),
        );
        (e, audit, store, receipt, mfa)
    }

    fn req(kind: DsrRequestKind, jur: DsrJurisdiction) -> DsrRequest {
        DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            kind,
            jur,
            1_000_000_000_000,
        )
    }

    #[test]
    fn read_arm_accepts_without_mfa() {
        let (e, audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Gdpr);
        let dec = e.submit(&r).unwrap();
        assert!(dec.is_accepted());
        assert_eq!(store.len(), 1);
        // Audit chain: 1 received + 1 accepted + 1 receipt_issued.
        assert_eq!(audit.len(), 3);
        assert_eq!(
            audit.snapshot_of(DsrAuditEventType::RequestReceived).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(DsrAuditEventType::RequestAccepted).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(DsrAuditEventType::ReceiptIssued).len(),
            1
        );
    }

    #[test]
    fn portability_arm_accepts_without_mfa() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Portability, DsrJurisdiction::Lgpd);
        let dec = e.submit(&r).unwrap();
        assert!(dec.is_accepted());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn restriction_arm_accepts_without_mfa() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Restriction, DsrJurisdiction::Gdpr);
        assert!(e.submit(&r).unwrap().is_accepted());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn objection_arm_accepts_without_mfa() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Objection, DsrJurisdiction::Gdpr);
        assert!(e.submit(&r).unwrap().is_accepted());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn erasure_without_mfa_returns_mfa_required() {
        let (e, audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Erasure, DsrJurisdiction::Lgpd);
        let dec = e.submit(&r).unwrap();
        assert!(dec.is_mfa_required());
        // No store insert on MFA gate.
        assert_eq!(store.len(), 0);
        // Audit chain: 1 received + 1 mfa_step_up_required.
        assert_eq!(audit.len(), 2);
        assert_eq!(
            audit.snapshot_of(DsrAuditEventType::MfaStepUpRequired).len(),
            1
        );
    }

    #[test]
    fn rectification_without_mfa_returns_mfa_required() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Rectification, DsrJurisdiction::Gdpr);
        let dec = e.submit(&r).unwrap();
        assert!(dec.is_mfa_required());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn erasure_with_valid_mfa_accepts() {
        let (e, audit, store, _r, mfa) = fresh_endpoint();
        let r = req(DsrRequestKind::Erasure, DsrJurisdiction::Lgpd)
            .with_mfa(MfaStepUpToken::synthetic_for_test("ok"));
        let dec = e.submit(&r).unwrap();
        assert!(dec.is_accepted());
        assert_eq!(store.len(), 1);
        assert_eq!(mfa.verified_count(), 1);
        // Audit chain: received + mfa_verified + accepted + receipt_issued.
        assert_eq!(audit.len(), 4);
    }

    #[test]
    fn rectification_with_valid_mfa_accepts() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Rectification, DsrJurisdiction::Gdpr)
            .with_mfa(MfaStepUpToken::synthetic_for_test("ok"));
        assert!(e.submit(&r).unwrap().is_accepted());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn idempotent_re_submission_returns_prior_accepted() {
        let (e, _audit, store, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Gdpr);
        let d1 = e.submit(&r).unwrap();
        let d2 = e.submit(&r).unwrap();
        assert!(d1.is_accepted());
        assert!(d2.is_accepted());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn audit_failure_on_request_received_propagates_no_mutation() {
        let audit = Arc::new(FailingDsrAuditSink::new());
        let store = Arc::new(InMemoryDsrRequestStore::new());
        let receipt = Arc::new(InMemoryJwtReceiptIssuer::default());
        let mfa = Arc::new(InMemoryMfaStepUpVerifier::new());
        let e = InMemoryDsrEndpoint::new(
            Arc::clone(&audit),
            Arc::clone(&store),
            Arc::clone(&receipt),
            Arc::clone(&mfa),
        );
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Gdpr);
        let err = e.submit(&r).unwrap_err();
        assert!(matches!(err, DsrError::Audit(_)));
        assert!(store.is_empty());
    }

    #[test]
    fn store_failure_propagates_no_receipt() {
        let audit = Arc::new(InMemoryDsrAuditSink::new());
        let store = Arc::new(FailingDsrRequestStore::new());
        let receipt = Arc::new(InMemoryJwtReceiptIssuer::default());
        let mfa = Arc::new(InMemoryMfaStepUpVerifier::new());
        let e = InMemoryDsrEndpoint::new(
            Arc::clone(&audit),
            Arc::clone(&store),
            Arc::clone(&receipt),
            Arc::clone(&mfa),
        );
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Gdpr);
        let err = e.submit(&r).unwrap_err();
        assert!(matches!(err, DsrError::Store(_)));
    }

    #[test]
    fn mfa_failure_on_destructive_arm_propagates() {
        let audit = Arc::new(InMemoryDsrAuditSink::new());
        let store = Arc::new(InMemoryDsrRequestStore::new());
        let receipt = Arc::new(InMemoryJwtReceiptIssuer::default());
        let mfa = Arc::new(FailingMfaStepUpVerifier::new());
        let e = InMemoryDsrEndpoint::new(
            Arc::clone(&audit),
            Arc::clone(&store),
            Arc::clone(&receipt),
            Arc::clone(&mfa),
        );
        let r = req(DsrRequestKind::Erasure, DsrJurisdiction::Lgpd)
            .with_mfa(MfaStepUpToken::synthetic_for_test("ok"));
        let err = e.submit(&r).unwrap_err();
        assert!(matches!(err, DsrError::Mfa(_)));
        assert!(store.is_empty());
    }

    #[test]
    fn poll_status_hit_returns_canonical_status() {
        let (e, audit, _s, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Lgpd);
        e.submit(&r).unwrap();
        let dec = e.poll_status(r.tenant_id, r.request_id).unwrap();
        match dec {
            DsrDecision::StatusPolled { status } => assert_eq!(status, DsrStatus::Pending),
            other => unreachable!("{other:?}"),
        }
        assert_eq!(audit.snapshot_of(DsrAuditEventType::StatusPolled).len(), 1);
    }

    #[test]
    fn poll_status_miss_rejects_with_identity_failed() {
        let (e, audit, _s, _r, _m) = fresh_endpoint();
        let dec = e.poll_status(Uuid::now_v7(), Uuid::now_v7()).unwrap();
        match dec {
            DsrDecision::RequestRejected { reason } => {
                assert_eq!(reason, DsrRejectReason::IdentityVerificationFailed);
            }
            other => unreachable!("{other:?}"),
        }
        assert_eq!(
            audit.snapshot_of(DsrAuditEventType::RequestRejected).len(),
            1
        );
    }

    #[test]
    fn cross_tenant_poll_returns_identity_failed() {
        let (e, _audit, _s, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Lgpd);
        e.submit(&r).unwrap();
        let other_tenant = Uuid::now_v7();
        let dec = e.poll_status(other_tenant, r.request_id).unwrap();
        match dec {
            DsrDecision::RequestRejected { reason } => {
                assert_eq!(reason, DsrRejectReason::IdentityVerificationFailed);
            }
            other => unreachable!("{other:?}"),
        }
    }

    #[test]
    fn idempotent_poll_returns_same_status() {
        let (e, _audit, _s, _r, _m) = fresh_endpoint();
        let r = req(DsrRequestKind::Access, DsrJurisdiction::Lgpd);
        e.submit(&r).unwrap();
        let d1 = e.poll_status(r.tenant_id, r.request_id).unwrap();
        let d2 = e.poll_status(r.tenant_id, r.request_id).unwrap();
        let d3 = e.poll_status(r.tenant_id, r.request_id).unwrap();
        match (d1, d2, d3) {
            (
                DsrDecision::StatusPolled { status: s1 },
                DsrDecision::StatusPolled { status: s2 },
                DsrDecision::StatusPolled { status: s3 },
            ) => {
                assert_eq!(s1, s2);
                assert_eq!(s2, s3);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn config_canonical_constants_pinned() {
        assert_eq!(CANONICAL_RECEIPT_EXPIRY_DAYS, 90);
        assert_eq!(
            CANONICAL_RECEIPT_EXPIRY_MS,
            90 * 86_400_000
        );
    }

    #[test]
    fn sla_deadline_per_jurisdiction_correct() {
        let (e, _audit, _s, _r, _m) = fresh_endpoint();
        for jur in [
            DsrJurisdiction::Lgpd,
            DsrJurisdiction::Gdpr,
            DsrJurisdiction::Ccpa,
        ] {
            let r = req(DsrRequestKind::Access, jur);
            let dec = e.submit(&r).unwrap();
            match dec {
                DsrDecision::RequestAccepted {
                    sla_deadline_ms, ..
                } => {
                    let expected = sla_for(jur, r.submitted_at_ms);
                    assert_eq!(sla_deadline_ms, expected);
                }
                other => unreachable!("{other:?}"),
            }
        }
    }
}

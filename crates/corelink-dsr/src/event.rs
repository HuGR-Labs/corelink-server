//! Canonical DSR self-service types: [`DsrRequestKind`] +
//! [`DsrStatus`] + [`DsrDecision`] + [`DsrJurisdiction`] +
//! [`DsrRequest`] + [`DsrTicket`] + per-jurisdiction SLA helper
//! [`sla_for`].
//!
//! ## Why typed enums (NOT string discriminants)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S11-001 §1
//! invariant): the DSR submissions stored in the canonical Neon
//! `dsr_tickets` table (5y retention per privacy_model.md §2) must be
//! byte-deterministic across the auditor evidence trail. Untyped
//! `serde_json::Value` defeats compile-time taxonomy enforcement; the
//! typed enum + per-variant payload struct is the canonical Lote
//! 10.9-quinquies absorption pattern (mirrors `UsageEventKind`,
//! `AggregationDecision`, `StripeAdapterDecision`, `ReconcileDecision`,
//! `QuotaTransition`, `ReplayReason`).
//!
//! All public enums are `#[non_exhaustive]` so follow-on WIs can
//! extend the taxonomy additively without breaking downstream `match`
//! sites (e.g. WI-S11-002 erasure worker may add a `LegalHoldPaused`
//! status arm; WI-S11-008 PRR ship gate may add a
//! `BulkExportRequested` decision arm; S-14 BYOK may add a
//! `CryptoEraseInitiated` decision arm).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical 6-arm DSR request kind taxonomy. Per WI-S11-001 §1: every
/// DSR submission lands in exactly one of these 6 right categories
/// canonical per LGPD Art. 18 / GDPR Art. 15-22 / CCPA §1798.105/115/
/// 120/125.
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g. `Confirmation` for LGPD Art. 18 I — present in the wider WI
/// spec but not the 7-canonical-endpoint subset for WI-S11-001;
/// `ConsentRevoke` is intentionally OUT of the 6-arm trait surface
/// here because it lands as its own surface in WI-S11-003 consent
/// ledger).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DsrRequestKind {
    /// LGPD Art. 18 II / GDPR Art. 15 — read access. Read-only;
    /// destructive=false; SKIPS MFA step-up gate per ADR-S11-001.
    Access,
    /// LGPD Art. 18 V / GDPR Art. 20 — portable export (machine-
    /// readable JSON / CSV). Read-only; destructive=false; SKIPS MFA
    /// step-up gate per ADR-S11-001.
    Portability,
    /// LGPD Art. 18 III / GDPR Art. 16 — correction of inaccurate
    /// data. Destructive=true; REQUIRES MFA step-up gate per
    /// CTRL-AUTH-010 + ADR-S11-001 (the destructive arm pair with
    /// Erasure).
    Rectification,
    /// LGPD Art. 18 VI / GDPR Art. 17 — right to be forgotten.
    /// Destructive=true; REQUIRES MFA step-up gate per CTRL-AUTH-010 +
    /// ADR-S11-001 (THE canonical destructive arm; cross-backend
    /// erasure cascade lands at WI-S11-002 worker).
    Erasure,
    /// GDPR Art. 18 — restriction of processing. Policy-only;
    /// destructive=false (no data mutation; flag flips on tenant
    /// policy table); SKIPS MFA step-up gate.
    Restriction,
    /// GDPR Art. 21 — objection to processing. Policy-only;
    /// destructive=false; SKIPS MFA step-up gate.
    Objection,
}

impl DsrRequestKind {
    /// Canonical lower-snake-case mnemonic. Pinned for Neon CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Portability => "portability",
            Self::Rectification => "rectification",
            Self::Erasure => "erasure",
            Self::Restriction => "restriction",
            Self::Objection => "objection",
        }
    }

    /// Whether this arm is **destructive** (mutates data) and therefore
    /// requires an MFA step-up gate per CTRL-AUTH-010 + ADR-S11-001.
    /// Only Erasure + Rectification are destructive; the other 4 arms
    /// are read-only or policy-only.
    #[must_use]
    pub const fn is_destructive(self) -> bool {
        matches!(self, Self::Erasure | Self::Rectification)
    }
}

impl core::fmt::Display for DsrRequestKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 6-arm DSR request kind list. Pinned for cardinality
/// estimate + cross-component regression tests.
#[must_use]
pub const fn canonical_dsr_request_kinds() -> &'static [DsrRequestKind; 6] {
    &[
        DsrRequestKind::Access,
        DsrRequestKind::Portability,
        DsrRequestKind::Rectification,
        DsrRequestKind::Erasure,
        DsrRequestKind::Restriction,
        DsrRequestKind::Objection,
    ]
}

/// Helper: whether the canonical DSR request kind is destructive
/// (requires MFA step-up gate per ADR-S11-001).
#[must_use]
pub const fn is_destructive_arm(kind: DsrRequestKind) -> bool {
    kind.is_destructive()
}

/// Canonical 4-arm DSR status taxonomy. Per WI-S11-001 §1 + AC-008
/// status state machine subset (the WI-wide canonical 7-state machine
/// `received|verified|queued|in_progress|completed|denied|failed` is
/// the durable Neon mirror granularity; this 4-arm trait-surface
/// enum collapses verified/queued/in_progress under `InProgress` and
/// denied/failed under `Rejected` so the customer-visible status poll
/// surface stays minimal).
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g. WI-S11-002 may add `LegalHoldPaused` for the cross-backend
/// erasure cascade).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DsrStatus {
    /// Submitted but not yet picked up by the processing worker.
    /// Maps to durable Neon `received` (post-submit pre-MFA) +
    /// `verified` (post-MFA pre-queue).
    Pending,
    /// In active processing (queued + in_progress at the durable
    /// state machine granularity). Customer poll surface collapses
    /// these.
    InProgress,
    /// Successfully completed; for Access / Portability the canonical
    /// receipt URL is bound to the ticket.
    Completed,
    /// Denied (denied + failed at the durable state machine
    /// granularity). The customer poll surface collapses these so the
    /// reason is exposed via the canonical `reject_reason` field on
    /// the ticket rather than a separate status arm. Canonical wire
    /// name is `denied` (LGPD Art. 18 §1 / GDPR Art. 12.5(b) language
    /// + matches D1 `dsr_tickets.status` CHECK constraint). Sprint-
    /// close P1-3 fix: prior `rejected` serde rename diverged from
    /// the canonical wire contract.
    #[serde(rename = "denied")]
    Rejected,
}

impl DsrStatus {
    /// Canonical lower-snake-case mnemonic. Wire name `denied` (not
    /// `rejected`) per LGPD Art. 18 §1 / GDPR Art. 12.5(b) and D1
    /// CHECK constraint. Variant name kept as `Rejected` for binary
    /// API stability; wire output is `denied`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Rejected => "denied",
        }
    }

    /// Whether this arm is terminal (no further state transitions).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Rejected)
    }
}

impl core::fmt::Display for DsrStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 4-arm DSR status list.
#[must_use]
pub const fn canonical_dsr_statuses() -> &'static [DsrStatus; 4] {
    &[
        DsrStatus::Pending,
        DsrStatus::InProgress,
        DsrStatus::Completed,
        DsrStatus::Rejected,
    ]
}

/// Canonical 4-arm DSR decision taxonomy. Per WI-S11-001 §1: every
/// DSR endpoint invocation lands in exactly one of these arms; the
/// canonical audit envelope `corelink.dsr.<arm>` fires BEFORE the
/// state mutation on each arm.
///
/// `#[non_exhaustive]` reserves additive growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum DsrDecision {
    /// Authorization passed + (if destructive) MFA step-up verified +
    /// store insert succeeded + receipt issued. The canonical
    /// `request_accepted` + `receipt_issued` audit rows fired BEFORE
    /// returning.
    RequestAccepted {
        /// The canonical receipt token (JWT compact form). Customer
        /// stores this as proof of submission verifiable post-facto.
        receipt: super::receipt::JwtReceiptToken,
        /// The canonical SLA deadline (Unix epoch ms) per
        /// jurisdiction.
        sla_deadline_ms: u64,
    },
    /// Authorization REJECTED: rate-limit / unsupported kind /
    /// jurisdiction mismatch / etc. The canonical `request_rejected`
    /// audit row fires BEFORE returning.
    RequestRejected {
        /// The canonical reject reason mnemonic.
        reason: DsrRejectReason,
    },
    /// Destructive arm (Erasure / Rectification) submitted WITHOUT a
    /// step-up MFA token. The canonical `mfa_step_up_required` audit
    /// row fires BEFORE returning. The customer SDK / UI should
    /// re-submit after acquiring a step-up token via
    /// `corelink-webauthn::start_authentication(Ceremony::AdminStepUp)`.
    MfaRequired {
        /// The canonical request kind that triggered the gate.
        kind: DsrRequestKind,
    },
    /// Status poll arm: the canonical `status_polled` audit row fires
    /// BEFORE returning the snapshot. Pure read; no state mutation.
    StatusPolled {
        /// The canonical durable status snapshot.
        status: DsrStatus,
    },
}

impl DsrDecision {
    /// Canonical lower-snake-case mnemonic. Pinned for Neon CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RequestAccepted { .. } => "request_accepted",
            Self::RequestRejected { .. } => "request_rejected",
            Self::MfaRequired { .. } => "mfa_required",
            Self::StatusPolled { .. } => "status_polled",
        }
    }

    /// Whether this arm represents a successful acceptance.
    #[must_use]
    pub const fn is_accepted(&self) -> bool {
        matches!(self, Self::RequestAccepted { .. })
    }

    /// Whether this arm represents a rejection.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self, Self::RequestRejected { .. })
    }

    /// Whether this arm represents an MFA-required gate.
    #[must_use]
    pub const fn is_mfa_required(&self) -> bool {
        matches!(self, Self::MfaRequired { .. })
    }
}

/// Canonical reject-reason taxonomy. Per WI-S11-001 §6.1.7 + AC-005:
/// every rejection lands one of these mnemonics so the dashboard +
/// runbook surface filters cleanly.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DsrRejectReason {
    /// Rate-limit hit (10/day/subject per S-08 quota inheritance).
    /// Humane LGPD Art. 20 — the appeal endpoint routes to the
    /// Privacy Officer queue.
    RateLimit,
    /// Jurisdiction unsupported (the request_kind / jurisdiction
    /// pair has no canonical SLA mapping).
    JurisdictionMismatch,
    /// Identity verification failed (PAT principal does not match
    /// the data_subject_id; cross-tenant attack vector).
    IdentityVerificationFailed,
    /// Duplicate request (idempotency UNIQUE quad collision with a
    /// divergent payload — SEV-1 forensic signal).
    DuplicateRequest,
    /// Legal hold (the canonical privacy_model.md §6.1 F-11 pause
    /// arm; processing paused until legal review clears).
    LegalHold,
    /// Administrative override (Privacy Officer manual reject; rare;
    /// audit row carries the operator + rationale).
    AdminOverride,
}

impl DsrRejectReason {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RateLimit => "rate_limit",
            Self::JurisdictionMismatch => "jurisdiction_mismatch",
            Self::IdentityVerificationFailed => "identity_verification_failed",
            Self::DuplicateRequest => "duplicate_request",
            Self::LegalHold => "legal_hold",
            Self::AdminOverride => "admin_override",
        }
    }
}

impl core::fmt::Display for DsrRejectReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-arm jurisdiction taxonomy (LGPD / GDPR / CCPA). Each
/// jurisdiction has its own canonical response SLA per [`sla_for`].
///
/// `#[non_exhaustive]` reserves additive growth for follow-on
/// jurisdictions (e.g. PIPL China, LGPD-equivalent UK Data Protection
/// Act, Quebec Law 25).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DsrJurisdiction {
    /// Brazil — Lei Geral de Proteção de Dados (LGPD). Art. 19
    /// canonical SLA = 15 days for the controller response.
    Lgpd,
    /// EU — General Data Protection Regulation (GDPR). Art. 12.3
    /// canonical SLA = 1 month (extendable to 3 for complex requests;
    /// the trait-level deadline is the un-extended 30 days).
    Gdpr,
    /// California — CCPA + CPRA. §1798.130 canonical SLA = 45 days
    /// for the controller response.
    Ccpa,
}

impl DsrJurisdiction {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lgpd => "lgpd",
            Self::Gdpr => "gdpr",
            Self::Ccpa => "ccpa",
        }
    }
}

impl core::fmt::Display for DsrJurisdiction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-arm jurisdiction list.
#[must_use]
pub const fn canonical_dsr_jurisdictions() -> &'static [DsrJurisdiction; 3] {
    &[
        DsrJurisdiction::Lgpd,
        DsrJurisdiction::Gdpr,
        DsrJurisdiction::Ccpa,
    ]
}

/// LGPD Art. 19 canonical SLA: 15 days for the controller response.
pub const SLA_LGPD_DAYS: u32 = 15;

/// GDPR Art. 12.3 canonical SLA: 1 month (un-extended; the law
/// allows extension to 3 months for complex requests, but the
/// trait-level deadline is the un-extended 30 days so the system
/// fails-CLOSED on time-budget surprises).
pub const SLA_GDPR_DAYS: u32 = 30;

/// CCPA §1798.130 canonical SLA: 45 days.
pub const SLA_CCPA_DAYS: u32 = 45;

/// JWT receipt expiration window (anti-replay cap). Per WI-S11-001 §1:
/// receipts older than 90 days fail verification. The customer's
/// proof-of-submission token is short-lived; the canonical durable
/// audit trail (R2 audit Object Lock 7y) is the long-term forensic
/// record.
pub const RECEIPT_EXPIRY_DAYS: u32 = 90;

/// Compute the canonical SLA deadline (Unix epoch ms) for a given
/// (jurisdiction, submitted_at) pair. The trait-level deadline is the
/// un-extended canonical SLA per jurisdiction; production wiring at
/// WI-S11-008 may extend this for "complex" GDPR Art. 12.3 cases (cap
/// 3 months = 90 days; the extension is decided by the Privacy
/// Officer review path).
#[must_use]
pub const fn sla_for(jurisdiction: DsrJurisdiction, submitted_at_ms: u64) -> u64 {
    let days = match jurisdiction {
        DsrJurisdiction::Lgpd => SLA_LGPD_DAYS,
        DsrJurisdiction::Gdpr => SLA_GDPR_DAYS,
        DsrJurisdiction::Ccpa => SLA_CCPA_DAYS,
    };
    submitted_at_ms.saturating_add((days as u64) * 86_400_000)
}

/// Canonical DSR request input shape. Per WI-S11-001 §1: every DSR
/// submission carries an explicit `request_id` UUIDv7 (idempotency
/// key) + `tenant_id` (S-03 inheritance enforcement; never the request
/// body) + `data_subject_id` (PAT principal post-authn; pseudonymous
/// hash at the audit boundary per CTRL-PRIV-014) + `request_kind` +
/// `jurisdiction` + `submitted_at_ms` + optional `mfa_step_up_token`
/// (REQUIRED for destructive arms — Erasure + Rectification — per
/// CTRL-AUTH-010 + ADR-S11-001).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsrRequest {
    /// Canonical UUIDv7 request id (the idempotency ledger key;
    /// time-ordered for forensic trail localization).
    pub request_id: Uuid,
    /// Tenant id (canonical UUIDv7 from the S-03 auth middleware;
    /// NEVER the request body per Lote 10.4bis).
    pub tenant_id: Uuid,
    /// Data subject id (canonical UUIDv7; PAT principal post-authn).
    /// Per CTRL-PRIV-014 the audit boundary stores
    /// `sha256(subject_id || tenant_salt)` not the raw id.
    pub data_subject_id: Uuid,
    /// Canonical 6-arm request kind taxonomy.
    pub request_kind: DsrRequestKind,
    /// Canonical 3-arm jurisdiction taxonomy (drives the SLA deadline
    /// computation per [`sla_for`]).
    pub jurisdiction: DsrJurisdiction,
    /// Wall-clock instant the submission was accepted at the auth
    /// middleware (Unix epoch ms; canonical no `_ms` suffix per Lote
    /// 10.7bis P0-3 — the suffix is intentional here because the
    /// type is `u64` and the field name disambiguates from
    /// `*_at_seconds` / `*_at_micros` peers in the wider codebase).
    pub submitted_at_ms: u64,
    /// Optional MFA step-up token. REQUIRED for destructive arms
    /// (Erasure + Rectification) per CTRL-AUTH-010 + ADR-S11-001;
    /// MUST be `None` for read / policy arms (Access / Portability /
    /// Restriction / Objection).
    pub mfa_step_up_token: Option<super::mfa::MfaStepUpToken>,
}

impl DsrRequest {
    /// Construct a fresh DSR request.
    #[must_use]
    pub fn new(
        request_id: Uuid,
        tenant_id: Uuid,
        data_subject_id: Uuid,
        request_kind: DsrRequestKind,
        jurisdiction: DsrJurisdiction,
        submitted_at_ms: u64,
    ) -> Self {
        Self {
            request_id,
            tenant_id,
            data_subject_id,
            request_kind,
            jurisdiction,
            submitted_at_ms,
            mfa_step_up_token: None,
        }
    }

    /// Builder-style: attach an MFA step-up token (canonical for
    /// destructive arms — Erasure + Rectification).
    #[must_use]
    pub fn with_mfa(mut self, token: super::mfa::MfaStepUpToken) -> Self {
        self.mfa_step_up_token = Some(token);
        self
    }

    /// Whether this request is a destructive arm (Erasure +
    /// Rectification) per CTRL-AUTH-010 + ADR-S11-001.
    #[must_use]
    pub const fn is_destructive(&self) -> bool {
        self.request_kind.is_destructive()
    }
}

/// Canonical DSR ticket — the durable record stored in the Neon
/// `dsr_tickets` table mirror (cooperation with WI-S11-008 production
/// schema migration). The trait surface here ships the in-memory
/// shape; the real durable row carries the same logical fields plus
/// per-backend FK references + Object Lock retention metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsrTicket {
    /// Canonical UUIDv7 request id (PK).
    pub request_id: Uuid,
    /// Tenant id (S-03 inheritance — NEVER the request body).
    pub tenant_id: Uuid,
    /// Data subject id (PAT principal post-authn).
    pub data_subject_id: Uuid,
    /// Canonical 6-arm request kind taxonomy.
    pub request_kind: DsrRequestKind,
    /// Canonical 3-arm jurisdiction taxonomy.
    pub jurisdiction: DsrJurisdiction,
    /// Wall-clock instant of the canonical submission.
    pub submitted_at_ms: u64,
    /// Canonical SLA deadline (= submitted + sla_for(jurisdiction)).
    /// SEV-2 alert fires at the production wiring when `now >
    /// sla_deadline_ms AND status != Completed`.
    pub sla_deadline_ms: u64,
    /// Canonical 4-arm status taxonomy (collapsed customer-visible
    /// surface; durable Neon mirror has the wider 7-state machine
    /// granularity — the InProgress arm covers verified / queued /
    /// in_progress; the Rejected arm covers denied / failed).
    pub status: DsrStatus,
    /// The canonical receipt token issued at submission time
    /// (None on Rejected arm).
    pub receipt: Option<super::receipt::JwtReceiptToken>,
    /// Reject reason (None when status != Rejected).
    pub reject_reason: Option<DsrRejectReason>,
}

impl DsrTicket {
    /// Construct a fresh accepted ticket.
    #[must_use]
    pub fn accepted(
        request: &DsrRequest,
        sla_deadline_ms: u64,
        receipt: super::receipt::JwtReceiptToken,
    ) -> Self {
        Self {
            request_id: request.request_id,
            tenant_id: request.tenant_id,
            data_subject_id: request.data_subject_id,
            request_kind: request.request_kind,
            jurisdiction: request.jurisdiction,
            submitted_at_ms: request.submitted_at_ms,
            sla_deadline_ms,
            status: DsrStatus::Pending,
            receipt: Some(receipt),
            reject_reason: None,
        }
    }

    /// Construct a fresh rejected ticket.
    #[must_use]
    pub fn rejected(request: &DsrRequest, reason: DsrRejectReason) -> Self {
        Self {
            request_id: request.request_id,
            tenant_id: request.tenant_id,
            data_subject_id: request.data_subject_id,
            request_kind: request.request_kind,
            jurisdiction: request.jurisdiction,
            submitted_at_ms: request.submitted_at_ms,
            sla_deadline_ms: 0,
            status: DsrStatus::Rejected,
            receipt: None,
            reject_reason: Some(reason),
        }
    }

    /// Whether this ticket has missed its canonical SLA deadline at
    /// `now`. Production wiring fires SEV-2 when this returns true on
    /// a non-terminal status.
    #[must_use]
    pub const fn sla_missed(&self, now_ms: u64) -> bool {
        // Rejected tickets carry sla_deadline_ms = 0 (sentinel) so the
        // canonical SLA check skips them.
        self.sla_deadline_ms != 0 && now_ms > self.sla_deadline_ms && !self.status.is_terminal()
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

    #[test]
    fn dsr_request_kind_canonical_strings_unique() {
        let v = canonical_dsr_request_kinds();
        assert_eq!(v.len(), 6);
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()), "duplicate: {k}");
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn dsr_request_kind_destructive_arms_pinned() {
        assert!(DsrRequestKind::Erasure.is_destructive());
        assert!(DsrRequestKind::Rectification.is_destructive());
        assert!(!DsrRequestKind::Access.is_destructive());
        assert!(!DsrRequestKind::Portability.is_destructive());
        assert!(!DsrRequestKind::Restriction.is_destructive());
        assert!(!DsrRequestKind::Objection.is_destructive());
    }

    #[test]
    fn helper_is_destructive_arm_matches_method() {
        for k in canonical_dsr_request_kinds() {
            assert_eq!(is_destructive_arm(*k), k.is_destructive());
        }
    }

    #[test]
    fn dsr_status_canonical_strings_unique() {
        let v = canonical_dsr_statuses();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for s in v {
            assert!(set.insert(s.as_str()), "duplicate: {s}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn dsr_status_terminal_pinned() {
        assert!(DsrStatus::Completed.is_terminal());
        assert!(DsrStatus::Rejected.is_terminal());
        assert!(!DsrStatus::Pending.is_terminal());
        assert!(!DsrStatus::InProgress.is_terminal());
    }

    #[test]
    fn dsr_jurisdiction_canonical_strings_unique() {
        let v = canonical_dsr_jurisdictions();
        assert_eq!(v.len(), 3);
        let mut set = std::collections::HashSet::new();
        for j in v {
            assert!(set.insert(j.as_str()), "duplicate: {j}");
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn sla_for_lgpd_15d() {
        let submitted = 1_000_000_000_000_u64;
        let deadline = sla_for(DsrJurisdiction::Lgpd, submitted);
        // 15 days × 86_400_000 ms/day = 1_296_000_000 ms.
        assert_eq!(deadline, submitted + 1_296_000_000);
    }

    #[test]
    fn sla_for_gdpr_30d() {
        let submitted = 1_000_000_000_000_u64;
        let deadline = sla_for(DsrJurisdiction::Gdpr, submitted);
        assert_eq!(deadline, submitted + 30 * 86_400_000);
    }

    #[test]
    fn sla_for_ccpa_45d() {
        let submitted = 1_000_000_000_000_u64;
        let deadline = sla_for(DsrJurisdiction::Ccpa, submitted);
        assert_eq!(deadline, submitted + 45 * 86_400_000);
    }

    #[test]
    fn sla_for_saturating_at_overflow_boundary() {
        // Pin the canonical saturating_add semantic (no wrap, no panic).
        let submitted = u64::MAX - 100;
        let deadline = sla_for(DsrJurisdiction::Lgpd, submitted);
        assert_eq!(deadline, u64::MAX);
    }

    #[test]
    fn dsr_request_new_initializes_no_mfa_token() {
        let r = DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Access,
            DsrJurisdiction::Gdpr,
            1,
        );
        assert!(r.mfa_step_up_token.is_none());
        assert!(!r.is_destructive());
    }

    #[test]
    fn dsr_request_with_mfa_attaches_token() {
        let r = DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Erasure,
            DsrJurisdiction::Lgpd,
            1,
        )
        .with_mfa(super::super::mfa::MfaStepUpToken::synthetic_for_test("ok"));
        assert!(r.mfa_step_up_token.is_some());
        assert!(r.is_destructive());
    }

    #[test]
    fn dsr_decision_canonical_strings_unique() {
        let arms = [
            DsrDecision::RequestRejected {
                reason: DsrRejectReason::RateLimit,
            },
            DsrDecision::MfaRequired {
                kind: DsrRequestKind::Erasure,
            },
            DsrDecision::StatusPolled {
                status: DsrStatus::Pending,
            },
        ];
        let mut set: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for a in &arms {
            assert!(set.insert(a.as_str()), "duplicate: {}", a.as_str());
        }
    }

    #[test]
    fn dsr_decision_helpers_pin() {
        let r = DsrDecision::RequestRejected {
            reason: DsrRejectReason::RateLimit,
        };
        assert!(r.is_rejected());
        assert!(!r.is_accepted());
        assert!(!r.is_mfa_required());

        let m = DsrDecision::MfaRequired {
            kind: DsrRequestKind::Erasure,
        };
        assert!(m.is_mfa_required());
        assert!(!m.is_accepted());
        assert!(!m.is_rejected());
    }

    #[test]
    fn dsr_reject_reason_strings_unique() {
        let reasons = [
            DsrRejectReason::RateLimit,
            DsrRejectReason::JurisdictionMismatch,
            DsrRejectReason::IdentityVerificationFailed,
            DsrRejectReason::DuplicateRequest,
            DsrRejectReason::LegalHold,
            DsrRejectReason::AdminOverride,
        ];
        let mut set = std::collections::HashSet::new();
        for r in reasons {
            assert!(set.insert(r.as_str()), "duplicate: {r}");
        }
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn dsr_ticket_accepted_seeds_pending() {
        let r = DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Access,
            DsrJurisdiction::Gdpr,
            1,
        );
        let receipt = super::super::receipt::JwtReceiptToken::synthetic_for_test("token");
        let t = DsrTicket::accepted(&r, sla_for(DsrJurisdiction::Gdpr, 1), receipt);
        assert_eq!(t.status, DsrStatus::Pending);
        assert!(t.receipt.is_some());
        assert!(t.reject_reason.is_none());
    }

    #[test]
    fn dsr_ticket_rejected_carries_reason() {
        let r = DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Erasure,
            DsrJurisdiction::Lgpd,
            1,
        );
        let t = DsrTicket::rejected(&r, DsrRejectReason::RateLimit);
        assert_eq!(t.status, DsrStatus::Rejected);
        assert!(t.receipt.is_none());
        assert_eq!(t.reject_reason, Some(DsrRejectReason::RateLimit));
        assert_eq!(t.sla_deadline_ms, 0);
    }

    #[test]
    fn dsr_ticket_sla_missed_pin() {
        let r = DsrRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            DsrRequestKind::Access,
            DsrJurisdiction::Lgpd,
            0,
        );
        let receipt = super::super::receipt::JwtReceiptToken::synthetic_for_test("t");
        let t = DsrTicket::accepted(&r, sla_for(DsrJurisdiction::Lgpd, 0), receipt);
        // Before deadline: not missed.
        assert!(!t.sla_missed(1));
        // Past deadline + still pending: missed.
        assert!(t.sla_missed(t.sla_deadline_ms + 1));
        // Terminal status: not flagged even past deadline.
        let mut completed = t.clone();
        completed.status = DsrStatus::Completed;
        assert!(!completed.sla_missed(completed.sla_deadline_ms + 1));
    }

    #[test]
    fn dsr_request_kind_display_matches_as_str() {
        assert_eq!(format!("{}", DsrRequestKind::Erasure), "erasure");
    }

    #[test]
    fn dsr_status_display_matches_as_str() {
        assert_eq!(format!("{}", DsrStatus::InProgress), "in_progress");
    }

    #[test]
    fn dsr_jurisdiction_display_matches_as_str() {
        assert_eq!(format!("{}", DsrJurisdiction::Lgpd), "lgpd");
    }

    #[test]
    fn dsr_reject_reason_display_matches_as_str() {
        assert_eq!(format!("{}", DsrRejectReason::RateLimit), "rate_limit");
    }

    #[test]
    fn receipt_expiry_constant_pinned() {
        assert_eq!(RECEIPT_EXPIRY_DAYS, 90);
        assert_eq!(SLA_LGPD_DAYS, 15);
        assert_eq!(SLA_GDPR_DAYS, 30);
        assert_eq!(SLA_CCPA_DAYS, 45);
    }
}

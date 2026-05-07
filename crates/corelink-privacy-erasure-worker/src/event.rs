//! Canonical erasure event types — `BackendKind`, `ErasureDecision`,
//! `BackendErasureOutcome`, `ErasureRequest`, `ErasurePlan`,
//! `BackendCompletion`, the canonical SLA constants, and the 5
//! canonical CloudEvents type strings per WI-S11-002 §1
//! (`ErasureCloudEventType`).
//!
//! ## Why typed enums (NOT string discriminants / serde_json::Value)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S11-002 §1
//! invariant): the erasure tombstones stored in the canonical D1
//! `dsr_erasure_log` table (7y retention canonical pós cycle 15 SEAL
//! unified per privacy_model.md §8) must be byte-deterministic across
//! the auditor evidence trail. Untyped `serde_json::Value` defeats
//! compile-time taxonomy enforcement; the typed enum + per-variant
//! payload struct is the canonical Lote 10.9-quinquies absorption
//! pattern (mirrors `UsageEventKind`, `AggregationDecision`,
//! `StripeAdapterDecision`, `ReconcileDecision`, `QuotaTransition`,
//! `ReplayReason`, `DsrRequestKind`).
//!
//! All public enums are `#[non_exhaustive]` so follow-on WIs can extend
//! the taxonomy additively without breaking downstream `match` sites
//! (e.g. WI-S14 BYOK may add a `CryptoEraseInitiated` decision arm;
//! future jurisdictions may add new backends to the canonical 12-arm
//! list).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical 12-arm backend kind taxonomy pós Lote 10.11.0-bis
/// (privacy_model.md §6.2 source-of-truth). 8 effective slots
/// (erasure física possível) + 4 pseudonymized slots (legal_hold WORM
/// regulatory immutability — GDPR Recital 26 + Art. 11 + WP29 Op.
/// 05/2014 endorsed by EDPB anonymization techniques escape valve).
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g. S-14 BYOK may add an HSM-backed crypto-erase backend; future
/// jurisdictions may add region-specific archival backends).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    // ===== Effective backends (8 canonical pós Lote 10.11.0-bis) =====
    /// Neon multi-tabela (`dsr_tickets / account / tenant /
    /// user_account / consent_ledger / subscription`): SQL DELETE
    /// WHERE subject_user_id; PG-side CHECK + FK enforcement;
    /// tombstone in `dsr_erasure_log`.
    NeonMain,
    /// Neon billing fiscal exception (`invoice / usage_event`):
    /// preserve `legal_hold = true` rows (LGPD Art. 16 fiscal 5y);
    /// pseudonymize PII in retained rows; full DELETE only post
    /// retention expiry.
    NeonBilling,
    /// R2 CAS refcount-aware (CAS blobs mutable):
    /// `subject_unaffiliated` → decrement refcount only (blob shared);
    /// `subject_dedicated` → tombstone + GC sweep grace 72h
    /// (privacy_model.md §6.2 + S-07 dedup safety).
    R2Cas,
    /// R2 AC (Action Cache mutable): DELETE entries
    /// WHERE owner_tenant_id; per-region pinned.
    R2Ac,
    /// D1 (`blob_meta / ac_meta`): subject-scoped row delete;
    /// refcount sync with R2.
    D1,
    /// KV (sessions + cached metadata): DELETE keys matching tenant +
    /// subject prefix; eventual consistency tolerable.
    Kv,
    /// Stripe `Customer.update` (NOT delete): PII nullified in
    /// `customer.metadata + email/name/address`; preserves invoice
    /// integrity per PCI scope GAAP ASC 606 + LGPD Art. 16 fiscal
    /// compliance.
    Stripe,
    /// Loki / Grafana log deletion API: `/loki/api/v1/delete` per
    /// subject; retention compaction trigger.
    Loki,

    // ===== Pseudonymized backends (4 canonical pós Lote 10.11.0-bis;
    //                                legal_hold WORM regulatory
    //                                immutability) =====
    /// R2 audit Object Lock 7y (CTRL-AUDIT-IMMUTABILITY): substitute
    /// `subject_id` by `erased_<HMAC(salt, subject_id)>` via HKDF
    /// info = `corelink/v1/audit-pseudonym`; payload original
    /// preserved (legal hold).
    R2AuditPseudo,
    /// Neon PITR backup 30d (Point-In-Time Recovery): natural
    /// rotation; tombstone replay on any restore; auto-expires after
    /// 30d retention window (no manual delete).
    NeonPitrPseudo,
    /// R2 CAS legal_hold partition (governance mode): content retained
    /// while under active hold; pseudonymize index references; release
    /// post legal_hold expiry.
    R2CasLegalHoldPseudo,
    /// R2 evidence-* buckets 7y (DPIA / LIA / DSR evidence): retain
    /// per SLA framework (EVT-046 LIA + EVT-049 consent record +
    /// EVT-048 DSR evidence); subject_id pseudonymized.
    R2EvidencePseudo,
}

impl BackendKind {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NeonMain => "neon_main",
            Self::NeonBilling => "neon_billing",
            Self::R2Cas => "r2_cas",
            Self::R2Ac => "r2_ac",
            Self::D1 => "d1",
            Self::Kv => "kv",
            Self::Stripe => "stripe",
            Self::Loki => "loki",
            Self::R2AuditPseudo => "r2_audit_pseudo",
            Self::NeonPitrPseudo => "neon_pitr_pseudo",
            Self::R2CasLegalHoldPseudo => "r2_cas_legalhold_pseudo",
            Self::R2EvidencePseudo => "r2_evidence_pseudo",
        }
    }

    /// Whether this backend is **effective** (erasure física possível;
    /// expects 0 records remaining post-erasure) vs **pseudonymized**
    /// (expects 100% `pii_redacted=true` marker presence).
    ///
    /// 8 effective + 4 pseudonymized canonical pós Lote 10.11.0-bis.
    #[must_use]
    pub const fn is_effective(self) -> bool {
        match self {
            Self::NeonMain
            | Self::NeonBilling
            | Self::R2Cas
            | Self::R2Ac
            | Self::D1
            | Self::Kv
            | Self::Stripe
            | Self::Loki => true,
            Self::R2AuditPseudo
            | Self::NeonPitrPseudo
            | Self::R2CasLegalHoldPseudo
            | Self::R2EvidencePseudo => false,
        }
    }

    /// Whether this backend is **pseudonymized** (legal_hold WORM
    /// regulatory immutability; GDPR Recital 26 + Art. 11 + WP29 Op.
    /// 05/2014 endorsed by EDPB anonymization techniques escape valve).
    #[must_use]
    pub const fn is_pseudonymized(self) -> bool {
        !self.is_effective()
    }
}

impl core::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Total canonical backend count (8 effective + 4 pseudonymized).
pub const BACKEND_COUNT: usize = 12;

/// Effective backend count (erasure física possível).
pub const EFFECTIVE_BACKEND_COUNT: usize = 8;

/// Pseudonymized backend count (legal_hold WORM regulatory
/// immutability).
pub const PSEUDONYMIZED_BACKEND_COUNT: usize = 4;

/// Canonical 12-arm backend list. The order is canonical and pinned
/// for the dsr_erasure_log fanout + report rendering + cross-component
/// regression tests.
#[must_use]
pub const fn canonical_backend_kinds() -> &'static [BackendKind; BACKEND_COUNT] {
    &[
        BackendKind::NeonMain,
        BackendKind::NeonBilling,
        BackendKind::R2Cas,
        BackendKind::R2Ac,
        BackendKind::D1,
        BackendKind::Kv,
        BackendKind::Stripe,
        BackendKind::Loki,
        BackendKind::R2AuditPseudo,
        BackendKind::NeonPitrPseudo,
        BackendKind::R2CasLegalHoldPseudo,
        BackendKind::R2EvidencePseudo,
    ]
}

/// Canonical verification SLA in hours. Per WI-S11-002 §9.1 DD-005:
/// 24h is minimum for Loki cold archive settle (S-09 R-S09-5
/// "≤30 min lag" with daily batch up to 24h); 48h is more conservative
/// but adds 1d to the SLO clock perception. Rationale:
/// LGPD Art. 18 §III + GDPR Art. 17.1 "without undue delay".
pub const VERIFICATION_SLA_HOURS: u32 = 24;

/// Canonical verification SLA in milliseconds (24h × 3_600_000 ms/h).
pub const VERIFICATION_SLA_MS: u64 = (VERIFICATION_SLA_HOURS as u64) * 3_600_000;

/// Canonical erasure SLA in days. Per LGPD Art. 18 IV + GDPR Art. 17 +
/// CCPA §1798.105 + sprint contract §5.1 R-S11-1 erasure 30d corridos
/// from `dsr_tickets.status = 'verified'` clock_start.
pub const ERASURE_SLA_DAYS: u32 = 30;

/// Canonical 32-byte erasure salt. Per WI-S11-002 §9.1 DD-003:
/// per-tenant + per-DSR scope (NOT global); rationale = prevent
/// cross-DSR correlation by attacker; per-DSR rotation provides
/// forward secrecy if 1 salt leaked. Production wiring at WI-S11-008
/// binds this to the encrypted D1 vault per ADR-S11-003 interim
/// (full BYOK KMS vault deferred to S-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ErasureSalt(pub [u8; 32]);

impl ErasureSalt {
    /// Construct a fresh salt from a raw 32-byte slice.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte salt.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Synthetic deterministic salt for in-memory tests. Production
    /// code MUST use a cryptographically random salt drawn from the
    /// canonical encrypted D1 vault per ADR-S11-003.
    #[must_use]
    pub fn synthetic_for_test(seed: u8) -> Self {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        Self(bytes)
    }
}

/// Canonical erasure request input shape. Per WI-S11-002 §1: every
/// erasure request consumes the canonical `dsr.queued.v1` payload
/// from WI-S11-001 carrying `dsr_id` UUIDv7 (idempotency ledger key
/// — same as DSR `request_id`) + `tenant_id` (S-03 inheritance
/// enforcement; never the request body) + `subject_id` (PAT principal
/// post-authn) + `erasure_salt` 32 bytes (per-tenant + per-DSR scope
/// per ADR-S11-003) + `queued_at_ms` (canonical SLA clock_start
/// anchor).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasureRequest {
    /// Canonical UUIDv7 DSR id (the idempotency ledger key; matches
    /// `DsrRequest::request_id` from WI-S11-001).
    pub dsr_id: Uuid,
    /// Tenant id (canonical UUIDv7 from the S-03 auth middleware;
    /// NEVER the request body per Lote 10.4bis).
    pub tenant_id: Uuid,
    /// Data subject id (canonical UUIDv7; PAT principal post-authn).
    /// Per CTRL-PRIV-014 the production audit boundary stores
    /// `sha256(subject_id || tenant_salt)` not the raw id; the trait
    /// surface here ships the raw UUID for in-memory test pinning.
    pub subject_id: Uuid,
    /// Per-tenant + per-DSR erasure salt (32 bytes; AES-256-GCM
    /// encrypted at rest in the D1 vault per ADR-S11-003 interim;
    /// full BYOK KMS vault deferred to S-14).
    pub erasure_salt: ErasureSalt,
    /// Wall-clock instant the canonical `dsr.queued.v1` payload was
    /// enqueued (Unix epoch ms). Anchors the canonical 30d
    /// SLO-FRESH-DSR-ERASURE clock + the 24h verification SLA window.
    pub queued_at_ms: u64,
    /// Whether the upstream DSR ticket is under active legal hold
    /// (CTRL-PRIV-033 dual-approval flow). When `true` the
    /// orchestrator skips effective backends with the canonical
    /// [`BackendErasureOutcome::NotApplicable`] arm; pseudonymized
    /// backends are unaffected because Object Lock retains the row
    /// regardless of the legal hold flag.
    pub legal_hold: bool,
}

impl ErasureRequest {
    /// Construct a fresh erasure request with `legal_hold = false`.
    #[must_use]
    pub const fn new(
        dsr_id: Uuid,
        tenant_id: Uuid,
        subject_id: Uuid,
        erasure_salt: ErasureSalt,
        queued_at_ms: u64,
    ) -> Self {
        Self {
            dsr_id,
            tenant_id,
            subject_id,
            erasure_salt,
            queued_at_ms,
            legal_hold: false,
        }
    }

    /// Builder: mark this DSR as under active legal hold (effective
    /// backends skipped; pseudonymized backends still process).
    #[must_use]
    pub const fn with_legal_hold(mut self, legal_hold: bool) -> Self {
        self.legal_hold = legal_hold;
        self
    }

    /// Canonical verification deadline (Unix epoch ms) =
    /// `queued_at_ms + 24h`. Production wiring fires SEV-1 alert when
    /// `now > verification_deadline_ms AND any backend not verified`
    /// per FM-450.
    #[must_use]
    pub const fn verification_deadline_ms(&self) -> u64 {
        self.queued_at_ms.saturating_add(VERIFICATION_SLA_MS)
    }
}

/// Per-backend ledger entry within a canonical 12-arm erasure plan.
/// Pinned so the orchestrator's fanout step is structurally
/// 12-deep; cross-component regression tests assert
/// `plan.entries.len() == BACKEND_COUNT`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasurePlanEntry {
    /// Canonical 12-arm backend kind.
    pub backend: BackendKind,
    /// Canonical 35-char idempotency key (per WI §6.1 step 6 +
    /// Lote 10.10-quaters Idempotency-Key 35-char canonical):
    /// `corelink-{dsr_id_short(8)}-{backend}-{retry_count(3)}`.
    /// Rendered deterministically from the request + backend + retry.
    pub idempotency_key: String,
}

impl ErasurePlanEntry {
    /// Render the canonical 35-char idempotency key for
    /// `(dsr_id, backend, retry_count)`.
    #[must_use]
    pub fn idempotency_key_for(dsr_id: Uuid, backend: BackendKind, retry_count: u32) -> String {
        let dsr_simple = dsr_id.simple().to_string();
        let dsr_short = dsr_simple.get(..8).unwrap_or("00000000");
        format!(
            "corelink-{dsr}-{backend}-{retry:03}",
            dsr = dsr_short,
            backend = backend.as_str(),
            retry = retry_count.min(999),
        )
    }
}

/// Canonical erasure plan: 12 ordered backend ledger entries +
/// canonical request anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasurePlan {
    /// Canonical UUIDv7 DSR id (matches the request).
    pub dsr_id: Uuid,
    /// Tenant id (S-03 inheritance — NEVER the request body).
    pub tenant_id: Uuid,
    /// Wall-clock instant the plan was generated (Unix epoch ms;
    /// drives the 24h verification SLA window).
    pub generated_at_ms: u64,
    /// 12 canonical backend ledger entries in canonical order
    /// (matches `canonical_backend_kinds()`).
    pub entries: Vec<ErasurePlanEntry>,
}

impl ErasurePlan {
    /// Canonical plan generation: deterministic 12-entry fanout in
    /// canonical order. Replay-safe (PAT-RETRY-IDEMPOTENT-001):
    /// same `(dsr_id, generated_at_ms)` → identical plan.
    #[must_use]
    pub fn canonical(request: &ErasureRequest, generated_at_ms: u64) -> Self {
        let entries = canonical_backend_kinds()
            .iter()
            .map(|backend| ErasurePlanEntry {
                backend: *backend,
                idempotency_key: ErasurePlanEntry::idempotency_key_for(
                    request.dsr_id,
                    *backend,
                    0,
                ),
            })
            .collect();
        Self {
            dsr_id: request.dsr_id,
            tenant_id: request.tenant_id,
            generated_at_ms,
            entries,
        }
    }
}

/// Canonical 5-arm per-backend outcome taxonomy. Aligned with
/// sprint contract §5.2 R-S11-6 status enum +
/// WI-S11-002 §1 `BackendErasureOutcome` 5-arm enum
/// (`erased | pseudonymized | partial_failure | failed | not_applicable`).
///
/// `#[non_exhaustive]` reserves additive growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum BackendErasureOutcome {
    /// Effective backend: 0 records remaining post-DELETE. The
    /// canonical happy-path arm for the 8 effective backends.
    Erased {
        /// Number of records deleted by this erasure step.
        records_deleted: u64,
    },
    /// Pseudonymized backend: PII fields substituted by
    /// `sha256(subject_id || erasure_salt)` + marker
    /// `pii_redacted = true`. The canonical happy-path arm for the
    /// 4 pseudonymized backends.
    Pseudonymized {
        /// Number of records redacted (pseudonym substituted) by this
        /// erasure step.
        records_redacted: u64,
    },
    /// Some records erased / redacted, others failed (e.g.
    /// `legal_hold = true` preserved per CTRL-PRIV-033, residency
    /// lock per INV-DATA-RESIDENCY, refcount-aware blob shared
    /// between tenants per S-07 dedup). Triggers
    /// [`ErasureDecision::VerifiedPartial`] + SEV-1 alert per FM-450.
    PartialFailure {
        /// Records that succeeded.
        records_succeeded: u64,
        /// Records that failed (e.g. legal_hold preserved).
        records_failed: u64,
    },
    /// Total backend failure; backend unavailable; retry queue
    /// activates. Triggers exponential backoff retry up to
    /// retry_count = 5; SEV-2 alert on retry exhaustion per FM-061
    /// mapping.
    Failed {
        /// Canonical retry-after window in seconds.
        retry_after_seconds: u64,
    },
    /// Backend not applicable for this DSR (e.g. tenant has no
    /// Stripe customer; tenant has no Loki retention bucket;
    /// blob refcount > 1 with subject_unaffiliated semantic per
    /// S-07 dedup, or legal hold flag set on the DSR ticket
    /// CTRL-PRIV-033). No SEV alert.
    NotApplicable,
}

impl BackendErasureOutcome {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Erased { .. } => "erased",
            Self::Pseudonymized { .. } => "pseudonymized",
            Self::PartialFailure { .. } => "partial_failure",
            Self::Failed { .. } => "failed",
            Self::NotApplicable => "not_applicable",
        }
    }

    /// Whether this outcome represents successful erasure (Erased,
    /// Pseudonymized, or NotApplicable). PartialFailure + Failed
    /// land in `VerifiedPartial`.
    #[must_use]
    pub const fn is_successful(&self) -> bool {
        matches!(
            self,
            Self::Erased { .. } | Self::Pseudonymized { .. } | Self::NotApplicable
        )
    }
}

impl core::fmt::Display for BackendErasureOutcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-backend completion record stored in the canonical D1
/// `dsr_erasure_log` tombstone (per WI §6.1.7). Drives the canonical
/// `(dsr_id, backend)` UNIQUE constraint replay-safe arm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCompletion {
    /// Canonical UUIDv7 DSR id.
    pub dsr_id: Uuid,
    /// Tenant id (S-03 inheritance).
    pub tenant_id: Uuid,
    /// Canonical 12-arm backend kind.
    pub backend: BackendKind,
    /// Per-backend outcome (5-arm taxonomy).
    pub outcome: BackendErasureOutcome,
    /// Canonical 35-char idempotency key.
    pub idempotency_key: String,
    /// Wall-clock instant the canonical erasure step started
    /// (Unix epoch ms).
    pub started_at_ms: u64,
    /// Wall-clock instant the canonical erasure step completed
    /// (Unix epoch ms; >= started_at_ms).
    pub completed_at_ms: u64,
    /// Retry count (0..=5; bounded by D1 CHECK constraint).
    pub retry_count: u32,
    /// 32-byte BLAKE3 fingerprint of the remaining-rows-for-tenant
    /// state per backend post-erasure. The canonical 24h verification
    /// sweep compares this against
    /// [`crate::backends::CANONICAL_EMPTY_TENANT_HASH`] (effective
    /// backends) or the pseudonymized "100% pii_redacted=true marker
    /// presence" sentinel; mismatch lands `VerifiedPartial`.
    pub verification_hash: [u8; 32],
}

/// Canonical 6-arm erasure decision taxonomy. Per WI-S11-002 §1 +
/// AC-001..012: every erasure pipeline run lands in exactly one of
/// these arms; the canonical CloudEvents envelope (5 types per spec
/// §1) fires BEFORE the state mutation on each arm.
///
/// `#[non_exhaustive]` reserves additive growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum ErasureDecision {
    /// Plan generated + fan-out initiated: 12 canonical backend ledger
    /// entries enumerated + dispatched. Maps to the canonical
    /// `dev.hugr.corelink.dsr.erasure.started.v1` CloudEvent.
    Started {
        /// Canonical 12-arm plan.
        plan: ErasurePlan,
    },
    /// Verification 24h sweep complete: every backend reports a
    /// successful outcome. Canonical
    /// `dev.hugr.corelink.dsr.erasure.completed.v1` event fires; the
    /// signed report is uploaded to R2 evidence-dsr.
    VerifiedComplete {
        /// Per-backend completion ledger (12 entries).
        completions: Vec<BackendCompletion>,
    },
    /// Verification 24h sweep partial: at least one backend reports a
    /// non-successful outcome (PartialFailure / Failed); SEV-1 alert
    /// fires per FM-450 + RB-DSR-ERASURE-INCOMPLETE runbook;
    /// `verification_failed.v1` audit row emitted; `dsr_tickets.status`
    /// does NOT transition to `completed` until a second sweep at
    /// 48h post-original timestamp.
    VerifiedPartial {
        /// Per-backend completion ledger (12 entries).
        completions: Vec<BackendCompletion>,
        /// Number of backends that reported a non-successful outcome.
        failed_count: usize,
    },
    /// Verification step itself failed (sweep job crashed / D1
    /// query rejected / R2 evidence-dsr upload failed). Distinct
    /// from `VerifiedPartial` (which is per-backend semantic).
    /// Triggers SEV-2 + sweep job retry.
    VerificationFailed {
        /// Canonical reason mnemonic for the verification job
        /// failure.
        reason: String,
    },
    /// SLA breached: > 24h elapsed since plan generation and at least
    /// one backend not yet verified. SEV-1 alert per FM-450 +
    /// RB-DSR-ERASURE-INCOMPLETE runbook. Rationale: LGPD Art. 18
    /// §III + GDPR Art. 17.1 "without undue delay".
    SlaBreached {
        /// Number of backends still unverified at SLA breach.
        unverified_count: usize,
        /// Elapsed time since plan generation in milliseconds.
        elapsed_ms: u64,
    },
    /// Cross-tenant attack rejected (forged `dsr.queued.v1` payload
    /// with mismatching `tenant_id`). Per WI AC-009 the queue consumer
    /// MUST verify `payload.tenant_id == dsr_tickets[dsr_id].tenant_id`
    /// pre-mutation and reject on mismatch with SEV-1 alert.
    Rejected {
        /// Canonical reject reason mnemonic.
        reason: String,
    },
}

impl ErasureDecision {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::VerifiedComplete { .. } => "verified_complete",
            Self::VerifiedPartial { .. } => "verified_partial",
            Self::VerificationFailed { .. } => "verification_failed",
            Self::SlaBreached { .. } => "sla_breached",
            Self::Rejected { .. } => "rejected",
        }
    }

    /// Whether this decision represents a fully-completed erasure
    /// (every backend successful).
    #[must_use]
    pub const fn is_verified_complete(&self) -> bool {
        matches!(self, Self::VerifiedComplete { .. })
    }

    /// Whether this decision represents a partial-failure or SLA-
    /// breach state (SEV-1 alert path).
    #[must_use]
    pub const fn is_sev1(&self) -> bool {
        matches!(
            self,
            Self::VerifiedPartial { .. } | Self::SlaBreached { .. } | Self::Rejected { .. }
        )
    }
}

/// 5 canonical CloudEvents `type` strings per WI-S11-002 §1
/// (`dev.hugr.corelink.dsr.erasure.{started, backend_completed,
/// verification_passed, verification_failed, completed}.v1`). Pinned
/// per Lote 10.9bis P0-G prefix discipline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErasureCloudEventType {
    /// `dev.hugr.corelink.dsr.erasure.started.v1` — orchestrator
    /// emitted a canonical 12-arm plan + dispatched fan-out.
    Started,
    /// `dev.hugr.corelink.dsr.erasure.backend_completed.v1` — a
    /// per-backend erasure step completed (Erased / Pseudonymized /
    /// PartialFailure / Failed / NotApplicable).
    BackendCompleted,
    /// `dev.hugr.corelink.dsr.erasure.verification_passed.v1` —
    /// 24h verification sweep confirmed every backend reports a
    /// successful outcome.
    VerificationPassed,
    /// `dev.hugr.corelink.dsr.erasure.verification_failed.v1` —
    /// 24h verification sweep detected at least one non-successful
    /// backend outcome.
    VerificationFailed,
    /// `dev.hugr.corelink.dsr.erasure.completed.v1` — pipeline
    /// terminal: dsr_tickets.status → `completed`.
    Completed,
}

impl ErasureCloudEventType {
    /// Canonical CloudEvents `type` attribute string. Pinned per
    /// privacy_model.md §6.2 + Lote 10.9bis P0-G prefix discipline.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "dev.hugr.corelink.dsr.erasure.started.v1",
            Self::BackendCompleted => "dev.hugr.corelink.dsr.erasure.backend_completed.v1",
            Self::VerificationPassed => "dev.hugr.corelink.dsr.erasure.verification_passed.v1",
            Self::VerificationFailed => "dev.hugr.corelink.dsr.erasure.verification_failed.v1",
            Self::Completed => "dev.hugr.corelink.dsr.erasure.completed.v1",
        }
    }

    /// Whether this event is a SEV-1 alert path (verification_failed
    /// only; `completed` is informational; `started` /
    /// `backend_completed` / `verification_passed` are not SEV-1).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::VerificationFailed)
    }
}

impl core::fmt::Display for ErasureCloudEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-event CloudEvents type string list. Pinned for
/// dashboard widget configuration + regression tests +
/// `schemas/cloudevents/dsr-erasure-*.v1.json` schema cross-reference.
#[must_use]
pub const fn canonical_cloudevent_types() -> &'static [&'static str; 5] {
    &[
        "dev.hugr.corelink.dsr.erasure.started.v1",
        "dev.hugr.corelink.dsr.erasure.backend_completed.v1",
        "dev.hugr.corelink.dsr.erasure.verification_passed.v1",
        "dev.hugr.corelink.dsr.erasure.verification_failed.v1",
        "dev.hugr.corelink.dsr.erasure.completed.v1",
    ]
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

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    #[test]
    fn backend_kind_canonical_strings_unique() {
        let v = canonical_backend_kinds();
        assert_eq!(v.len(), BACKEND_COUNT);
        let mut set: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()), "duplicate canonical: {k}");
        }
        assert_eq!(set.len(), BACKEND_COUNT);
    }

    #[test]
    fn backend_kind_effective_count_pinned() {
        let effective = canonical_backend_kinds()
            .iter()
            .filter(|k| k.is_effective())
            .count();
        assert_eq!(effective, EFFECTIVE_BACKEND_COUNT);

        let pseudonymized = canonical_backend_kinds()
            .iter()
            .filter(|k| k.is_pseudonymized())
            .count();
        assert_eq!(pseudonymized, PSEUDONYMIZED_BACKEND_COUNT);

        assert_eq!(
            EFFECTIVE_BACKEND_COUNT + PSEUDONYMIZED_BACKEND_COUNT,
            BACKEND_COUNT
        );
    }

    #[test]
    fn backend_kind_effective_pseudonymized_disjoint() {
        for k in canonical_backend_kinds() {
            assert_ne!(k.is_effective(), k.is_pseudonymized());
        }
    }

    #[test]
    fn backend_kind_display_matches_as_str() {
        assert_eq!(format!("{}", BackendKind::NeonMain), "neon_main");
        assert_eq!(
            format!("{}", BackendKind::R2CasLegalHoldPseudo),
            "r2_cas_legalhold_pseudo"
        );
    }

    #[test]
    fn verification_sla_constants_pinned() {
        assert_eq!(VERIFICATION_SLA_HOURS, 24);
        assert_eq!(VERIFICATION_SLA_MS, 24 * 3_600_000);
        assert_eq!(ERASURE_SLA_DAYS, 30);
    }

    #[test]
    fn erasure_request_verification_deadline_pinned() {
        let salt = ErasureSalt::synthetic_for_test(7);
        let req = ErasureRequest::new(
            fixed_uuid(1),
            fixed_uuid(2),
            fixed_uuid(3),
            salt,
            1_000_000_000_000,
        );
        assert_eq!(
            req.verification_deadline_ms(),
            1_000_000_000_000 + 24 * 3_600_000
        );
    }

    #[test]
    fn erasure_request_deadline_saturates_at_overflow() {
        let salt = ErasureSalt::synthetic_for_test(0);
        let req = ErasureRequest::new(
            fixed_uuid(1),
            fixed_uuid(2),
            fixed_uuid(3),
            salt,
            u64::MAX - 100,
        );
        assert_eq!(req.verification_deadline_ms(), u64::MAX);
    }

    #[test]
    fn erasure_salt_synthetic_deterministic() {
        let s1 = ErasureSalt::synthetic_for_test(42);
        let s2 = ErasureSalt::synthetic_for_test(42);
        assert_eq!(s1, s2);
        let s3 = ErasureSalt::synthetic_for_test(43);
        assert_ne!(s1, s3);
    }

    #[test]
    fn idempotency_key_format_canonical() {
        let dsr = fixed_uuid(7);
        let key = ErasurePlanEntry::idempotency_key_for(dsr, BackendKind::NeonMain, 0);
        assert!(key.starts_with("corelink-"));
        assert!(key.contains("-neon_main-"));
        assert!(key.ends_with("-000"));
    }

    #[test]
    fn idempotency_key_clamps_retry_count() {
        let dsr = fixed_uuid(8);
        let key = ErasurePlanEntry::idempotency_key_for(dsr, BackendKind::D1, 9999);
        assert!(key.ends_with("-999"));
    }

    #[test]
    fn erasure_plan_canonical_has_12_entries() {
        let salt = ErasureSalt::synthetic_for_test(1);
        let req = ErasureRequest::new(
            fixed_uuid(11),
            fixed_uuid(12),
            fixed_uuid(13),
            salt,
            1_000,
        );
        let plan = ErasurePlan::canonical(&req, 2_000);
        assert_eq!(plan.entries.len(), BACKEND_COUNT);
        for (i, expected) in canonical_backend_kinds().iter().enumerate() {
            let entry = plan.entries.get(i).unwrap();
            assert_eq!(entry.backend, *expected);
        }
        assert_eq!(plan.generated_at_ms, 2_000);
        assert_eq!(plan.dsr_id, req.dsr_id);
    }

    #[test]
    fn erasure_plan_canonical_replay_safe() {
        let salt = ErasureSalt::synthetic_for_test(1);
        let req = ErasureRequest::new(
            fixed_uuid(21),
            fixed_uuid(22),
            fixed_uuid(23),
            salt,
            1_000,
        );
        let p1 = ErasurePlan::canonical(&req, 2_000);
        let p2 = ErasurePlan::canonical(&req, 2_000);
        assert_eq!(p1, p2);
    }

    #[test]
    fn backend_outcome_strings_unique() {
        let arms = [
            BackendErasureOutcome::Erased {
                records_deleted: 1,
            },
            BackendErasureOutcome::Pseudonymized {
                records_redacted: 1,
            },
            BackendErasureOutcome::PartialFailure {
                records_succeeded: 1,
                records_failed: 1,
            },
            BackendErasureOutcome::Failed {
                retry_after_seconds: 60,
            },
            BackendErasureOutcome::NotApplicable,
        ];
        let mut set: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for a in &arms {
            assert!(set.insert(a.as_str()), "duplicate: {}", a.as_str());
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn backend_outcome_is_successful_pinned() {
        let success_erased = BackendErasureOutcome::Erased {
            records_deleted: 0,
        }
        .is_successful();
        let success_pseudo = BackendErasureOutcome::Pseudonymized {
            records_redacted: 0,
        }
        .is_successful();
        let success_na = BackendErasureOutcome::NotApplicable.is_successful();
        let fail_partial = BackendErasureOutcome::PartialFailure {
            records_succeeded: 0,
            records_failed: 1,
        }
        .is_successful();
        let fail_failed = BackendErasureOutcome::Failed {
            retry_after_seconds: 60,
        }
        .is_successful();
        assert!(success_erased);
        assert!(success_pseudo);
        assert!(success_na);
        assert!(!fail_partial);
        assert!(!fail_failed);
    }

    #[test]
    fn cloudevent_types_canonical_5() {
        let v = canonical_cloudevent_types();
        assert_eq!(v.len(), 5);
        for s in v {
            assert!(s.starts_with("dev.hugr.corelink.dsr.erasure."));
            assert!(s.ends_with(".v1"));
        }
        let mut set = std::collections::HashSet::new();
        for s in v {
            assert!(set.insert(*s));
        }
    }

    #[test]
    fn cloudevent_type_strings_match_enum() {
        let pinned = [
            (
                ErasureCloudEventType::Started,
                "dev.hugr.corelink.dsr.erasure.started.v1",
            ),
            (
                ErasureCloudEventType::BackendCompleted,
                "dev.hugr.corelink.dsr.erasure.backend_completed.v1",
            ),
            (
                ErasureCloudEventType::VerificationPassed,
                "dev.hugr.corelink.dsr.erasure.verification_passed.v1",
            ),
            (
                ErasureCloudEventType::VerificationFailed,
                "dev.hugr.corelink.dsr.erasure.verification_failed.v1",
            ),
            (
                ErasureCloudEventType::Completed,
                "dev.hugr.corelink.dsr.erasure.completed.v1",
            ),
        ];
        for (e, s) in pinned {
            assert_eq!(e.as_str(), s);
        }
    }

    #[test]
    fn erasure_decision_sev1_pinned() {
        let p = ErasureDecision::VerifiedPartial {
            completions: vec![],
            failed_count: 1,
        }
        .is_sev1();
        let s = ErasureDecision::SlaBreached {
            unverified_count: 1,
            elapsed_ms: 1,
        }
        .is_sev1();
        let r = ErasureDecision::Rejected {
            reason: "x".to_string(),
        }
        .is_sev1();
        let c = ErasureDecision::VerifiedComplete {
            completions: vec![],
        }
        .is_sev1();
        assert!(p);
        assert!(s);
        assert!(r);
        assert!(!c);
    }

    #[test]
    fn legal_hold_builder() {
        let salt = ErasureSalt::synthetic_for_test(1);
        let req = ErasureRequest::new(
            fixed_uuid(1),
            fixed_uuid(2),
            fixed_uuid(3),
            salt,
            10,
        )
        .with_legal_hold(true);
        assert!(req.legal_hold);
    }
}

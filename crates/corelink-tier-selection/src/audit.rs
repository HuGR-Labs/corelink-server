//! Tier-selection audit-of-audit taxonomy.
//!
//! Per Lote 10.6bis pattern + S-07 P1-1 fix: every orchestrator state
//! mutation emits an audit record BEFORE the mutation. Audit failure
//! returns a typed [`crate::error::TierError::Audit`] and the
//! mutation is aborted (fail-CLOSED).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::tenant::TenantId;
use crate::tier::TierKind;

/// Canonical 8-element audit event taxonomy per WI §21.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TierSelectionAuditEventType {
    /// `corelink.onboarding.tier_select_attempted` — tier-select
    /// callback received (before DPA / lock validation).
    TierSelectAttempted,
    /// `corelink.onboarding.dpa_first_violation_attempt` — INV
    /// violation prevented (tier selection rejected because DPA not
    /// accepted). Alert > 0 = SEV-1.
    DpaFirstViolationAttempt,
    /// `corelink.onboarding.tier_activated_free` — Free tier instant
    /// activation succeeded (no Stripe).
    TierActivatedFree,
    /// `corelink.onboarding.stripe_checkout_session_created` — paid
    /// tier Checkout Session created; tenant_id mapped to Stripe
    /// customer atomically.
    StripeCheckoutSessionCreated,
    /// `corelink.onboarding.enterprise_route_bypass_attempt` —
    /// enterprise tier attempted direct Checkout; rejected. Alert > 0.
    EnterpriseRouteBypassAttempt,
    /// `corelink.onboarding.stripe_subscription_activated` — Stripe
    /// webhook `checkout.session.completed` processed; subscription
    /// state advanced.
    StripeSubscriptionActivated,
    /// `corelink.onboarding.stripe_webhook_duplicate` — idempotency
    /// dedup hit; second delivery ignored.
    StripeWebhookDuplicate,
    /// `corelink.onboarding.stripe_webhook_invalid_signature` —
    /// HMAC / replay verification failed.
    StripeWebhookInvalidSignature,
}

impl TierSelectionAuditEventType {
    /// Canonical CloudEvents type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TierSelectAttempted => "corelink.onboarding.tier_select_attempted",
            Self::DpaFirstViolationAttempt => "corelink.onboarding.dpa_first_violation_attempt",
            Self::TierActivatedFree => "corelink.onboarding.tier_activated_free",
            Self::StripeCheckoutSessionCreated => {
                "corelink.onboarding.stripe_checkout_session_created"
            }
            Self::EnterpriseRouteBypassAttempt => {
                "corelink.onboarding.enterprise_route_bypass_attempt"
            }
            Self::StripeSubscriptionActivated => {
                "corelink.onboarding.stripe_subscription_activated"
            }
            Self::StripeWebhookDuplicate => "corelink.onboarding.stripe_webhook_duplicate",
            Self::StripeWebhookInvalidSignature => {
                "corelink.onboarding.stripe_webhook_invalid_signature"
            }
        }
    }
}

impl core::fmt::Display for TierSelectionAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 8-element list for surface-stability regression tests.
#[must_use]
pub const fn canonical_tier_selection_audit_event_strings() -> &'static [&'static str; 8] {
    &[
        "corelink.onboarding.tier_select_attempted",
        "corelink.onboarding.dpa_first_violation_attempt",
        "corelink.onboarding.tier_activated_free",
        "corelink.onboarding.stripe_checkout_session_created",
        "corelink.onboarding.enterprise_route_bypass_attempt",
        "corelink.onboarding.stripe_subscription_activated",
        "corelink.onboarding.stripe_webhook_duplicate",
        "corelink.onboarding.stripe_webhook_invalid_signature",
    ]
}

/// Audit record emitted on every orchestrator decision arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TierSelectionAuditRecord {
    /// CloudEvents type.
    pub event_type: TierSelectionAuditEventType,
    /// Subject tenant (always populated).
    pub tenant_id: TenantId,
    /// Tier the record pertains to (None for webhook signature
    /// failures where the tier isn't yet known).
    pub tier: Option<TierKind>,
    /// Timestamp (ms since epoch).
    pub ts_ms: u64,
    /// Correlation id (PAT-CORRELATION-ID-001).
    pub correlation_id: String,
}

impl TierSelectionAuditRecord {
    /// Construct a record.
    #[must_use]
    pub fn new(
        event_type: TierSelectionAuditEventType,
        tenant_id: TenantId,
        tier: Option<TierKind>,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            tenant_id,
            tier,
            ts_ms,
            correlation_id: correlation_id.into(),
        }
    }
}

/// Audit-of-audit emit error.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TierSelectionAuditEmitError {
    /// Downstream audit sink rejected the record (e.g. R2 write
    /// failure / chain-link advance failure).
    #[error("tier-selection audit sink rejected emit: {0}")]
    Rejected(String),
}

/// Trait every tier-selection audit-of-audit sink satisfies.
pub trait TierSelectionAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit record. MUST be invoked BEFORE the state
    /// mutation; a returned `Err` aborts the mutation per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
    fn emit(&self, record: &TierSelectionAuditRecord) -> Result<(), TierSelectionAuditEmitError>;
}

/// In-memory test sink — accumulates emitted records for property
/// inspection.
#[derive(Clone, Debug, Default)]
pub struct InMemoryTierSelectionAuditSink {
    records: Arc<Mutex<Vec<TierSelectionAuditRecord>>>,
}

impl InMemoryTierSelectionAuditSink {
    /// Construct an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events.
    #[must_use]
    pub fn snapshot(&self) -> Vec<TierSelectionAuditRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Count of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.records.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// True if no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if any event of the given type was emitted.
    #[must_use]
    pub fn has_event(&self, event_type: TierSelectionAuditEventType) -> bool {
        self.snapshot().iter().any(|r| r.event_type == event_type)
    }
}

impl TierSelectionAuditSink for InMemoryTierSelectionAuditSink {
    fn emit(&self, record: &TierSelectionAuditRecord) -> Result<(), TierSelectionAuditEmitError> {
        let mut g = self
            .records
            .lock()
            .map_err(|e| TierSelectionAuditEmitError::Rejected(format!("mutex poisoned: {e}")))?;
        g.push(record.clone());
        Ok(())
    }
}

/// Adversarial fixture sink that always rejects (forces ledger
/// fail-CLOSED behaviour in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingTierSelectionAuditSink;

impl TierSelectionAuditSink for FailingTierSelectionAuditSink {
    fn emit(&self, _record: &TierSelectionAuditRecord) -> Result<(), TierSelectionAuditEmitError> {
        Err(TierSelectionAuditEmitError::Rejected(
            "adversarial fixture: always rejects".to_string(),
        ))
    }
}

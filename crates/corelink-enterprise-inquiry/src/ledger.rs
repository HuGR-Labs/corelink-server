//! Enterprise inquiry ledger orchestrator.
//!
//! The ledger persists the inquiry record + the outbox row, executes
//! the PAT-SAGA-001 atomic Slack + CRM dispatch (both succeed or
//! both roll back with compensating Slack mark), and exposes
//! `sla_breaches` for the 24h auto-reply SLA detector worker.
//!
//! ## Concurrency model
//!
//! State is wrapped in `Arc<Mutex<>>` per Lote 10.6bis pattern. The
//! ledger is `Send + Sync` and may be shared across worker tasks;
//! the production wiring serialises mutations behind a Durable
//! Object so the mutex contention floor is bounded.
//!
//! ## Encryption at ingress (R2-11; S-19 P1-NEW-3)
//!
//! The plaintext [`EnterpriseInquiryForm`] arrives at the HTTP handler
//! boundary and is passed into [`EnterpriseInquiryLedger::submit_inquiry`]
//! ONCE. Inside `submit_inquiry`, the form is sealed via the
//! configured [`InquiryPayloadEncryptor`] BEFORE any persistence /
//! outbox row insert, and the [`LedgerState`] mirror only ever holds
//! the [`SealedInquiry`] (sealed payload + non-PII sanitised
//! surrogates). The plaintext form is dropped at the end of the
//! `submit_inquiry` stack frame. The Slack adapter receives an
//! anonymised summary derived from the sanitised surrogates. The CRM
//! adapter receives the [`SealedInquiry`] + an
//! [`InquiryPayloadEncryptor`] handle and decrypts at its OWN
//! boundary (the HubSpot adapter is the only permitted decryption
//! boundary on the dispatch path; the auto-reply mailer is the
//! secondary permitted boundary, restricted to email + locale).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{InquiryAuditEventType, InquiryAuditRecord, InquiryAuditSink};
use crate::crm::{CrmClient, CrmEntryId};
use crate::encryption::{
    anonymised_slack_summary, seal_inquiry, InquiryPayloadEncryptor, SealedInquiry,
    SYSTEM_CMK_TENANT_TAG,
};
use crate::error::EnterpriseInquiryError;
use crate::form::{
    EnterpriseInquiryForm, IdempotencyKey, InquiryId, InquiryReceipt, InquiryStatus,
};
use crate::mailer::AutoReplyMailer;
use crate::outbox::{OutboxRecord, OutboxStatus};
use crate::slack::{SlackClient, SlackMessageId, SlackPostKind};
use crate::SLA_RESPONSE_MS;

/// Inquiry record persisted in the in-memory store (D1 mirror).
///
/// Per CTRL-PRIV-001 + R2-11 / S-19 P1-NEW-3: this struct holds the
/// BYOK-sealed payload + non-PII sanitised surrogates, NEVER plaintext
/// PII. A `Debug` print of this struct cannot surface the plaintext
/// email / phone / company / use_case / additional_notes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InquiryRecord {
    /// Stable inquiry id.
    pub inquiry_id: InquiryId,
    /// Idempotency key (the original caller-supplied dedup token).
    pub idempotency_key: IdempotencyKey,
    /// Sealed inquiry payload + sanitised surrogates. Plaintext PII is
    /// NEVER held in this struct — see the [`SealedInquiry`] docstring.
    pub sealed: SealedInquiry,
    /// Tenant id under which the payload was sealed (binds to the
    /// payload AAD; equals [`crate::encryption::SYSTEM_CMK_TENANT_TAG`]
    /// for un-authenticated prospect inquiries).
    pub tenant_id: String,
    /// Computed lead score.
    pub lead_score: u32,
    /// Lifecycle status.
    pub status: InquiryStatus,
    /// Server-side timestamp (ms since epoch) when the record was
    /// first inserted.
    pub created_ms: u64,
    /// Server-side timestamp (ms since epoch) when the auto-reply
    /// dispatched (None until SES confirms).
    pub replied_ms: Option<u64>,
}

/// SLA breach descriptor surfaced by [`EnterpriseInquiryLedger::sla_breaches`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SlaBreach {
    /// Inquiry id whose 24h SLA window expired.
    pub inquiry_id: InquiryId,
    /// Original creation timestamp (ms since epoch).
    pub created_ms: u64,
    /// Elapsed time since creation (ms).
    pub elapsed_ms: u64,
}

/// Encryption configuration bound to the ledger at construction time.
///
/// Carries the tenant identifier + the HMAC search-domain key used to
/// derive the AAD `company_hash_hex`. For un-authenticated prospect
/// inquiries the `tenant_id` is set to
/// [`crate::encryption::SYSTEM_CMK_TENANT_TAG`] and the production
/// wiring binds the system CMK at `CORELINK_SYSTEM_CMK_ARN`
/// (or `CORELINK_SYSTEM_CMK_RESOURCE` for GCP) per
/// `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LedgerEncryptionConfig {
    /// Tenant id bound into the AAD context. For prospect inquiries
    /// (no tenant yet), set to [`crate::encryption::SYSTEM_CMK_TENANT_TAG`].
    pub tenant_id: String,
    /// HMAC-SHA256 search-domain key (per-deployment static secret,
    /// rotated annually per `RB-SYSTEM-CMK-ROTATION.md`).
    pub search_domain_key: Vec<u8>,
}

impl LedgerEncryptionConfig {
    /// Construct a config bound to a specific tenant.
    #[must_use]
    pub fn for_tenant(tenant_id: impl Into<String>, search_domain_key: Vec<u8>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            search_domain_key,
        }
    }

    /// Construct a config bound to the system CMK (un-authenticated
    /// prospect-side submissions).
    #[must_use]
    pub fn system_prospect(search_domain_key: Vec<u8>) -> Self {
        Self {
            tenant_id: SYSTEM_CMK_TENANT_TAG.to_string(),
            search_domain_key,
        }
    }
}

/// Enterprise inquiry ledger orchestrator.
#[derive(Clone, Debug)]
pub struct EnterpriseInquiryLedger<A, S, C, M, E>
where
    A: InquiryAuditSink + Clone,
    S: SlackClient + Clone,
    C: CrmClient + Clone,
    M: AutoReplyMailer + Clone,
    E: InquiryPayloadEncryptor + Clone,
{
    state: Arc<Mutex<LedgerState>>,
    audit: A,
    slack: S,
    crm: C,
    mailer: M,
    encryptor: E,
    encryption_config: LedgerEncryptionConfig,
}

#[derive(Debug, Default)]
struct LedgerState {
    inquiries: HashMap<InquiryId, InquiryRecord>,
    outbox: HashMap<InquiryId, OutboxRecord>,
    /// Reverse index: idempotency key → inquiry id (for dedup).
    by_idempotency: HashMap<IdempotencyKey, InquiryId>,
}

impl<A, S, C, M, E> EnterpriseInquiryLedger<A, S, C, M, E>
where
    A: InquiryAuditSink + Clone,
    S: SlackClient + Clone,
    C: CrmClient + Clone,
    M: AutoReplyMailer + Clone,
    E: InquiryPayloadEncryptor + Clone,
{
    /// Construct a new ledger backed by the five collaborators + the
    /// encryption config.
    #[must_use]
    pub fn new(
        audit: A,
        slack: S,
        crm: C,
        mailer: M,
        encryptor: E,
        encryption_config: LedgerEncryptionConfig,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(LedgerState::default())),
            audit,
            slack,
            crm,
            mailer,
            encryptor,
            encryption_config,
        }
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, LedgerState>, EnterpriseInquiryError> {
        self.state
            .lock()
            .map_err(|e| EnterpriseInquiryError::Internal(format!("ledger mutex poisoned: {e}")))
    }

    /// Submit an inquiry — runs validation + the PAT-SAGA-001 atomic
    /// Slack + CRM saga + the auto-reply mail send. Returns the
    /// receipt the caller hands back to the form submitter.
    ///
    /// # Encryption at ingress (R2-11; S-19 P1-NEW-3)
    ///
    /// The plaintext [`EnterpriseInquiryForm`] is sealed via the
    /// configured [`InquiryPayloadEncryptor`] BEFORE any persistence
    /// step. If the encryptor rejects (revoked CMK, throttled,
    /// transport failure), `submit_inquiry` returns
    /// [`EnterpriseInquiryError::Encryption`] and NO D1 row +
    /// NO outbox entry are created — atomic rollback at ingress.
    /// The in-memory [`LedgerState`] mirror only ever holds the
    /// sealed payload + sanitised surrogates; the plaintext form is
    /// dropped at the end of this stack frame.
    ///
    /// Idempotency: a second invocation with the same `idempotency_key`
    /// is a no-op that returns the original receipt without re-firing
    /// any of Slack / CRM / email.
    ///
    /// Atomicity: BOTH Slack + CRM must succeed for the outbox to
    /// advance to `Committed`. If either leg rejects, the outbox
    /// advances to `RolledBack` (the compensating Slack mark or CRM
    /// PATCH is dispatched inline) and the typed error is propagated.
    ///
    /// # Errors
    ///
    /// - [`EnterpriseInquiryError::InvalidForm`] when the form fails
    ///   schema validation.
    /// - [`EnterpriseInquiryError::Encryption`] when the BYOK seal
    ///   rejects at ingress (atomic rollback: no D1 row, no outbox).
    /// - [`EnterpriseInquiryError::Audit`] when the audit sink
    ///   rejects the pre-mutation emit (fail-CLOSED).
    /// - [`EnterpriseInquiryError::Slack`] when the Slack leg rejects.
    /// - [`EnterpriseInquiryError::Crm`] when the CRM leg rejects
    ///   (the compensating Slack ROLLBACK mark is dispatched inline;
    ///   if that compensation also fails, the error is upgraded to
    ///   [`EnterpriseInquiryError::Compensation`]).
    /// - [`EnterpriseInquiryError::AutoReply`] when the auto-reply
    ///   mail send fails after the saga committed (treated as a SLA
    ///   risk, Sev3).
    /// - [`EnterpriseInquiryError::Internal`] on mutex poisoning.
    pub fn submit_inquiry(
        &self,
        form: EnterpriseInquiryForm,
        idempotency_key: IdempotencyKey,
        inquiry_id: InquiryId,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Result<InquiryReceipt, EnterpriseInquiryError> {
        form.validate()?;
        let correlation_id = correlation_id.into();

        // Idempotency check FIRST: a repeat submission with the same
        // idempotency key returns the original receipt without
        // re-firing Slack / CRM / email.
        {
            let g = self.lock_state()?;
            if let Some(existing_id) = g.by_idempotency.get(&idempotency_key) {
                if let Some(rec) = g.inquiries.get(existing_id) {
                    return Ok(InquiryReceipt {
                        inquiry_id: rec.inquiry_id.clone(),
                        idempotency_key: rec.idempotency_key.clone(),
                        status: rec.status,
                        created_ms: rec.created_ms,
                        sla_ms: SLA_RESPONSE_MS,
                    });
                }
            }
        }

        let lead_score = form.lead_score();

        // ============================================================
        // R2-11 / S-19 P1-NEW-3 — ENCRYPT AT INGRESS.
        // Seal BEFORE any audit emit / state mutation. If the seal
        // rejects, return the typed error and exit — NO D1 row, NO
        // outbox entry, NO Slack post, NO CRM call.
        // ============================================================
        let sealed = seal_inquiry(
            &form,
            &self.encryption_config.tenant_id,
            &self.encryption_config.search_domain_key,
            &self.encryptor,
        )?;

        // Audit BEFORE the mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
        self.audit.emit(
            &InquiryAuditRecord::new(
                InquiryAuditEventType::Received,
                inquiry_id.clone(),
                ts_ms,
                correlation_id.clone(),
            )
            .with_status(InquiryStatus::Pending),
        )?;

        // Persist sealed inquiry + outbox row in `Pending` state.
        {
            let mut g = self.lock_state()?;
            let record = InquiryRecord {
                inquiry_id: inquiry_id.clone(),
                idempotency_key: idempotency_key.clone(),
                sealed: sealed.clone(),
                tenant_id: self.encryption_config.tenant_id.clone(),
                lead_score,
                status: InquiryStatus::Pending,
                created_ms: ts_ms,
                replied_ms: None,
            };
            g.inquiries.insert(inquiry_id.clone(), record);
            g.outbox.insert(
                inquiry_id.clone(),
                OutboxRecord::pending(inquiry_id.clone(), ts_ms),
            );
            g.by_idempotency
                .insert(idempotency_key.clone(), inquiry_id.clone());
        }

        // ============================================================
        // Saga PAT-SAGA-001 — atomic Slack + CRM dispatch.
        // ============================================================

        // Leg 1: Slack notification — ANONYMISED summary; no PII.
        let slack_body = anonymised_slack_summary(&inquiry_id, &sealed.sanitized, lead_score);
        let slack_message_id =
            match self
                .slack
                .post(&inquiry_id, SlackPostKind::NewInquiry, &slack_body)
            {
                Ok(id) => id,
                Err(e) => {
                    // Slack failed → no CRM POST initiated; saga
                    // rolls back immediately.
                    self.advance_outbox_to_rollback(
                        &inquiry_id,
                        None,
                        None,
                        ts_ms,
                        &correlation_id,
                    )?;
                    return Err(EnterpriseInquiryError::Slack(e));
                }
            };

        // Leg 2: CRM entry. The HubSpot adapter is the permitted
        // decryption boundary — it unseals at its own wire boundary.
        let crm_entry_id = match self.crm.create_entry(
            &inquiry_id,
            &sealed,
            &self.encryption_config.tenant_id,
            &self.encryptor,
            lead_score,
        ) {
            Ok(id) => id,
            Err(e) => {
                // CRM failed → dispatch compensating Slack ROLLBACK
                // mark. If the compensating Slack mark itself fails,
                // upgrade the error to `Compensation` so the worker
                // drain escalates the row to `PartialEscalated`.
                match self
                    .slack
                    .post(&inquiry_id, SlackPostKind::Rollback, &slack_body)
                {
                    Ok(_) => {
                        self.advance_outbox_to_rollback(
                            &inquiry_id,
                            Some(slack_message_id),
                            None,
                            ts_ms,
                            &correlation_id,
                        )?;
                        return Err(EnterpriseInquiryError::Crm(e));
                    }
                    Err(comp_err) => {
                        // Compensation failed too — partial state.
                        self.advance_outbox_to_rollback(
                            &inquiry_id,
                            Some(slack_message_id),
                            None,
                            ts_ms,
                            &correlation_id,
                        )?;
                        return Err(EnterpriseInquiryError::Compensation(format!(
                            "CRM rejected ({e}); compensating Slack mark also rejected ({comp_err})"
                        )));
                    }
                }
            }
        };

        // Both legs succeeded → audit + commit the outbox.
        self.audit.emit(
            &InquiryAuditRecord::new(
                InquiryAuditEventType::AtomicOk,
                inquiry_id.clone(),
                ts_ms,
                correlation_id.clone(),
            )
            .with_status(InquiryStatus::Committed),
        )?;

        {
            let mut g = self.lock_state()?;
            if let Some(rec) = g.inquiries.get_mut(&inquiry_id) {
                rec.status = InquiryStatus::Committed;
            }
            if let Some(row) = g.outbox.get_mut(&inquiry_id) {
                row.commit(slack_message_id, crm_entry_id, ts_ms);
            }
        }

        // Auto-reply email (permitted decryption boundary; restricted
        // to email + locale, no other PII fields surfaced).  This call
        // uses the IN-SCOPE plaintext form variable (still alive on
        // the stack); the mailer adapter never reads from the ledger
        // mirror, which only holds the sealed payload.
        match self.mailer.send(&inquiry_id, &form.email, &form.locale) {
            Ok(()) => {
                self.audit.emit(&InquiryAuditRecord::new(
                    InquiryAuditEventType::AutoReplySent,
                    inquiry_id.clone(),
                    ts_ms,
                    correlation_id.clone(),
                ))?;
                let mut g = self.lock_state()?;
                if let Some(rec) = g.inquiries.get_mut(&inquiry_id) {
                    rec.replied_ms = Some(ts_ms);
                }
            }
            Err(e) => {
                return Err(EnterpriseInquiryError::AutoReply(e));
            }
        }

        // `form` goes out of scope here — plaintext PII is dropped.
        drop(form);
        // `sealed` is the same value already cloned into the ledger
        // state; the local clone is dropped too.
        drop(sealed);

        Ok(InquiryReceipt {
            inquiry_id,
            idempotency_key,
            status: InquiryStatus::Committed,
            created_ms: ts_ms,
            sla_ms: SLA_RESPONSE_MS,
        })
    }

    /// Outbox → RolledBack helper: audit BEFORE mutation + update
    /// state in a single lock acquisition.
    fn advance_outbox_to_rollback(
        &self,
        inquiry_id: &InquiryId,
        slack_message_id: Option<SlackMessageId>,
        crm_entry_id: Option<CrmEntryId>,
        ts_ms: u64,
        correlation_id: &str,
    ) -> Result<(), EnterpriseInquiryError> {
        self.audit.emit(
            &InquiryAuditRecord::new(
                InquiryAuditEventType::AtomicRollback,
                inquiry_id.clone(),
                ts_ms,
                correlation_id,
            )
            .with_status(InquiryStatus::RolledBack),
        )?;

        let mut g = self.lock_state()?;
        if let Some(row) = g.outbox.get_mut(inquiry_id) {
            row.rollback(slack_message_id, crm_entry_id, ts_ms);
        }
        if let Some(rec) = g.inquiries.get_mut(inquiry_id) {
            rec.status = InquiryStatus::RolledBack;
        }
        Ok(())
    }

    /// Worker drain hook: scan the outbox for `Pending` rows older
    /// than `partial_state_ceiling_ms`. Each surfaced row is
    /// escalated to `PartialEscalated` + audited via the
    /// `PartialEscalated` arm + returned to the caller for SEV-2
    /// alerting per `RB-FM-ENTERPRISE-HANDOFF-PARTIAL`.
    ///
    /// # Errors
    ///
    /// Returns [`EnterpriseInquiryError::Audit`] when the
    /// pre-mutation audit emit fails (fail-CLOSED).
    pub fn drain_partial_escalations(
        &self,
        now_ms: u64,
        partial_state_ceiling_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Result<Vec<InquiryId>, EnterpriseInquiryError> {
        let correlation_id = correlation_id.into();
        // Snapshot ids to escalate WITHOUT holding the lock across
        // audit emits.
        let to_escalate: Vec<InquiryId> = {
            let g = self.lock_state()?;
            g.outbox
                .values()
                .filter(|r| {
                    r.status == OutboxStatus::Pending
                        && now_ms.saturating_sub(r.created_ms) >= partial_state_ceiling_ms
                })
                .map(|r| r.inquiry_id.clone())
                .collect()
        };

        let mut escalated = Vec::with_capacity(to_escalate.len());
        for id in to_escalate {
            self.audit.emit(
                &InquiryAuditRecord::new(
                    InquiryAuditEventType::PartialEscalated,
                    id.clone(),
                    now_ms,
                    correlation_id.clone(),
                )
                .with_status(InquiryStatus::PartialEscalated),
            )?;
            let mut g = self.lock_state()?;
            if let Some(row) = g.outbox.get_mut(&id) {
                row.escalate_partial(now_ms);
            }
            if let Some(rec) = g.inquiries.get_mut(&id) {
                rec.status = InquiryStatus::PartialEscalated;
            }
            escalated.push(id);
        }
        Ok(escalated)
    }

    /// 24h SLA breach detector. Surfaces every committed-but-still-
    /// unanswered inquiry whose creation timestamp is older than the
    /// SLA window at `now_ms`. Each surfaced breach is also audited
    /// via the `SlaBreached` arm + flagged for Sev2 escalation per
    /// WI §21 (`corelink_onboarding_slack_crm_atomicity_violations_total`
    /// + auto-reply SLA breach metric).
    ///
    /// # Errors
    ///
    /// Returns [`EnterpriseInquiryError::Audit`] when the
    /// pre-mutation audit emit fails (fail-CLOSED).
    pub fn sla_breaches(
        &self,
        now_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Result<Vec<SlaBreach>, EnterpriseInquiryError> {
        let correlation_id = correlation_id.into();
        let breaches: Vec<SlaBreach> = {
            let g = self.lock_state()?;
            g.inquiries
                .values()
                .filter(|r| {
                    r.replied_ms.is_none()
                        && r.status != InquiryStatus::RolledBack
                        && r.status != InquiryStatus::PartialEscalated
                        && now_ms.saturating_sub(r.created_ms) >= SLA_RESPONSE_MS
                })
                .map(|r| SlaBreach {
                    inquiry_id: r.inquiry_id.clone(),
                    created_ms: r.created_ms,
                    elapsed_ms: now_ms.saturating_sub(r.created_ms),
                })
                .collect()
        };

        for b in &breaches {
            self.audit.emit(&InquiryAuditRecord::new(
                InquiryAuditEventType::SlaBreached,
                b.inquiry_id.clone(),
                now_ms,
                correlation_id.clone(),
            ))?;
        }
        Ok(breaches)
    }

    /// Record a `SpamBlocked` audit (reCAPTCHA / rate-limit / bot
    /// detection rejected the submission). The form is NOT persisted
    /// and no Slack / CRM / email is dispatched.
    ///
    /// # Errors
    ///
    /// Returns [`EnterpriseInquiryError::Audit`] when the audit sink
    /// rejects the emit (fail-CLOSED).
    pub fn record_spam_blocked(
        &self,
        inquiry_id: InquiryId,
        ts_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Result<(), EnterpriseInquiryError> {
        self.audit.emit(&InquiryAuditRecord::new(
            InquiryAuditEventType::SpamBlocked,
            inquiry_id,
            ts_ms,
            correlation_id,
        ))?;
        Ok(())
    }

    /// Snapshot the inquiry record (for tests / Grafana hydration).
    #[must_use]
    pub fn get_inquiry(&self, inquiry_id: &InquiryId) -> Option<InquiryRecord> {
        let g = self.state.lock().ok()?;
        g.inquiries.get(inquiry_id).cloned()
    }

    /// Snapshot the outbox row (for tests / worker drain).
    #[must_use]
    pub fn get_outbox(&self, inquiry_id: &InquiryId) -> Option<OutboxRecord> {
        let g = self.state.lock().ok()?;
        g.outbox.get(inquiry_id).cloned()
    }

    /// Snapshot all inquiry ids (for tests / Grafana hydration).
    #[must_use]
    pub fn inquiry_ids(&self) -> Vec<InquiryId> {
        match self.state.lock() {
            Ok(g) => g.inquiries.keys().cloned().collect(),
            Err(p) => p.into_inner().inquiries.keys().cloned().collect(),
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
#[path = "tests.rs"]
mod tests;

//! [`ConsentLedger`] trait and [`InMemoryConsentLedger`] orchestrator.
//!
//! # Pipeline (canonical audit-fail-CLOSED ordering — S-06 P0-2 lesson)
//!
//! ## Grant (`grant_consent`)
//!
//! 1. Locale enforce (CTRL-PRIV-CONSENT-005) → 422 on mismatch.
//! 2. Notice version check (CTRL-PRIV-CONSENT-005) → stale error.
//! 3. Idempotency lookup (UNIQUE 5-tuple) — if exists, return replay=true.
//! 4. **emit_audit** `consent.granted.v1` BEFORE store insert.
//! 5. HMAC sign.
//! 6. Store insert.
//! 7. Return `ConsentGrantReceipt`.
//!
//! ## Revoke (`revoke_consent`)
//!
//! 1. Locale enforce.
//! 2. Validate purpose is revocable (basis = consent).
//! 3. Lookup active grant (for FK).
//! 4. **emit_audit** `consent.revoked.v1` BEFORE store insert.
//! 5. HMAC sign (symmetric — uses same 6-field proof).
//! 6. Store insert.
//! 7. Enqueue cascade task.
//! 8. Return `ConsentRevokeReceipt`.
//!
//! ## Verify (`verify_grant_signature` / `verify_revocation_signature`)
//!
//! Stateless HMAC re-derive — no DB lookup (AC-004). Returns
//! `VerifyResponse { valid, ... }`.
//!
//! # Fail-CLOSED guarantee
//!
//! If audit emit fails, the store insert is NOT executed and the request
//! returns 503. State is UNCHANGED. Verified by `chaos_audit_emit_failure`
//! test (INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116).

use std::sync::{Arc, Mutex};

use crate::{
    audit_emit::{ConsentAuditEventType, ConsentAuditRecord, ConsentAuditSink},
    cascade::{CascadeSink, CascadeTask},
    error::ConsentLedgerError,
    hmac_sign::{ConsentHmacSigner, HmacParams},
    locale_enforce::enforce_locale,
    notice_version_check::check_notice_version,
    schema::{
        ConsentEntry, ConsentGrantReceipt, ConsentListResponse, ConsentProofPayload,
        ConsentPurpose, ConsentRevokeReceipt, RevocationEntry, VerifyResponse,
    },
    store::{ConsentRecord, ConsentRevocationRecord, ConsentStore},
};

/// Parameters for the stateless verify endpoints.
///
/// Groups the fields that make up the verify preimage to avoid
/// `too_many_arguments` clippy lint on [`ConsentLedger::verify_grant_signature`].
#[derive(Debug, Clone)]
pub struct VerifyParams<'a> {
    /// Consent or revocation record ID.
    pub record_id: &'a str,
    /// Tenant identifier.
    pub tenant_id: &'a str,
    /// Purpose string.
    pub purpose: &'a str,
    /// SHA-256 hex of the notice text.
    pub notice_text_hash: &'a str,
    /// Semver notice version.
    pub notice_version: &'a str,
    /// BCP-47 locale string.
    pub locale: &'a str,
    /// Wording variant ID.
    pub wording_id: &'a str,
    /// UI capture timestamp.
    pub ui_capture_ts: &'a str,
    /// Server submission timestamp.
    pub submission_ts: &'a str,
    /// HMAC-SHA256 hex-64 signature to verify.
    pub signature_hex: &'a str,
    /// Key identifier.
    pub kid: &'a str,
}

/// SHA-256 of the subject ID for audit-safe redaction (CTRL-PRIV-014).
fn hash_subject_id(subject_id: &str, tenant_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(subject_id.as_bytes());
    hasher.update(b"||");
    hasher.update(tenant_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a pseudo-ULID for in-memory tests.
///
/// Production wiring will use a real ULID library; for tests we use a
/// deterministic counter to avoid pulling wasm32-incompatible RNG.
fn new_ulid(prefix: &str, seq: u64) -> String {
    format!("{prefix}-{seq:020}")
}

/// ISO 8601 UTC "now" placeholder.
fn now_iso8601(seq: u64) -> String {
    format!("2026-05-13T10:00:{:02}Z", seq % 60)
}

/// Current canonical notice version for tests.
pub const CANONICAL_NOTICE_VERSION: &str = "1.0.0";

/// Cascade SLA in seconds (24h).
pub const CASCADE_SLA_SECONDS: u64 = 86_400;

/// Consent Ledger trait — 5 REST endpoint surface.
pub trait ConsentLedger: Send + Sync {
    /// `POST /v1/consent/<purpose>` — capture consent grant with 6-field proof.
    ///
    /// Idempotency: `(tenant_id, subject_id, purpose, notice_text_hash,
    /// ui_capture_ts)` UNIQUE.
    fn grant_consent(
        &self,
        tenant_id: &str,
        subject_id: &str,
        accept_language: &str,
        purpose: ConsentPurpose,
        proof: ConsentProofPayload,
    ) -> Result<ConsentGrantReceipt, ConsentLedgerError>;

    /// `DELETE /v1/consent/<purpose>` — capture consent revoke with symmetric
    /// schema (Lote 9.4 H-05). Cascade unsubscribe ≤24h.
    fn revoke_consent(
        &self,
        tenant_id: &str,
        subject_id: &str,
        accept_language: &str,
        purpose: ConsentPurpose,
        revoke_proof: ConsentProofPayload,
    ) -> Result<ConsentRevokeReceipt, ConsentLedgerError>;

    /// `GET /v1/consent` — list all grants + revocations for a subject.
    fn list_consents(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<ConsentListResponse, ConsentLedgerError>;

    /// `GET /v1/consent/verify?signature=X&consent_id=Y&tenant_short=Z` —
    /// stateless public verify (no DB lookup; HMAC re-derive only).
    fn verify_grant_signature(
        &self,
        params: &VerifyParams<'_>,
    ) -> Result<VerifyResponse, ConsentLedgerError>;

    /// `GET /v1/consent/revocation/verify?revocation_id=X` — stateless
    /// public verify for revocations (Lote 9.4 H-05 symmetric).
    fn verify_revocation_signature(
        &self,
        params: &VerifyParams<'_>,
    ) -> Result<VerifyResponse, ConsentLedgerError>;
}

/// In-memory Consent Ledger orchestrator.
///
/// Uses `Arc<Mutex<u64>>` for sequence counter — F-001 closure
/// (per-instance, NEVER `static LazyLock`).
pub struct InMemoryConsentLedger {
    store: Arc<dyn ConsentStore>,
    audit_sink: Arc<dyn ConsentAuditSink>,
    signer: Arc<dyn ConsentHmacSigner>,
    cascade_sink: Arc<dyn CascadeSink>,
    /// Current canonical notice version for stale check.
    current_notice_version: String,
    /// Monotonic sequence for pseudo-ULID generation.
    seq: Arc<Mutex<u64>>,
}

impl std::fmt::Debug for InMemoryConsentLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryConsentLedger")
            .field("current_notice_version", &self.current_notice_version)
            .finish_non_exhaustive()
    }
}

impl InMemoryConsentLedger {
    /// Create a new orchestrator.
    pub fn new(
        store: Arc<dyn ConsentStore>,
        audit_sink: Arc<dyn ConsentAuditSink>,
        signer: Arc<dyn ConsentHmacSigner>,
        cascade_sink: Arc<dyn CascadeSink>,
        current_notice_version: impl Into<String>,
    ) -> Self {
        Self {
            store,
            audit_sink,
            signer,
            cascade_sink,
            current_notice_version: current_notice_version.into(),
            seq: Arc::new(Mutex::new(0)),
        }
    }

    fn next_seq(&self) -> u64 {
        let mut guard = self.seq.lock().unwrap_or_else(|p| p.into_inner());
        let s = *guard;
        *guard += 1;
        s
    }
}

impl ConsentLedger for InMemoryConsentLedger {
    fn grant_consent(
        &self,
        tenant_id: &str,
        subject_id: &str,
        accept_language: &str,
        purpose: ConsentPurpose,
        proof: ConsentProofPayload,
    ) -> Result<ConsentGrantReceipt, ConsentLedgerError> {
        // Step 1: locale enforcement (CTRL-PRIV-CONSENT-005; AC-007)
        enforce_locale(accept_language, proof.locale).map_err(|e| {
            ConsentLedgerError::LocaleMismatch(e.to_string())
        })?;

        // Step 2: notice version check (CTRL-PRIV-CONSENT-005; AC-006)
        check_notice_version(&self.current_notice_version, &proof.notice_version)
            .map_err(|e| ConsentLedgerError::NoticeVersionStale {
                current: e.current_major,
                submitted: e.submitted_major,
            })?;

        let seq = self.next_seq();
        let consent_id = new_ulid("grant", seq);
        let subject_id_hash = hash_subject_id(subject_id, tenant_id);

        // Step 3: audit emit BEFORE store insert (fail-CLOSED; S-06 P0-2)
        let audit_record = ConsentAuditRecord {
            event_type: ConsentAuditEventType::Granted,
            record_id: consent_id.clone(),
            tenant_id: tenant_id.to_owned(),
            subject_id_hash: subject_id_hash.clone(),
            purpose: purpose.to_string(),
            submission_ts: proof.submission_ts.clone(),
        };
        self.audit_sink.emit(audit_record)?;

        // Step 4: HMAC sign
        let purpose_str = purpose.to_string();
        let hmac_params = HmacParams {
            record_id: &consent_id,
            tenant_id,
            purpose: &purpose_str,
            notice_text_hash: &proof.notice_text_hash,
            notice_version: &proof.notice_version,
            locale: proof.locale.as_str(),
            wording_id: &proof.wording_id,
            ui_capture_ts: &proof.ui_capture_ts,
            submission_ts: &proof.submission_ts,
        };
        let sig = self.signer.sign(&hmac_params)?;

        // Step 5: store insert (idempotency by 5-tuple)
        let record = ConsentRecord {
            consent_id: consent_id.clone(),
            tenant_id: tenant_id.to_owned(),
            subject_id: subject_id.to_owned(),
            subject_id_hash,
            purpose: purpose.to_string(),
            basis_legal: purpose.legal_basis().to_string(),
            proof,
            signature: sig.hex.clone(),
            signature_kid: sig.kid.clone(),
            evidence_screenshot_hash: None,
            ip_country: None,
            user_agent_class: None,
            stale_consent: false,
        };
        let (stored, was_inserted) = self.store.store_grant(record)?;

        let verify_url = format!(
            "/v1/consent/verify?signature={}&consent_id={}&tenant_short={}",
            sig.hex, stored.consent_id, &tenant_id[..tenant_id.len().min(8)]
        );

        Ok(ConsentGrantReceipt {
            consent_id: stored.consent_id,
            signature: stored.signature,
            verify_url,
            replay: !was_inserted,
        })
    }

    fn revoke_consent(
        &self,
        tenant_id: &str,
        subject_id: &str,
        accept_language: &str,
        purpose: ConsentPurpose,
        revoke_proof: ConsentProofPayload,
    ) -> Result<ConsentRevokeReceipt, ConsentLedgerError> {
        // Step 1: locale enforcement
        enforce_locale(accept_language, revoke_proof.locale).map_err(|e| {
            ConsentLedgerError::LocaleMismatch(e.to_string())
        })?;

        // Step 2: validate purpose is revocable
        if !purpose.is_revocable() {
            return Err(ConsentLedgerError::NotRevocable(format!(
                "purpose '{}' has basis '{}' which is not revocable",
                purpose,
                purpose.legal_basis()
            )));
        }

        // Step 3: lookup active grant (for FK — NULL-safe if never granted)
        let active_grant = self
            .store
            .get_active_grant(tenant_id, subject_id, &purpose.to_string())?;
        let revokes_consent_id = active_grant.map(|r| r.consent_id);

        let seq = self.next_seq();
        let revocation_id = new_ulid("revoke", seq);
        let subject_id_hash = hash_subject_id(subject_id, tenant_id);
        let cascade_started_at = now_iso8601(seq);
        let cascade_eta = format!(
            "cascade+{}s-from-{}",
            CASCADE_SLA_SECONDS, cascade_started_at
        );

        // Step 4: audit emit BEFORE store insert (fail-CLOSED)
        let audit_record = ConsentAuditRecord {
            event_type: ConsentAuditEventType::Revoked,
            record_id: revocation_id.clone(),
            tenant_id: tenant_id.to_owned(),
            subject_id_hash: subject_id_hash.clone(),
            purpose: purpose.to_string(),
            submission_ts: revoke_proof.submission_ts.clone(),
        };
        self.audit_sink.emit(audit_record)?;

        // Step 5: HMAC sign (symmetric — same 6-field structure as grant)
        let purpose_str = purpose.to_string();
        let hmac_params = HmacParams {
            record_id: &revocation_id,
            tenant_id,
            purpose: &purpose_str,
            notice_text_hash: &revoke_proof.notice_text_hash,
            notice_version: &revoke_proof.notice_version,
            locale: revoke_proof.locale.as_str(),
            wording_id: &revoke_proof.wording_id,
            ui_capture_ts: &revoke_proof.ui_capture_ts,
            submission_ts: &revoke_proof.submission_ts,
        };
        let sig = self.signer.sign(&hmac_params)?;

        // Step 6: store insert
        let record = ConsentRevocationRecord {
            revocation_id: revocation_id.clone(),
            tenant_id: tenant_id.to_owned(),
            subject_id: subject_id.to_owned(),
            subject_id_hash,
            purpose: purpose.to_string(),
            revokes_consent_id,
            proof: revoke_proof,
            signature: sig.hex.clone(),
            signature_kid: sig.kid.clone(),
            cascade_started_at: cascade_started_at.clone(),
            cascade_completed_at: None,
            cascade_status: "pending".to_owned(),
            ip_country: None,
            user_agent_class: None,
        };
        let stored = self.store.store_revocation(record)?;

        // Step 7: enqueue cascade task (async; does NOT block response)
        let cascade_task = CascadeTask {
            revocation_id: stored.revocation_id.clone(),
            tenant_id: tenant_id.to_owned(),
            subject_id_hash: stored.subject_id_hash.clone(),
            purpose: stored.purpose.clone(),
            cascade_started_at,
            cascade_eta: cascade_eta.clone(),
        };
        self.cascade_sink.enqueue(cascade_task)?;

        Ok(ConsentRevokeReceipt {
            revocation_id: stored.revocation_id,
            signature: stored.signature,
            cascade_eta,
        })
    }

    fn list_consents(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<ConsentListResponse, ConsentLedgerError> {
        let grants = self.store.list_grants(tenant_id, subject_id)?;
        let revocations = self.store.list_revocations(tenant_id, subject_id)?;

        let consents: Vec<ConsentEntry> = grants
            .into_iter()
            .map(|r| {
                let verify_url = format!(
                    "/v1/consent/verify?signature={}&consent_id={}&tenant_short={}",
                    r.signature,
                    r.consent_id,
                    &tenant_id[..tenant_id.len().min(8)]
                );
                ConsentEntry {
                    consent_id: r.consent_id,
                    purpose: r.purpose,
                    basis_legal: r.basis_legal,
                    proof: r.proof,
                    signature: r.signature,
                    verify_url,
                }
            })
            .collect();

        let revocation_entries: Vec<RevocationEntry> = revocations
            .into_iter()
            .map(|r| RevocationEntry {
                revocation_id: r.revocation_id,
                purpose: r.purpose,
                proof: r.proof,
                signature: r.signature,
                cascade_status: r.cascade_status,
            })
            .collect();

        Ok(ConsentListResponse {
            consents,
            revocations: revocation_entries,
        })
    }

    fn verify_grant_signature(
        &self,
        params: &VerifyParams<'_>,
    ) -> Result<VerifyResponse, ConsentLedgerError> {
        let hmac_params = HmacParams {
            record_id: params.record_id,
            tenant_id: params.tenant_id,
            purpose: params.purpose,
            notice_text_hash: params.notice_text_hash,
            notice_version: params.notice_version,
            locale: params.locale,
            wording_id: params.wording_id,
            ui_capture_ts: params.ui_capture_ts,
            submission_ts: params.submission_ts,
        };
        let valid = self
            .signer
            .verify(params.signature_hex, params.kid, &hmac_params)?;

        let tenant_short_id = params.tenant_id[..params.tenant_id.len().min(8)].to_owned();

        if valid {
            Ok(VerifyResponse {
                valid: true,
                record_id: params.record_id.to_owned(),
                purpose: params.purpose.to_owned(),
                notice_version: params.notice_version.to_owned(),
                locale: params.locale.to_owned(),
                submission_ts: params.submission_ts.to_owned(),
                tenant_short_id,
                error: None,
            })
        } else {
            Ok(VerifyResponse {
                valid: false,
                record_id: params.record_id.to_owned(),
                purpose: params.purpose.to_owned(),
                notice_version: params.notice_version.to_owned(),
                locale: params.locale.to_owned(),
                submission_ts: params.submission_ts.to_owned(),
                tenant_short_id,
                error: Some("invalid_signature".to_owned()),
            })
        }
    }

    fn verify_revocation_signature(
        &self,
        params: &VerifyParams<'_>,
    ) -> Result<VerifyResponse, ConsentLedgerError> {
        // Symmetric verify — same HMAC logic as grant (Lote 9.4 H-05)
        self.verify_grant_signature(params)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::Arc;

    use crate::{
        audit_emit::InMemoryConsentAuditSink,
        cascade::InMemoryCascadeSink,
        hmac_sign::InMemoryConsentHmacSigner,
        ledger::{ConsentLedger, InMemoryConsentLedger, VerifyParams},
        schema::{ConsentProofPayload, ConsentPurpose, LocaleBcp47},
        store::InMemoryConsentStore,
    };

    fn make_ledger() -> (InMemoryConsentLedger, Arc<InMemoryConsentAuditSink>, Arc<InMemoryCascadeSink>) {
        let store = Arc::new(InMemoryConsentStore::new());
        let audit = Arc::new(InMemoryConsentAuditSink::new());
        let signer = Arc::new(InMemoryConsentHmacSigner::new_test());
        let cascade = Arc::new(InMemoryCascadeSink::new());
        let ledger = InMemoryConsentLedger::new(
            store,
            audit.clone(),
            signer,
            cascade.clone(),
            "1.0.0",
        );
        (ledger, audit, cascade)
    }

    fn proof(locale: LocaleBcp47) -> ConsentProofPayload {
        ConsentProofPayload {
            notice_text_hash: "abc123".to_owned(),
            notice_version: "1.0.0".to_owned(),
            locale,
            wording_id: "wording-v1".to_owned(),
            ui_capture_ts: "2026-05-13T10:00:00Z".to_owned(),
            submission_ts: "2026-05-13T10:00:02Z".to_owned(),
        }
    }

    #[test]
    fn grant_happy_path() {
        let (ledger, audit, _) = make_ledger();
        let receipt = ledger
            .grant_consent(
                "tenant-01",
                "subject-01",
                "pt-BR",
                ConsentPurpose::AnalyticsPersonalized,
                proof(LocaleBcp47::PtBr),
            )
            .unwrap();
        assert!(!receipt.replay);
        assert!(!receipt.signature.is_empty());
        assert!(receipt.verify_url.contains("verify"));
        let events = audit.captured();
        assert_eq!(events.len(), 1);
        let first = events.first().expect("one event expected");
        assert_eq!(
            first.event_type.as_cloudevent_type(),
            "dev.hugr.corelink.consent.granted.v1"
        );
    }

    #[test]
    fn grant_idempotency_replay() {
        let (ledger, _, _) = make_ledger();
        let p = proof(LocaleBcp47::PtBr);
        let r1 = ledger
            .grant_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, p.clone())
            .unwrap();
        let r2 = ledger
            .grant_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, p)
            .unwrap();
        assert!(!r1.replay);
        assert!(r2.replay);
        assert_eq!(r1.consent_id, r2.consent_id);
    }

    #[test]
    fn revoke_happy_path() {
        let (ledger, audit, cascade) = make_ledger();
        // First grant
        ledger
            .grant_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, proof(LocaleBcp47::PtBr))
            .unwrap();
        // Then revoke
        let revoke_proof = ConsentProofPayload {
            notice_text_hash: "unsubscribe-hash".to_owned(),
            notice_version: "1.0.0".to_owned(),
            locale: LocaleBcp47::PtBr,
            wording_id: "unsubscribe-v1".to_owned(),
            ui_capture_ts: "2026-05-13T11:00:00Z".to_owned(),
            submission_ts: "2026-05-13T11:00:02Z".to_owned(),
        };
        let receipt = ledger
            .revoke_consent("t", "s", "pt-BR", ConsentPurpose::MarketingEmail, revoke_proof)
            .unwrap();
        assert!(!receipt.revocation_id.is_empty());
        let events = audit.captured();
        assert_eq!(events.len(), 2);
        let second = events.get(1).expect("second event expected");
        assert_eq!(
            second.event_type.as_cloudevent_type(),
            "dev.hugr.corelink.consent.revoked.v1"
        );
        assert_eq!(cascade.captured().len(), 1);
    }

    #[test]
    fn non_revocable_purpose_rejects() {
        let (ledger, _, _) = make_ledger();
        let err = ledger
            .revoke_consent(
                "t",
                "s",
                "pt-BR",
                ConsentPurpose::ServiceDelivery,
                proof(LocaleBcp47::PtBr),
            )
            .unwrap_err();
        let valid = matches!(err, crate::error::ConsentLedgerError::NotRevocable(_));
        assert!(valid, "expected NotRevocable error");
    }

    #[test]
    fn locale_mismatch_rejects() {
        let (ledger, _, _) = make_ledger();
        let err = ledger
            .grant_consent(
                "t",
                "s",
                "en-US",                              // header
                ConsentPurpose::MarketingEmail,
                proof(LocaleBcp47::PtBr),             // payload says pt-BR
            )
            .unwrap_err();
        let valid = matches!(err, crate::error::ConsentLedgerError::LocaleMismatch(_));
        assert!(valid, "expected LocaleMismatch error");
    }

    #[test]
    fn verify_signature_roundtrip() {
        let (ledger, _, _) = make_ledger();
        let receipt = ledger
            .grant_consent(
                "tenant-01",
                "subject-01",
                "pt-BR",
                ConsentPurpose::AnalyticsPersonalized,
                proof(LocaleBcp47::PtBr),
            )
            .unwrap();
        let vp = VerifyParams {
            record_id: &receipt.consent_id,
            tenant_id: "tenant-01",
            purpose: "analytics_personalized",
            notice_text_hash: "abc123",
            notice_version: "1.0.0",
            locale: "pt-BR",
            wording_id: "wording-v1",
            ui_capture_ts: "2026-05-13T10:00:00Z",
            submission_ts: "2026-05-13T10:00:02Z",
            signature_hex: &receipt.signature,
            kid: "kid",
        };
        let resp = ledger.verify_grant_signature(&vp).unwrap();
        assert!(resp.valid);
        assert!(resp.error.is_none());
    }
}

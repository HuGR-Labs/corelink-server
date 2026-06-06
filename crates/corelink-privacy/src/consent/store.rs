//! Consent store trait and in-memory implementation.
//!
//! Production wiring (Neon `consent_ledger` + `consent_revocation` tables,
//! DDL §6.1.7) is deferred to WI-S11-008 PRR ship gate per
//! `trait-abstraction-defer` charter pattern.
//!
//! # Idempotency
//!
//! UNIQUE 5-tuple `(tenant_id, subject_id, purpose, notice_text_hash,
//! ui_capture_ts)` guards replay safety (PAT-RETRY-IDEMPOTENT-001).
//! `store_grant` returns `(record, was_inserted: bool)` — callers set
//! `ConsentGrantReceipt.replay = !was_inserted`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::error::ConsentLedgerError;
use super::schema::ConsentProofPayload;

/// Durable consent grant record (mirrors `consent_ledger` Neon table).
#[derive(Debug, Clone)]
pub struct ConsentRecord {
    /// ULID 26 chars.
    pub consent_id: String,
    /// UUIDv7 hex-32.
    pub tenant_id: String,
    /// UUIDv7 hex-32.
    pub subject_id: String,
    /// sha256(subject_id || tenant_salt) — audit-safe redacted form.
    pub subject_id_hash: String,
    /// Purpose SQL string value.
    pub purpose: String,
    /// Legal basis string.
    pub basis_legal: String,
    /// 6-field proof payload.
    pub proof: ConsentProofPayload,
    /// HMAC-SHA256 hex-64 tenant-scoped signature.
    pub signature: String,
    /// Key identifier for rotation.
    pub signature_kid: String,
    /// Optional R2 hash-addressed screenshot key (CTRL-PRIV-CONSENT-006).
    pub evidence_screenshot_hash: Option<String>,
    /// ISO 3166-1 alpha-2 country (CTRL-PRIV-001; NOT raw IP).
    pub ip_country: Option<String>,
    /// Top-20 user agent class enum string (CTRL-PRIV-012).
    pub user_agent_class: Option<String>,
    /// `true` when this record has been superseded by a major notice
    /// version bump (notice_version_check.rs AC-006).
    pub stale_consent: bool,
}

/// Durable consent revocation record (mirrors `consent_revocation` Neon table).
#[derive(Debug, Clone)]
pub struct ConsentRevocationRecord {
    /// ULID 26 chars.
    pub revocation_id: String,
    /// Tenant identifier.
    pub tenant_id: String,
    /// Subject identifier.
    pub subject_id: String,
    /// sha256(subject_id || tenant_salt).
    pub subject_id_hash: String,
    /// Purpose SQL string value.
    pub purpose: String,
    /// FK to the grant being revoked (NULL if purpose never granted).
    pub revokes_consent_id: Option<String>,
    /// 6-field revocation proof (symmetric with grant).
    pub proof: ConsentProofPayload,
    /// HMAC-SHA256 hex-64 revocation signature.
    pub signature: String,
    /// Key identifier.
    pub signature_kid: String,
    /// ISO 8601 UTC timestamp when cascade fanout was triggered.
    pub cascade_started_at: String,
    /// ISO 8601 UTC timestamp when all downstream systems confirmed.
    pub cascade_completed_at: Option<String>,
    /// Cascade status string.
    pub cascade_status: String,
    /// Country code.
    pub ip_country: Option<String>,
    /// User agent class.
    pub user_agent_class: Option<String>,
}

/// Consent store trait — Neon mirror surface.
pub trait ConsentStore: Send + Sync {
    /// Insert or retrieve an existing consent record (idempotency by 5-tuple).
    ///
    /// Returns `(record, was_inserted)`. If `was_inserted = false`, the
    /// caller MUST set `ConsentGrantReceipt.replay = true`.
    fn store_grant(
        &self,
        record: ConsentRecord,
    ) -> Result<(ConsentRecord, bool), ConsentLedgerError>;

    /// Insert a revocation record (idempotency by 5-tuple).
    fn store_revocation(
        &self,
        record: ConsentRevocationRecord,
    ) -> Result<ConsentRevocationRecord, ConsentLedgerError>;

    /// Return the latest active grant for `(tenant_id, subject_id, purpose)`.
    fn get_active_grant(
        &self,
        tenant_id: &str,
        subject_id: &str,
        purpose: &str,
    ) -> Result<Option<ConsentRecord>, ConsentLedgerError>;

    /// Return all grant records for a subject, ordered submission_ts DESC.
    fn list_grants(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<Vec<ConsentRecord>, ConsentLedgerError>;

    /// Return all revocation records for a subject, ordered submission_ts DESC.
    fn list_revocations(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<Vec<ConsentRevocationRecord>, ConsentLedgerError>;

    /// Mark a grant record as stale (notice major version bump, AC-006).
    fn mark_grant_stale(&self, consent_id: &str) -> Result<(), ConsentLedgerError>;
}

/// Idempotency key for grant records.
type GrantIdempotencyKey = (String, String, String, String, String);

/// In-memory consent store.
///
/// Uses `Arc<Mutex<_>>` F-001 closure (per-instance, NEVER `static LazyLock`).
#[derive(Debug, Clone)]
pub struct InMemoryConsentStore {
    grants: Arc<Mutex<HashMap<String, ConsentRecord>>>,
    revocations: Arc<Mutex<HashMap<String, ConsentRevocationRecord>>>,
    /// Idempotency index: (tenant_id, subject_id, purpose, notice_text_hash,
    /// ui_capture_ts) → consent_id
    grant_index: Arc<Mutex<HashMap<GrantIdempotencyKey, String>>>,
}

impl InMemoryConsentStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            grants: Arc::new(Mutex::new(HashMap::new())),
            revocations: Arc::new(Mutex::new(HashMap::new())),
            grant_index: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn lock_grants(&self) -> std::sync::MutexGuard<'_, HashMap<String, ConsentRecord>> {
        self.grants.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_revocations(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, ConsentRevocationRecord>> {
        self.revocations.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_grant_index(&self) -> std::sync::MutexGuard<'_, HashMap<GrantIdempotencyKey, String>> {
        self.grant_index.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Default for InMemoryConsentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsentStore for InMemoryConsentStore {
    fn store_grant(
        &self,
        record: ConsentRecord,
    ) -> Result<(ConsentRecord, bool), ConsentLedgerError> {
        let idem_key: GrantIdempotencyKey = (
            record.tenant_id.clone(),
            record.subject_id.clone(),
            record.purpose.clone(),
            record.proof.notice_text_hash.clone(),
            record.proof.ui_capture_ts.clone(),
        );

        let mut index = self.lock_grant_index();
        if let Some(existing_id) = index.get(&idem_key) {
            let grants = self.lock_grants();
            let existing = grants.get(existing_id).ok_or_else(|| {
                ConsentLedgerError::Internal("idempotency index inconsistency".to_owned())
            })?;
            return Ok((existing.clone(), false));
        }

        let consent_id = record.consent_id.clone();
        index.insert(idem_key, consent_id.clone());
        drop(index);

        let mut grants = self.lock_grants();
        grants.insert(consent_id, record.clone());
        Ok((record, true))
    }

    fn store_revocation(
        &self,
        record: ConsentRevocationRecord,
    ) -> Result<ConsentRevocationRecord, ConsentLedgerError> {
        let mut revocations = self.lock_revocations();
        revocations.insert(record.revocation_id.clone(), record.clone());
        Ok(record)
    }

    fn get_active_grant(
        &self,
        tenant_id: &str,
        subject_id: &str,
        purpose: &str,
    ) -> Result<Option<ConsentRecord>, ConsentLedgerError> {
        let grants = self.lock_grants();
        let result = grants.values().find(|r| {
            r.tenant_id == tenant_id
                && r.subject_id == subject_id
                && r.purpose == purpose
                && !r.stale_consent
        });
        Ok(result.cloned())
    }

    fn list_grants(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<Vec<ConsentRecord>, ConsentLedgerError> {
        let grants = self.lock_grants();
        let mut results: Vec<ConsentRecord> = grants
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.subject_id == subject_id)
            .cloned()
            .collect();
        results.sort_by(|a, b| b.proof.submission_ts.cmp(&a.proof.submission_ts));
        Ok(results)
    }

    fn list_revocations(
        &self,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<Vec<ConsentRevocationRecord>, ConsentLedgerError> {
        let revocations = self.lock_revocations();
        let mut results: Vec<ConsentRevocationRecord> = revocations
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.subject_id == subject_id)
            .cloned()
            .collect();
        results.sort_by(|a, b| b.proof.submission_ts.cmp(&a.proof.submission_ts));
        Ok(results)
    }

    fn mark_grant_stale(&self, consent_id: &str) -> Result<(), ConsentLedgerError> {
        let mut grants = self.lock_grants();
        let record = grants
            .get_mut(consent_id)
            .ok_or_else(|| ConsentLedgerError::NotFound(consent_id.to_owned()))?;
        record.stale_consent = true;
        Ok(())
    }
}

/// Failing consent store — always returns an error.
#[derive(Debug)]
pub struct FailingConsentStore;

impl ConsentStore for FailingConsentStore {
    fn store_grant(
        &self,
        _record: ConsentRecord,
    ) -> Result<(ConsentRecord, bool), ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }

    fn store_revocation(
        &self,
        _record: ConsentRevocationRecord,
    ) -> Result<ConsentRevocationRecord, ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }

    fn get_active_grant(
        &self,
        _tenant_id: &str,
        _subject_id: &str,
        _purpose: &str,
    ) -> Result<Option<ConsentRecord>, ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }

    fn list_grants(
        &self,
        _tenant_id: &str,
        _subject_id: &str,
    ) -> Result<Vec<ConsentRecord>, ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }

    fn list_revocations(
        &self,
        _tenant_id: &str,
        _subject_id: &str,
    ) -> Result<Vec<ConsentRevocationRecord>, ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }

    fn mark_grant_stale(&self, _consent_id: &str) -> Result<(), ConsentLedgerError> {
        Err(ConsentLedgerError::Store(
            "FailingConsentStore: injected failure".to_owned(),
        ))
    }
}

//! [`SurveyResponseRecorder`] trait + [`InMemoryFake`].
//!
//! The recorder enforces five distinct gates on every `record` call,
//! in this canonical order:
//!
//! 1. **Token decode + signature verify** (constant-time;
//!    [`super::token::decode_and_verify`]).
//! 2. **TTL check** (`now_ms < token.exp`).
//! 3. **Kind match** — the response variant must match the token's
//!    `kind` field (defends a tampered URL that swaps kinds).
//! 4. **Response payload validate** (range / length-cap / dedup —
//!    [`super::SurveyResponse`] constructors enforce this when used,
//!    but the recorder re-validates defensively in case the value was
//!    deserialized from an external wire surface).
//! 5. **Replay reject** (per-`jti` dedup). The recorder remembers
//!    every `jti` it has already recorded and refuses re-record even
//!    against a still-valid HMAC + TTL.
//!
//! After all gates pass, the recorder emits an audit envelope
//! **BEFORE** the storage insert. If audit emit fails, the recorder
//! returns [`super::SurveyError::AuditEmitFailed`] and DOES NOT
//! persist the response (fail-CLOSED). Production wiring will make
//! the audit emit + storage insert atomic via the same D1-batch
//! pattern used by `corelink-audit::OutboxEmitter`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::error::SurveyError;
use super::hash::RecipientHash;
use super::ids::{SurveyId, TenantId};
use super::signer::{mint_token, SigningKey, SurveyLinkSigner};
use super::token::{decode_and_verify, SurveyToken};
use super::types::{
    sanitize_free_text, validate_csat, validate_multi_choice, validate_nps, SurveyKind,
    SurveyResponse,
};

/// A persisted survey-response row, as captured by [`InMemoryFake`].
///
/// Shape mirrors the `survey_responses` D1 table defined in
/// `migrations/d1/0046_survey_responses.sql`. The `response_value` is
/// stored as canonical JSON so the analytics layer can re-parse via
/// `SurveyResponse`'s `serde::Deserialize` derive without losing fidelity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedResponse {
    /// Server-side row id (UUIDv7) minted at insert time.
    pub id: Uuid,
    /// Tenant id (joined from the token).
    pub tenant_id: TenantId,
    /// Recipient hash (joined from the token).
    pub recipient_hash: RecipientHash,
    /// Survey slug (joined from the token).
    pub survey_id: SurveyId,
    /// Survey kind (joined from the token).
    pub response_kind: SurveyKind,
    /// Response payload, canonical JSON.
    pub response_value: String,
    /// Submitted-at unix ms (recorder's `now_ms`).
    pub submitted_at_ms: u64,
    /// IP-address SHA-256 hash (32 bytes). The recorder is privacy-by-
    /// design: it stores only the hash, never the raw IP.
    pub ip_hash: [u8; 32],
    /// User-agent SHA-256 hash (32 bytes), same rationale as `ip_hash`.
    pub user_agent_hash: [u8; 32],
    /// Token id (`jti`) the response was recorded against. Replay
    /// rejection uses this column as the dedup key in production; the
    /// in-memory fake mirrors that semantics via [`InMemoryFake::seen_jtis`].
    pub token_jti: Uuid,
}

/// Trait for any backend that records survey responses.
pub trait SurveyResponseRecorder: Send + Sync {
    /// Validate + record a response against the encoded token string.
    ///
    /// The recorder is responsible for re-running every gate (sig /
    /// TTL / kind / payload / replay) so a buggy caller cannot bypass
    /// any of them by handing in an already-decoded `SurveyToken`.
    ///
    /// # Errors
    ///
    /// Any [`SurveyError`] variant per the gate that failed.
    fn record(
        &self,
        encoded_token: &str,
        response: SurveyResponse,
        now_ms: u64,
        ip_hash: [u8; 32],
        user_agent_hash: [u8; 32],
    ) -> Result<(), SurveyError>;
}

/// Shared in-memory fake that implements both [`SurveyLinkSigner`] and
/// [`SurveyResponseRecorder`]. Used by unit / property / integration
/// tests. The two traits live on the same struct so test sites can
/// mint a token + record against it via the same handle.
///
/// Cloning shares the underlying state (`Arc<Mutex<_>>`) so multiple
/// call sites can hold separate handles to the same fake — mirrors the
/// `corelink-audit::InMemoryEmitter` pattern.
#[derive(Clone, Debug)]
pub struct InMemoryFake {
    key: SigningKey,
    state: Arc<Mutex<FakeState>>,
}

#[derive(Debug, Default)]
struct FakeState {
    /// All persisted rows, in insertion order.
    rows: Vec<PersistedResponse>,
    /// `jti`s already recorded (for replay rejection).
    seen_jtis: HashSet<Uuid>,
    /// Audit events emitted (mirror of the production audit envelope —
    /// keys are stable strings so tests can assert ordering).
    audit_events: Vec<AuditRecord>,
    /// Whether to fail the next audit emit (test hook).
    audit_fail_once: bool,
    /// Whether to fail the next storage insert (test hook).
    storage_fail_once: bool,
    /// Per-survey emit count, used as test introspection.
    emit_count_by_survey: HashMap<String, u64>,
}

/// Recorded audit envelope shape captured by the fake's audit sink.
///
/// Production emits a full `corelink-audit::AuthEvent` via the existing
/// outbox; the fake captures the minimal envelope shape so tests can
/// assert (a) an event was emitted, (b) it was emitted **before** the
/// row insert, and (c) the event has the expected outcome label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// `survey.response.recorded` on success, `survey.response.rejected`
    /// on a guarded reject path.
    pub event_type: &'static str,
    /// Tenant id from the token.
    pub tenant: TenantId,
    /// Survey id from the token.
    pub survey_id: SurveyId,
    /// Survey kind from the token.
    pub kind: SurveyKind,
    /// Token `jti` (used by replay-rejection audit pivoting).
    pub jti: Uuid,
    /// Outcome label — stable string ok for SIEM pivots.
    pub outcome: &'static str,
    /// Emitted-at timestamp.
    pub at_ms: u64,
}

impl InMemoryFake {
    /// Construct a fresh fake with the given signing key.
    #[must_use]
    pub fn new(key: SigningKey) -> Self {
        Self {
            key,
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }

    /// Snapshot every persisted row.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedResponse> {
        match self.state.lock() {
            Ok(g) => g.rows.clone(),
            Err(p) => p.into_inner().rows.clone(),
        }
    }

    /// Snapshot every audit event captured.
    #[must_use]
    pub fn audit_snapshot(&self) -> Vec<AuditRecord> {
        match self.state.lock() {
            Ok(g) => g.audit_events.clone(),
            Err(p) => p.into_inner().audit_events.clone(),
        }
    }

    /// Number of persisted rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.state.lock() {
            Ok(g) => g.rows.len(),
            Err(p) => p.into_inner().rows.len(),
        }
    }

    /// True iff no rows have been persisted yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Test hook: cause the next audit emit to fail. Used by the
    /// fail-CLOSED audit-ordering test.
    pub fn fail_next_audit_emit(&self) {
        if let Ok(mut g) = self.state.lock() {
            g.audit_fail_once = true;
        }
    }

    /// Test hook: cause the next storage insert to fail. Used by the
    /// "audit was emitted before insert" ordering test.
    pub fn fail_next_storage(&self) {
        if let Ok(mut g) = self.state.lock() {
            g.storage_fail_once = true;
        }
    }

    fn validate_payload(
        kind: SurveyKind,
        response: &SurveyResponse,
    ) -> Result<String, SurveyError> {
        match (kind, response) {
            (SurveyKind::Nps, SurveyResponse::Nps { score }) => validate_nps(*score)?,
            (SurveyKind::Csat, SurveyResponse::Csat { score }) => validate_csat(*score)?,
            (SurveyKind::FreeText, SurveyResponse::FreeText { text }) => {
                // Re-sanitize defensively: rejects an already-trimmed
                // payload that smuggled a control char past the wire layer.
                let _ = sanitize_free_text(text)?;
            }
            (SurveyKind::MultiChoice, SurveyResponse::MultiChoice { selected }) => {
                validate_multi_choice(selected)?;
            }
            _ => return Err(SurveyError::KindMismatch),
        }
        serde_json::to_string(response)
            .map_err(|e| SurveyError::Storage(format!("response serialize: {e}")))
    }

    fn emit_audit_locked(
        state: &mut FakeState,
        token: &SurveyToken,
        outcome: &'static str,
        event_type: &'static str,
        now_ms: u64,
    ) -> Result<(), SurveyError> {
        if state.audit_fail_once {
            state.audit_fail_once = false;
            return Err(SurveyError::AuditEmitFailed(
                "test hook: audit_fail_once".into(),
            ));
        }
        state.audit_events.push(AuditRecord {
            event_type,
            tenant: token.tenant(),
            survey_id: token.survey_id().clone(),
            kind: token.kind(),
            jti: token.jti(),
            outcome,
            at_ms: now_ms,
        });
        *state
            .emit_count_by_survey
            .entry(token.survey_id().as_str().to_owned())
            .or_insert(0) += 1;
        Ok(())
    }
}

impl SurveyLinkSigner for InMemoryFake {
    fn sign_invite(
        &self,
        tenant: TenantId,
        recipient: RecipientHash,
        survey_id: SurveyId,
        kind: SurveyKind,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<SurveyToken, SurveyError> {
        mint_token(
            &self.key,
            tenant,
            recipient,
            survey_id,
            kind,
            now_ms,
            ttl_ms,
        )
    }
}

impl SurveyResponseRecorder for InMemoryFake {
    fn record(
        &self,
        encoded_token: &str,
        response: SurveyResponse,
        now_ms: u64,
        ip_hash: [u8; 32],
        user_agent_hash: [u8; 32],
    ) -> Result<(), SurveyError> {
        // Gate 1: decode + constant-time sig verify.
        let token = decode_and_verify(&self.key, encoded_token)?;

        // Gate 2: TTL.
        if now_ms >= token.expires_at_ms() {
            // Audit the reject path so abuse / monitoring sees it.
            if let Ok(mut g) = self.state.lock() {
                let _ = Self::emit_audit_locked(
                    &mut g,
                    &token,
                    "expired",
                    "survey.response.rejected",
                    now_ms,
                );
            }
            return Err(SurveyError::TokenExpired);
        }

        // Gate 3: kind match.
        if !token.kind().matches(&response) {
            if let Ok(mut g) = self.state.lock() {
                let _ = Self::emit_audit_locked(
                    &mut g,
                    &token,
                    "kind_mismatch",
                    "survey.response.rejected",
                    now_ms,
                );
            }
            return Err(SurveyError::KindMismatch);
        }

        // Gate 4: payload validate + canonical serialize.
        let response_json = Self::validate_payload(token.kind(), &response)?;

        // Lock for the replay-check + audit-emit + storage-insert
        // critical section. The fake's lock-scope mirrors the
        // production D1 BEGIN .. COMMIT transaction.
        let mut state = self.state.lock().map_err(|_| {
            SurveyError::Storage("in-memory fake mutex poisoned".to_owned())
        })?;

        // Gate 5: replay.
        if state.seen_jtis.contains(&token.jti()) {
            let _ = Self::emit_audit_locked(
                &mut state,
                &token,
                "replay",
                "survey.response.rejected",
                now_ms,
            );
            return Err(SurveyError::ReplayRejected);
        }

        // Audit emit BEFORE insert (fail-CLOSED).
        Self::emit_audit_locked(
            &mut state,
            &token,
            "ok",
            "survey.response.recorded",
            now_ms,
        )?;

        // Storage insert.
        if state.storage_fail_once {
            state.storage_fail_once = false;
            return Err(SurveyError::Storage(
                "test hook: storage_fail_once".to_owned(),
            ));
        }

        let row = PersistedResponse {
            id: Uuid::now_v7(),
            tenant_id: token.tenant(),
            recipient_hash: token.recipient().clone(),
            survey_id: token.survey_id().clone(),
            response_kind: token.kind(),
            response_value: response_json,
            submitted_at_ms: now_ms,
            ip_hash,
            user_agent_hash,
            token_jti: token.jti(),
        };
        state.rows.push(row);
        state.seen_jtis.insert(token.jti());
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;

    fn fake() -> (InMemoryFake, TenantId, RecipientHash, SurveyId) {
        let f = InMemoryFake::new(SigningKey::from_bytes([0x44; 32]));
        let t = TenantId::from_uuid(Uuid::nil());
        let r = RecipientHash::derive_with_salt("a@b.com", b"salt-v1");
        let s = SurveyId::new("nps-w1");
        (f, t, r, s)
    }

    #[test]
    fn happy_path_nps() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r.clone(), s, SurveyKind::Nps, 100, 1_000)
            .unwrap();
        f.record(tok.as_str(), SurveyResponse::Nps { score: 9 }, 200, [0; 32], [0; 32])
            .unwrap();
        assert_eq!(f.len(), 1);
        let audit = f.audit_snapshot();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].event_type, "survey.response.recorded");
        assert_eq!(audit[0].outcome, "ok");
    }

    #[test]
    fn expired_rejects_and_audits() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Nps, 100, 50)
            .unwrap();
        let err = f
            .record(tok.as_str(), SurveyResponse::Nps { score: 5 }, 200, [0; 32], [0; 32])
            .unwrap_err();
        assert!(matches!(err, SurveyError::TokenExpired));
        assert!(f.is_empty());
        let audit = f.audit_snapshot();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].outcome, "expired");
    }

    #[test]
    fn kind_mismatch_rejects() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Nps, 100, 1_000)
            .unwrap();
        let err = f
            .record(
                tok.as_str(),
                SurveyResponse::Csat { score: 5 },
                200,
                [0; 32],
                [0; 32],
            )
            .unwrap_err();
        assert!(matches!(err, SurveyError::KindMismatch));
    }

    #[test]
    fn replay_rejected() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Nps, 100, 1_000)
            .unwrap();
        f.record(tok.as_str(), SurveyResponse::Nps { score: 9 }, 200, [0; 32], [0; 32])
            .unwrap();
        let err = f
            .record(tok.as_str(), SurveyResponse::Nps { score: 9 }, 250, [0; 32], [0; 32])
            .unwrap_err();
        assert!(matches!(err, SurveyError::ReplayRejected));
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn audit_fail_closed_no_row() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Csat, 100, 1_000)
            .unwrap();
        f.fail_next_audit_emit();
        let err = f
            .record(tok.as_str(), SurveyResponse::Csat { score: 5 }, 200, [0; 32], [0; 32])
            .unwrap_err();
        assert!(matches!(err, SurveyError::AuditEmitFailed(_)));
        assert!(f.is_empty(), "storage MUST NOT insert when audit fails");
    }

    #[test]
    fn storage_fail_after_audit_does_not_poison_replay() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Csat, 100, 1_000)
            .unwrap();
        f.fail_next_storage();
        let err = f
            .record(tok.as_str(), SurveyResponse::Csat { score: 4 }, 200, [0; 32], [0; 32])
            .unwrap_err();
        assert!(matches!(err, SurveyError::Storage(_)));
        // Audit emitted but row not inserted; jti should NOT be in
        // seen_jtis (caller can retry).
        let audit = f.audit_snapshot();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].outcome, "ok");
        // Retry should succeed (storage_fail_once consumed).
        f.record(tok.as_str(), SurveyResponse::Csat { score: 4 }, 200, [0; 32], [0; 32])
            .unwrap();
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn free_text_sanitized_on_record() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::FreeText, 0, 100)
            .unwrap();
        // The well-formed constructor trims; here we go through
        // record() to make sure a hand-built struct still gets the
        // defensive re-validate.
        let resp = SurveyResponse::FreeText {
            text: "great product".to_owned(),
        };
        f.record(tok.as_str(), resp, 10, [0; 32], [0; 32]).unwrap();
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn free_text_control_char_rejected() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::FreeText, 0, 100)
            .unwrap();
        let resp = SurveyResponse::FreeText {
            text: "bad\x07input".to_owned(),
        };
        let err = f.record(tok.as_str(), resp, 10, [0; 32], [0; 32]).unwrap_err();
        assert!(matches!(err, SurveyError::InvalidResponse(_)));
    }

    #[test]
    fn multi_choice_dedup_at_record() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::MultiChoice, 0, 100)
            .unwrap();
        let resp = SurveyResponse::MultiChoice {
            selected: vec![1, 1, 2],
        };
        let err = f.record(tok.as_str(), resp, 10, [0; 32], [0; 32]).unwrap_err();
        assert!(matches!(err, SurveyError::InvalidResponse(_)));
    }

    #[test]
    fn tampered_signature_rejects() {
        let (f, t, r, s) = fake();
        let tok = f
            .sign_invite(t, r, s, SurveyKind::Nps, 0, 1_000)
            .unwrap();
        // Forge a token signed with a different key.
        let other = InMemoryFake::new(SigningKey::from_bytes([0xff; 32]));
        let forged = other
            .sign_invite(
                TenantId::from_uuid(Uuid::nil()),
                RecipientHash::derive_with_salt("a@b.com", b"salt-v1"),
                SurveyId::new("nps-w1"),
                SurveyKind::Nps,
                0,
                1_000,
            )
            .unwrap();
        // Different jti so the forged token IS distinct from `tok`.
        assert_ne!(forged.jti(), tok.jti());
        let err = f
            .record(forged.as_str(), SurveyResponse::Nps { score: 5 }, 10, [0; 32], [0; 32])
            .unwrap_err();
        assert!(matches!(err, SurveyError::InvalidSignature));
    }

    #[test]
    fn distinct_jtis_per_invite() {
        let (f, t, r, s) = fake();
        let a = f
            .sign_invite(t, r.clone(), s.clone(), SurveyKind::Nps, 0, 1_000)
            .unwrap();
        let b = f
            .sign_invite(t, r, s, SurveyKind::Nps, 0, 1_000)
            .unwrap();
        assert_ne!(a.jti(), b.jti());
    }
}

//! Real D1-backed [`ErasureAuditSink`] over the canonical `audit_outbox`
//! intake table (`migrations/d1/0001_blob_meta.sql`), WI-S11-008 Wave 1.
//!
//! The orchestrator emits a CloudEvents envelope BEFORE every state
//! mutation (fail-CLOSED per ADR-S11-002): a non-`Ok` return aborts the
//! erasure with no backend mutation + no idempotency tombstone. This sink
//! appends the envelope to `audit_outbox`; the S-09 drain worker later
//! flips `emitted_at` when the event reaches the immutable chain.
//!
//! Minimization (CTRL-PRIV-014): the durable envelope NEVER carries the
//! raw `subject_id`. The production pseudonym is `sha256(subject_id ||
//! erasure_salt)`, but this sink does not hold the erasure_salt, so the
//! raw subject id is OMITTED entirely — `dsr_id` + `tenant_id` are
//! sufficient correlation keys and neither is subject PII.

use std::sync::Arc;

use serde_json::json;

use corelink_privacy_erasure_worker::audit_emit::{ErasureAuditRecord, ErasureAuditSink};
use corelink_privacy_erasure_worker::error::ErasureAuditSinkError;

use super::d1util::{clamp_ms, d1_query_blocking};
use crate::customer_d1::ms_to_iso8601;
use crate::storage::d1_http::D1HttpClient;

/// Durable erasure audit sink backed by the D1 `audit_outbox` table.
pub(super) struct D1ErasureAuditSink {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1ErasureAuditSink {
    // Never surface the inner client's Debug — it holds the CF API token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1ErasureAuditSink")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl D1ErasureAuditSink {
    /// Construct over a shared [`D1HttpClient`].
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

impl ErasureAuditSink for D1ErasureAuditSink {
    fn emit(&self, record: ErasureAuditRecord) -> Result<(), ErasureAuditSinkError> {
        let event_type = record.event_type.as_str();
        let backend = record.backend.map(|b| b.as_str());
        // Deterministic, replay-safe keys: a re-emit of the same logical
        // event collides on the PK / UNIQUE(request_id, event_type) and is
        // an `INSERT OR IGNORE` no-op (so audit emit stays idempotent on a
        // pipeline retry — and still returns Ok so the run proceeds).
        let id = format!(
            "{}:{}:{}",
            record.dsr_id.simple(),
            event_type,
            backend.unwrap_or("-")
        );
        let request_id = format!("{}:{}", record.dsr_id.simple(), backend.unwrap_or("dsr"));

        let payload = json!({
            "specversion": "1.0",
            "type": event_type,
            "source": "corelink/dsr/erasure",
            "id": id,
            "subject": record.dsr_id.to_string(),
            "time": ms_to_iso8601(clamp_ms(record.now_ms)),
            "data": {
                "dsr_id": record.dsr_id.to_string(),
                "tenant_id": record.tenant_id.to_string(),
                "backend": backend,
                "outcome": record.outcome.as_ref().map(|o| o.as_str()),
                "context": record.context,
            }
        });
        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| ErasureAuditSinkError::Store(format!("audit payload serialize: {e}")))?;

        let sql = "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at) \
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, NULL)";
        let params = vec![
            json!(id),
            json!(record.tenant_id.to_string()),
            json!(request_id),
            json!(event_type),
            json!(payload_json),
            json!(clamp_ms(record.now_ms)),
        ];
        d1_query_blocking(&self.d1, sql, params).map_err(ErasureAuditSinkError::Store)?;
        Ok(())
    }
}

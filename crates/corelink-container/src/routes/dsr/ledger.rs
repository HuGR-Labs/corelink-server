//! Real D1-backed [`ErasureIdempotencyLedger`] over the canonical
//! `dsr_erasure_log` table (`migrations/d1/0022_dsr_erasure_log.sql`),
//! WI-S11-008 Wave 1 increment 1.
//!
//! The pure `corelink-privacy-erasure-worker` crate ships only the
//! in-memory ledger; this is the durable transport. The canonical
//! replay-safe guarantee is the table's `UNIQUE (dsr_id, backend)`
//! constraint (PAT-RETRY-IDEMPOTENT-001): a re-run of an already-recorded
//! `(dsr_id, backend)` is a [`LedgerOutcome::Replayed`]; the same key with
//! a *divergent* outcome is a [`ErasureIdempotencyError::DivergentPayload`]
//! forensic anomaly.
//!
//! Timestamps: the canonical `started_at`/`completed_at` columns are ISO
//! 8601 second-precision TEXT, so the `[u8;32]` `verification_hash` (a
//! transient re-fingerprint recomputed by the 24h verify sweep) is NOT
//! persisted here and reconstructed completions carry the canonical empty
//! hash. This is a property of the canonical schema, documented in
//! ADR-S11-013.

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use corelink_privacy_erasure_worker::error::ErasureIdempotencyError;
use corelink_privacy_erasure_worker::event::{
    canonical_backend_kinds, BackendCompletion, BackendErasureOutcome, BackendKind,
};
use corelink_privacy_erasure_worker::idempotency::{ErasureIdempotencyLedger, LedgerOutcome};

use super::d1util::{clamp_ms, col_i64, col_str, d1_query_blocking, iso8601_to_ms};
use crate::customer_d1::ms_to_iso8601;
use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Columns selected when reconstructing a [`BackendCompletion`] tombstone.
const SELECT_COLS: &str = "dsr_id, tenant_id, subject_id_hash, backend, outcome, \
     records_affected, retry_count, idempotency_key, started_at, completed_at";

/// Durable idempotency ledger backed by the D1 `dsr_erasure_log` table.
pub(super) struct D1ErasureIdempotencyLedger {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1ErasureIdempotencyLedger {
    // Never surface the inner client's Debug — it holds the CF API token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1ErasureIdempotencyLedger")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl D1ErasureIdempotencyLedger {
    /// Construct over a shared [`D1HttpClient`].
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Reconstruct a [`BackendCompletion`] from a `dsr_erasure_log` row.
    /// `verification_hash` is not a stored column (transient, recomputed by
    /// the verify sweep) → canonical zero hash.
    fn row_to_completion(row: &D1Row) -> Option<BackendCompletion> {
        let dsr_id = Uuid::parse_str(&col_str(row, "dsr_id")?).ok()?;
        let tenant_id = Uuid::parse_str(&col_str(row, "tenant_id")?).ok()?;
        let subject_id_hash = hex32(&col_str(row, "subject_id_hash")?)?;
        let backend = backend_from_str(&col_str(row, "backend")?)?;
        let records = u64::try_from(col_i64(row, "records_affected").unwrap_or(0)).unwrap_or(0);
        let outcome = outcome_from_str(&col_str(row, "outcome")?, records)?;
        let idempotency_key = col_str(row, "idempotency_key")?;
        let started_at_ms = col_str(row, "started_at")
            .and_then(|s| iso8601_to_ms(&s))
            .unwrap_or(0);
        let completed_at_ms = col_str(row, "completed_at")
            .and_then(|s| iso8601_to_ms(&s))
            .unwrap_or(0);
        let retry_count = u32::try_from(col_i64(row, "retry_count").unwrap_or(0)).unwrap_or(0);
        Some(BackendCompletion {
            dsr_id,
            tenant_id,
            subject_id_hash,
            backend,
            outcome,
            idempotency_key,
            started_at_ms,
            completed_at_ms,
            retry_count,
            verification_hash: [0u8; 32],
        })
    }
}

impl ErasureIdempotencyLedger for D1ErasureIdempotencyLedger {
    fn upsert(
        &self,
        completion: BackendCompletion,
    ) -> Result<LedgerOutcome, ErasureIdempotencyError> {
        let sel = format!(
            "SELECT {SELECT_COLS} FROM dsr_erasure_log WHERE dsr_id = ?1 AND backend = ?2"
        );
        let rows = d1_query_blocking(
            &self.d1,
            &sel,
            vec![
                json!(completion.dsr_id.to_string()),
                json!(completion.backend.as_str()),
            ],
        )
        .map_err(ErasureIdempotencyError::Backend)?;

        if let Some(row) = rows.first() {
            let prior = Self::row_to_completion(row).ok_or_else(|| {
                ErasureIdempotencyError::Backend(
                    "dsr_erasure_log: failed to reconstruct prior tombstone".to_owned(),
                )
            })?;
            // Canonical idempotency contract (idempotency.rs trait doc): a
            // true replay matches on outcome + tenant_id + subject_id_hash +
            // idempotency_key. ANY divergence (incl. a different tenant or
            // subject for the same (dsr_id, backend)) is a SEV-1 forensic
            // anomaly (tampering / cross-tenant signal), NOT a silent replay.
            if prior.outcome.as_str() == completion.outcome.as_str()
                && prior.tenant_id == completion.tenant_id
                && prior.subject_id_hash == completion.subject_id_hash
                && prior.idempotency_key == completion.idempotency_key
            {
                return Ok(LedgerOutcome::Replayed { prior });
            }
            return Err(ErasureIdempotencyError::DivergentPayload);
        }

        // Deterministic log_id per (dsr_id, backend) → re-run replays onto
        // the same PK; `INSERT OR IGNORE` makes the cross-instance race a
        // no-op under the UNIQUE (dsr_id, backend) constraint.
        let log_id = format!("{}-{}", completion.dsr_id.simple(), completion.backend.as_str());
        let ins = "INSERT OR IGNORE INTO dsr_erasure_log \
             (log_id, dsr_id, tenant_id, subject_id_hash, backend, outcome, \
              records_affected, error_classes, retry_count, idempotency_key, \
              started_at, completed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11)";
        let params = vec![
            json!(log_id),
            json!(completion.dsr_id.to_string()),
            json!(completion.tenant_id.to_string()),
            json!(hex::encode(completion.subject_id_hash)),
            json!(completion.backend.as_str()),
            json!(completion.outcome.as_str()),
            json!(records_affected(&completion.outcome)),
            json!(i64::from(completion.retry_count)),
            json!(completion.idempotency_key),
            json!(ms_to_iso8601(clamp_ms(completion.started_at_ms))),
            json!(ms_to_iso8601(clamp_ms(completion.completed_at_ms))),
        ];
        d1_query_blocking(&self.d1, ins, params).map_err(ErasureIdempotencyError::Backend)?;
        Ok(LedgerOutcome::Inserted)
    }

    fn get(
        &self,
        dsr_id: Uuid,
        backend: BackendKind,
    ) -> Result<Option<BackendCompletion>, ErasureIdempotencyError> {
        let sel = format!(
            "SELECT {SELECT_COLS} FROM dsr_erasure_log WHERE dsr_id = ?1 AND backend = ?2"
        );
        let rows = d1_query_blocking(
            &self.d1,
            &sel,
            vec![json!(dsr_id.to_string()), json!(backend.as_str())],
        )
        .map_err(ErasureIdempotencyError::Backend)?;
        Ok(rows.first().and_then(Self::row_to_completion))
    }

    fn snapshot(&self, dsr_id: Uuid) -> Result<Vec<BackendCompletion>, ErasureIdempotencyError> {
        let sel = format!("SELECT {SELECT_COLS} FROM dsr_erasure_log WHERE dsr_id = ?1");
        let rows = d1_query_blocking(&self.d1, &sel, vec![json!(dsr_id.to_string())])
            .map_err(ErasureIdempotencyError::Backend)?;
        let mut out: Vec<BackendCompletion> =
            rows.iter().filter_map(Self::row_to_completion).collect();
        // Canonical order (matches canonical_backend_kinds) for the report.
        out.sort_by_key(|c| canonical_index(c.backend));
        Ok(out)
    }

    fn set_outcome_snapshot(
        &self,
        dsr_id: Uuid,
        outcome_json: &str,
    ) -> Result<(), ErasureIdempotencyError> {
        let sql = "UPDATE dsr_erasure_log SET outcome_json = ?2 WHERE dsr_id = ?1";
        d1_query_blocking(
            &self.d1,
            sql,
            vec![json!(dsr_id.to_string()), json!(outcome_json)],
        )
        .map_err(ErasureIdempotencyError::Backend)?;
        Ok(())
    }

    fn get_outcome_snapshot(
        &self,
        dsr_id: Uuid,
    ) -> Result<Option<String>, ErasureIdempotencyError> {
        let sql = "SELECT outcome_json FROM dsr_erasure_log \
             WHERE dsr_id = ?1 AND outcome_json IS NOT NULL LIMIT 1";
        let rows = d1_query_blocking(&self.d1, sql, vec![json!(dsr_id.to_string())])
            .map_err(ErasureIdempotencyError::Backend)?;
        Ok(rows.first().and_then(|r| col_str(r, "outcome_json")))
    }
}

/// `records_affected` column value (count_deleted OR count_redacted).
fn records_affected(o: &BackendErasureOutcome) -> i64 {
    let n = match o {
        BackendErasureOutcome::Erased { records_deleted } => *records_deleted,
        BackendErasureOutcome::Pseudonymized { records_redacted } => *records_redacted,
        BackendErasureOutcome::PartialFailure {
            records_succeeded, ..
        } => *records_succeeded,
        BackendErasureOutcome::Failed { .. } | BackendErasureOutcome::NotApplicable => 0,
        // `BackendErasureOutcome` is `#[non_exhaustive]`; any future arm has no
        // records-affected meaning here and defaults to 0.
        _ => 0,
    };
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Reconstruct the canonical outcome from its stored string + count.
/// `records_failed` for `partial_failure` is not separately persisted
/// (the column carries the succeeded count), so it reconstructs as 0 — the
/// load-bearing signal for the verify sweep is the outcome *arm*, not the
/// per-record split.
fn outcome_from_str(s: &str, records: u64) -> Option<BackendErasureOutcome> {
    Some(match s {
        "erased" => BackendErasureOutcome::Erased {
            records_deleted: records,
        },
        "pseudonymized" => BackendErasureOutcome::Pseudonymized {
            records_redacted: records,
        },
        "partial_failure" => BackendErasureOutcome::PartialFailure {
            records_succeeded: records,
            records_failed: 0,
        },
        "failed" => BackendErasureOutcome::Failed {
            retry_after_seconds: 60,
        },
        "not_applicable" => BackendErasureOutcome::NotApplicable,
        _ => return None,
    })
}

/// Inverse of [`BackendKind::as_str`].
fn backend_from_str(s: &str) -> Option<BackendKind> {
    canonical_backend_kinds()
        .iter()
        .copied()
        .find(|k| k.as_str() == s)
}

/// Position of `backend` in the canonical 12-arm order.
fn canonical_index(backend: BackendKind) -> usize {
    canonical_backend_kinds()
        .iter()
        .position(|k| *k == backend)
        .unwrap_or(usize::MAX)
}

/// Decode a 64-char hex string to `[u8; 32]`.
fn hex32(s: &str) -> Option<[u8; 32]> {
    let v = hex::decode(s).ok()?;
    <[u8; 32]>::try_from(v.as_slice()).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn backend_str_roundtrips_all_12() {
        for k in canonical_backend_kinds() {
            assert_eq!(backend_from_str(k.as_str()), Some(*k));
        }
    }

    #[test]
    fn outcome_str_roundtrips() {
        let cases = [
            BackendErasureOutcome::Erased { records_deleted: 7 },
            BackendErasureOutcome::Pseudonymized { records_redacted: 3 },
            BackendErasureOutcome::Failed { retry_after_seconds: 60 },
            BackendErasureOutcome::NotApplicable,
        ];
        for o in cases {
            let back = outcome_from_str(o.as_str(), 7).unwrap();
            assert_eq!(back.as_str(), o.as_str());
        }
        assert!(outcome_from_str("bogus", 0).is_none());
    }

    #[test]
    fn records_affected_reads_the_right_field() {
        assert_eq!(
            records_affected(&BackendErasureOutcome::Erased { records_deleted: 9 }),
            9
        );
        assert_eq!(
            records_affected(&BackendErasureOutcome::NotApplicable),
            0
        );
    }

    #[test]
    fn canonical_index_is_dense_and_ordered() {
        let kinds = canonical_backend_kinds();
        for (i, k) in kinds.iter().enumerate() {
            assert_eq!(canonical_index(*k), i);
        }
    }
}

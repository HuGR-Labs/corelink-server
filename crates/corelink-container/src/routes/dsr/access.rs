//! DSR ACCESS (GDPR Art.15) + PORTABILITY (Art.20) + RECTIFICATION (Art.16) —
//! the three data-subject rights that complete the `/_internal/dsr/*` surface
//! alongside ERASURE (Art.17) + verify.
//!
//! ## Design (frozen tech-lead policy)
//!
//! - **Gather is single-sourced with the erase-set.** [`build_gather_plan`]
//!   enumerates the subject's rows across the EXACT same tenant-keyed tables the
//!   D1 erase adapter deletes ([`super::adapter_d1::TENANT_ID_TABLES`] +
//!   [`super::adapter_d1::NAMESPACE_TABLES`] + the bespoke
//!   [`super::adapter_d1::SPECIAL_ERASE_TABLES`]). A future tenant-keyed table
//!   added to the erase-set is therefore covered by BOTH erase AND access — no
//!   parallel list to drift (asserted by `access_set_equals_erase_set`).
//! - **Retained data is disclosed, not erased.** RETAIN-set tables (fiscal /
//!   audit, e.g. `stripe_*`) carry a lawful retention basis so they survive an
//!   Art.17 erasure — but the subject has an Art.15 right to SEE them. They are
//!   gathered with `retained = true` (the `tenant_id`-keyed disclosable subset
//!   [`RETAIN_DISCLOSABLE_TABLES`] of [`super::adapter_d1::RETAIN_SET`]).
//! - **Secrets are never exported raw.** [`redact_row`] replaces credential /
//!   key material (PAT digests, reveal tokens, BYOK envelopes, …) with a
//!   `"<redacted>"` marker — presence/metadata is disclosed, the secret material
//!   is not.
//! - **Fail-CLOSED.** Any gather / D1 error aborts the whole export with `Err`
//!   (never a partial-looking "complete" bundle), and every access / portability
//!   / rectification request emits its audit envelope BEFORE the disclosure /
//!   mutation (ADR-S11-002 discipline; idempotent on a deterministic id so a
//!   retry never double-acts).
//! - **Rectification is bounded + honest.** Cache CONTENT is content-addressed +
//!   immutable → NOT rectifiable. Only the editable subject PII allowlist
//!   ([`RECTIFIABLE_FIELDS`]) is correctable; any other target fails CLOSED 4xx.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::d1util::{clamp_ms, d1_query_blocking};
use crate::customer_d1::ms_to_iso8601;
use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::storage::r2_s3::R2S3Client;

/// CloudEvents `type` for the Art.15 access event (audit_outbox).
pub(super) const EVENT_ACCESS: &str = "corelink.dsr.access";
/// CloudEvents `type` for the Art.20 portability event.
pub(super) const EVENT_PORTABILITY: &str = "corelink.dsr.portability";
/// CloudEvents `type` for the Art.16 rectification event.
pub(super) const EVENT_RECTIFICATION: &str = "corelink.dsr.rectification";

/// Schema tag on the machine-readable export bundle (portability contract).
const EXPORT_SCHEMA: &str = "corelink.dsr.subject_export.v1";

/// The `tenant_id`-keyed disclosable subset of
/// [`super::adapter_d1::RETAIN_SET`]. These tables survive an Art.17 erasure
/// (lawful retention basis) but are disclosed under Art.15. Every entry is a
/// real `tenant_id`-keyed table (verified against `migrations/d1/*.sql`) — the
/// two RETAIN entries WITHOUT a `tenant_id` column (`export_audit_log` is
/// `authenticated_tenant`-keyed; `session_exchange_throttle` is `clerk_sub`-
/// keyed) and the legacy singular `erasure_attestation` alias are intentionally
/// excluded so a `SELECT … WHERE tenant_id = ?` never errors. Asserted a subset
/// of RETAIN_SET + disjoint from the erase-set by tests.
const RETAIN_DISCLOSABLE_TABLES: &[&str] = &[
    "dsr_erasure_log",
    "dsr_requested",
    "dpa_acceptances",
    "erasure_attestations",
    "stripe_customers",
    "stripe_subscriptions",
    "stripe_invoices",
    "stripe_disputes",
    "stripe_refunds",
    "audit_outbox",
    "billing_replay_audit",
    "stripe_idempotency_keys",
    "billing_reconciliation_drift",
    "stripe_submission_state",
    "customer_audit_events",
    "audit_chain_head",
    "tenant_legal_hold",
    "abuse_score_history",
];

/// Lower-snake column-name tokens that mark a column as secret/credential
/// material. Any column whose lowercased name CONTAINS one of these is redacted
/// (presence kept, value replaced) before export — never the secret itself.
/// Deliberately precise: `token` catches `shown_once_token` but not `key_id`
/// (we do NOT redact `key_id` / `idempotency_key` — they are not secrets).
const SENSITIVE_COL_TOKENS: &[&str] = &[
    "secret",
    "pat_hash",
    "token",
    "envelope",
    "cipher",
    "wrapped",
    "private_key",
    "signing_key",
    "kms_key",
    "key_material",
];

/// The ONLY editable subject-PII fields a rectification (Art.16) may correct,
/// as `(table, field)` pairs. Bounded + honest: CoreLink stores PII
/// pseudonymized, so the one meaningful editable contact field is the account
/// email — stored as `tenant.email_hash` (SHA-256 of the normalized email per
/// CTRL-PRIV-001). The rectification takes the new RAW email and stores its
/// hash (we never persist the raw email), reusing the canonical scheme.
const RECTIFIABLE_FIELDS: &[(&str, &str)] = &[("tenant", "email_hash")];

/// Content-addressed / cache tables whose data is IMMUTABLE by construction —
/// a rectification target here gets the "content is immutable" message.
const CONTENT_IMMUTABLE_TABLES: &[&str] = &[
    "blob_meta",
    "ac_meta",
    "chunks",
    "manifest_chunks",
    "multipart_sessions",
    "cas_tombstone",
    "hot_blobs",
];

/// Canonical pseudonymized email hash (CTRL-PRIV-001) — delegates to the ONE
/// shared [`crate::email_hash::hash_email`] so this rectification site stays
/// byte-identical to the `customer_d1` team-invite write (and the signup-worker
/// `emailHashFor` accept-match): normalized email, HMAC-SHA256 under
/// `EMAIL_HASH_SALT` when set, else unsalted SHA-256. The raw email is NEVER
/// stored.
#[must_use]
pub(crate) fn email_hash(email: &str) -> String {
    crate::email_hash::hash_email(email)
}

/// Whether `col` names secret/credential material that must be redacted.
#[must_use]
fn is_sensitive_col(col: &str) -> bool {
    let lower = col.to_ascii_lowercase();
    SENSITIVE_COL_TOKENS.iter().any(|t| lower.contains(t))
}

/// Redact a gathered row in place: any sensitive-named column with a non-null
/// value is replaced by `"<redacted>"` (presence disclosed, secret withheld);
/// a SQL NULL stays null (so the export truthfully shows "no value present").
#[must_use]
fn redact_row(mut row: D1Row) -> D1Row {
    for (col, val) in row.iter_mut() {
        if is_sensitive_col(col) && !val.is_null() {
            *val = Value::String("<redacted>".to_owned());
        }
    }
    row
}

/// One per-table slice of the subject export.
#[derive(Debug, Clone, Serialize)]
pub(super) struct SubjectTable {
    /// D1 table name.
    pub table: String,
    /// `true` for RETAIN-set tables (disclosed under Art.15 but NOT erased —
    /// a lawful retention basis survives an Art.17 erasure).
    pub retained: bool,
    /// Number of rows gathered for the subject.
    pub row_count: usize,
    /// The redacted rows (secret columns replaced; per-table JSON objects).
    pub rows: Vec<D1Row>,
}

/// The structured, machine-readable subject data export (Art.15 access body /
/// Art.20 portability payload). Serializes as `{ schema, tenant_id,
/// generated_at_ms, tables: [{ table, retained, row_count, rows }] }`.
#[derive(Debug, Clone, Serialize)]
pub(super) struct SubjectExport {
    /// Stable schema tag.
    pub schema: &'static str,
    /// The subject's tenant id (one-user-per-tenant).
    pub tenant_id: String,
    /// Generation instant (Unix epoch ms).
    pub generated_at_ms: u64,
    /// Per-table data slices (erase-set tables first, then retained tables).
    pub tables: Vec<SubjectTable>,
}

/// One planned gather query: the table label, whether it is RETAIN-set, and the
/// exact SQL + bound params. Pure (no I/O) so the plan is unit-testable and the
/// erase-set/access-set coverage invariant is asserted offline.
#[derive(Debug, Clone)]
pub(super) struct GatherQuery {
    pub table: &'static str,
    pub retained: bool,
    pub sql: String,
    pub params: Vec<Value>,
}

/// Build the full gather plan for `tenant_id`, single-sourced with the D1
/// erase-set (so a future erase-set table is covered by access too):
///
/// 1. erase-set `tenant_id`-keyed tables ([`super::adapter_d1::TENANT_ID_TABLES`]);
/// 2. erase-set `namespace`-keyed tables (bound value = tenant UUID, never
///    `'_public'`, so shared public-registry content is never gathered);
/// 3. the bespoke specials (`signup_orchestration`/`tenant` by `tenant_id`;
///    `signup_attempts` via the `idempotency_key` join the erase path uses);
/// 4. the `tenant_id`-keyed RETAIN-set disclosable subset (marked retained).
#[must_use]
pub(super) fn build_gather_plan(tenant_id: &str) -> Vec<GatherQuery> {
    let mut plan: Vec<GatherQuery> = Vec::new();
    let tid = || vec![json!(tenant_id)];

    for t in super::adapter_d1::TENANT_ID_TABLES {
        plan.push(GatherQuery {
            table: t,
            retained: false,
            sql: format!("SELECT * FROM {t} WHERE tenant_id = ?1"),
            params: tid(),
        });
    }
    for t in super::adapter_d1::NAMESPACE_TABLES {
        plan.push(GatherQuery {
            table: t,
            retained: false,
            sql: format!("SELECT * FROM {t} WHERE namespace = ?1"),
            params: tid(),
        });
    }
    // Specials (kept exactly aligned with SPECIAL_ERASE_TABLES): the two
    // identity rows by tenant_id + signup_attempts via the join.
    plan.push(GatherQuery {
        table: "signup_orchestration",
        retained: false,
        sql: "SELECT * FROM signup_orchestration WHERE tenant_id = ?1".to_owned(),
        params: tid(),
    });
    plan.push(GatherQuery {
        table: "signup_attempts",
        retained: false,
        sql: "SELECT * FROM signup_attempts WHERE idempotency_key IN \
              (SELECT idempotency_key FROM signup_orchestration WHERE tenant_id = ?1)"
            .to_owned(),
        params: tid(),
    });
    plan.push(GatherQuery {
        table: "tenant",
        retained: false,
        sql: "SELECT * FROM tenant WHERE tenant_id = ?1".to_owned(),
        params: tid(),
    });
    // RETAIN-set disclosable subset (Art.15 right to SEE retained data).
    for t in RETAIN_DISCLOSABLE_TABLES {
        plan.push(GatherQuery {
            table: t,
            retained: true,
            sql: format!("SELECT * FROM {t} WHERE tenant_id = ?1"),
            params: tid(),
        });
    }
    plan
}

/// Gather the subject's data into a structured [`SubjectExport`], running each
/// planned query through `query` and redacting secret columns. FAIL-CLOSED: the
/// FIRST query error aborts with `Err` (never a partial export); a table with
/// zero rows is included with an empty `rows` (truthful "no data here").
///
/// `query` is injected so the assembly + fail-closed + redaction behaviour is
/// unit-testable offline; the live caller passes a D1-backed closure.
pub(super) fn gather_subject_data<Q>(
    tenant_id: &str,
    generated_at_ms: u64,
    mut query: Q,
) -> Result<SubjectExport, String>
where
    Q: FnMut(&str, &[Value]) -> Result<Vec<D1Row>, String>,
{
    let plan = build_gather_plan(tenant_id);
    let mut tables: Vec<SubjectTable> = Vec::with_capacity(plan.len());
    for q in plan {
        // Fail-CLOSED: propagate the first error — no partial "complete" export.
        let rows = query(&q.sql, &q.params)
            .map_err(|e| format!("gather {table}: {e}", table = q.table))?;
        let redacted: Vec<D1Row> = rows.into_iter().map(redact_row).collect();
        tables.push(SubjectTable {
            table: q.table.to_owned(),
            retained: q.retained,
            row_count: redacted.len(),
            rows: redacted,
        });
    }
    Ok(SubjectExport {
        schema: EXPORT_SCHEMA,
        tenant_id: tenant_id.to_owned(),
        generated_at_ms,
        tables,
    })
}

/// Deterministic audit-row id for a DSR right-event, so a retry is an
/// `INSERT OR IGNORE` no-op (idempotent — never double-acts). Mirrors the
/// erase audit sink's keying.
#[must_use]
pub(super) fn audit_event_id(dsr_id: &str, event_type: &str, suffix: &str) -> String {
    format!("{dsr_id}:{event_type}:{suffix}")
}

/// Append a DSR right-event to `audit_outbox` (unchained row, same posture as
/// the erase audit sink). Deterministic id ⇒ idempotent on retry. FAIL-CLOSED:
/// the caller emits this BEFORE the disclosure / mutation and propagates an
/// `Err` (no audit ⇒ no act).
fn audit_dsr_event(
    d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    tenant_id: &str,
    event_type: &str,
    suffix: &str,
    context: &Value,
    now_ms: u64,
) -> Result<(), String> {
    let id = audit_event_id(dsr_id, event_type, suffix);
    let request_id = format!("{dsr_id}:dsr");
    let payload = json!({
        "specversion": "1.0",
        "type": event_type,
        "source": "corelink/dsr",
        "id": id,
        "subject": dsr_id,
        "time": ms_to_iso8601(clamp_ms(now_ms)),
        "data": {
            "dsr_id": dsr_id,
            "tenant_id": tenant_id,
            "context": context,
        }
    });
    let payload_json =
        serde_json::to_string(&payload).map_err(|e| format!("audit payload serialize: {e}"))?;
    let sql = "INSERT OR IGNORE INTO audit_outbox \
         (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at) \
         VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, NULL)";
    let params = vec![
        json!(id),
        json!(tenant_id),
        json!(request_id),
        json!(event_type),
        json!(payload_json),
        json!(clamp_ms(now_ms)),
    ];
    d1_query_blocking(d1, sql, params).map(|_| ())
}

/// Live D1-backed gather closure for a real handler.
fn gather_live(
    d1: &Arc<D1HttpClient>,
    tenant_id: &str,
    now_ms: u64,
) -> Result<SubjectExport, String> {
    gather_subject_data(tenant_id, now_ms, |sql, params| {
        d1_query_blocking(d1, sql, params.to_vec())
    })
}

/// Run the ACCESS (Art.15) right: audit FIRST (no disclosure without a logged
/// access), then gather. FAIL-CLOSED on either step.
pub(super) fn run_access(
    d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    tenant_id: &str,
    now_ms: u64,
) -> Result<SubjectExport, String> {
    audit_dsr_event(
        d1,
        dsr_id,
        tenant_id,
        EVENT_ACCESS,
        "-",
        &json!({ "surface": "access" }),
        now_ms,
    )?;
    gather_live(d1, tenant_id, now_ms)
}

/// A portability receipt: the durable signed export handle (when the per-region
/// signing infra is configured). The machine-readable bundle is always returned
/// inline alongside this (the Art.20 core); a `persisted`/`signed` copy is an
/// additional, non-blocking durable artifact.
#[derive(Debug, Clone, Serialize)]
pub(super) struct ExportReceipt {
    /// SHA-256 hex of the canonical export bundle bytes (integrity handle).
    pub content_sha256: String,
    /// Whether the bundle was persisted to the R2 audit bucket.
    pub persisted: bool,
    /// Bucket-qualified R2 key of the persisted bundle (when `persisted`).
    pub r2_key: Option<String>,
    /// Whether a detached Ed25519 signature over the content digest was written.
    pub signed: bool,
}

/// Run the PORTABILITY (Art.20) right: audit FIRST, gather the SAME structured
/// bundle as access (fail-closed), then best-effort persist a durable signed
/// copy to the R2 audit bucket. Returns the machine-readable bundle (always)
/// plus a receipt describing the durable copy.
pub(super) fn run_portability(
    d1: &Arc<D1HttpClient>,
    r2_audit: Option<&Arc<R2S3Client>>,
    dsr_id: &str,
    tenant_id: &str,
    now_ms: u64,
) -> Result<(SubjectExport, ExportReceipt), String> {
    audit_dsr_event(
        d1,
        dsr_id,
        tenant_id,
        EVENT_PORTABILITY,
        "-",
        &json!({ "surface": "portability" }),
        now_ms,
    )?;
    let export = gather_live(d1, tenant_id, now_ms)?;
    let receipt = persist_export(r2_audit, dsr_id, tenant_id, &export, now_ms);
    Ok((export, receipt))
}

/// Best-effort durable persistence of the export bundle to the R2 audit bucket,
/// plus a detached Ed25519 signature over its content digest (reusing the
/// erasure attestation signer infra). NON-BLOCKING: a missing R2 client / sign
/// infra (or a transient PUT failure) downgrades to `persisted: false` — the
/// inline machine-readable bundle is the Art.20 core and always returns.
fn persist_export(
    r2_audit: Option<&Arc<R2S3Client>>,
    dsr_id: &str,
    tenant_id: &str,
    export: &SubjectExport,
    now_ms: u64,
) -> ExportReceipt {
    let bundle_bytes = serde_json::to_vec(export).unwrap_or_default();
    let content_sha256 = hex::encode(Sha256::digest(&bundle_bytes));

    let Some(r2) = r2_audit else {
        return ExportReceipt {
            content_sha256,
            persisted: false,
            r2_key: None,
            signed: false,
        };
    };

    let object_key = format!("dsr_exports/{dsr_id}.json");
    let handle = tokio::runtime::Handle::current();
    let put_ok =
        tokio::task::block_in_place(|| handle.block_on(r2.put(&object_key, bundle_bytes.clone())));
    if let Err(e) = put_ok {
        tracing::warn!(dsr_id = %dsr_id, error = %e, "dsr/portability: export bundle R2 PUT failed (inline bundle still returned)");
        return ExportReceipt {
            content_sha256,
            persisted: false,
            r2_key: None,
            signed: false,
        };
    }

    // Detached Ed25519 signature over the content digest (reuses the per-region
    // erasure-attestation signer). Best-effort: skipped (signed=false) when the
    // region/seed infra is unset — never a forgeable certificate.
    let mut signed = false;
    if let Some((attestation, _region)) =
        super::attestation::sign_export_digest(dsr_id, tenant_id, now_ms, &content_sha256)
    {
        if let Ok(sig_bytes) = serde_json::to_vec(&attestation) {
            let sig_key = format!("dsr_exports/{dsr_id}.sig.json");
            let sig_ok =
                tokio::task::block_in_place(|| handle.block_on(r2.put(&sig_key, sig_bytes)));
            match sig_ok {
                Ok(()) => signed = true,
                Err(e) => {
                    tracing::warn!(dsr_id = %dsr_id, error = %e, "dsr/portability: export signature PUT failed")
                }
            }
        }
    }

    ExportReceipt {
        content_sha256,
        persisted: true,
        r2_key: Some(object_key),
        signed,
    }
}

/// Why a rectification was refused (maps to a fail-CLOSED 4xx).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RectifyReject {
    /// Target is content-addressed / immutable cache data (Art.16 N/A).
    ContentImmutable(String),
    /// Target is not an editable subject-PII field.
    NotEditable(String),
    /// The supplied new value failed validation (e.g. not a plausible email).
    InvalidValue(String),
}

impl RectifyReject {
    /// Human-readable reason (also the HTTP body).
    #[must_use]
    pub(super) fn message(&self) -> &str {
        match self {
            Self::ContentImmutable(m) | Self::NotEditable(m) | Self::InvalidValue(m) => m,
        }
    }
}

/// A validated rectification plan: the exact UPDATE to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RectifyPlan {
    pub sql: String,
    /// Stored value (already transformed, e.g. email → email_hash).
    pub stored_value: String,
}

/// Classify a rectification target against the editable-PII allowlist, applying
/// the field-specific value transform. FAIL-CLOSED: a content / non-editable
/// target or an invalid value is a [`RectifyReject`] (4xx), never a silent
/// pass. Pure (no I/O) so the policy is unit-tested offline.
pub(super) fn classify_rectify(
    table: &str,
    field: &str,
    new_value: &str,
) -> Result<RectifyPlan, RectifyReject> {
    if CONTENT_IMMUTABLE_TABLES.contains(&table) {
        return Err(RectifyReject::ContentImmutable(
            "cache content is content-addressed and immutable; \
             rectification (Art.16) applies to editable PII fields only"
                .to_owned(),
        ));
    }
    if !RECTIFIABLE_FIELDS.contains(&(table, field)) {
        return Err(RectifyReject::NotEditable(format!(
            "field '{table}.{field}' is not a rectifiable subject-PII field; \
             rectification (Art.16) applies to editable PII fields only"
        )));
    }
    // The only allowlisted field is tenant.email_hash — the account contact
    // email, stored pseudonymized. Validate a plausible email + hash it (never
    // store the raw email).
    let email = new_value.trim();
    if email.len() < 3 || !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err(RectifyReject::InvalidValue(
            "new value must be a valid email address".to_owned(),
        ));
    }
    Ok(RectifyPlan {
        sql: format!("UPDATE {table} SET {field} = ?2 WHERE tenant_id = ?1"),
        stored_value: email_hash(email),
    })
}

/// Outcome of a successful rectification.
#[derive(Debug, Clone, Serialize)]
pub(super) struct RectifyResult {
    pub table: String,
    pub field: String,
    pub rows_updated: u64,
}

/// Run the RECTIFICATION (Art.16) right: classify (fail-CLOSED on content /
/// non-editable / invalid), audit BEFORE the mutation, then apply the bounded
/// UPDATE. Idempotent: re-applying the same value is a no-op at the value level
/// and the audit row is `INSERT OR IGNORE` on a deterministic id.
pub(super) fn run_rectification(
    d1: &Arc<D1HttpClient>,
    dsr_id: &str,
    tenant_id: &str,
    table: &str,
    field: &str,
    new_value: &str,
    now_ms: u64,
) -> Result<Result<RectifyResult, RectifyReject>, String> {
    let plan = match classify_rectify(table, field, new_value) {
        Ok(p) => p,
        Err(reject) => return Ok(Err(reject)),
    };
    // Audit BEFORE the mutation (ADR-S11-002). Records the (table, field) — NOT
    // the new value (which is PII) — so the trail never re-leaks the email.
    audit_dsr_event(
        d1,
        dsr_id,
        tenant_id,
        EVENT_RECTIFICATION,
        &format!("{table}.{field}"),
        &json!({ "surface": "rectification", "table": table, "field": field }),
        now_ms,
    )?;
    let rows = d1_query_blocking(
        d1,
        &plan.sql,
        vec![json!(tenant_id), json!(plan.stored_value)],
    )?;
    // D1 returns the changed-row set / meta; we report the count defensively.
    let rows_updated = u64::try_from(rows.len()).unwrap_or(0);
    Ok(Ok(RectifyResult {
        table: table.to_owned(),
        field: field.to_owned(),
        rows_updated,
    }))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests"
)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TID: &str = "00000000-0000-7000-8000-000000000002";

    /// The access gather covers EXACTLY the erase-set (single-sourced): the
    /// non-retained (erasable) tables in the plan == TENANT_ID_TABLES ∪
    /// NAMESPACE_TABLES ∪ SPECIAL_ERASE_TABLES. A future erase-set table is then
    /// automatically gathered by access too — no parallel list to drift.
    #[test]
    fn access_set_equals_erase_set() {
        let plan = build_gather_plan(TID);
        let gathered: HashSet<&str> = plan
            .iter()
            .filter(|q| !q.retained)
            .map(|q| q.table)
            .collect();
        let mut erase_set: HashSet<&str> = HashSet::new();
        erase_set.extend(super::super::adapter_d1::TENANT_ID_TABLES.iter().copied());
        erase_set.extend(super::super::adapter_d1::NAMESPACE_TABLES.iter().copied());
        erase_set.extend(
            super::super::adapter_d1::SPECIAL_ERASE_TABLES
                .iter()
                .copied(),
        );
        assert_eq!(
            gathered, erase_set,
            "access gather (erasable) must equal the D1 erase-set exactly"
        );
    }

    /// The retained disclosable subset is a real subset of RETAIN_SET and never
    /// overlaps the erase-set (so we disclose retained data without ever
    /// touching it on an erasure).
    #[test]
    fn retain_disclosable_is_subset_and_disjoint_from_erase_set() {
        let retain: HashSet<&str> = super::super::adapter_d1::RETAIN_SET
            .iter()
            .copied()
            .collect();
        for t in RETAIN_DISCLOSABLE_TABLES {
            assert!(retain.contains(t), "{t} must be in RETAIN_SET");
        }
        let mut erase_set: HashSet<&str> = HashSet::new();
        erase_set.extend(super::super::adapter_d1::TENANT_ID_TABLES.iter().copied());
        erase_set.extend(super::super::adapter_d1::NAMESPACE_TABLES.iter().copied());
        erase_set.extend(
            super::super::adapter_d1::SPECIAL_ERASE_TABLES
                .iter()
                .copied(),
        );
        for t in RETAIN_DISCLOSABLE_TABLES {
            assert!(
                !erase_set.contains(t),
                "{t} is retained — must NOT be in the erase-set"
            );
        }
        // The plan tags exactly the disclosable subset as retained.
        let plan = build_gather_plan(TID);
        let plan_retained: HashSet<&str> = plan
            .iter()
            .filter(|q| q.retained)
            .map(|q| q.table)
            .collect();
        let want: HashSet<&str> = RETAIN_DISCLOSABLE_TABLES.iter().copied().collect();
        assert_eq!(plan_retained, want);
    }

    /// gather assembles rows across multiple tables for a seeded tenant, and
    /// secret columns are NEVER exported raw.
    #[test]
    fn gather_assembles_rows_and_redacts_secrets() {
        let export = gather_subject_data(TID, 1_700_000_000_000, |sql, _params| {
            if sql.starts_with("SELECT * FROM pat ") {
                let mut row = D1Row::new();
                row.insert("pat_id".into(), json!("p1"));
                row.insert("tenant_id".into(), json!(TID));
                row.insert("pat_hash".into(), json!("SECRET-DIGEST"));
                row.insert("shown_once_token".into(), json!("SECRET-TOKEN"));
                row.insert("scope".into(), json!("read-write"));
                Ok(vec![row])
            } else if sql.starts_with("SELECT * FROM tenant ") {
                let mut row = D1Row::new();
                row.insert("tenant_id".into(), json!(TID));
                row.insert("email_hash".into(), json!("abc123"));
                Ok(vec![row])
            } else {
                Ok(vec![])
            }
        })
        .unwrap();

        let pat = export
            .tables
            .iter()
            .find(|t| t.table == "pat")
            .expect("pat table present");
        assert_eq!(pat.row_count, 1);
        let row = &pat.rows[0];
        assert_eq!(row.get("pat_hash").unwrap(), &json!("<redacted>"));
        assert_eq!(row.get("shown_once_token").unwrap(), &json!("<redacted>"));
        // Non-secret columns survive verbatim.
        assert_eq!(row.get("scope").unwrap(), &json!("read-write"));
        assert_eq!(row.get("pat_id").unwrap(), &json!("p1"));

        // The bundle is machine-readable structured JSON (per-table).
        let v = serde_json::to_value(&export).unwrap();
        assert_eq!(v["schema"], json!(EXPORT_SCHEMA));
        assert!(v["tables"].is_array());
    }

    /// FAIL-CLOSED: a D1 error on ANY table aborts the whole export — never a
    /// partial-looking "complete" bundle.
    #[test]
    fn gather_fails_closed_on_d1_error() {
        let res = gather_subject_data(TID, 1, |sql, _params| {
            if sql.contains("FROM tenant_quota") {
                Err("simulated D1 outage".to_owned())
            } else {
                Ok(vec![])
            }
        });
        assert!(res.is_err(), "a D1 error must fail the whole export");
        assert!(res.unwrap_err().contains("tenant_quota"));
    }

    /// SQL NULL in a secret column stays null (presence truthfully absent),
    /// non-null secret material is redacted.
    #[test]
    fn redact_keeps_null_present_redacts_value() {
        let mut row = D1Row::new();
        row.insert("pat_hash".into(), Value::Null);
        row.insert("secret_blob".into(), json!("xyz"));
        row.insert("region".into(), json!("weur"));
        let out = redact_row(row);
        assert!(out.get("pat_hash").unwrap().is_null());
        assert_eq!(out.get("secret_blob").unwrap(), &json!("<redacted>"));
        assert_eq!(out.get("region").unwrap(), &json!("weur"));
    }

    #[test]
    fn sensitive_col_detection() {
        assert!(is_sensitive_col("pat_hash"));
        assert!(is_sensitive_col("shown_once_token"));
        assert!(is_sensitive_col("byok_envelope"));
        assert!(is_sensitive_col("dek_ciphertext"));
        // Not secrets — must NOT be redacted.
        assert!(!is_sensitive_col("key_id"));
        assert!(!is_sensitive_col("idempotency_key"));
        assert!(!is_sensitive_col("tenant_id"));
        // email_hash is a pseudonym (the subject's own data), intentionally NOT
        // a credential token — disclosed, not redacted.
        assert!(!is_sensitive_col("email_hash"));
    }

    /// Rectification: content target → ContentImmutable; non-editable field →
    /// NotEditable; the allowlisted email field → a hashing UPDATE plan.
    #[test]
    fn rectify_rejects_content_and_non_editable_allows_email() {
        // `email_hash` reads process-global `EMAIL_HASH_SALT`; hold the shared
        // lock (forces it UNSET) so the unsalted assertion below is stable.
        let _env = crate::email_hash::EnvGuard::acquire();
        // Cache content is immutable.
        let r = classify_rectify("blob_meta", "digest", "x");
        assert!(matches!(r, Err(RectifyReject::ContentImmutable(_))));

        // A non-editable field fails CLOSED.
        let r = classify_rectify("tenant", "tier", "enterprise");
        assert!(matches!(r, Err(RectifyReject::NotEditable(_))));

        // The allowlisted email field is accepted + hashed (raw email never stored).
        let plan = classify_rectify("tenant", "email_hash", "  New.User@Example.COM ").unwrap();
        assert!(plan
            .sql
            .starts_with("UPDATE tenant SET email_hash = ?2 WHERE tenant_id = ?1"));
        assert_eq!(plan.stored_value, email_hash("new.user@example.com"));
        assert_ne!(
            plan.stored_value, "new.user@example.com",
            "raw email never stored"
        );
    }

    #[test]
    fn rectify_rejects_invalid_email() {
        let r = classify_rectify("tenant", "email_hash", "not-an-email");
        assert!(matches!(r, Err(RectifyReject::InvalidValue(_))));
        let r = classify_rectify("tenant", "email_hash", "a b@c.com");
        assert!(matches!(r, Err(RectifyReject::InvalidValue(_))));
    }

    /// Audit ids are deterministic per (dsr_id, event_type, suffix) — the
    /// idempotency key that makes a retry an INSERT OR IGNORE no-op.
    #[test]
    fn audit_event_id_is_deterministic() {
        let a = audit_event_id("dsr-1", EVENT_ACCESS, "-");
        let b = audit_event_id("dsr-1", EVENT_ACCESS, "-");
        assert_eq!(a, b);
        // Distinct per surface + per rectification target.
        assert_ne!(a, audit_event_id("dsr-1", EVENT_PORTABILITY, "-"));
        assert_ne!(
            audit_event_id("dsr-1", EVENT_RECTIFICATION, "tenant.email_hash"),
            audit_event_id("dsr-1", EVENT_RECTIFICATION, "tenant.tier")
        );
    }

    #[test]
    fn email_hash_matches_canonical_scheme() {
        let env = crate::email_hash::EnvGuard::acquire(); // salt UNSET
                                                          // Unsalted: equals hex(sha256(trim+lowercase)) — pre-salt parity.
        let want = hex::encode(Sha256::digest(b"user@example.com"));
        assert_eq!(email_hash("  USER@Example.com  "), want);

        // Matching invariant: this rectification site routes through the ONE
        // shared helper, so it equals it in BOTH modes (hence equals the
        // customer_d1 write site, which also delegates to the helper).
        assert_eq!(
            email_hash("user@example.com"),
            crate::email_hash::hash_email("user@example.com")
        );
        env.set_salt("the-server-salt");
        assert_eq!(
            email_hash("user@example.com"),
            crate::email_hash::hash_email("user@example.com")
        );
        // Under a salt the pseudonym is no longer the rainbow-attackable SHA-256.
        assert_ne!(email_hash("user@example.com"), want);
    }
}

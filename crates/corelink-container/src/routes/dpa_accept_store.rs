//! Production [`DpaAcceptStore`] adapter: the durable D1-over-HTTP writer +
//! reader backing `POST /v1/onboarding/dpa-accept`.
//!
//! This mirrors the [`super::tier_select_store::D1HttpTierSelectStore`]
//! pattern exactly — a thin adapter over the shared [`D1HttpClient`] that runs
//! parameterised SQL against the Cloudflare D1 REST API and is **fail-CLOSED**
//! (any transport / non-2xx / decode failure surfaces as `Err(String)`; a
//! failed read is NEVER silently treated as "already accepted").
//!
//! # Statements (migration `0038_dpa_acceptances.sql`)
//!
//! - `find_by_signup_id` → `SELECT jwt_receipt_jti, dpa_version, locale,
//!   accepted_at FROM dpa_acceptances WHERE signup_id = ?1` (idempotency
//!   replay lookup — an existing row returns the ORIGINAL receipt without a
//!   re-sign, matching the crate's PAT-RETRY-IDEMPOTENT-001 semantics).
//! - `insert_acceptance` → `INSERT OR IGNORE INTO dpa_acceptances (...) VALUES
//!   (...) RETURNING signup_id`. `Ok(true)` iff WE inserted (no prior row);
//!   an `Ok(false)` means a concurrent writer won the PK race and the caller
//!   re-reads the winning row. Every column is a REAL consent value produced
//!   by the orchestrator (no placeholders): the client-attested SHA-256 hex64
//!   notice hash, the server-hashed IP (sha256(ip‖salt), hex64 — CTRL-PRIV-001,
//!   raw IP never persisted), the deterministic `wording_id`, and the RS256
//!   receipt `jti`.
//!
//! # Security invariants (do NOT regress)
//!
//! - **Tenant comes from the verified header only** — the orchestration passes
//!   the `tenant_id` that `authorize_dpa` extracted from `x-corelink-tenant-id`;
//!   this adapter NEVER re-derives or defaults it.
//! - **Fail-CLOSED** on every transport / decode error.
//! - **Secrets never logged** — the CF API bearer lives inside [`D1HttpClient`]
//!   (which redacts it) and is never surfaced by this adapter's `Debug`.

use std::sync::Arc;

use serde_json::json;

use crate::routes::dpa_accept::{AcceptanceRow, DpaAcceptStore, StoredAcceptance};
use crate::storage::d1_http::D1HttpClient;

/// Production durable store for DPA acceptances, backed by Cloudflare D1 over
/// the REST API. Holds the shared [`D1HttpClient`] (which owns + redacts the
/// CF API token).
#[derive(Clone)]
pub struct D1HttpDpaAcceptStore {
    d1: Arc<D1HttpClient>,
}

impl D1HttpDpaAcceptStore {
    /// Wire the store over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

impl std::fmt::Debug for D1HttpDpaAcceptStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The D1 client redacts its own CF API token; the marker keeps a leaked
        // Debug from ever exposing credentials.
        f.debug_struct("D1HttpDpaAcceptStore")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

/// Extract a required string column from a D1 row, mapping absence to a
/// fail-CLOSED `Err` (a malformed result set must never masquerade as data).
fn str_col(row: &crate::storage::d1_http::D1Row, key: &str) -> Result<String, String> {
    row.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("dpa_acceptances row missing string column `{key}`"))
}

/// Extract a required integer column (D1 returns `BIGINT` as a JSON number).
fn i64_col(row: &crate::storage::d1_http::D1Row, key: &str) -> Result<i64, String> {
    row.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("dpa_acceptances row missing i64 column `{key}`"))
}

impl DpaAcceptStore for D1HttpDpaAcceptStore {
    async fn find_by_signup_id(&self, signup_id: &str) -> Result<Option<StoredAcceptance>, String> {
        let rows = self
            .d1
            .query(
                "SELECT jwt_receipt_jti, dpa_version, locale, accepted_at \
                 FROM dpa_acceptances WHERE signup_id = ?1 LIMIT 1",
                &[json!(signup_id)],
            )
            .await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(Some(StoredAcceptance {
            jwt_receipt_jti: str_col(row, "jwt_receipt_jti")?,
            dpa_version: str_col(row, "dpa_version")?,
            locale: str_col(row, "locale")?,
            accepted_at: i64_col(row, "accepted_at")?,
        }))
    }

    async fn insert_acceptance(&self, row: &AcceptanceRow) -> Result<bool, String> {
        // INSERT OR IGNORE is the atomic idempotency guard: the PK on
        // `signup_id` makes a re-accept a no-op (no row → RETURNING empty →
        // Ok(false)); the caller then re-reads the winning row. A fresh accept
        // returns its `signup_id` → Ok(true).
        let rows = self
            .d1
            .query(
                "INSERT OR IGNORE INTO dpa_acceptances \
                 (signup_id, tenant_id, dpa_version, locale, notice_hash, wording_id, \
                  ui_capture_ts, submission_ts, jwt_receipt_jti, accepted_ip_hash, \
                  accepted_at, schema_version) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                 RETURNING signup_id",
                &[
                    json!(row.signup_id),
                    json!(row.tenant_id),
                    json!(row.dpa_version),
                    json!(row.locale),
                    json!(row.notice_hash),
                    json!(row.wording_id),
                    json!(row.ui_capture_ts),
                    json!(row.submission_ts),
                    json!(row.jwt_receipt_jti),
                    json!(row.accepted_ip_hash),
                    json!(row.accepted_at),
                    json!(row.schema_version),
                ],
            )
            .await?;
        Ok(!rows.is_empty())
    }
}

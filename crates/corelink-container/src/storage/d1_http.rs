//! Cloudflare D1 HTTP API client for native-container metadata reads.
//!
//! D1 is accessible outside a CF Worker via the Cloudflare REST API:
//! `https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{db_id}/query`
//!
//! This module provides [`D1HttpClient`] — an async `reqwest`-based
//! client for running parameterised SQL queries against a D1 database
//! from the native Firecracker container.
//!
//! # Security charter compliance
//!
//! - CF API token is loaded from env via [`StorageEnv`] and never
//!   logged.
//! - No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.
//! - `#![forbid(unsafe_code)]` inherited from crate root.
//!
//! # Usage pattern
//!
//! ```ignore
//! let client = D1HttpClient::new(&env);
//! let rows = client.query("SELECT * FROM cas_meta WHERE digest = ?1",
//!                         &[serde_json::json!("abc123")]).await?;
//! ```

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::StorageEnv;

/// Async D1 HTTP API client.
///
/// `Debug` is a manual redacting impl (NOT `#[derive(Debug)]`) so a `{:?}` of
/// the root client can never print the CF API bearer token. Wrapper structs
/// (`D1HttpCustomerDb` etc.) already redact, but the root type must too.
pub struct D1HttpClient {
    http: reqwest::Client,
    /// Base URL: `https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{db_id}/query`.
    query_url: String,
    /// Bearer token for the `Authorization` header (CF API token).
    /// Never logged — stored as a plain `String` but treated as a secret.
    api_token: String,
}

impl core::fmt::Debug for D1HttpClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1HttpClient")
            .field("query_url", &self.query_url)
            .field("api_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// A single row returned from D1: a map of column name → JSON value.
pub type D1Row = serde_json::Map<String, serde_json::Value>;

/// Wire shape of a D1 query response.
#[derive(Debug, Deserialize)]
struct D1Response {
    result: Vec<D1QueryResult>,
    success: bool,
    errors: Vec<D1Error>,
}

/// Per-statement result inside a [`D1Response`].
#[derive(Debug, Deserialize)]
struct D1QueryResult {
    results: Vec<D1Row>,
}

/// A Cloudflare API error object.
#[derive(Debug, Deserialize)]
struct D1Error {
    message: String,
}

/// Request body for the D1 query endpoint.
#[derive(Debug, Serialize)]
struct D1QueryRequest<'a> {
    sql: &'a str,
    params: Vec<serde_json::Value>,
}

impl D1HttpClient {
    /// Construct a new [`D1HttpClient`] from a validated [`StorageEnv`].
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if `reqwest::Client` cannot be built (in
    /// practice this only fails on platforms that lack TLS support).
    pub fn new(env: &StorageEnv) -> Result<Self, String> {
        let query_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/d1/database/{}/query",
            env.cloudflare_account_id, env.d1_database_id,
        );
        // Bound EVERY D1-over-HTTP call. A single erase drives ~15 serial D1
        // round-trips (legitimacy + idempotency ledger + audit envelope + the
        // D1/R2 adapters), and the CF D1 REST API rate-limits + slows under a
        // burst (e.g. a bulk DSR backlog drain). A reqwest client built with NO
        // timeout lets a slowed/stuck call hold its container worker
        // INDEFINITELY; under sustained load those held workers accrete until the
        // tokio executor is saturated and the whole container stops responding
        // (observed: the `_system` container hanging after ~35-45 DSR ops, only a
        // recycle restoring it). Explicit connect + total timeouts convert a slow
        // backend into a fast, retryable error that RELEASES the worker, so the
        // container degrades gracefully instead of wedging. `pool_idle_timeout`
        // keeps the idle-connection set from lingering across a long drain.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(20))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("D1HttpClient: reqwest build failed: {e}"))?;
        Ok(Self {
            http,
            query_url,
            api_token: env.cf_api_token.clone(),
        })
    }

    /// Execute a parameterised SQL query against the D1 database.
    ///
    /// `params` must be positional (D1 uses `?1`, `?2`, … syntax for
    /// CF Workers; the HTTP API accepts a JSON array of values in
    /// order).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on HTTP failure, JSON parse error, or
    /// a D1-level error returned in the `errors` array.
    pub async fn query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<D1Row>, String> {
        debug!(sql = %sql, params = params.len(), "D1HttpClient::query");

        let body = D1QueryRequest {
            sql,
            params: params.to_vec(),
        };

        let resp = self
            .http
            .post(&self.query_url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("D1 HTTP request failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_owned());
            return Err(format!("D1 HTTP {status}: {text}"));
        }

        let parsed: D1Response = resp
            .json()
            .await
            .map_err(|e| format!("D1 response JSON parse failed: {e}"))?;

        if !parsed.success {
            let msgs: Vec<&str> = parsed.errors.iter().map(|e| e.message.as_str()).collect();
            let msg = msgs.join("; ");
            warn!(sql = %sql, errors = %msg, "D1 query returned errors");
            return Err(format!("D1 query errors: {msg}"));
        }

        // We send a single statement per request; take the first result set.
        Ok(parsed
            .result
            .into_iter()
            .next()
            .map(|r| r.results)
            .unwrap_or_default())
    }
}

/// Metadata record for a CAS blob, sourced from D1.
///
/// Mirrors the `cas_meta` D1 table shape. Fields are `#[non_exhaustive]`
/// so new columns can be added without breaking existing code.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CasMetaRecord {
    /// BLAKE3 digest of the blob.
    pub digest: String,
    /// Tenant ID (UUID string).
    pub tenant_id: String,
    /// Size in bytes.
    pub size_bytes: i64,
}

impl D1HttpClient {
    /// Look up a CAS metadata record by tenant + digest.
    ///
    /// Returns `Ok(Some(record))` when found, `Ok(None)` when the row
    /// does not exist, and `Err(String)` on query error.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors.
    pub async fn cas_meta_lookup(
        &self,
        tenant_id: &str,
        digest: &str,
    ) -> Result<Option<CasMetaRecord>, String> {
        let rows = self
            .query(
                "SELECT digest, tenant_id, size_bytes FROM cas_meta WHERE tenant_id = ?1 AND digest = ?2 LIMIT 1",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::String(digest.to_owned()),
                ],
            )
            .await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = rows.into_iter().next().ok_or("D1: empty result set")?;

        let record_digest = row
            .get("digest")
            .and_then(|v| v.as_str())
            .ok_or("D1 cas_meta: missing `digest` column")?
            .to_owned();

        let record_tenant = row
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or("D1 cas_meta: missing `tenant_id` column")?
            .to_owned();

        let size_bytes = row
            .get("size_bytes")
            .and_then(|v| v.as_i64())
            .ok_or("D1 cas_meta: missing or non-integer `size_bytes` column")?;

        Ok(Some(CasMetaRecord {
            digest: record_digest,
            tenant_id: record_tenant,
            size_bytes,
        }))
    }
}

/// Tenant admin record sourced from the `tier_selections` D1 table.
///
/// Mirrors the operational tier+subscription columns used by the
/// admin plane (`migrations/d1/0039_tier_selection.sql`). Fields are
/// `#[non_exhaustive]` so additive column changes don't break callers.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct TenantAdminRecord {
    /// Opaque tenant id (matches `tenant.tenant_id`).
    pub tenant_id: String,
    /// Selected tier: free | starter | team | pro | enterprise.
    pub tier: String,
    /// Canonical subscription state.
    pub subscription_state: String,
    /// Stripe customer id (mapped atomically with the tier write).
    pub stripe_customer_id: Option<String>,
}

impl D1HttpClient {
    /// Look up a tenant admin record by tenant id.
    ///
    /// Returns `Ok(Some(record))` when found, `Ok(None)` when the row
    /// does not exist, and `Err(String)` on query error.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors.
    pub async fn tenant_admin_lookup(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantAdminRecord>, String> {
        let rows = self
            .query(
                "SELECT tenant_id, tier, subscription_state, stripe_customer_id \
                 FROM tier_selections WHERE tenant_id = ?1 LIMIT 1",
                &[serde_json::Value::String(tenant_id.to_owned())],
            )
            .await?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        let tenant_id = row
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or("D1 tier_selections: missing `tenant_id` column")?
            .to_owned();
        let tier = row
            .get("tier")
            .and_then(|v| v.as_str())
            .ok_or("D1 tier_selections: missing `tier` column")?
            .to_owned();
        let subscription_state = row
            .get("subscription_state")
            .and_then(|v| v.as_str())
            .ok_or("D1 tier_selections: missing `subscription_state` column")?
            .to_owned();
        let stripe_customer_id = row
            .get("stripe_customer_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        Ok(Some(TenantAdminRecord {
            tenant_id,
            tier,
            subscription_state,
            stripe_customer_id,
        }))
    }

    /// Set the tier for a tenant in `tier_selections`.
    ///
    /// Returns `Ok(true)` if a row was updated, `Ok(false)` if no row
    /// matched (tenant does not exist), or `Err(String)` on D1 error.
    /// The write is additive — `subscription_state` is preserved.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors.
    pub async fn tenant_set_tier(&self, tenant_id: &str, tier: &str) -> Result<bool, String> {
        // First confirm the row exists — D1 HTTP `success` does not
        // discriminate "0 rows updated" from "1 row updated".
        let pre = self.tenant_admin_lookup(tenant_id).await?;
        if pre.is_none() {
            return Ok(false);
        }
        let _ = self
            .query(
                "UPDATE tier_selections SET tier = ?1 WHERE tenant_id = ?2",
                &[
                    serde_json::Value::String(tier.to_owned()),
                    serde_json::Value::String(tenant_id.to_owned()),
                ],
            )
            .await?;
        Ok(true)
    }
}

/// Outcome of [`D1HttpClient::admin_approval_verify_consume`] (finding H5).
/// Mirrors the reject taxonomy of `corelink-handler-admin`'s
/// `ApprovalRejection`; the container's `D1ApprovalLedger` maps it across.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdminApprovalConsume {
    /// Verified + atomically consumed; carries the recorded approver.
    Consumed {
        /// The authenticated approver recorded at approval-creation time.
        approver: String,
    },
    /// No approval row exists for the id (forged / absent).
    Unknown,
    /// The row was recorded for a different `resource`.
    ScopeMismatch,
    /// The recorded approver equals the initiator (self-approval).
    SelfApproval {
        /// The recorded approver that collided with the initiator.
        approver: String,
    },
    /// The approval was already spent (single-use replay / lost race).
    AlreadyConsumed,
}

impl D1HttpClient {
    /// Record ("create") an admin dual-approval row (migration 0091,
    /// finding H5). Idempotent on an UNCONSUMED `approval_id`; refuses to
    /// overwrite a consumed one (a spent approval can never be resurrected).
    ///
    /// `approver` MUST be the independently-authenticated approver identity
    /// from the approve endpoint's own auth gate — never a client-body value.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on a D1 error or an attempt to re-record a
    /// consumed approval.
    pub async fn admin_approval_create(
        &self,
        approval_id: &str,
        approver: &str,
        resource: &str,
        now_ms: i64,
    ) -> Result<(), String> {
        // Guard: never resurrect a consumed approval.
        let existing = self
            .query(
                "SELECT consumed FROM admin_approvals WHERE approval_id = ?1 LIMIT 1",
                &[serde_json::json!(approval_id)],
            )
            .await?;
        if let Some(row) = existing.first() {
            let consumed = row
                .get("consumed")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            if consumed != 0 {
                return Err(format!(
                    "approval_id={approval_id} already consumed; cannot re-record"
                ));
            }
        }
        let _ = self
            .query(
                "INSERT INTO admin_approvals \
                   (approval_id, approver, resource, consumed, created_at_ms) \
                 VALUES (?1, ?2, ?3, 0, ?4) \
                 ON CONFLICT(approval_id) DO UPDATE SET \
                   approver = excluded.approver, resource = excluded.resource \
                 WHERE admin_approvals.consumed = 0",
                &[
                    serde_json::json!(approval_id),
                    serde_json::json!(approver),
                    serde_json::json!(resource),
                    serde_json::json!(now_ms),
                ],
            )
            .await?;
        Ok(())
    }

    /// Verify an admin approval against `initiator` + `resource`, then
    /// ATOMICALLY consume it (single-use). The recorded approver — not any
    /// request-body value — is the authority for the distinct-approver check.
    ///
    /// The consume is a conditional `UPDATE ... WHERE consumed = 0 RETURNING`
    /// so two concurrent spends of the same approval cannot both succeed.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on a D1 error or a malformed row. A malformed /
    /// unreachable ledger is fail-CLOSED at the caller (no mutation commits).
    pub async fn admin_approval_verify_consume(
        &self,
        approval_id: &str,
        initiator: &str,
        resource: &str,
        now_ms: i64,
    ) -> Result<AdminApprovalConsume, String> {
        let rows = self
            .query(
                "SELECT approver, resource, consumed FROM admin_approvals \
                 WHERE approval_id = ?1 LIMIT 1",
                &[serde_json::json!(approval_id)],
            )
            .await?;
        let Some(row) = rows.first() else {
            return Ok(AdminApprovalConsume::Unknown);
        };
        let rec_approver = row
            .get("approver")
            .and_then(|v| v.as_str())
            .ok_or("D1 admin_approvals: missing `approver` column")?
            .to_owned();
        let rec_resource = row
            .get("resource")
            .and_then(|v| v.as_str())
            .ok_or("D1 admin_approvals: missing `resource` column")?
            .to_owned();
        let consumed = row
            .get("consumed")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        // Ordering mirrors the in-memory ledger: self-approval, then scope,
        // then already-consumed.
        if rec_approver == initiator {
            return Ok(AdminApprovalConsume::SelfApproval {
                approver: rec_approver,
            });
        }
        if rec_resource != resource {
            return Ok(AdminApprovalConsume::ScopeMismatch);
        }
        if consumed != 0 {
            return Ok(AdminApprovalConsume::AlreadyConsumed);
        }
        // Atomic single-use consume — only one racer flips 0 -> 1.
        let consumed_rows = self
            .query(
                "UPDATE admin_approvals SET consumed = 1, consumed_at_ms = ?2 \
                 WHERE approval_id = ?1 AND consumed = 0 RETURNING approver",
                &[serde_json::json!(approval_id), serde_json::json!(now_ms)],
            )
            .await?;
        if consumed_rows.is_empty() {
            // Lost the race to a concurrent consume.
            return Ok(AdminApprovalConsume::AlreadyConsumed);
        }
        Ok(AdminApprovalConsume::Consumed {
            approver: rec_approver,
        })
    }
}

impl D1HttpClient {
    /// Read a tenant's `tenant_quota` row (migration 0066).
    ///
    /// Returns `Ok(Some(state))` when the row exists, `Ok(None)` when it
    /// does not (a fresh tenant), and `Err(String)` on D1 transport /
    /// decode error. Backs [`crate::tenant_quota::D1QuotaStore::get`];
    /// the quota guard fail-CLOSES on the `Err` arm.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors or a malformed
    /// row (missing / non-integer column).
    pub async fn tenant_quota_lookup(
        &self,
        tenant_id: &str,
    ) -> Result<Option<crate::tenant_quota::QuotaState>, String> {
        let rows = self
            .query(
                "SELECT monthly_budget_usd_micros, accrued_usd_micros, cycle_anchor_ms \
                 FROM tenant_quota WHERE tenant_id = ?1 LIMIT 1",
                &[serde_json::Value::String(tenant_id.to_owned())],
            )
            .await?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        let monthly_budget_usd_micros = row
            .get("monthly_budget_usd_micros")
            .and_then(serde_json::Value::as_i64)
            .ok_or("D1 tenant_quota: missing or non-integer `monthly_budget_usd_micros`")?;
        let accrued_usd_micros = row
            .get("accrued_usd_micros")
            .and_then(serde_json::Value::as_i64)
            .ok_or("D1 tenant_quota: missing or non-integer `accrued_usd_micros`")?;
        let cycle_anchor_ms = row
            .get("cycle_anchor_ms")
            .and_then(serde_json::Value::as_i64)
            .ok_or("D1 tenant_quota: missing or non-integer `cycle_anchor_ms`")?;

        Ok(Some(crate::tenant_quota::QuotaState {
            monthly_budget_usd_micros,
            accrued_usd_micros,
            cycle_anchor_ms,
        }))
    }

    /// **Absolute** upsert of a tenant's `tenant_quota` row (migration
    /// 0066). Backs [`crate::tenant_quota::D1QuotaStore::put`] — used ONLY
    /// to seed a fresh tenant and to roll the cycle, where the new
    /// `accrued_usd_micros` is a computed value (the op's cost on a fresh
    /// cycle), NOT a delta over the prior row.
    ///
    /// Idempotent on `tenant_id` (PRIMARY KEY) via
    /// `INSERT … ON CONFLICT … DO UPDATE`. The `monthly_budget_usd_micros`
    /// ceiling is preserved on conflict (only the operator tunes it); the
    /// accrued counter + cycle anchor + `updated_at_ms` are overwritten
    /// with the guard-computed values. This is a deliberate BLIND
    /// overwrite — correct for the seed/roll path, where the prior accrued
    /// value is being intentionally discarded. The hot-path accrual uses
    /// the atomic [`Self::tenant_quota_accrue`] instead.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors.
    pub async fn tenant_quota_upsert(
        &self,
        tenant_id: &str,
        state: crate::tenant_quota::QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        let _ = self
            .query(
                "INSERT INTO tenant_quota \
                   (tenant_id, monthly_budget_usd_micros, accrued_usd_micros, \
                    cycle_anchor_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   accrued_usd_micros = excluded.accrued_usd_micros, \
                   cycle_anchor_ms    = excluded.cycle_anchor_ms, \
                   updated_at_ms      = excluded.updated_at_ms",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(state.monthly_budget_usd_micros),
                    serde_json::Value::from(state.accrued_usd_micros),
                    serde_json::Value::from(state.cycle_anchor_ms),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        Ok(())
    }

    /// **Atomic increment** of a tenant's accrued cost (migration 0066).
    /// Backs [`crate::tenant_quota::D1QuotaStore::accrue`] — the hot-path
    /// accrual after a served billable op.
    ///
    /// The increment is done by the DB inside `ON CONFLICT … DO UPDATE`:
    ///
    /// ```sql
    /// accrued_usd_micros = tenant_quota.accrued_usd_micros + excluded.accrued_usd_micros
    /// ```
    ///
    /// i.e. the bound `?3` carries the per-op **delta** (`cost`), and the
    /// add happens in SQLite, not in the application. This closes the
    /// TOCTOU lost-update window: a blind `SET accrued = excluded.accrued`
    /// (read total in app, write total back) loses one op's spend when two
    /// requests interleave; letting the DB compute `accrued + delta` makes
    /// concurrent accruals sum. On the INSERT (fresh-row) path the row is
    /// seeded at the default tripwire with `accrued = delta` and
    /// `cycle_anchor_ms = seed_anchor_ms`; an existing row keeps its anchor
    /// and budget (only the operator tunes the budget).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on D1 communication errors.
    pub async fn tenant_quota_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        let _ = self
            .query(
                // Explicitly seed `monthly_budget_usd_micros` with the
                // effectively-unlimited default (ADR-0068 2026-07-09) on the
                // INSERT path. Omitting it made a fresh tenant's first accrual
                // fall to the table's stale `DEFAULT 5000000` ($5) — the
                // miscalibrated placeholder the reconciliation retired. ON
                // CONFLICT never touches the budget, so an existing tenant's
                // operator-set ceiling is preserved.
                "INSERT INTO tenant_quota \
                   (tenant_id, monthly_budget_usd_micros, accrued_usd_micros, cycle_anchor_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   accrued_usd_micros = tenant_quota.accrued_usd_micros \
                                        + excluded.accrued_usd_micros, \
                   updated_at_ms      = excluded.updated_at_ms",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(crate::tenant_quota::DEFAULT_MONTHLY_BUDGET_USD_MICROS),
                    serde_json::Value::from(delta_micros),
                    serde_json::Value::from(seed_anchor_ms),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::storage::StorageEnv;

    #[test]
    fn d1_http_client_new_fails_gracefully_without_env() {
        // Simulate the env not being set — StorageEnv::from_env()
        // returns None so this path never constructs D1HttpClient;
        // here we construct directly with stub values to test the URL
        // format only.
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "acct123".to_owned(),
            cf_api_token: "tok".to_owned(),
            d1_database_id: "db456".to_owned(),
        };
        let client = D1HttpClient::new(&stub_env).expect("build client");
        assert!(
            client.query_url.contains("acct123"),
            "URL must include account_id"
        );
        assert!(
            client.query_url.contains("db456"),
            "URL must include database_id"
        );
    }

    /// Live D1 query test — requires real credentials.
    ///
    /// Run manually:
    ///
    /// ```bash
    /// CLOUDFLARE_ACCOUNT_ID=<acc> CF_API_TOKEN=<tok> D1_DATABASE_ID=<id> \
    ///   ... other vars ...
    ///   cargo test -p corelink-server d1_http_cas_meta_round_trip -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "requires live CF D1 credentials"]
    async fn d1_http_cas_meta_round_trip() {
        let env = StorageEnv::from_env().expect("all env vars must be set");
        let client = D1HttpClient::new(&env).expect("client");
        // Query a definitely-absent record — should return Ok(None).
        let result = client
            .cas_meta_lookup("00000000-0000-0000-0000-000000000000", "__no_such_digest__")
            .await
            .expect("query");
        assert!(result.is_none());
    }

    /// Live D1 tenant admin lookup — requires real credentials.
    /// WP-S1 Phase 2 (Admin half) acceptance probe.
    #[tokio::test]
    #[ignore = "requires live CF D1 credentials"]
    async fn d1_http_tenant_admin_lookup_round_trip() {
        let env = StorageEnv::from_env().expect("all env vars must be set");
        let client = D1HttpClient::new(&env).expect("client");
        let result = client
            .tenant_admin_lookup("00000000-0000-0000-0000-000000000000")
            .await
            .expect("query");
        // No such tenant — Ok(None).
        assert!(result.is_none());
    }
}

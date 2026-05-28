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
#[derive(Debug)]
pub struct D1HttpClient {
    http: reqwest::Client,
    /// Base URL: `https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{db_id}/query`.
    query_url: String,
    /// Bearer token for the `Authorization` header (CF API token).
    /// Never logged — stored as a plain `String` but treated as a secret.
    api_token: String,
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
        let http = reqwest::Client::builder()
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
    pub async fn tenant_set_tier(
        &self,
        tenant_id: &str,
        tier: &str,
    ) -> Result<bool, String> {
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

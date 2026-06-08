//! Durable D1-backed [`ManifestKvStore`] for the OCI registry adapter.
//!
//! # Why this exists (and is NOT the moat)
//!
//! The 2-level [`crate::adapter_cache::MoatCache`] stores IMMUTABLE,
//! content-addressed blob bytes (image layers + config blobs) keyed by
//! `blake3(content)`. OCI MANIFESTS + TAG LISTS are different: a tag
//! re-points to a new manifest digest on every push (MUTABLE), and
//! `GET /v2/<repo>/tags/list` needs prefix enumeration — neither fits a
//! content-addressed store. So the adapter's
//! [`corelink_adapter_host::oci::ports::ManifestKvStore`] port needs a
//! small mutable per-tenant `(key → bytes)` store with upsert + prefix
//! scan. This is the production binding over the `adapter_oci_kv` D1
//! table (migration 0061), mirroring [`crate::adapter_kv::NpmKvStore`].
//!
//! Keys are the adapter's own (`oci_manifest:<repo>:<ref>`,
//! `oci_tags:<repo>`, `oci_blob_index:<oci-digest>`); the tenant is the
//! port's [`TenantId`] arg (PAT-derived, never the path), stored as the
//! row's `tenant_id`. Values are hex-encoded TEXT (the D1 HTTP API is
//! JSON-only), same as `adapter_npm_meta`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use corelink_adapter_host::oci::ports::{ManifestKvStore, PortResult};
use corelink_core::TenantId;

use crate::storage::d1_http::D1HttpClient;

/// Production [`ManifestKvStore`] over the CF D1 `adapter_oci_kv` table.
pub struct OciKvStore {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for OciKvStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciKvStore").finish_non_exhaustive()
    }
}

impl OciKvStore {
    /// Construct from a D1 HTTP client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

/// Escape SQLite `LIKE` metacharacters (`\`, `%`, `_`) in a literal prefix
/// so [`OciKvStore::list_prefix`] matches the prefix VERBATIM. OCI keys
/// contain `_` (e.g. `oci_manifest:…`), which is the `LIKE` single-char
/// wildcard — without escaping, `oci_tags:` would also match `ociXtags:`.
/// Pair with `… LIKE ?n ESCAPE '\'` and append `%` to the escaped result.
fn escape_like_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Current wall-clock unix milliseconds (best-effort; saturating).
fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[async_trait]
impl ManifestKvStore for OciKvStore {
    async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
        let rows = self
            .d1
            .query(
                "SELECT value_hex FROM adapter_oci_kv \
                 WHERE tenant_id = ?1 AND kv_key = ?2 LIMIT 1",
                &[
                    serde_json::Value::String(tenant.to_canonical_text()),
                    serde_json::Value::String(key.to_owned()),
                ],
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let value_hex = row
            .get("value_hex")
            .and_then(|v| v.as_str())
            .ok_or("D1 adapter_oci_kv: missing `value_hex` column")?;
        let value = hex::decode(value_hex)
            .map_err(|e| format!("D1 adapter_oci_kv: value_hex not hex: {e}"))?;
        Ok(Some(Bytes::from(value)))
    }

    async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
        let value_hex = hex::encode(&value);
        self.d1
            .query(
                "INSERT INTO adapter_oci_kv (tenant_id, kv_key, value_hex, updated_ms) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(tenant_id, kv_key) DO UPDATE SET \
                   value_hex  = excluded.value_hex, \
                   updated_ms = excluded.updated_ms",
                &[
                    serde_json::Value::String(tenant.to_canonical_text()),
                    serde_json::Value::String(key.to_owned()),
                    serde_json::Value::String(value_hex),
                    serde_json::Value::from(unix_ms_now()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
        let pattern = format!("{}%", escape_like_literal(prefix));
        let rows = self
            .d1
            .query(
                "SELECT kv_key FROM adapter_oci_kv \
                 WHERE tenant_id = ?1 AND kv_key LIKE ?2 ESCAPE '\\' \
                 ORDER BY kv_key",
                &[
                    serde_json::Value::String(tenant.to_canonical_text()),
                    serde_json::Value::String(pattern),
                ],
            )
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(k) = row.get("kv_key").and_then(|v| v.as_str()) {
                out.push(k.to_owned());
            }
        }
        Ok(out)
    }
}

/// Build the production [`OciKvStore`] from process env
/// ([`crate::storage::StorageEnv`]). Returns `None` (fail-CLOSED — the OCI
/// route is not mounted) when the storage env is unset/invalid. Mirrors
/// [`crate::adapter_kv::npm_kv_from_env`].
#[must_use]
pub fn oci_kv_from_env() -> Option<Arc<OciKvStore>> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = D1HttpClient::new(&storage_env)
        .map_err(|e| tracing::warn!(error = %e, "oci manifest kv: D1 client init failed"))
        .ok()?;
    Some(Arc::new(OciKvStore::new(Arc::new(d1))))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_literal_escapes_wildcards() {
        // `_` and `%` and `\` are escaped; everything else is verbatim.
        assert_eq!(escape_like_literal("oci_tags:"), r"oci\_tags:");
        assert_eq!(
            escape_like_literal("oci_manifest:repo:tag"),
            r"oci\_manifest:repo:tag"
        );
        assert_eq!(escape_like_literal("a%b_c\\d"), r"a\%b\_c\\d");
        assert_eq!(escape_like_literal("plain:digest"), "plain:digest");
    }

    #[test]
    fn escaped_prefix_cannot_wildcard_match_a_sibling() {
        // The whole point: `oci_tags:` must NOT LIKE-match `ociXtags:`.
        // We assert the escaped pattern contains the backslash-escaped `_`
        // so the SQL `ESCAPE '\'` treats it as a literal underscore.
        let pat = format!("{}%", escape_like_literal("oci_tags:"));
        assert_eq!(pat, r"oci\_tags:%");
        assert!(pat.contains(r"\_"), "underscore must be escaped");
    }
}

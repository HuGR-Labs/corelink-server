//! `GET /-/v1/search` — package search proxy logic.
//!
//! Pure-logic layer (no `axum` types) so the body can be unit tested
//! without spinning up a server. The router in [`crate::npm::server`] is a
//! thin wrapper adapting `axum` Query/Response to/from this module.
//!
//! npm search (`npm search <text>`, the Web UI, and `GET /-/v1/search`) is a
//! registry-WIDE fuzzy search over the entire npm corpus. The adapter caches
//! only the individual packages a tenant has fetched, so there is no local
//! index that could answer a corpus-wide query honestly — a locally-computed
//! result would silently omit every package the tenant never touched. The
//! honest, real implementation is therefore a same-origin, SSRF-guarded
//! read-through to the configured upstream registry, which returns the
//! canonical `{ objects, total, time }` result set. The response is NOT cached
//! (unlike metadata): search results are query-shaped, high-cardinality, and
//! change as the corpus does, so a TTL cache would bloat KV for little hit
//! value.
//!
//! Auth (PAT → tenant) happens at the server layer BEFORE this runs (same as
//! [`crate::npm::metadata`]); the access is audited (`search.served.v1`) under
//! the resolved tenant. A search performs no state mutation, so the audit is an
//! access-log emit; it still fails CLOSED (a failed audit emit surfaces as a
//! 503) to keep one uniform audit posture across the adapter.

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;

use crate::npm::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::npm::error::NpmAdapterError;
use crate::npm::upstream::{UpstreamClient, NPM_SEARCH_DEFAULT_SIZE, NPM_SEARCH_MAX_SIZE};

/// Search response: raw JSON bytes to forward to the client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SearchResponse {
    /// Raw upstream JSON (`{ objects, total, time }`).
    pub body: Vec<u8>,
}

/// Clamp the client-requested page `size` into `[1, NPM_SEARCH_MAX_SIZE]`,
/// defaulting to [`NPM_SEARCH_DEFAULT_SIZE`] when the client omits it (or sends
/// `0`). A hostile `size` can never amplify one client request into an
/// unbounded upstream fetch.
#[must_use]
pub fn clamp_size(requested: Option<u32>) -> u32 {
    match requested {
        None | Some(0) => NPM_SEARCH_DEFAULT_SIZE,
        Some(n) => n.min(NPM_SEARCH_MAX_SIZE),
    }
}

/// Serve `GET /-/v1/search` — proxy a registry search to upstream.
///
/// Algorithm:
/// 1. Clamp `size`; fetch `{registry}/-/v1/search?text&size&from` (SSRF-guarded
///    same-origin) from upstream.
/// 2. Validate the response is a well-formed npm search envelope (fail-CLOSED
///    on malformed upstream — never forward garbage as a 200).
/// 3. Emit `search.served.v1` (audit-fail-CLOSED) and return the raw JSON.
///
/// # Errors
///
/// Surfaces any [`NpmAdapterError`] from the upstream / validation / audit
/// layers.
pub async fn serve_search(
    text: &str,
    size: Option<u32>,
    from: u32,
    tenant: &TenantId,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
) -> Result<SearchResponse, NpmAdapterError> {
    let effective_size = clamp_size(size);
    let raw = upstream.fetch_search(text, effective_size, from).await?;
    // Fail-CLOSED on malformed upstream: only forward a well-formed envelope.
    validate_search_json(&raw)?;

    let now = now_unix_ms();
    emit_npm_audit(
        auditor,
        event_types::SEARCH_SERVED,
        tenant,
        now,
        // `text` is a package-search string (public), never PII; `size`/`from`
        // are the paging window. No secret material is recorded.
        serde_json::json!({ "text": text, "size": effective_size, "from": from }),
    )?;

    Ok(SearchResponse { body: raw })
}

/// Validate that bytes are a well-formed npm search envelope: a JSON object
/// carrying an `objects` array. Fails CLOSED on malformed input.
///
/// # Errors
///
/// Returns [`NpmAdapterError::MetadataParse`] on malformed JSON or a missing /
/// non-array `objects` field (maps to `502 Bad Gateway`, the "bad upstream"
/// class the adapter already uses for malformed registry payloads).
pub fn validate_search_json(bytes: &[u8]) -> Result<serde_json::Value, NpmAdapterError> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| NpmAdapterError::MetadataParse(format!("invalid search JSON: {e}")))?;
    if !v.get("objects").is_some_and(serde_json::Value::is_array) {
        return Err(NpmAdapterError::MetadataParse(
            "expected npm search envelope with an `objects` array".into(),
        ));
    }
    Ok(v)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn clamp_size_defaults_and_bounds() {
        assert_eq!(clamp_size(None), NPM_SEARCH_DEFAULT_SIZE);
        assert_eq!(clamp_size(Some(0)), NPM_SEARCH_DEFAULT_SIZE);
        assert_eq!(clamp_size(Some(5)), 5);
        assert_eq!(clamp_size(Some(NPM_SEARCH_MAX_SIZE)), NPM_SEARCH_MAX_SIZE);
        // A hostile oversize is clamped to the ceiling, never forwarded raw.
        assert_eq!(clamp_size(Some(10_000)), NPM_SEARCH_MAX_SIZE);
    }

    #[test]
    fn validate_search_json_accepts_canonical_envelope() {
        let raw = br#"{"objects":[{"package":{"name":"lodash"}}],"total":1,"time":"x"}"#;
        assert!(validate_search_json(raw).is_ok());
    }

    #[test]
    fn validate_search_json_accepts_empty_objects() {
        let raw = br#"{"objects":[],"total":0,"time":"x"}"#;
        assert!(validate_search_json(raw).is_ok());
    }

    #[test]
    fn validate_search_json_rejects_missing_objects() {
        let raw = br#"{"total":0}"#;
        assert!(matches!(
            validate_search_json(raw),
            Err(NpmAdapterError::MetadataParse(_))
        ));
    }

    #[test]
    fn validate_search_json_rejects_non_array_objects() {
        let raw = br#"{"objects":"nope"}"#;
        assert!(matches!(
            validate_search_json(raw),
            Err(NpmAdapterError::MetadataParse(_))
        ));
    }

    #[test]
    fn validate_search_json_rejects_malformed() {
        assert!(matches!(
            validate_search_json(b"{not json"),
            Err(NpmAdapterError::MetadataParse(_))
        ));
    }

    #[test]
    fn validate_search_json_bad_upstream_maps_to_502() {
        let err = validate_search_json(b"[]").unwrap_err();
        assert_eq!(err.status_code(), 502);
    }
}

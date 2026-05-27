//! `GET /simple/<project>/` — index endpoint handler logic.
//!
//! Pure-logic layer (no `axum` types) so the body can be unit
//! tested without spinning up a server. The router in
//! [`crate::pip::server`] is a thin wrapper that adapts `axum`
//! `Request`/`Response` to/from this module's signatures.

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;

use crate::pip::audit::{emit_pip_audit, event_types, now_unix_ms};
use crate::pip::error::PipAdapterError;
use crate::pip::pep503_html::{encode_html, parse_json_index, ProjectIndex};
use crate::pip::ports::KvStoreHandle;
use crate::pip::upstream::UpstreamClient;

/// Content-type the adapter returns for PEP 691 (JSON) responses.
pub const JSON_CONTENT_TYPE: &str = "application/vnd.pypi.simple.v1+json";
/// Content-type the adapter returns for PEP 503 (HTML) responses.
pub const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";

/// Negotiated representation the client wants back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IndexFormat {
    /// PEP 691 JSON.
    Json,
    /// PEP 503 HTML.
    Html,
}

impl IndexFormat {
    /// Negotiate the response format from an HTTP `Accept` header.
    ///
    /// Rules:
    /// - If the client lists `application/vnd.pypi.simple.v1+json`
    ///   and `prefer_json_index` is true → [`Self::Json`].
    /// - If the client lists `text/html` and not the JSON content
    ///   type → [`Self::Html`].
    /// - Otherwise (no header, `*/*`, missing) → JSON if
    ///   `prefer_json_index`, else HTML.
    #[must_use]
    pub fn negotiate(accept: Option<&str>, prefer_json_index: bool) -> Self {
        let Some(hdr) = accept else {
            return if prefer_json_index { Self::Json } else { Self::Html };
        };
        let lower = hdr.to_ascii_lowercase();
        let accepts_json = lower.contains("application/vnd.pypi.simple.v1+json");
        let accepts_html = lower.contains("text/html");
        match (accepts_json, accepts_html, prefer_json_index) {
            (true, _, true) => Self::Json,
            (true, false, false) => Self::Json,
            (false, true, _) => Self::Html,
            (true, true, false) => Self::Html,
            (false, false, _) => {
                if prefer_json_index {
                    Self::Json
                } else {
                    Self::Html
                }
            }
        }
    }
}

/// Result of serving an index request — the body bytes and the
/// content-type to send.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct IndexResponse {
    /// Body bytes.
    pub body: Vec<u8>,
    /// Content-type to set on the response.
    pub content_type: &'static str,
}

/// KV key the adapter uses to store an index payload for `(tenant,
/// project)`. Pure helper extracted so unit tests can pin the key
/// shape.
#[must_use]
pub fn kv_key_for_project(project: &str) -> String {
    format!(
        "pip:idx:{}",
        crate::pip::pep503_html::normalise_project_name(project)
    )
}

/// Return `true` if the cached (`inserted_at_unix_ms`, `value`) tuple
/// is still within `ttl_seconds` of `now_unix_ms`.
#[must_use]
pub fn is_fresh(inserted_at_unix_ms: u64, now_unix_ms: u64, ttl_seconds: u64) -> bool {
    let ttl_ms = ttl_seconds.saturating_mul(1000);
    now_unix_ms.saturating_sub(inserted_at_unix_ms) < ttl_ms
}

/// Serve `GET /simple/<project>/`.
///
/// Algorithm:
/// 1. Look up cached JSON in `kv`. If fresh, decode + serve.
/// 2. Else fetch from upstream, parse, store back, emit audit
///    (`pip.index.refreshed.v1`).
/// 3. Encode the parsed payload as the negotiated format.
///
/// # Errors
///
/// Surfaces any `PipAdapterError` from the KV / upstream / audit
/// layers.
pub async fn serve_index(
    project: &str,
    format: IndexFormat,
    ttl_seconds: u64,
    tenant: &TenantId,
    kv: &KvStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
) -> Result<IndexResponse, PipAdapterError> {
    let key = kv_key_for_project(project);
    let now = now_unix_ms();
    let cached = kv.get(tenant, &key).await?;

    let (index, was_fresh) = if let Some((bytes, inserted)) = cached {
        if is_fresh(inserted, now, ttl_seconds) {
            let parsed = parse_json_index(&bytes)?;
            (parsed, true)
        } else {
            refresh_from_upstream(project, ttl_seconds, tenant, kv, upstream, auditor, now)
                .await
                .map(|i| (i, false))?
        }
    } else {
        refresh_from_upstream(project, ttl_seconds, tenant, kv, upstream, auditor, now)
            .await
            .map(|i| (i, false))?
    };

    if was_fresh {
        emit_pip_audit(
            auditor,
            event_types::INDEX_CACHE_HIT,
            tenant,
            now,
            serde_json::json!({ "project": project, "files": index.files.len() }),
        )?;
    }

    Ok(serialise_for_format(&index, format))
}

async fn refresh_from_upstream(
    project: &str,
    _ttl_seconds: u64,
    tenant: &TenantId,
    kv: &KvStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
    now: u64,
) -> Result<ProjectIndex, PipAdapterError> {
    let raw = upstream.fetch_json_index(project).await?;
    // Parse first so we fail-CLOSED on malformed upstream before we
    // pollute the KV cache (spec §8 adversarial row 5).
    let parsed = parse_json_index(&raw)?;
    // Audit BEFORE the KV write, per the audit-fail-CLOSED contract.
    emit_pip_audit(
        auditor,
        event_types::INDEX_REFRESHED,
        tenant,
        now,
        serde_json::json!({ "project": project, "files": parsed.files.len() }),
    )?;
    kv.put(tenant, &kv_key_for_project(project), raw, now).await?;
    Ok(parsed)
}

fn serialise_for_format(index: &ProjectIndex, format: IndexFormat) -> IndexResponse {
    match format {
        IndexFormat::Json => {
            let body = serde_json::to_vec(&serde_json::json!({
                "meta": { "api-version": "1.0" },
                "name": index.name,
                "files": index.files.iter().map(|f| serde_json::json!({
                    "filename": f.filename,
                    "url": f.url,
                    "hashes": { "sha256": f.sha256 },
                    "requires-python": f.requires_python,
                    "yanked": f.yanked,
                })).collect::<Vec<_>>(),
            }))
            .unwrap_or_default();
            IndexResponse {
                body,
                content_type: JSON_CONTENT_TYPE,
            }
        }
        IndexFormat::Html => IndexResponse {
            body: encode_html(index).into_bytes(),
            content_type: HTML_CONTENT_TYPE,
        },
    }
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
    fn negotiate_defaults_to_json_when_prefer_json() {
        assert_eq!(IndexFormat::negotiate(None, true), IndexFormat::Json);
        assert_eq!(IndexFormat::negotiate(Some("*/*"), true), IndexFormat::Json);
    }

    #[test]
    fn negotiate_returns_html_when_only_html_accepted() {
        assert_eq!(
            IndexFormat::negotiate(Some("text/html"), true),
            IndexFormat::Html
        );
    }

    #[test]
    fn negotiate_returns_json_when_pep691_accepted() {
        assert_eq!(
            IndexFormat::negotiate(
                Some("application/vnd.pypi.simple.v1+json"),
                true
            ),
            IndexFormat::Json
        );
    }

    #[test]
    fn negotiate_respects_prefer_flag_when_both_accepted() {
        assert_eq!(
            IndexFormat::negotiate(
                Some("application/vnd.pypi.simple.v1+json, text/html"),
                false
            ),
            IndexFormat::Html
        );
    }

    #[test]
    fn is_fresh_table() {
        assert!(is_fresh(0, 1000, 2));
        assert!(!is_fresh(0, 2001, 2));
        assert!(is_fresh(0, 0, 0).not());
    }

    #[test]
    fn kv_key_normalises_project_name() {
        assert_eq!(kv_key_for_project("Foo.Bar"), "pip:idx:foo-bar");
    }

    trait BoolExt {
        fn not(self) -> bool;
    }
    impl BoolExt for bool {
        fn not(self) -> bool {
            !self
        }
    }
}

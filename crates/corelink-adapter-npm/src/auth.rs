//! PAT extraction + tenant resolution for the npm adapter.
//!
//! npm clients send `Authorization: Bearer <token>`. The adapter
//! expects `Bearer hugr-pat_<token>` (CoreLink PAT). The PAT
//! plaintext is held in a [`secrecy::SecretString`] so it never
//! lands in a `Display` / log line by accident. PAT bytes are
//! compared in constant time at the resolver layer via
//! [`subtle::ConstantTimeEq`] (the resolver trait contract documents
//! this).

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;
use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};

use crate::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::error::NpmAdapterError;
use crate::ports::TenantResolverHandle;

/// Extract the PAT plaintext from a request's [`HeaderMap`].
///
/// Returns `Ok(SecretString)` on success; `Err(NpmAdapterError::Auth)`
/// if no `Authorization: Bearer` header is present, the format is
/// malformed, or the token is empty.
///
/// # Errors
///
/// Returns [`NpmAdapterError::Auth`] for every reject path.
pub fn extract_pat(headers: &HeaderMap) -> Result<SecretString, NpmAdapterError> {
    let Some(value) = headers.get(http::header::AUTHORIZATION) else {
        return Err(NpmAdapterError::Auth("missing Authorization header".into()));
    };
    let value_str = value
        .to_str()
        .map_err(|_| NpmAdapterError::Auth("non-ASCII Authorization header".into()))?;

    if let Some(token) = value_str.strip_prefix("Bearer ") {
        if token.is_empty() {
            return Err(NpmAdapterError::Auth("empty Bearer token".into()));
        }
        return Ok(SecretString::new(token.to_owned().into()));
    }

    Err(NpmAdapterError::Auth(
        "Authorization must be `Bearer <pat>`".into(),
    ))
}

/// Convenience helper: extract the PAT from `headers`, resolve to a
/// tenant via `resolver`, and on `Err` emit a
/// `corelink.npm.auth.rejected.v1` audit row before bubbling the
/// error up.
///
/// # Errors
///
/// Returns [`NpmAdapterError::Auth`] (extract or resolver failure) or
/// [`NpmAdapterError::Audit`] (audit emit failure).
pub async fn authenticate(
    headers: &HeaderMap,
    resolver: &TenantResolverHandle,
    auditor: &Arc<dyn AuditEmitter>,
) -> Result<TenantId, NpmAdapterError> {
    let pat = match extract_pat(headers) {
        Ok(p) => p,
        Err(e) => {
            let zero_tenant = TenantId::from_uuid(uuid::Uuid::nil());
            emit_npm_audit(
                auditor,
                event_types::AUTH_REJECTED,
                &zero_tenant,
                now_unix_ms(),
                serde_json::json!({ "reason": e.to_string() }),
            )?;
            return Err(e);
        }
    };
    resolver.resolve(pat.expose_secret()).await
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
    use http::header::AUTHORIZATION;

    fn h(value: &str) -> HeaderMap {
        let mut m = HeaderMap::new();
        if let Ok(v) = http::HeaderValue::from_str(value) {
            m.insert(AUTHORIZATION, v);
        }
        m
    }

    #[test]
    fn extracts_bearer_token() {
        let pat = extract_pat(&h("Bearer hugr-pat_abc123")).expect("bearer ok");
        assert_eq!(pat.expose_secret(), "hugr-pat_abc123");
    }

    #[test]
    fn rejects_missing_header() {
        let result = extract_pat(&HeaderMap::new());
        assert!(matches!(result, Err(NpmAdapterError::Auth(_))));
    }

    #[test]
    fn rejects_empty_bearer() {
        let result = extract_pat(&h("Bearer "));
        assert!(matches!(result, Err(NpmAdapterError::Auth(_))));
    }

    #[test]
    fn rejects_unknown_scheme() {
        let result = extract_pat(&h("Basic foobar"));
        assert!(matches!(result, Err(NpmAdapterError::Auth(_))));
    }
}

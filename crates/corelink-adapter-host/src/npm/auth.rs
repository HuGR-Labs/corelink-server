//! PAT extraction + tenant resolution for the npm adapter.
//!
//! npm clients send `Authorization: Bearer <token>`. The adapter
//! expects `Bearer corelink_<token>` (the canonical CoreLink PAT
//! format). The [`PAT_PREFIX`] check is a fast-path reject — done in
//! constant time via [`subtle::ConstantTimeEq`] — that runs BEFORE the
//! resolver lookup, so a malformed/foreign token never reaches the
//! per-tenant index. The PAT plaintext is held in a
//! [`secrecy::SecretString`] so it never lands in a `Display` / log
//! line by accident. PAT bytes are compared in constant time at the
//! resolver layer too (the resolver trait contract documents this).

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;
use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;

use crate::npm::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::npm::error::NpmAdapterError;
use crate::npm::ports::TenantResolverHandle;

/// Canonical CoreLink PAT plaintext prefix.
pub const PAT_PREFIX: &str = "corelink_";

/// Compile-time pin: the production wire prefix is `corelink_` (9 bytes,
/// = `corelink_pat::format::PAT_PREFIX_LEN`). Kept a string literal so the
/// adapter crate stays decoupled from `corelink_pat` per the ports charter;
/// the authoritative parse + crypto verify happens in the container resolver.
const _: () = assert!(PAT_PREFIX.len() == 9);

/// Extract the PAT plaintext from a request's [`HeaderMap`].
///
/// Returns `Ok(SecretString)` on success; `Err(NpmAdapterError::Auth)`
/// if no `Authorization: Bearer` header is present, the format is
/// malformed, the token is empty, or the token does not start with
/// [`PAT_PREFIX`].
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
        if !bearer_eq(token, PAT_PREFIX) {
            return Err(NpmAdapterError::Auth("PAT prefix mismatch".into()));
        }
        return Ok(SecretString::new(token.to_owned().into()));
    }

    Err(NpmAdapterError::Auth(
        "Authorization must be `Bearer <pat>`".into(),
    ))
}

/// Constant-time prefix check: does `token` start with `expected_prefix`?
///
/// Iterates over the prefix bytes only (length is public input). The
/// constant-time comparison hides the per-byte mismatch position so an
/// attacker timing the response can't binary-search the prefix. The prefix
/// is itself fixed (`corelink_`), so the loop iteration count is
/// data-independent.
#[must_use]
fn bearer_eq(token: &str, expected_prefix: &str) -> bool {
    let token_bytes = token.as_bytes();
    let expected = expected_prefix.as_bytes();
    if token_bytes.len() < expected.len() {
        return false;
    }
    // Bounded by the length check above; route via `get` to stay clear of
    // the `indexing_slicing` lint.
    let head = match token_bytes.get(..expected.len()) {
        Some(slice) => slice,
        None => return false,
    };
    head.ct_eq(expected).into()
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
        let pat = extract_pat(&h("Bearer corelink_abc123")).expect("bearer ok");
        assert_eq!(pat.expose_secret(), "corelink_abc123");
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

    #[test]
    fn rejects_wrong_prefix() {
        // A non-`corelink_` token (e.g. a GitHub PAT or the old placeholder
        // prefix) is rejected at extraction, before the resolver lookup.
        assert!(matches!(
            extract_pat(&h("Bearer ghp_github")),
            Err(NpmAdapterError::Auth(_))
        ));
        assert!(matches!(
            extract_pat(&h("Bearer hugr-pat_legacy")),
            Err(NpmAdapterError::Auth(_))
        ));
    }

    #[test]
    fn bearer_eq_matches_and_rejects() {
        assert!(bearer_eq("corelink_deadbeef", PAT_PREFIX));
        assert!(!bearer_eq("hugr", PAT_PREFIX)); // shorter than prefix
        assert!(!bearer_eq("ghp_deadbeef", PAT_PREFIX));
        assert!(!bearer_eq("hugr-pat_x", PAT_PREFIX)); // legacy placeholder rejected
    }
}

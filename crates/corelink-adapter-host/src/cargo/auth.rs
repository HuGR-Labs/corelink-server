//! PAT-based authentication shim.
//!
//! The cargo adapter accepts the bearer token via the standard
//! `Authorization: Bearer <pat>` header. sccache's HTTP backend
//! supports a configurable HTTP header; we align with the CoreLink
//! PAT format (`hugr-pat_<token>`) for consistency with sibling adapters.
//!
//! Tokens MUST start with the canonical `hugr-pat_` prefix; the prefix
//! check is a fast-path reject that runs BEFORE the constant-time
//! resolver lookup so malformed input never reaches the per-tenant index.
//!
//! ## Constant-time compare
//!
//! `bearer_eq` does the constant-time prefix verify (`subtle::
//! ConstantTimeEq`). The downstream [`crate::cargo::ports::TenantResolver`]
//! implementor is contracted to perform the *value* compare in
//! constant time as well; the trait can't enforce that at the type
//! level, so see the production resolver crate for the actual hash
//! lookup constant-time guarantee.

use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;

use crate::cargo::error::CargoAdapterError;
use crate::cargo::ports::{SharedTenantResolver, TenantResolveError};

/// Canonical CoreLink PAT plaintext prefix.
pub const PAT_PREFIX: &str = "hugr-pat_";

/// Extract the bearer PAT plaintext from `headers`. Returns
/// [`CargoAdapterError::Auth`] when:
///
/// - the `Authorization` header is missing;
/// - the value is not UTF-8;
/// - the scheme is not `Bearer`;
/// - the token does not start with [`PAT_PREFIX`].
///
/// On success the plaintext is returned wrapped in a `SecretString` so
/// it can't accidentally land in a `Debug` / `Display` site.
pub fn extract_bearer(headers: &HeaderMap) -> Result<SecretString, CargoAdapterError> {
    let header = headers
        .get(http::header::AUTHORIZATION)
        .ok_or_else(|| CargoAdapterError::Auth("missing Authorization header".to_owned()))?;

    let header_str = header
        .to_str()
        .map_err(|_| CargoAdapterError::Auth("Authorization header is not ASCII".to_owned()))?;

    let token = header_str
        .strip_prefix("Bearer ")
        .ok_or_else(|| CargoAdapterError::Auth("Authorization scheme is not Bearer".to_owned()))?;

    if !bearer_eq(token, PAT_PREFIX) {
        return Err(CargoAdapterError::Auth("PAT prefix mismatch".to_owned()));
    }

    Ok(SecretString::from(token.to_owned()))
}

/// Constant-time prefix check: does `token` start with `expected_prefix`?
///
/// Iterates over the prefix bytes only (length is public input). The
/// constant-time comparison hides the per-byte mismatch position so an
/// attacker timing the response can't binary-search the prefix.
#[must_use]
pub fn bearer_eq(token: &str, expected_prefix: &str) -> bool {
    let token_bytes = token.as_bytes();
    let expected = expected_prefix.as_bytes();
    if token_bytes.len() < expected.len() {
        return false;
    }
    let head = match token_bytes.get(..expected.len()) {
        Some(slice) => slice,
        None => return false,
    };
    head.ct_eq(expected).into()
}

/// Resolve a PAT to its owning tenant id via the configured resolver.
/// Maps [`TenantResolveError`] onto [`CargoAdapterError::Auth`] (401)
/// for `InvalidPat` and onto [`CargoAdapterError::Auth`] with a
/// `backend:` prefix for transient resolver failures.
pub async fn resolve_tenant(
    resolver: &SharedTenantResolver,
    pat: &SecretString,
) -> Result<String, CargoAdapterError> {
    match resolver.resolve(pat.expose_secret()).await {
        Ok(tenant_id) => Ok(tenant_id),
        Err(TenantResolveError::InvalidPat) => {
            Err(CargoAdapterError::Auth("invalid PAT".to_owned()))
        }
        Err(TenantResolveError::Backend(msg)) => {
            Err(CargoAdapterError::Auth(format!("backend: {msg}")))
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    #[test]
    fn bearer_eq_matches_prefix() {
        assert!(bearer_eq("hugr-pat_deadbeef", PAT_PREFIX));
    }

    #[test]
    fn bearer_eq_rejects_short_token() {
        assert!(!bearer_eq("hugr", PAT_PREFIX));
    }

    #[test]
    fn bearer_eq_rejects_wrong_prefix() {
        assert!(!bearer_eq("ghp_deadbeef_token", PAT_PREFIX));
    }

    #[test]
    fn extract_bearer_missing_header() {
        let headers = HeaderMap::new();
        let err = match extract_bearer(&headers) {
            Err(e) => e,
            Ok(_) => panic!("must reject missing header"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("missing"), "msg = {msg}");
    }

    fn header_value(s: &str) -> http::HeaderValue {
        http::HeaderValue::from_str(s).unwrap()
    }

    #[test]
    fn extract_bearer_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            header_value("Basic dXNlcjpwYXNz"),
        );
        let err = match extract_bearer(&headers) {
            Err(e) => e,
            Ok(_) => panic!("must reject Basic scheme"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("Bearer"), "msg = {msg}");
    }

    #[test]
    fn extract_bearer_wrong_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            header_value("Bearer ghp_unexpected"),
        );
        let err = match extract_bearer(&headers) {
            Err(e) => e,
            Ok(_) => panic!("must reject non-PAT prefix"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("prefix"), "msg = {msg}");
    }

    #[test]
    fn extract_bearer_accepts_valid_pat() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            header_value("Bearer hugr-pat_abc123"),
        );
        let pat = match extract_bearer(&headers) {
            Ok(p) => p,
            Err(e) => panic!("must accept canonical PAT: {e}"),
        };
        assert_eq!(pat.expose_secret(), "hugr-pat_abc123");
    }
}

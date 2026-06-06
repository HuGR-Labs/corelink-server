//! PAT extraction + tenant resolution for the pip adapter.
//!
//! `pip install` carries the PAT either as
//! `Authorization: Bearer <pat>` (preferred — what `pip` emits when
//! the URL embeds `https://hugr:<pat>@host/simple/`) or as HTTP basic
//! auth where the username is `hugr` and the password is the PAT.
//! Both are accepted.
//!
//! The PAT plaintext is held in a [`secrecy::SecretString`] so it
//! never lands in a `Display` / log line by accident. PAT bytes are
//! compared in constant time at the resolver layer via
//! [`subtle::ConstantTimeEq`] (the resolver trait contract documents
//! this).

use std::sync::Arc;

use corelink_audit::ports::AuditEmitter;
use corelink_core::types::tenant::TenantId;
use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;

use crate::pip::audit::{emit_pip_audit, event_types, now_unix_ms};
use crate::pip::error::PipAdapterError;
use crate::pip::ports::TenantResolverHandle;

/// Username the adapter accepts for HTTP basic auth (the password is
/// the PAT). `pip` synthesises basic auth from
/// `--index-url http://hugr:<pat>@host/simple/`.
pub const BASIC_AUTH_USERNAME: &str = "hugr";

/// Extract the PAT plaintext from a request's [`HeaderMap`] without
/// allocating until the very last step.
///
/// Returns `Ok(SecretString)` on success; `Err(PipAdapterError::Auth)`
/// if neither header is present, the format is malformed, or the
/// basic-auth username is not [`BASIC_AUTH_USERNAME`].
///
/// # Errors
///
/// Returns [`PipAdapterError::Auth`] for every reject path.
pub fn extract_pat(headers: &HeaderMap) -> Result<SecretString, PipAdapterError> {
    let Some(value) = headers.get(http::header::AUTHORIZATION) else {
        return Err(PipAdapterError::Auth("missing Authorization header".into()));
    };
    let value_str = value
        .to_str()
        .map_err(|_| PipAdapterError::Auth("non-ASCII Authorization header".into()))?;

    if let Some(token) = value_str.strip_prefix("Bearer ") {
        if token.is_empty() {
            return Err(PipAdapterError::Auth("empty Bearer token".into()));
        }
        return Ok(SecretString::new(token.to_owned().into()));
    }

    if let Some(b64) = value_str.strip_prefix("Basic ") {
        return decode_basic(b64);
    }

    Err(PipAdapterError::Auth(
        "Authorization must be `Bearer <pat>` or `Basic <base64>`".into(),
    ))
}

fn decode_basic(b64: &str) -> Result<SecretString, PipAdapterError> {
    let raw = decode_base64_padded(b64)
        .map_err(|e| PipAdapterError::Auth(format!("malformed basic-auth base64: {e}")))?;
    let decoded = std::str::from_utf8(&raw)
        .map_err(|_| PipAdapterError::Auth("non-UTF8 basic-auth payload".into()))?;
    let (user, pass) = decoded
        .split_once(':')
        .ok_or_else(|| PipAdapterError::Auth("basic-auth missing `:` separator".into()))?;

    // Constant-time username compare — defends against side-channel
    // username probing.
    let user_bytes = user.as_bytes();
    let expected_bytes = BASIC_AUTH_USERNAME.as_bytes();
    if user_bytes.ct_eq(expected_bytes).unwrap_u8() != 1 {
        return Err(PipAdapterError::Auth(format!(
            "basic-auth username must be `{BASIC_AUTH_USERNAME}`"
        )));
    }
    if pass.is_empty() {
        return Err(PipAdapterError::Auth("empty basic-auth password".into()));
    }
    Ok(SecretString::new(pass.to_owned().into()))
}

/// Minimal `=`-padded standard-base64 decoder. The adapter avoids
/// pulling the full `base64` crate for the auth-header path because
/// the only base64 input it ever sees is the basic-auth payload
/// (`username:password`); a single-purpose decoder keeps the
/// dependency graph slim and the failure modes explicit.
///
/// # Errors
///
/// Returns a `String` description on any malformed byte.
fn decode_base64_padded(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("invalid base64 char {:?}", c as char)),
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(format!("base64 length {} not multiple of 4", bytes.len()));
    }
    let pad = bytes.iter().rev().take_while(|&&b| b == b'=').count();
    if pad > 2 {
        return Err(format!("base64 has {pad} `=` (max 2)"));
    }
    let core_len = bytes.len().saturating_sub(pad);
    let mut out = Vec::with_capacity(core_len * 3 / 4);
    let mut chunks = bytes.chunks_exact(4);
    let mut remaining = core_len;
    for chunk in chunks.by_ref() {
        if remaining < 4 {
            break;
        }
        let c0 = val(*chunk.first().ok_or("chunk[0] missing")?)?;
        let c1 = val(*chunk.get(1).ok_or("chunk[1] missing")?)?;
        let c2 = val(*chunk.get(2).ok_or("chunk[2] missing")?)?;
        let c3 = val(*chunk.get(3).ok_or("chunk[3] missing")?)?;
        out.push((c0 << 2) | (c1 >> 4));
        out.push((c1 << 4) | (c2 >> 2));
        out.push((c2 << 6) | c3);
        remaining -= 4;
    }
    if remaining > 0 {
        // The trailing partial chunk (with `=` padding stripped).
        let start = core_len.saturating_sub(remaining);
        let tail = bytes.get(start..core_len).ok_or("tail slice oob")?;
        let mut decoded = [0u8; 4];
        for (i, b) in tail.iter().enumerate() {
            if let Some(slot) = decoded.get_mut(i) {
                *slot = val(*b)?;
            }
        }
        if remaining >= 2 {
            out.push((decoded[0] << 2) | (decoded[1] >> 4));
        }
        if remaining >= 3 {
            out.push((decoded[1] << 4) | (decoded[2] >> 2));
        }
    }
    Ok(out)
}

/// Convenience helper: extract the PAT from `headers`, resolve to a
/// tenant via `resolver`, and on `Err` emit a
/// `corelink.pip.auth.rejected.v1` audit row before bubbling the
/// error up.
///
/// # Errors
///
/// Returns [`PipAdapterError::Auth`] (extract or resolver failure) or
/// [`PipAdapterError::Audit`] (audit emit failure).
pub async fn authenticate(
    headers: &HeaderMap,
    resolver: &TenantResolverHandle,
    auditor: &Arc<dyn AuditEmitter>,
) -> Result<TenantId, PipAdapterError> {
    let pat = match extract_pat(headers) {
        Ok(p) => p,
        Err(e) => {
            // Audit-emit the rejection. Use a zero-uuid tenant id
            // because we have no real tenant context yet.
            let zero_tenant = TenantId::from_uuid(uuid::Uuid::nil());
            emit_pip_audit(
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
        assert!(matches!(result, Err(PipAdapterError::Auth(_))));
    }

    #[test]
    fn rejects_empty_bearer() {
        let result = extract_pat(&h("Bearer "));
        assert!(matches!(result, Err(PipAdapterError::Auth(_))));
    }

    #[test]
    fn rejects_unknown_scheme() {
        let result = extract_pat(&h("Digest foobar"));
        assert!(matches!(result, Err(PipAdapterError::Auth(_))));
    }

    #[test]
    fn decodes_basic_with_correct_username() {
        // base64("hugr:hugr-pat_xyz") = "aHVncjpodWdyLXBhdF94eXo="
        let pat = extract_pat(&h("Basic aHVncjpodWdyLXBhdF94eXo=")).expect("basic ok");
        assert_eq!(pat.expose_secret(), "hugr-pat_xyz");
    }

    #[test]
    fn rejects_basic_wrong_username() {
        // base64("alice:hugr-pat_xyz") = "YWxpY2U6aHVnci1wYXRfeHl6"
        let result = extract_pat(&h("Basic YWxpY2U6aHVnci1wYXRfeHl6"));
        assert!(matches!(result, Err(PipAdapterError::Auth(_))));
    }

    #[test]
    fn rejects_basic_malformed_b64() {
        let result = extract_pat(&h("Basic !!!"));
        assert!(matches!(result, Err(PipAdapterError::Auth(_))));
    }

    #[test]
    fn base64_decoder_roundtrips() {
        // base64("hugr:") = "aHVncjo="
        let raw = decode_base64_padded("aHVncjo=").expect("ok");
        assert_eq!(raw, b"hugr:");
    }
}

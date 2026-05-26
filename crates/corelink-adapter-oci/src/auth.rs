//! Bearer-realm token mint + verify.
//!
//! OCI Distribution Spec v1.1 uses a two-leg authn flow:
//!
//! 1. Client `GET /v2/` → adapter returns
//!    `401 Unauthorized` + `Www-Authenticate: Bearer realm="<url>",service="<svc>",scope="<scope>"`.
//! 2. Client `GET <realm>?service=...&scope=...` with
//!    `Authorization: Basic <base64(user:pat)>` → adapter validates
//!    the PAT (via [`crate::ports::TenantResolver`]), mints an
//!    HMAC-signed opaque bearer token carrying `(tenant, scope,
//!    expiry)`, returns `{"token": "...", "expires_in": 3600}`.
//! 3. Client retries the original op with `Authorization: Bearer <token>`.
//!
//! ## Token shape
//!
//! ```text
//! corelink-oci.<tenant-uuid>.<scope-b64>.<expiry-unix-secs>.<hmac-b64>
//! ```
//!
//! - `tenant-uuid` is the canonical UUIDv7 text form (hyphenated lowercase).
//! - `scope-b64` is URL-safe base64 (no pad) of the raw scope string,
//!   so embedded `:` chars don't collide with the field separator.
//! - `expiry-unix-secs` is `now + token_ttl_secs` at mint time.
//! - `hmac-b64` is HMAC-SHA256 (URL-safe base64, no pad) over the
//!   exact preimage `<tenant>.<scope-b64>.<expiry>`, keyed with
//!   [`crate::config::OciAdapterConfig::token_signing_key`].
//!
//! Verify is `subtle::ConstantTimeEq` on the recomputed HMAC. Wrong
//! key, expired, or malformed → [`OciAdapterError::InvalidToken`].

use base64::Engine as _;
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret as _;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use corelink_core::{SecretWrap, TenantId};

use crate::error::OciAdapterError;

const TOKEN_PREFIX: &str = "corelink-oci.";

/// Parsed scope grant carried inside a bearer token.
///
/// Currently only repository-level scopes are minted; future versions
/// MAY extend to admin-plane scopes (`registry:catalog:*` etc.) once
/// `_catalog` per-tenant scoping ships.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OciScope {
    /// Repository name, e.g. `"corelink-test/hello"`.
    pub repo: String,
    /// Actions granted: any of `"pull"`, `"push"`, `"delete"`.
    pub actions: Vec<String>,
}

impl OciScope {
    /// Construct an [`OciScope`] from `(repo, actions)`.
    ///
    /// Needed because the struct is `#[non_exhaustive]` and therefore
    /// cannot be built with struct-literal syntax from outside this
    /// crate (e.g. by the integration-test rig in `tests/common.rs`).
    #[must_use]
    pub fn new(repo: impl Into<String>, actions: Vec<String>) -> Self {
        Self {
            repo: repo.into(),
            actions,
        }
    }

    /// Build a scope from a wire string like
    /// `repository:corelink-test/hello:pull,push`.
    ///
    /// # Errors
    ///
    /// Returns [`OciAdapterError::Auth`] on malformed scope strings.
    pub fn parse(wire: &str) -> Result<Self, OciAdapterError> {
        let mut parts = wire.splitn(3, ':');
        let resource = parts
            .next()
            .ok_or_else(|| OciAdapterError::Auth(String::from("empty scope")))?;
        if resource != "repository" {
            return Err(OciAdapterError::Auth(format!(
                "unsupported scope resource: {resource}"
            )));
        }
        let repo = parts
            .next()
            .ok_or_else(|| OciAdapterError::Auth(String::from("missing repo in scope")))?;
        let actions_str = parts
            .next()
            .ok_or_else(|| OciAdapterError::Auth(String::from("missing actions in scope")))?;
        let actions = actions_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if actions.is_empty() {
            return Err(OciAdapterError::Auth(String::from("empty actions in scope")));
        }
        Ok(Self {
            repo: repo.to_string(),
            actions,
        })
    }

    /// Render scope back to wire string.
    #[must_use]
    pub fn to_wire(&self) -> String {
        format!("repository:{}:{}", self.repo, self.actions.join(","))
    }

    /// Does this scope grant `action` on `repo`?
    #[must_use]
    pub fn allows(&self, repo: &str, action: &str) -> bool {
        self.repo == repo && self.actions.iter().any(|a| a == action)
    }
}

/// Token verification produces this — the call site can then check
/// the `scope` against the operation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VerifiedToken {
    /// Tenant the token was minted for.
    pub tenant: TenantId,
    /// Scope grant.
    pub scope: OciScope,
    /// Unix-seconds expiry.
    pub expiry: u64,
}

fn b64_url_no_pad(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn b64_url_no_pad_decode(s: &str) -> Result<Vec<u8>, OciAdapterError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| OciAdapterError::InvalidToken)
}

fn hmac_preimage(tenant_text: &str, scope_b64: &str, expiry: u64) -> String {
    format!("{tenant_text}.{scope_b64}.{expiry}")
}

fn hmac_sign(key: &SecretWrap, preimage: &str) -> Result<Vec<u8>, OciAdapterError> {
    let key_str = key.as_secret_string().expose_secret();
    let mut mac = <Hmac<Sha256>>::new_from_slice(key_str.as_bytes())
        .map_err(|e| OciAdapterError::Auth(format!("hmac key: {e}")))?;
    mac.update(preimage.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Mint a bearer token granting `scope` to `tenant` for the next
/// `ttl_secs` seconds.
///
/// `now_unix_secs` is a parameter so the call site can pin a clock
/// (used by tests + the production wiring which reads
/// `corelink_core::Clock`).
///
/// # Errors
///
/// Returns [`OciAdapterError::Auth`] only on internal HMAC misuse
/// (which shouldn't happen with a valid `SecretWrap` of ≥32 bytes —
/// enforced at config sanity-check time).
pub fn mint(
    key: &SecretWrap,
    tenant: &TenantId,
    scope: &OciScope,
    now_unix_secs: u64,
    ttl_secs: u64,
) -> Result<String, OciAdapterError> {
    let tenant_text = tenant.to_canonical_text();
    let scope_b64 = b64_url_no_pad(scope.to_wire().as_bytes());
    let expiry = now_unix_secs.saturating_add(ttl_secs);
    let preimage = hmac_preimage(&tenant_text, &scope_b64, expiry);
    let sig = hmac_sign(key, &preimage)?;
    let sig_b64 = b64_url_no_pad(&sig);
    Ok(format!(
        "{TOKEN_PREFIX}{tenant_text}.{scope_b64}.{expiry}.{sig_b64}"
    ))
}

/// Verify a bearer token. Returns the parsed [`VerifiedToken`] on
/// success.
///
/// # Errors
///
/// - [`OciAdapterError::InvalidToken`] on any of: missing prefix,
///   wrong field count, malformed expiry, expired, bad b64 encoding,
///   HMAC mismatch.
pub fn verify(
    key: &SecretWrap,
    token: &str,
    now_unix_secs: u64,
) -> Result<VerifiedToken, OciAdapterError> {
    let stripped = token
        .strip_prefix(TOKEN_PREFIX)
        .ok_or(OciAdapterError::InvalidToken)?;
    let parts: Vec<&str> = stripped.split('.').collect();
    let [tenant_text, scope_b64, expiry_text, sig_b64] = parts.as_slice() else {
        return Err(OciAdapterError::InvalidToken);
    };
    // Recompute HMAC. Constant-time compare on the raw sig bytes.
    let expiry: u64 = expiry_text
        .parse()
        .map_err(|_| OciAdapterError::InvalidToken)?;
    let preimage = hmac_preimage(tenant_text, scope_b64, expiry);
    let recomputed = hmac_sign(key, &preimage)?;
    let provided = b64_url_no_pad_decode(sig_b64)?;
    if recomputed.len() != provided.len() {
        return Err(OciAdapterError::InvalidToken);
    }
    let ok: bool = recomputed.ct_eq(&provided).into();
    if !ok {
        return Err(OciAdapterError::InvalidToken);
    }
    // HMAC ok — now (and only now) check expiry. Order matters: we
    // do NOT want a forged token to learn it would have been expired
    // anyway via timing diff.
    if now_unix_secs >= expiry {
        return Err(OciAdapterError::InvalidToken);
    }
    let tenant_uuid =
        uuid::Uuid::parse_str(tenant_text).map_err(|_| OciAdapterError::InvalidToken)?;
    let scope_raw = b64_url_no_pad_decode(scope_b64)?;
    let scope_str =
        std::str::from_utf8(&scope_raw).map_err(|_| OciAdapterError::InvalidToken)?;
    let scope = OciScope::parse(scope_str).map_err(|_| OciAdapterError::InvalidToken)?;
    Ok(VerifiedToken {
        tenant: TenantId::from_uuid(tenant_uuid),
        scope,
        expiry,
    })
}

/// Parse an `Authorization: Basic <base64(user:pat)>` header value
/// and return the embedded PAT wrapped in [`SecretWrap`].
///
/// User portion is ignored (OCI clients send `hugr` by convention but
/// the adapter does not enforce a username — only the PAT matters).
///
/// # Errors
///
/// [`OciAdapterError::Auth`] if header doesn't start with `Basic `,
/// is not valid base64, or doesn't contain a `:` separator.
pub fn parse_basic_authorization(header: &str) -> Result<SecretWrap, OciAdapterError> {
    let b64 = header
        .strip_prefix("Basic ")
        .ok_or_else(|| OciAdapterError::Auth(String::from("not basic")))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| OciAdapterError::Auth(format!("basic b64: {e}")))?;
    let s = std::str::from_utf8(&decoded)
        .map_err(|_| OciAdapterError::Auth(String::from("basic utf8")))?;
    let (_user, pat) = s
        .split_once(':')
        .ok_or_else(|| OciAdapterError::Auth(String::from("basic no colon")))?;
    Ok(SecretWrap::new(pat.to_string()))
}

/// Build the `Www-Authenticate: Bearer realm="...",service="...",scope="..."`
/// header value the adapter advertises on a 401.
#[must_use]
pub fn www_authenticate_header(realm: &str, service: &str, scope: &str) -> String {
    format!(r#"Bearer realm="{realm}",service="{service}",scope="{scope}""#)
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

    fn key() -> SecretWrap {
        SecretWrap::new("a".repeat(32))
    }

    fn tenant() -> TenantId {
        TenantId::from_uuid(uuid::Uuid::nil())
    }

    fn scope() -> OciScope {
        OciScope {
            repo: String::from("alpine"),
            actions: vec![String::from("pull"), String::from("push")],
        }
    }

    #[test]
    fn scope_parse_roundtrip() {
        let s = OciScope::parse("repository:foo/bar:pull,push").expect("parse");
        assert_eq!(s.repo, "foo/bar");
        assert_eq!(s.actions, vec!["pull", "push"]);
        assert_eq!(s.to_wire(), "repository:foo/bar:pull,push");
    }

    #[test]
    fn scope_allows_only_listed_actions() {
        let s = scope();
        assert!(s.allows("alpine", "pull"));
        assert!(s.allows("alpine", "push"));
        assert!(!s.allows("alpine", "delete"));
        assert!(!s.allows("nginx", "pull"));
    }

    #[test]
    fn mint_verify_roundtrip() {
        let token = mint(&key(), &tenant(), &scope(), 1000, 3600).expect("mint");
        let v = verify(&key(), &token, 1500).expect("verify");
        assert_eq!(v.tenant, tenant());
        assert_eq!(v.scope.repo, "alpine");
    }

    #[test]
    fn expired_token_rejected() {
        let token = mint(&key(), &tenant(), &scope(), 1000, 60).expect("mint");
        // now = 2000, expiry = 1060.
        let err = verify(&key(), &token, 2000).expect_err("must reject");
        assert!(matches!(err, OciAdapterError::InvalidToken));
    }

    #[test]
    fn wrong_key_rejected() {
        let token = mint(&key(), &tenant(), &scope(), 1000, 3600).expect("mint");
        let other = SecretWrap::new("b".repeat(32));
        let err = verify(&other, &token, 1500).expect_err("must reject");
        assert!(matches!(err, OciAdapterError::InvalidToken));
    }

    #[test]
    fn tampered_token_rejected() {
        let token = mint(&key(), &tenant(), &scope(), 1000, 3600).expect("mint");
        // Flip last char of sig by re-assembling the string. Avoids
        // any `unsafe` per `#![forbid(unsafe_code)]`.
        let mut tampered = String::with_capacity(token.len());
        let mut iter = token.chars().peekable();
        while let Some(c) = iter.next() {
            if iter.peek().is_none() {
                tampered.push(if c == 'a' { 'b' } else { 'a' });
            } else {
                tampered.push(c);
            }
        }
        let err = verify(&key(), &tampered, 1500).expect_err("must reject");
        assert!(matches!(err, OciAdapterError::InvalidToken));
    }

    #[test]
    fn parse_basic_authorization_ok() {
        // base64("hugr:my-pat-secret") = "aHVnci1wYXQtdGVzdA==" no — recompute.
        use base64::Engine as _;
        let enc = base64::engine::general_purpose::STANDARD.encode("hugr:my-pat-secret");
        let header = format!("Basic {enc}");
        let pat = parse_basic_authorization(&header).expect("parse");
        assert_eq!(pat.as_secret_string().expose_secret(), "my-pat-secret");
    }

    #[test]
    fn www_authenticate_format() {
        let h = www_authenticate_header(
            "http://localhost:5000/token",
            "corelink-oci",
            "repository:alpine:pull",
        );
        assert!(h.starts_with("Bearer realm=\""));
        assert!(h.contains("service=\"corelink-oci\""));
        assert!(h.contains("scope=\"repository:alpine:pull\""));
    }
}

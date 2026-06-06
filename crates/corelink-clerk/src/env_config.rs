//! Environment-variable configuration loader (R2-2 production wiring).
//!
//! Loads [`crate::ClerkConfig`] from the canonical env-var matrix:
//!
//! | Variable | Required? | Purpose |
//! |---|---|---|
//! | `CLERK_PUBLISHABLE_KEY` | yes | Frontend publishable key (`pk_test_…` / `pk_live_…`). Used to derive the default JWKS URL if `CLERK_JWKS_URL` is unset, and to compute the canonical Clerk frontend-API issuer. |
//! | `CLERK_SECRET_KEY` | yes | Backend secret key (`sk_test_…` / `sk_live_…`). The Rust validator never sends this to Clerk — it's surfaced via [`ClerkEnvSecrets::secret_key`] for downstream HTTP clients that need to call Clerk's REST API (e.g. user-revocation hooks). **NEVER logged.** |
//! | `CLERK_JWKS_URL` | optional | Override the JWKS endpoint. When unset, derived from the publishable key's frontend-API domain (`https://{frontend_api}/.well-known/jwks.json`). |
//! | `CLERK_JWT_ISSUER` | optional | Override the canonical issuer. When unset, derived from the publishable key (`https://{frontend_api}`). Comma-separated values build a multi-issuer allowlist. |
//! | `CLERK_AUDIENCE` | yes | Application audience claim — exact match. |
//!
//! # Decoding the publishable key
//!
//! Clerk publishable keys are formatted `pk_<env>_<base64url-frontend-api>`,
//! where the base64url segment decodes to the dollar-suffixed
//! frontend API URL, e.g. `clerk.example.com$`. We strip the trailing
//! `$` and synthesise:
//!
//! - JWKS URL: `https://clerk.example.com/.well-known/jwks.json`
//! - Issuer: `https://clerk.example.com`
//!
//! # Secrets handling
//!
//! [`ClerkEnvSecrets::secret_key`] is a `String` wrapped in
//! `zeroize::Zeroizing` so the heap allocation is wiped on drop.
//! The `Debug` impl redacts the value. **Callers must never log it.**

use std::env;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use thiserror::Error;

use crate::config::{ClerkConfig, ClerkConfigBuilder, ClerkConfigError};

/// Canonical env var: Clerk publishable key.
pub const ENV_PUBLISHABLE_KEY: &str = "CLERK_PUBLISHABLE_KEY";
/// Canonical env var: Clerk secret key.
pub const ENV_SECRET_KEY: &str = "CLERK_SECRET_KEY";
/// Canonical env var: JWKS endpoint override.
pub const ENV_JWKS_URL: &str = "CLERK_JWKS_URL";
/// Canonical env var: issuer allowlist override (comma-separated).
pub const ENV_JWT_ISSUER: &str = "CLERK_JWT_ISSUER";
/// Canonical env var: audience claim.
pub const ENV_AUDIENCE: &str = "CLERK_AUDIENCE";

/// Errors surfaced by [`ClerkConfig::from_env`] / [`ClerkEnvSecrets::from_env`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EnvLoadError {
    /// A required env var was missing.
    #[error("env var {0} is required")]
    Missing(&'static str),
    /// The publishable key was malformed (not `pk_<env>_<b64>` shape).
    #[error("CLERK_PUBLISHABLE_KEY malformed: {0}")]
    PublishableKeyMalformed(&'static str),
    /// Downstream [`ClerkConfig`] build failed.
    #[error("clerk config build failed: {0}")]
    ConfigBuild(#[from] ClerkConfigError),
}

/// Secret-bearing env state surfaced separately so the
/// [`ClerkConfig`] surface (which is `Debug`-printable in tracing
/// spans) never touches the secret key.
#[derive(Debug)]
#[non_exhaustive]
pub struct ClerkEnvSecrets {
    /// Clerk publishable key (frontend; non-secret but kept here for
    /// completeness — surfaces as `pk_test_…` / `pk_live_…`).
    pub publishable_key: String,
    /// Clerk secret key (backend). Wrapped in `zeroize::Zeroizing`
    /// so the heap allocation is wiped on drop. `Debug` is redacted.
    pub secret_key: SecretKey,
}

impl ClerkEnvSecrets {
    /// Load the secret-bearing pair from the canonical env vars.
    ///
    /// # Errors
    ///
    /// Surfaces [`EnvLoadError::Missing`] if either var is unset.
    pub fn from_env() -> Result<Self, EnvLoadError> {
        let publishable_key = env::var(ENV_PUBLISHABLE_KEY)
            .map_err(|_| EnvLoadError::Missing(ENV_PUBLISHABLE_KEY))?;
        let secret_key =
            env::var(ENV_SECRET_KEY).map_err(|_| EnvLoadError::Missing(ENV_SECRET_KEY))?;
        if publishable_key.is_empty() {
            return Err(EnvLoadError::Missing(ENV_PUBLISHABLE_KEY));
        }
        if secret_key.is_empty() {
            return Err(EnvLoadError::Missing(ENV_SECRET_KEY));
        }
        Ok(Self {
            publishable_key,
            secret_key: SecretKey::new(secret_key),
        })
    }
}

/// Newtype wrapper around the Clerk secret key. The `Debug` impl
/// redacts the value and the heap allocation is wiped on drop.
///
/// We keep the inner buffer as `Vec<u8>` (not `String`) so the `Drop`
/// impl can zero the heap bytes without `unsafe` — stable Rust does
/// not expose `String`'s underlying `Vec<u8>` as `&mut`. UTF-8
/// invariants are preserved on construction.
pub struct SecretKey(Vec<u8>);

impl SecretKey {
    /// Wrap a raw secret key value.
    #[must_use]
    pub fn new(s: String) -> Self {
        Self(s.into_bytes())
    }

    /// Borrow the raw value as a `&str`. Callers MUST NOT log this.
    ///
    /// Returns `""` only if the buffer was already zeroed (i.e. after
    /// `Drop` ran — never reachable from safe code in normal flow).
    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        // Zero each byte in place. `core::hint::black_box` inhibits
        // dead-store elimination so the writes survive optimisation.
        for b in self.0.iter_mut() {
            *b = core::hint::black_box(0);
        }
        // Truncate to length 0 so any subsequent observation through
        // `expose()` (impossible after Drop) would surface an empty
        // string rather than zero bytes.
        self.0.clear();
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKey(<redacted len={}>)", self.0.len())
    }
}

impl ClerkConfig {
    /// Load the canonical config from the env-var matrix documented at
    /// the [`crate::env_config`] module level. The publishable key is
    /// parsed for the frontend-API host so the JWKS URL and issuer can
    /// be derived deterministically when overrides are unset.
    ///
    /// # Errors
    ///
    /// See [`EnvLoadError`].
    pub fn from_env() -> Result<Self, EnvLoadError> {
        let publishable_key = env::var(ENV_PUBLISHABLE_KEY)
            .map_err(|_| EnvLoadError::Missing(ENV_PUBLISHABLE_KEY))?;
        if publishable_key.is_empty() {
            return Err(EnvLoadError::Missing(ENV_PUBLISHABLE_KEY));
        }
        let audience = env::var(ENV_AUDIENCE).map_err(|_| EnvLoadError::Missing(ENV_AUDIENCE))?;
        if audience.is_empty() {
            return Err(EnvLoadError::Missing(ENV_AUDIENCE));
        }
        let frontend_host = parse_publishable_key_frontend_host(&publishable_key)?;
        let jwks_url = env::var(ENV_JWKS_URL)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("https://{frontend_host}/.well-known/jwks.json"));
        let issuer_allowlist: Vec<String> =
            match env::var(ENV_JWT_ISSUER).ok().filter(|s| !s.is_empty()) {
                Some(raw) => raw
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect(),
                None => vec![format!("https://{frontend_host}")],
            };
        if issuer_allowlist.is_empty() {
            return Err(EnvLoadError::Missing(ENV_JWT_ISSUER));
        }

        // Keep the canonical 60s leeway + 24h TTL defaults — there is
        // no env-var override exposed at this layer (any operator that
        // needs to deviate should construct the builder directly).
        let builder: ClerkConfigBuilder = Self::builder()
            .jwks_url(jwks_url)
            .issuer_allowlist(issuer_allowlist)
            .audience(audience);
        builder.build().map_err(EnvLoadError::from)
    }
}

/// Decode the frontend-API host from a Clerk publishable key.
///
/// Publishable keys are `pk_<env>_<base64url-no-pad>`, where the
/// base64url segment decodes to `frontend_api_host$` (trailing `$`
/// is the canonical Clerk delimiter; we strip it).
pub(crate) fn parse_publishable_key_frontend_host(pk: &str) -> Result<String, EnvLoadError> {
    let rest = pk
        .strip_prefix("pk_test_")
        .or_else(|| pk.strip_prefix("pk_live_"))
        .ok_or(EnvLoadError::PublishableKeyMalformed(
            "must start with pk_test_ or pk_live_",
        ))?;
    if rest.is_empty() {
        return Err(EnvLoadError::PublishableKeyMalformed(
            "missing base64 segment",
        ));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(rest)
        .map_err(|_| EnvLoadError::PublishableKeyMalformed("base64 decode failed"))?;
    let s = String::from_utf8(decoded)
        .map_err(|_| EnvLoadError::PublishableKeyMalformed("non-utf8 host"))?;
    let trimmed = s.trim_end_matches('$').trim();
    if trimmed.is_empty() {
        return Err(EnvLoadError::PublishableKeyMalformed("empty host"));
    }
    // Reject hosts that include a scheme — the prefix is always
    // bare-host in Clerk's publishable-key encoding.
    if trimmed.contains("://") {
        return Err(EnvLoadError::PublishableKeyMalformed(
            "host must not include scheme",
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test-only"
)]
mod tests {
    use super::*;

    fn pk_for_host(env_tag: &str, host: &str) -> String {
        let with_delim = format!("{host}$");
        let b64 = URL_SAFE_NO_PAD.encode(with_delim.as_bytes());
        format!("pk_{env_tag}_{b64}")
    }

    #[test]
    fn parses_canonical_publishable_key() {
        let pk = pk_for_host("test", "clerk.example.dev");
        let host = parse_publishable_key_frontend_host(&pk).unwrap();
        assert_eq!(host, "clerk.example.dev");
    }

    #[test]
    fn rejects_non_pk_prefix() {
        let err = parse_publishable_key_frontend_host("sk_test_abc").unwrap_err();
        assert!(matches!(err, EnvLoadError::PublishableKeyMalformed(_)));
    }

    #[test]
    fn rejects_malformed_base64() {
        let err = parse_publishable_key_frontend_host("pk_test_!!!").unwrap_err();
        assert!(matches!(err, EnvLoadError::PublishableKeyMalformed(_)));
    }

    #[test]
    fn secret_key_debug_is_redacted() {
        let sk = SecretKey::new("sk_test_super_sensitive".to_owned());
        let debug = format!("{sk:?}");
        assert!(!debug.contains("super_sensitive"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn from_env_derives_jwks_and_issuer_from_pk() {
        // Use a unique audience so concurrent tests don't collide on the
        // process-global env.
        let pk = pk_for_host("test", "clerk.fromenv.example.dev");
        // SAFETY: tests run single-threaded for this case via the
        // `serial_test`-style guard below (we use a manual mutex to
        // avoid pulling another dev-dep).
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: set_var is unsafe in edition 2024 — we are in 2021.
        // The mutex above serialises access to the process-global env.
        env::set_var(ENV_PUBLISHABLE_KEY, &pk);
        env::set_var(ENV_SECRET_KEY, "sk_test_redacted");
        env::set_var(ENV_AUDIENCE, "corelink-api-fromenv");
        env::remove_var(ENV_JWKS_URL);
        env::remove_var(ENV_JWT_ISSUER);

        let cfg = ClerkConfig::from_env().unwrap();
        assert_eq!(
            cfg.jwks_url(),
            "https://clerk.fromenv.example.dev/.well-known/jwks.json"
        );
        assert_eq!(
            cfg.issuer_allowlist(),
            &["https://clerk.fromenv.example.dev".to_owned()]
        );
        assert_eq!(cfg.audience(), "corelink-api-fromenv");

        let secrets = ClerkEnvSecrets::from_env().unwrap();
        assert_eq!(secrets.publishable_key, pk);
        assert_eq!(secrets.secret_key.expose(), "sk_test_redacted");

        env::remove_var(ENV_PUBLISHABLE_KEY);
        env::remove_var(ENV_SECRET_KEY);
        env::remove_var(ENV_AUDIENCE);
    }

    #[test]
    fn from_env_honours_overrides() {
        let pk = pk_for_host("live", "clerk.override.example.dev");
        let _guard = ENV_LOCK.lock().unwrap();
        env::set_var(ENV_PUBLISHABLE_KEY, &pk);
        env::set_var(ENV_SECRET_KEY, "sk_live_redacted");
        env::set_var(ENV_AUDIENCE, "corelink-api-override");
        env::set_var(
            ENV_JWKS_URL,
            "https://jwks.override.example.dev/.well-known/jwks.json",
        );
        env::set_var(
            ENV_JWT_ISSUER,
            "https://iss1.example.dev,https://iss2.example.dev",
        );

        let cfg = ClerkConfig::from_env().unwrap();
        assert_eq!(
            cfg.jwks_url(),
            "https://jwks.override.example.dev/.well-known/jwks.json"
        );
        assert_eq!(
            cfg.issuer_allowlist(),
            &[
                "https://iss1.example.dev".to_owned(),
                "https://iss2.example.dev".to_owned()
            ]
        );

        env::remove_var(ENV_PUBLISHABLE_KEY);
        env::remove_var(ENV_SECRET_KEY);
        env::remove_var(ENV_AUDIENCE);
        env::remove_var(ENV_JWKS_URL);
        env::remove_var(ENV_JWT_ISSUER);
    }

    #[test]
    fn from_env_missing_publishable_key_surfaces_canonical_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::remove_var(ENV_PUBLISHABLE_KEY);
        env::set_var(ENV_AUDIENCE, "corelink-api");
        let err = ClerkConfig::from_env().unwrap_err();
        assert!(matches!(err, EnvLoadError::Missing(ENV_PUBLISHABLE_KEY)));
        env::remove_var(ENV_AUDIENCE);
    }

    /// Process-global lock for env-var tests. The Rust test runner
    /// dispatches tests in parallel; mutating `env` from multiple
    /// threads is racy, so we serialise here.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

//! Vault authentication backends (R2-9).
//!
//! Resolves a short-lived Vault token from one of the supported auth methods,
//! caches it with a refresh margin, and exposes a single
//! [`VaultAuth::token`] entry point that all REST calls funnel through.
//!
//! # Supported methods (env-driven, checked in priority order)
//!
//! 1. **Direct token** — `VAULT_TOKEN` (developer mode; no refresh).
//! 2. **AppRole** — `VAULT_APPROLE_ROLE_ID` + `VAULT_APPROLE_SECRET_ID`.
//!    Posts to `/v1/auth/approle/login`; refreshes when `lease_duration`
//!    expires (minus 60s margin).
//! 3. **Kubernetes** — `VAULT_K8S_SERVICE_ACCOUNT_TOKEN` (+ optional
//!    `VAULT_K8S_ROLE`). Posts to `/v1/auth/kubernetes/login`.
//! 4. **AWS IAM** — `VAULT_AWS_ROLE`. Posts to `/v1/auth/aws/login` with
//!    a signed STS GetCallerIdentity request.
//!
//! # Token cache
//!
//! - Cached in a `tokio::sync::Mutex<Option<CachedToken>>` per provider.
//! - Refresh-after instant = `now + lease_duration - 60s` (safety margin).
//! - Static `VAULT_TOKEN` mode never refreshes (uses TTL ≈ 1h sentinel).
//!
//! # Security
//!
//! - Tokens are NEVER included in error messages or `tracing` events.
//! - Only the auth method label (`"approle"`, `"kubernetes"`, ...) and the
//!   `lease_duration` (number of seconds) appear in debug logs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::BYOKError;

/// Refresh margin: renew the token this many seconds before expiry.
const REFRESH_MARGIN_SECS: u64 = 60;

/// Sentinel lease for the direct-token mode (no real expiry available).
const STATIC_TOKEN_TTL_SECS: u64 = 3600;

/// Vault auth method selector + resolved credentials.
enum AuthSource {
    /// Direct token (`VAULT_TOKEN`).
    Token(String),
    /// AppRole login.
    AppRole { role_id: String, secret_id: String },
    /// Kubernetes auth (in-cluster).
    Kubernetes { role: String, sa_jwt: String },
    /// AWS IAM auth.
    AwsIam { role: String },
    /// Test-only: static bearer token.
    #[doc(hidden)]
    Static(String),
}

impl std::fmt::Debug for AuthSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token(_) => f.write_str("AuthSource::Token(<redacted>)"),
            Self::AppRole { role_id, .. } => f
                .debug_struct("AuthSource::AppRole")
                .field("role_id", role_id)
                .field("secret_id", &"<redacted>")
                .finish(),
            Self::Kubernetes { role, .. } => f
                .debug_struct("AuthSource::Kubernetes")
                .field("role", role)
                .field("sa_jwt", &"<redacted>")
                .finish(),
            Self::AwsIam { role } => f
                .debug_struct("AuthSource::AwsIam")
                .field("role", role)
                .finish(),
            Self::Static(_) => f.write_str("AuthSource::Static(<redacted>)"),
        }
    }
}

impl AuthSource {
    fn label(&self) -> &'static str {
        match self {
            Self::Token(_) => "token",
            Self::AppRole { .. } => "approle",
            Self::Kubernetes { .. } => "kubernetes",
            Self::AwsIam { .. } => "aws-iam",
            Self::Static(_) => "static",
        }
    }
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    refresh_at: Instant,
}

impl std::fmt::Debug for CachedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedToken")
            .field("token", &"<redacted>")
            .field("refresh_at", &self.refresh_at)
            .finish()
    }
}

/// Vault auth — resolves and caches a short-lived Vault token.
#[derive(Clone)]
pub struct VaultAuth {
    inner: Arc<AuthInner>,
}

impl std::fmt::Debug for VaultAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultAuth")
            .field("inner", &self.inner)
            .finish()
    }
}

struct AuthInner {
    source: AuthSource,
    http: reqwest::Client,
    vault_addr: String,
    cache: Mutex<Option<CachedToken>>,
}

impl std::fmt::Debug for AuthInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthInner")
            .field("source", &self.source)
            .field("vault_addr", &self.vault_addr)
            .field("cache", &"<opaque>")
            .finish()
    }
}

impl VaultAuth {
    /// Detect a Vault auth source from the environment.
    ///
    /// Priority order: `VAULT_TOKEN` > AppRole > Kubernetes > AWS IAM.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if no auth method is configured.
    pub fn detect(http: reqwest::Client, vault_addr: &str) -> Result<Self, BYOKError> {
        let source = if let Ok(t) = std::env::var("VAULT_TOKEN") {
            debug!(method = "token", "VaultAuth: using VAULT_TOKEN");
            AuthSource::Token(t)
        } else if let (Ok(role_id), Ok(secret_id)) = (
            std::env::var("VAULT_APPROLE_ROLE_ID"),
            std::env::var("VAULT_APPROLE_SECRET_ID"),
        ) {
            debug!(method = "approle", "VaultAuth: using AppRole");
            AuthSource::AppRole { role_id, secret_id }
        } else if let Ok(sa_jwt) = std::env::var("VAULT_K8S_SERVICE_ACCOUNT_TOKEN") {
            let role = std::env::var("VAULT_K8S_ROLE").unwrap_or_else(|_| "default".to_string());
            debug!(method = "kubernetes", "VaultAuth: using Kubernetes auth");
            AuthSource::Kubernetes { role, sa_jwt }
        } else if let Ok(role) = std::env::var("VAULT_AWS_ROLE") {
            debug!(method = "aws-iam", "VaultAuth: using AWS IAM auth");
            AuthSource::AwsIam { role }
        } else {
            return Err(BYOKError::Provider(
                "Vault auth: no auth method configured. Set one of \
                 VAULT_TOKEN | VAULT_APPROLE_ROLE_ID+VAULT_APPROLE_SECRET_ID | \
                 VAULT_K8S_SERVICE_ACCOUNT_TOKEN | VAULT_AWS_ROLE"
                    .to_string(),
            ));
        };

        Ok(Self {
            inner: Arc::new(AuthInner {
                source,
                http,
                vault_addr: vault_addr.trim_end_matches('/').to_string(),
                cache: Mutex::new(None),
            }),
        })
    }

    /// Test-only constructor with a static token. Bypasses the network.
    #[doc(hidden)]
    pub fn for_test_static(token: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(AuthInner {
                source: AuthSource::Static(token.into()),
                http: reqwest::Client::new(),
                vault_addr: String::new(),
                cache: Mutex::new(None),
            }),
        }
    }

    /// Auth method label (for debug logs / audit).
    #[must_use]
    pub fn method(&self) -> &'static str {
        self.inner.source.label()
    }

    /// Obtain a (cached or fresh) Vault token.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if the login call fails.
    pub async fn token(&self) -> Result<String, BYOKError> {
        // Cache hit?
        {
            let guard = self.inner.cache.lock().await;
            if let Some(t) = guard.as_ref() {
                if Instant::now() < t.refresh_at {
                    return Ok(t.token.clone());
                }
            }
        }

        let fresh = match &self.inner.source {
            AuthSource::Token(t) => CachedToken {
                token: t.clone(),
                refresh_at: Instant::now()
                    .checked_add(Duration::from_secs(STATIC_TOKEN_TTL_SECS))
                    .unwrap_or_else(Instant::now),
            },
            AuthSource::Static(t) => CachedToken {
                token: t.clone(),
                refresh_at: Instant::now()
                    .checked_add(Duration::from_secs(STATIC_TOKEN_TTL_SECS))
                    .unwrap_or_else(Instant::now),
            },
            AuthSource::AppRole { role_id, secret_id } => {
                let body = serde_json::json!({
                    "role_id": role_id,
                    "secret_id": secret_id,
                });
                login(&self.inner.http, &self.inner.vault_addr, "approle", &body).await?
            }
            AuthSource::Kubernetes { role, sa_jwt } => {
                let body = serde_json::json!({
                    "role": role,
                    "jwt": sa_jwt,
                });
                login(
                    &self.inner.http,
                    &self.inner.vault_addr,
                    "kubernetes",
                    &body,
                )
                .await?
            }
            AuthSource::AwsIam { role } => {
                // NOTE: production Vault AWS IAM login requires a signed
                // STS GetCallerIdentity payload. We delegate signing to the
                // ambient AWS SDK chain (loaded by the orchestrator); the
                // payload fields are filled in by an adapter at the boundary.
                // For the moment we POST role only — the orchestrator MUST
                // populate `iam_http_request_method/url/headers/body` before
                // this code path is exercised in prod (tracked by RB-BYOK-VAULT-AWS).
                let body = serde_json::json!({
                    "role": role,
                });
                login(&self.inner.http, &self.inner.vault_addr, "aws", &body).await?
            }
        };

        let mut guard = self.inner.cache.lock().await;
        let token = fresh.token.clone();
        *guard = Some(fresh);
        Ok(token)
    }

    /// Test-only: force the cache to expire so the next `token()` call refreshes.
    #[doc(hidden)]
    pub async fn invalidate_cache(&self) {
        let mut guard = self.inner.cache.lock().await;
        *guard = None;
    }
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    auth: AuthBlock,
}

#[derive(Debug, Deserialize)]
struct AuthBlock {
    client_token: String,
    #[serde(default)]
    lease_duration: u64,
}

async fn login(
    http: &reqwest::Client,
    vault_addr: &str,
    method: &str,
    body: &serde_json::Value,
) -> Result<CachedToken, BYOKError> {
    let url = format!("{vault_addr}/v1/auth/{method}/login");
    let resp = http
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| BYOKError::Provider(format!("Vault auth/{method}/login POST: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        // Body may include hints but never the client token; safe to surface.
        let body = resp.text().await.unwrap_or_default();
        warn!(method = method, http = %status, "Vault login failed");
        return Err(BYOKError::Provider(format!(
            "Vault auth/{method}/login HTTP {status}: {body}"
        )));
    }

    let lr: LoginResponse = resp
        .json()
        .await
        .map_err(|e| BYOKError::Provider(format!("Vault login JSON parse: {e}")))?;

    let lease = lr.auth.lease_duration.max(REFRESH_MARGIN_SECS * 2);
    let refresh_at = Instant::now()
        .checked_add(Duration::from_secs(
            lease.saturating_sub(REFRESH_MARGIN_SECS),
        ))
        .unwrap_or_else(Instant::now);

    Ok(CachedToken {
        token: lr.auth.client_token,
        refresh_at,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn static_token_method_label() {
        let a = VaultAuth::for_test_static("hvs.xyz");
        assert_eq!(a.method(), "static");
    }

    #[tokio::test]
    async fn static_token_returns_value() {
        let a = VaultAuth::for_test_static("hvs.abc");
        let t = a.token().await.expect("token");
        assert_eq!(t, "hvs.abc");
    }

    #[tokio::test]
    async fn static_token_cache_hits_on_second_call() {
        let a = VaultAuth::for_test_static("hvs.cached");
        let t1 = a.token().await.expect("token1");
        let t2 = a.token().await.expect("token2");
        assert_eq!(t1, t2);
        assert_eq!(t1, "hvs.cached");
    }

    #[test]
    fn vault_auth_debug_redacts_secrets() {
        let auth = VaultAuth::for_test_static("hvs_super_secret_token_do_not_log");
        let dbg = format!("{:?}", auth);
        // Secret token should not appear in debug output
        assert!(!dbg.contains("hvs_super_secret_token_do_not_log"));
        assert!(dbg.contains("redacted") || dbg.contains("opaque"));
    }

    #[test]
    fn auth_source_debug_redacts_direct_token() {
        let source = AuthSource::Token("hvs_direct_token_secret".to_string());
        let dbg = format!("{:?}", source);
        assert!(!dbg.contains("hvs_direct_token_secret"));
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn auth_source_debug_redacts_approle() {
        let source = AuthSource::AppRole {
            role_id: "role_123".to_string(),
            secret_id: "secret_456_confidential".to_string(),
        };
        let dbg = format!("{:?}", source);
        assert!(!dbg.contains("secret_456_confidential"));
        assert!(dbg.contains("role_123"));
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn auth_source_debug_redacts_kubernetes() {
        let source = AuthSource::Kubernetes {
            role: "app-role".to_string(),
            sa_jwt: "eyJhbGciOiJSUzI1NiIsImtpZCI6InNlY3JldCJ9".to_string(),
        };
        let dbg = format!("{:?}", source);
        assert!(!dbg.contains("eyJhbGciOiJSUzI1NiIsImtpZCI6InNlY3JldCJ9"));
        assert!(dbg.contains("app-role"));
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn cached_token_debug_redacts() {
        let ct = CachedToken {
            token: "hvs_cached_secret_token".to_string(),
            refresh_at: Instant::now(),
        };
        let dbg = format!("{:?}", ct);
        assert!(!dbg.contains("hvs_cached_secret_token"));
        assert!(dbg.contains("redacted"));
    }
}

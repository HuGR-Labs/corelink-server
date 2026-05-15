//! Microsoft Entra ID (formerly Azure AD) OAuth2 token provider.
//!
//! Resolves bearer tokens for the Azure Key Vault data-plane scope
//! `https://vault.azure.net/.default` via one of two flows:
//!
//! 1. **Client credentials** — `AZURE_TENANT_ID` + `AZURE_CLIENT_ID` +
//!    `AZURE_CLIENT_SECRET` env vars. POST to
//!    `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token` with
//!    `grant_type=client_credentials`.
//! 2. **Workload identity** — `AZURE_TENANT_ID` + `AZURE_CLIENT_ID` +
//!    `AZURE_FEDERATED_TOKEN_FILE`. POSTs the federated SA token with
//!    `grant_type=client_credentials&client_assertion_type=urn:ietf:params:oauth:client-assertion-type:jwt-bearer&client_assertion=<sa-jwt>`.
//!    This is the AKS / GKE-style flow.
//!
//! Tokens are cached in-memory with a 60-second refresh margin. The cache is
//! guarded by a `tokio::sync::Mutex`; only one refresh runs at a time.
//!
//! # Security
//!
//! - `AZURE_CLIENT_SECRET` is wrapped in a [`SecretString`] newtype with
//!   redacted `Debug`; it is never logged.
//! - The federated-token-file flow re-reads the file on every refresh because
//!   AKS rotates the SA JWT on a short cadence (~hourly).
//! - Bearer tokens are not logged at any level.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use corelink_byok::BYOKError;

/// Public OAuth2 scope for Key Vault data plane.
pub(crate) const KV_SCOPE: &str = "https://vault.azure.net/.default";

/// Login endpoint host. Production = public cloud; test mocks override via
/// [`EntraCredentials::for_test_static`].
const LOGIN_HOST: &str = "https://login.microsoftonline.com";

/// Default refresh margin before token expiry.
const REFRESH_MARGIN_SECS: u64 = 60;

/// Redacted-Debug wrapper around a client secret.
///
/// `Debug` prints only the length; the secret value is never exposed.
#[derive(Clone)]
pub(crate) struct SecretString(String);

impl SecretString {
    pub(crate) fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretString(<redacted {} bytes>)", self.0.len())
    }
}

/// Cached access token + expiry.
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    /// Refresh-after instant (= `now + expires_in - REFRESH_MARGIN_SECS`).
    refresh_at: Instant,
}

/// Entra ID credential source (resolved at provider construction).
#[derive(Debug)]
enum EntraSource {
    /// Client credentials: tenant + client_id + client_secret.
    ClientSecret {
        tenant_id: String,
        client_id: String,
        client_secret: SecretString,
    },
    /// Federated workload identity: tenant + client_id + path to SA JWT file.
    WorkloadIdentity {
        tenant_id: String,
        client_id: String,
        token_file: PathBuf,
    },
    /// Test-only: a literal bearer token (no exchange, no refresh).
    #[doc(hidden)]
    Static(String),
}

/// Entra ID OAuth2 access-token provider.
///
/// Construct via [`EntraCredentials::detect`]; call
/// [`EntraCredentials::access_token`] to obtain a (cached) bearer token.
#[derive(Debug, Clone)]
pub struct EntraCredentials {
    inner: Arc<EntraInner>,
}

#[derive(Debug)]
struct EntraInner {
    source: EntraSource,
    http: reqwest::Client,
    login_host: String,
    token: Mutex<Option<CachedToken>>,
}

impl EntraCredentials {
    /// Detect Entra ID credentials per the documented chain.
    ///
    /// Order:
    /// 1. Workload identity: `AZURE_FEDERATED_TOKEN_FILE` + `AZURE_TENANT_ID`
    ///    + `AZURE_CLIENT_ID`.
    /// 2. Client secret: `AZURE_TENANT_ID` + `AZURE_CLIENT_ID` +
    ///    `AZURE_CLIENT_SECRET`.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if no usable credentials are found.
    pub fn detect(http: reqwest::Client) -> Result<Self, BYOKError> {
        Self::detect_with_host(http, LOGIN_HOST)
    }

    /// Detect with a custom login host (test mock).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if no usable credentials are found.
    pub fn detect_with_host(http: reqwest::Client, login_host: &str) -> Result<Self, BYOKError> {
        let tenant = std::env::var("AZURE_TENANT_ID").ok();
        let client_id = std::env::var("AZURE_CLIENT_ID").ok();

        // 1) Workload identity (preferred on AKS / federated CI).
        if let (Some(tenant), Some(client_id), Ok(path)) = (
            tenant.clone(),
            client_id.clone(),
            std::env::var("AZURE_FEDERATED_TOKEN_FILE"),
        ) {
            debug!("Entra: using workload-identity federation");
            return Ok(Self {
                inner: Arc::new(EntraInner {
                    source: EntraSource::WorkloadIdentity {
                        tenant_id: tenant,
                        client_id,
                        token_file: PathBuf::from(path),
                    },
                    http,
                    login_host: login_host.trim_end_matches('/').to_string(),
                    token: Mutex::new(None),
                }),
            });
        }

        // 2) Client secret.
        if let (Some(tenant), Some(client_id), Ok(secret)) =
            (tenant, client_id, std::env::var("AZURE_CLIENT_SECRET"))
        {
            debug!("Entra: using client-credentials (service principal)");
            return Ok(Self {
                inner: Arc::new(EntraInner {
                    source: EntraSource::ClientSecret {
                        tenant_id: tenant,
                        client_id,
                        client_secret: SecretString::new(secret),
                    },
                    http,
                    login_host: login_host.trim_end_matches('/').to_string(),
                    token: Mutex::new(None),
                }),
            });
        }

        Err(BYOKError::Provider(
            "Entra: no credentials found (set AZURE_TENANT_ID + AZURE_CLIENT_ID + \
             AZURE_CLIENT_SECRET, or AZURE_FEDERATED_TOKEN_FILE for workload identity)"
                .to_string(),
        ))
    }

    /// Test-only constructor with a fixed bearer token. Bypasses the network.
    #[doc(hidden)]
    pub fn for_test_static(token: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(EntraInner {
                source: EntraSource::Static(token.into()),
                http: reqwest::Client::new(),
                login_host: LOGIN_HOST.to_string(),
                token: Mutex::new(None),
            }),
        }
    }

    /// Test-only constructor for client-credentials flow with custom login
    /// host (used by `wiremock` to exercise the OAuth2 token-fetch path).
    #[doc(hidden)]
    pub fn for_test_client_secret(
        http: reqwest::Client,
        login_host: &str,
        tenant_id: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(EntraInner {
                source: EntraSource::ClientSecret {
                    tenant_id: tenant_id.into(),
                    client_id: client_id.into(),
                    client_secret: SecretString::new(client_secret),
                },
                http,
                login_host: login_host.trim_end_matches('/').to_string(),
                token: Mutex::new(None),
            }),
        }
    }

    /// Test-only constructor for workload-identity flow with custom login host.
    #[doc(hidden)]
    pub fn for_test_workload_identity(
        http: reqwest::Client,
        login_host: &str,
        tenant_id: impl Into<String>,
        client_id: impl Into<String>,
        token_file: PathBuf,
    ) -> Self {
        Self {
            inner: Arc::new(EntraInner {
                source: EntraSource::WorkloadIdentity {
                    tenant_id: tenant_id.into(),
                    client_id: client_id.into(),
                    token_file,
                },
                http,
                login_host: login_host.trim_end_matches('/').to_string(),
                token: Mutex::new(None),
            }),
        }
    }

    /// Obtain a (cached or fresh) OAuth2 access token.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if the token endpoint fails or the
    /// workload-identity SA JWT file is unreadable.
    pub async fn access_token(&self) -> Result<String, BYOKError> {
        // Check cache.
        {
            let guard = self.inner.token.lock().await;
            if let Some(t) = guard.as_ref() {
                if Instant::now() < t.refresh_at {
                    return Ok(t.access_token.clone());
                }
            }
        }

        // Refresh.
        let fresh = match &self.inner.source {
            EntraSource::ClientSecret {
                tenant_id,
                client_id,
                client_secret,
            } => {
                fetch_with_client_secret(
                    &self.inner.http,
                    &self.inner.login_host,
                    tenant_id,
                    client_id,
                    client_secret.as_str(),
                )
                .await?
            }
            EntraSource::WorkloadIdentity {
                tenant_id,
                client_id,
                token_file,
            } => {
                fetch_with_workload_identity(
                    &self.inner.http,
                    &self.inner.login_host,
                    tenant_id,
                    client_id,
                    token_file,
                )
                .await?
            }
            EntraSource::Static(tok) => CachedToken {
                access_token: tok.clone(),
                refresh_at: Instant::now()
                    .checked_add(Duration::from_secs(3600))
                    .unwrap_or_else(Instant::now),
            },
        };

        let mut guard = self.inner.token.lock().await;
        let token = fresh.access_token.clone();
        *guard = Some(fresh);
        Ok(token)
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: i64,
}

async fn fetch_with_client_secret(
    http: &reqwest::Client,
    login_host: &str,
    tenant_id: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<CachedToken, BYOKError> {
    let url = format!("{login_host}/{tenant_id}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("scope", KV_SCOPE),
    ];
    let resp = http
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|e| BYOKError::Provider(format!("Entra: token POST failed: {e}")))?;
    finish_token(resp).await
}

async fn fetch_with_workload_identity(
    http: &reqwest::Client,
    login_host: &str,
    tenant_id: &str,
    client_id: &str,
    token_file: &std::path::Path,
) -> Result<CachedToken, BYOKError> {
    let assertion = tokio::fs::read_to_string(token_file).await.map_err(|e| {
        BYOKError::Provider(format!(
            "Entra: cannot read AZURE_FEDERATED_TOKEN_FILE: {e}"
        ))
    })?;
    let assertion = assertion.trim();
    let url = format!("{login_host}/{tenant_id}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        (
            "client_assertion_type",
            "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        ),
        ("client_assertion", assertion),
        ("scope", KV_SCOPE),
    ];
    let resp = http
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            BYOKError::Provider(format!("Entra: workload-identity POST failed: {e}"))
        })?;
    finish_token(resp).await
}

async fn finish_token(resp: reqwest::Response) -> Result<CachedToken, BYOKError> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(status = %status, "Entra: token endpoint non-2xx");
        return Err(BYOKError::Provider(format!(
            "Entra: token endpoint status {status}: {body}"
        )));
    }
    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| BYOKError::Provider(format!("Entra: token JSON parse: {e}")))?;

    let lifetime = tr
        .expires_in
        .max(REFRESH_MARGIN_SECS as i64 + 1)
        .saturating_sub(REFRESH_MARGIN_SECS as i64) as u64;
    let refresh_at = Instant::now()
        .checked_add(Duration::from_secs(lifetime))
        .unwrap_or_else(Instant::now);
    Ok(CachedToken {
        access_token: tr.access_token,
        refresh_at,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn secret_string_debug_redacts() {
        let s = SecretString::new("super-secret-password-do-not-log");
        let dbg = format!("{:?}", s);
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("redacted"));
        assert_eq!(s.as_str(), "super-secret-password-do-not-log");
    }

    #[tokio::test]
    async fn static_token_cached() {
        let creds = EntraCredentials::for_test_static("my-test-token");
        let t1 = creds.access_token().await.unwrap();
        let t2 = creds.access_token().await.unwrap();
        assert_eq!(t1, "my-test-token");
        assert_eq!(t2, "my-test-token");
    }
}

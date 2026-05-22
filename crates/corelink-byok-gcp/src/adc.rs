//! Application Default Credentials (ADC) helper for GCP Cloud KMS.
//!
//! Implements a minimal subset of the ADC chain used by `GcpKmsRealProvider`:
//!
//! 1. **Service account JSON** — `GOOGLE_APPLICATION_CREDENTIALS` env points to
//!    a JSON key file. Mints a self-signed JWT (RS256) and exchanges it at
//!    `oauth2.googleapis.com/token` for an access token.
//! 2. **Workload Identity / GCE metadata server** — fetches access token from
//!    `http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token`
//!    when running on GKE / GCE.
//! 3. **`gcloud` default login** — falls back to
//!    `~/.config/gcloud/application_default_credentials.json` (user dev mode).
//!
//! Tokens are cached with a safety margin (refresh 60 s before expiry).
//!
//! # Security
//!
//! - The service-account private key is loaded into memory once at provider
//!   construction and never logged.
//! - `tracing` spans elide credential material; only the SA email is logged at
//!   `debug` level.
//! - Token cache is `tokio::sync::Mutex`-guarded; no `unsafe`, no panics on
//!   malformed inputs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use corelink_byok_core::BYOKError;

/// Public OAuth2 scope for Cloud KMS access.
pub(crate) const KMS_SCOPE: &str = "https://www.googleapis.com/auth/cloudkms";

/// Default OAuth2 token endpoint.
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

/// Metadata server URL for Workload Identity / GCE.
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

/// Service-account JSON key (subset of fields we care about).
#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

/// Cached access token + expiry.
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    /// Refresh-after instant (= `now + expires_in - 60 s`).
    refresh_at: Instant,
}

/// ADC credential source (resolved at provider construction).
#[derive(Debug)]
enum AdcSource {
    /// Service-account JSON file — sign self-signed JWT, exchange for token.
    ServiceAccount(Box<ServiceAccountKey>),
    /// Workload Identity / GCE metadata server.
    MetadataServer,
    /// Test-only: a literal bearer token (no exchange, no refresh).
    #[doc(hidden)]
    Static(String),
}

/// Application Default Credentials token provider.
///
/// Construct via [`AdcCredentials::detect`]; call [`AdcCredentials::access_token`]
/// to obtain a (possibly cached) OAuth2 bearer token for Cloud KMS.
///
/// Public to allow the in-crate `tests/` directory to inject a static bearer
/// token via [`AdcCredentials::for_test_static`]; production callers should
/// use [`AdcCredentials::detect`] only.
#[derive(Debug, Clone)]
pub struct AdcCredentials {
    inner: Arc<AdcInner>,
}

#[derive(Debug)]
struct AdcInner {
    source: AdcSource,
    http: reqwest::Client,
    token: Mutex<Option<CachedToken>>,
}

impl AdcCredentials {
    /// Detect ADC source per the documented chain.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if no credentials can be found.
    pub async fn detect(http: reqwest::Client) -> Result<Self, BYOKError> {
        // 1) Explicit env var.
        if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            debug!("ADC: using GOOGLE_APPLICATION_CREDENTIALS");
            let sa = load_service_account_file(PathBuf::from(path)).await?;
            return Ok(Self::with_service_account(http, sa));
        }

        // 2) gcloud user default login.
        if let Some(home) = std::env::var_os("HOME") {
            let path = PathBuf::from(home)
                .join(".config")
                .join("gcloud")
                .join("application_default_credentials.json");
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                // gcloud user creds are a different format (refresh_token, not
                // service_account). For now we only support SA JSON via this
                // path; user creds fall through to metadata server.
                if let Ok(sa) = load_service_account_file(path).await {
                    debug!("ADC: using gcloud application_default_credentials.json (service-account form)");
                    return Ok(Self::with_service_account(http, sa));
                }
            }
        }

        // 3) Metadata server (Workload Identity / GCE).
        debug!("ADC: falling back to GCE/GKE metadata server");
        Ok(Self {
            inner: Arc::new(AdcInner {
                source: AdcSource::MetadataServer,
                http,
                token: Mutex::new(None),
            }),
        })
    }

    fn with_service_account(http: reqwest::Client, sa: ServiceAccountKey) -> Self {
        Self {
            inner: Arc::new(AdcInner {
                source: AdcSource::ServiceAccount(Box::new(sa)),
                http,
                token: Mutex::new(None),
            }),
        }
    }

    /// Test-only constructor with a fixed bearer token. Bypasses the network.
    #[doc(hidden)]
    pub fn for_test_static(token: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(AdcInner {
                source: AdcSource::Static(token.into()),
                http: reqwest::Client::new(),
                token: Mutex::new(None),
            }),
        }
    }

    /// Obtain a (cached or fresh) OAuth2 access token.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if the underlying source (service
    /// account JWT exchange or metadata server) fails. Static-token sources
    /// never error.
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
            AdcSource::ServiceAccount(sa) => mint_token_service_account(&self.inner.http, sa).await?,
            AdcSource::MetadataServer => fetch_token_metadata_server(&self.inner.http).await?,
            AdcSource::Static(tok) => CachedToken {
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

async fn load_service_account_file(path: PathBuf) -> Result<ServiceAccountKey, BYOKError> {
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| BYOKError::Provider(format!("ADC: cannot read SA key file: {e}")))?;
    let key: ServiceAccountKey = serde_json::from_slice(&bytes)
        .map_err(|e| BYOKError::Provider(format!("ADC: malformed SA JSON: {e}")))?;
    Ok(key)
}

#[derive(Debug, Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

async fn mint_token_service_account(
    http: &reqwest::Client,
    sa: &ServiceAccountKey,
) -> Result<CachedToken, BYOKError> {
    let token_uri = sa.token_uri.as_deref().unwrap_or(TOKEN_ENDPOINT);
    let now = current_unix_seconds();
    let claims = JwtClaims {
        iss: &sa.client_email,
        scope: KMS_SCOPE,
        aud: token_uri,
        exp: now + 3600,
        iat: now,
    };

    let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
        .map_err(|e| BYOKError::Provider(format!("ADC: invalid SA private key: {e}")))?;
    let assertion = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| BYOKError::Provider(format!("ADC: JWT encode failed: {e}")))?;

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", assertion.as_str()),
    ];

    let resp = http
        .post(token_uri)
        .form(&params)
        .send()
        .await
        .map_err(|e| BYOKError::Provider(format!("ADC: token endpoint POST failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(status = %status, "ADC: token endpoint returned non-2xx");
        return Err(BYOKError::Provider(format!(
            "ADC: token endpoint status {status}: {body}"
        )));
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| BYOKError::Provider(format!("ADC: token JSON parse: {e}")))?;

    let refresh_at = Instant::now()
        .checked_add(Duration::from_secs(tr.expires_in.max(60).saturating_sub(60) as u64))
        .unwrap_or_else(Instant::now);

    Ok(CachedToken {
        access_token: tr.access_token,
        refresh_at,
    })
}

#[derive(Debug, Deserialize)]
struct MetadataTokenResponse {
    access_token: String,
    expires_in: i64,
}

async fn fetch_token_metadata_server(http: &reqwest::Client) -> Result<CachedToken, BYOKError> {
    let resp = http
        .get(METADATA_TOKEN_URL)
        .header("Metadata-Flavor", "Google")
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map_err(|e| BYOKError::Provider(format!("ADC: metadata server unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(BYOKError::Provider(format!(
            "ADC: metadata server status {status}"
        )));
    }

    let tr: MetadataTokenResponse = resp
        .json()
        .await
        .map_err(|e| BYOKError::Provider(format!("ADC: metadata JSON parse: {e}")))?;

    let refresh_at = Instant::now()
        .checked_add(Duration::from_secs(tr.expires_in.max(60).saturating_sub(60) as u64))
        .unwrap_or_else(Instant::now);

    Ok(CachedToken {
        access_token: tr.access_token,
        refresh_at,
    })
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Base64 (standard, no-padding) encoder used by both the ciphertext payload
/// shape and the AAD field of Cloud KMS REST `Encrypt` / `Decrypt` requests.
///
/// Retained as a `pub(crate)` helper to keep the ADC module
/// self-contained for downstream call sites that may not pull `base64`
/// directly. `real::native` now owns its own b64 helpers (gated to the
/// production feature), so this is currently dead in the dependency
/// graph — kept under `#[allow(dead_code)]` to avoid breaking the
/// crate-level `dead_code = "deny"` lint while preserving the helper
/// for future wire-format work.
#[allow(dead_code)]
pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Base64 (standard, padded) decoder. See [`b64_encode`] for the
/// retention rationale.
#[allow(dead_code)]
pub(crate) fn b64_decode(s: &str) -> Result<Vec<u8>, BYOKError> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| BYOKError::Provider(format!("base64 decode: {e}")))
}

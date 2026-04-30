//! Core [`ClerkAdapter`] implementation.
//!
//! Wires together [`crate::ClerkConfig`] + a [`crate::JwksFetcher`] +
//! a [`crate::KvJwksCache`] into the `validate(raw_jwt)` ->
//! `Result<ClerkPrincipal, AuthError>` surface mandated by WI-S03-001
//! §1 + §6.1.3.
//!
//! See module-level rustdoc on [`crate`] for the architectural rules
//! (RS256-only, exact-match issuer, lazy-refresh on KID miss, etc.).

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use subtle::ConstantTimeEq;

use crate::config::ClerkConfig;
use crate::error::AuthError;
use crate::jwks::{Jwks, JwksFetcher};
use crate::jwks_cache::{is_fresh, CachedJwks, KvJwksCache};
use crate::principal::{
    ClerkOrgId, ClerkPrincipal, ClerkRole, ClerkSessionId, ClerkUserId, Email,
};

/// Refresh trigger for `corelink_auth_clerk_jwks_refresh_total{trigger=…}` (WI §6.1.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshTrigger {
    /// Cache slot was empty / TTL expired.
    Scheduled,
    /// `kid` from JWT was not in the cached JWKS — single retry.
    KidMiss,
    /// Explicit `refresh_jwks` call by the operator.
    Manual,
}

impl RefreshTrigger {
    /// Canonical `{trigger=…}` label for metric emission.
    #[must_use]
    pub fn as_label(self) -> &'static str {
        match self {
            RefreshTrigger::Scheduled => "scheduled",
            RefreshTrigger::KidMiss => "kid_miss",
            RefreshTrigger::Manual => "manual",
        }
    }
}

/// In-process counters surfaced for tests + (eventually) metric
/// observation. Counters are atomic; cloning [`ClerkAdapter`] shares
/// the same counter instance behind an `Arc`.
#[derive(Debug, Default)]
struct Counters {
    validate_ok: AtomicUsize,
    validate_sig_invalid: AtomicUsize,
    validate_expired: AtomicUsize,
    validate_aud_mismatch: AtomicUsize,
    validate_iss_mismatch: AtomicUsize,
    validate_kid_miss: AtomicUsize,
    validate_jwks_fetch_failed: AtomicUsize,
    cache_hits: AtomicUsize,
    refresh_scheduled: AtomicUsize,
    refresh_kid_miss: AtomicUsize,
    refresh_manual: AtomicUsize,
}

/// Snapshot of canonical counter values surfaced via [`ClerkAdapter::counters`].
#[derive(Clone, Debug, Default)]
pub struct CountersSnapshot {
    /// `corelink_auth_clerk_validate_total{outcome="ok"}`
    pub validate_ok: usize,
    /// `…{outcome="sig_invalid"}`
    pub validate_sig_invalid: usize,
    /// `…{outcome="expired"}`
    pub validate_expired: usize,
    /// `…{outcome="aud_mismatch"}`
    pub validate_aud_mismatch: usize,
    /// `…{outcome="iss_mismatch"}`
    pub validate_iss_mismatch: usize,
    /// `…{outcome="kid_miss"}`
    pub validate_kid_miss: usize,
    /// `…{outcome="jwks_fetch_failed"}`
    pub validate_jwks_fetch_failed: usize,
    /// `corelink_auth_clerk_jwks_cache_hits_total`
    pub cache_hits: usize,
    /// `corelink_auth_clerk_jwks_refresh_total{trigger="scheduled"}`
    pub refresh_scheduled: usize,
    /// `…{trigger="kid_miss"}`
    pub refresh_kid_miss: usize,
    /// `…{trigger="manual"}`
    pub refresh_manual: usize,
}

/// Canonical adapter for Clerk-issued JWTs.
///
/// Cheap to clone: all collaborators are behind `Arc`s. Construction
/// uses [`ClerkAdapter::new`].
#[derive(Clone)]
pub struct ClerkAdapter {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ClerkAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClerkAdapter")
            .field("audience", &self.inner.config.audience())
            .field("issuer_allowlist", &self.inner.config.issuer_allowlist())
            .field("leeway_secs", &self.inner.config.leeway().as_secs())
            .field("jwks_ttl_secs", &self.inner.config.jwks_ttl().as_secs())
            .finish()
    }
}

struct Inner {
    config: ClerkConfig,
    fetcher: Arc<dyn JwksFetcher>,
    cache: Arc<dyn KvJwksCache>,
    counters: Counters,
    clock: Box<dyn Fn() -> SystemTime + Send + Sync>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: AudienceClaim,
    exp: i64,
    iat: Option<i64>,
    nbf: Option<i64>,
    sid: Option<String>,
    org_id: Option<String>,
    email: Option<String>,
    clerk_role: Option<String>,
    role: Option<String>,
}

/// Clerk emits `aud` as either a single string or an array of
/// strings; we accept both and require an exact match against the
/// configured audience.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    Single(String),
    Multi(Vec<String>),
}

impl AudienceClaim {
    fn matches(&self, expected: &str) -> bool {
        let expected_bytes = expected.as_bytes();
        let mut hit = false;
        match self {
            AudienceClaim::Single(s) => {
                if s.len() == expected_bytes.len()
                    && s.as_bytes().ct_eq(expected_bytes).into()
                {
                    hit = true;
                }
            }
            AudienceClaim::Multi(vs) => {
                for v in vs {
                    if v.len() == expected_bytes.len()
                        && v.as_bytes().ct_eq(expected_bytes).into()
                    {
                        hit = true;
                    }
                }
            }
        }
        hit
    }
}

impl ClerkAdapter {
    /// Construct a new adapter. `fetcher` and `cache` are wrapped in
    /// `Arc` internally so callers may pass owned values.
    #[must_use]
    pub fn new<F, C>(config: ClerkConfig, fetcher: F, cache: C) -> Self
    where
        F: JwksFetcher,
        C: KvJwksCache,
    {
        Self::new_with_clock(config, fetcher, cache, SystemTime::now)
    }

    /// Construct an adapter with an injectable clock (test helper).
    #[must_use]
    pub fn new_with_clock<F, C>(
        config: ClerkConfig,
        fetcher: F,
        cache: C,
        clock: impl Fn() -> SystemTime + Send + Sync + 'static,
    ) -> Self
    where
        F: JwksFetcher,
        C: KvJwksCache,
    {
        Self {
            inner: Arc::new(Inner {
                config,
                fetcher: Arc::new(fetcher),
                cache: Arc::new(cache),
                counters: Counters::default(),
                clock: Box::new(clock),
            }),
        }
    }

    /// Snapshot of all metric counters (test helper / metric-export hook).
    #[must_use]
    pub fn counters(&self) -> CountersSnapshot {
        let c = &self.inner.counters;
        CountersSnapshot {
            validate_ok: c.validate_ok.load(Ordering::SeqCst),
            validate_sig_invalid: c.validate_sig_invalid.load(Ordering::SeqCst),
            validate_expired: c.validate_expired.load(Ordering::SeqCst),
            validate_aud_mismatch: c.validate_aud_mismatch.load(Ordering::SeqCst),
            validate_iss_mismatch: c.validate_iss_mismatch.load(Ordering::SeqCst),
            validate_kid_miss: c.validate_kid_miss.load(Ordering::SeqCst),
            validate_jwks_fetch_failed: c.validate_jwks_fetch_failed.load(Ordering::SeqCst),
            cache_hits: c.cache_hits.load(Ordering::SeqCst),
            refresh_scheduled: c.refresh_scheduled.load(Ordering::SeqCst),
            refresh_kid_miss: c.refresh_kid_miss.load(Ordering::SeqCst),
            refresh_manual: c.refresh_manual.load(Ordering::SeqCst),
        }
    }

    /// Underlying canonical config (read-only).
    #[must_use]
    pub fn config(&self) -> &ClerkConfig {
        &self.inner.config
    }

    /// Validate `raw_jwt`. Implements WI-S03-001 §6.1.3 step-by-step.
    pub async fn validate(&self, raw_jwt: &str) -> Result<ClerkPrincipal, AuthError> {
        let result = self.validate_inner(raw_jwt).await;
        self.bump_outcome(&result);
        result
    }

    /// Force a JWKS refresh (rotation hook). Bumps the
    /// `corelink_auth_clerk_jwks_refresh_total{trigger="manual"}`
    /// counter on success.
    pub async fn refresh_jwks(&self) -> Result<(), AuthError> {
        let _jwks = self.fetch_and_cache(RefreshTrigger::Manual).await?;
        Ok(())
    }

    fn bump_outcome(&self, result: &Result<ClerkPrincipal, AuthError>) {
        let c = &self.inner.counters;
        match result {
            Ok(_) => {
                c.validate_ok.fetch_add(1, Ordering::SeqCst);
            }
            Err(err) => match err.metric_outcome() {
                "sig_invalid" => {
                    c.validate_sig_invalid.fetch_add(1, Ordering::SeqCst);
                }
                "expired" => {
                    c.validate_expired.fetch_add(1, Ordering::SeqCst);
                }
                "aud_mismatch" => {
                    c.validate_aud_mismatch.fetch_add(1, Ordering::SeqCst);
                }
                "iss_mismatch" => {
                    c.validate_iss_mismatch.fetch_add(1, Ordering::SeqCst);
                }
                "kid_miss" => {
                    c.validate_kid_miss.fetch_add(1, Ordering::SeqCst);
                }
                "jwks_fetch_failed" => {
                    c.validate_jwks_fetch_failed.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    c.validate_sig_invalid.fetch_add(1, Ordering::SeqCst);
                }
            },
        }
    }

    async fn validate_inner(&self, raw_jwt: &str) -> Result<ClerkPrincipal, AuthError> {
        // 1a. Pre-decode the header JSON ourselves so we can surface
        // a canonical `AlgNotAllowed` for `alg=none` (CVE-2015-9235
        // family) and `alg=HS256` (CVE-2018-0114 RS↔HS confusion).
        // jsonwebtoken's `decode_header` errors with a generic JSON
        // parse error on `alg=none` because the enum doesn't include
        // it (good, but we want our canonical taxonomy variant).
        let raw_header_json = decode_header_json(raw_jwt)?;
        let alg = raw_header_json
            .get("alg")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AuthError::Malformed("missing alg header".into()))?;
        if alg != "RS256" {
            return Err(AuthError::AlgNotAllowed);
        }

        // 1b. Now hand the token to jsonwebtoken's header decoder
        // (which round-trips the alg into the `Algorithm` enum).
        let header = decode_header(raw_jwt)
            .map_err(|e| AuthError::Malformed(format!("header decode: {e}")))?;
        if header.alg != Algorithm::RS256 {
            return Err(AuthError::AlgNotAllowed);
        }
        let kid = header
            .kid
            .ok_or_else(|| AuthError::Malformed("missing kid header".into()))?;

        // 2. Resolve key — KV cache lookup with lazy refresh on expiry
        // / KID miss. Single retry max per WI §9.5 / §6.1.3 step 4.
        let cached = self.load_cached().await?;
        let key = match cached.as_ref().and_then(|c| c.jwks.find(&kid)) {
            Some(k) => k.clone(),
            None => {
                // First fetch: scheduled (cold) or kid_miss (warm).
                let first_trigger = if cached.is_some() {
                    RefreshTrigger::KidMiss
                } else {
                    RefreshTrigger::Scheduled
                };
                let jwks = self.fetch_and_cache(first_trigger).await?;
                if let Some(k) = jwks.find(&kid).cloned() {
                    k
                } else {
                    // Second fetch only meaningful when the first
                    // was a cold-start: a warm cache that already
                    // missed has just been replaced by a fetch that
                    // also missed; retrying a third time has no
                    // upside (Clerk JWKS does not change between two
                    // requests issued microseconds apart) and risks
                    // a fetch storm. WI §6.1.3 step 4 / §9.5.
                    if first_trigger == RefreshTrigger::Scheduled {
                        let jwks2 = self.fetch_and_cache(RefreshTrigger::KidMiss).await?;
                        jwks2.find(&kid).cloned().ok_or(AuthError::KidNotInJwks)?
                    } else {
                        return Err(AuthError::KidNotInJwks);
                    }
                }
            }
        };

        // 3. Build the DecodingKey from the JWKS key components.
        let decoding_key =
            DecodingKey::from_rsa_components(&key.n_b64url, &key.e_b64url).map_err(|e| {
                AuthError::Malformed(format!("RSA components rejected: {e}"))
            })?;

        // 4. Build Validation with explicit RS256 allowlist. We
        // disable jsonwebtoken's built-in `exp`/`nbf`/`aud`/`iss`
        // checks because they consult `SystemTime::now()` directly —
        // our adapter respects an injectable clock for testability +
        // to surface canonical [`AuthError`] variants. Signature +
        // RS256-allowlist enforcement remain delegated to the audited
        // crate.
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = self.inner.config.leeway().as_secs();
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::new();
        validation.aud = None;
        validation.iss = None;

        // 5. Verify signature + decode claims.
        let token_data = decode::<Claims>(raw_jwt, &decoding_key, &validation).map_err(
            |e| match *e.kind() {
                jsonwebtoken::errors::ErrorKind::InvalidSignature => AuthError::SignatureInvalid,
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => AuthError::AlgNotAllowed,
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithmName => AuthError::AlgNotAllowed,
                jsonwebtoken::errors::ErrorKind::InvalidToken => {
                    AuthError::Malformed(format!("invalid token: {e}"))
                }
                jsonwebtoken::errors::ErrorKind::Base64(_) => {
                    AuthError::Malformed(format!("base64 decode: {e}"))
                }
                jsonwebtoken::errors::ErrorKind::Json(_) => {
                    AuthError::Malformed(format!("json decode: {e}"))
                }
                jsonwebtoken::errors::ErrorKind::Utf8(_) => {
                    AuthError::Malformed(format!("utf8 decode: {e}"))
                }
                _ => AuthError::SignatureInvalid,
            },
        )?;
        let claims = token_data.claims;

        // 5a. Clock-bound checks against the adapter's injected clock.
        let now = (self.inner.clock)();
        let leeway = self.inner.config.leeway();
        // exp: required.
        let exp_time = timestamp_to_systemtime(claims.exp)
            .map_err(|e| AuthError::Malformed(format!("exp: {e}")))?;
        if exp_time
            .checked_add(leeway)
            .map(|cutoff| cutoff <= now)
            .unwrap_or(true)
        {
            return Err(AuthError::Expired);
        }
        // nbf: optional but enforced when present.
        if let Some(nbf) = claims.nbf {
            let nbf_time = timestamp_to_systemtime(nbf)
                .map_err(|e| AuthError::Malformed(format!("nbf: {e}")))?;
            // Token rejected if `now < nbf - leeway`.
            let nbf_threshold = nbf_time
                .checked_sub(leeway)
                .unwrap_or(nbf_time);
            if now < nbf_threshold {
                return Err(AuthError::NotYetValid);
            }
        }

        // 6. Issuer check — exact match against allowlist (CT-safe).
        if !issuer_allowed(&claims.iss, self.inner.config.issuer_allowlist()) {
            return Err(AuthError::IssuerMismatch {
                got: claims.iss,
                expected: self.inner.config.issuer_allowlist().to_vec(),
            });
        }

        // 7. Audience check — exact match (CT-safe).
        if !claims.aud.matches(self.inner.config.audience()) {
            return Err(AuthError::AudienceMismatch);
        }

        // 8. Build principal.
        let issued_at = claims
            .iat
            .map(timestamp_to_systemtime)
            .transpose()
            .map_err(|e| AuthError::Malformed(format!("iat: {e}")))?
            .unwrap_or(now);
        let session_id = claims
            .sid
            .map(ClerkSessionId::new)
            .ok_or_else(|| AuthError::Malformed("missing sid claim".into()))?;
        let user_id = ClerkUserId::new(claims.sub);
        let org_id = claims.org_id.filter(|s| !s.is_empty()).map(ClerkOrgId::new);
        let email_raw = claims
            .email
            .ok_or_else(|| AuthError::Malformed("missing email claim".into()))?;
        let email = Email::parse(&email_raw)
            .map_err(|e| AuthError::Malformed(format!("email: {e}")))?;
        let role_claim = claims.clerk_role.or(claims.role);
        let clerk_role = ClerkRole::from_claim(role_claim.as_deref());

        Ok(ClerkPrincipal {
            user_id,
            org_id,
            email,
            clerk_role,
            session_id,
            issued_at,
            expires_at: exp_time,
        })
    }

    async fn load_cached(&self) -> Result<Option<CachedJwks>, AuthError> {
        let result = self
            .inner
            .cache
            .get(self.inner.config.instance_hash())
            .await
            .map_err(|e| AuthError::JwksFetchFailed(format!("cache get: {e}")))?;
        if let Some(ref cached) = result {
            let now = (self.inner.clock)();
            if is_fresh(cached, self.inner.config.jwks_ttl(), now) {
                self.inner
                    .counters
                    .cache_hits
                    .fetch_add(1, Ordering::SeqCst);
                return Ok(Some(cached.clone()));
            }
        }
        Ok(None)
    }

    async fn fetch_and_cache(&self, trigger: RefreshTrigger) -> Result<Jwks, AuthError> {
        let url = self.inner.config.jwks_url();
        let jwks = self
            .inner
            .fetcher
            .fetch(url)
            .await
            .map_err(|e| AuthError::JwksFetchFailed(e.to_string()))?;
        self.inner
            .cache
            .set(
                self.inner.config.instance_hash(),
                &jwks,
                self.inner.config.jwks_ttl(),
            )
            .await
            .map_err(|e| AuthError::JwksFetchFailed(format!("cache set: {e}")))?;
        match trigger {
            RefreshTrigger::Scheduled => {
                self.inner
                    .counters
                    .refresh_scheduled
                    .fetch_add(1, Ordering::SeqCst);
            }
            RefreshTrigger::KidMiss => {
                self.inner
                    .counters
                    .refresh_kid_miss
                    .fetch_add(1, Ordering::SeqCst);
            }
            RefreshTrigger::Manual => {
                self.inner
                    .counters
                    .refresh_manual
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
        tracing::debug!(target: "corelink::clerk", trigger = trigger.as_label(), "jwks refreshed");
        Ok(jwks)
    }
}

/// Constant-time issuer allowlist membership check.
fn issuer_allowed(got: &str, allowlist: &[String]) -> bool {
    let got_bytes = got.as_bytes();
    let mut hit = false;
    for candidate in allowlist {
        if candidate.len() == got_bytes.len()
            && candidate.as_bytes().ct_eq(got_bytes).into()
        {
            hit = true;
        }
    }
    hit
}

/// Convert a Unix-seconds timestamp into [`SystemTime`].
fn timestamp_to_systemtime(secs: i64) -> Result<SystemTime, &'static str> {
    if secs < 0 {
        return Err("negative unix seconds");
    }
    let secs_u = u64::try_from(secs).map_err(|_| "overflow")?;
    UNIX_EPOCH
        .checked_add(Duration::from_secs(secs_u))
        .ok_or("overflow")
}

/// Decode the raw header JSON into a `serde_json::Value` so we can
/// surface canonical errors for `alg=none` and friends BEFORE
/// jsonwebtoken's strongly-typed decoder rejects with a generic
/// JSON parse error.
fn decode_header_json(raw_jwt: &str) -> Result<serde_json::Value, AuthError> {
    let header_b64 = raw_jwt
        .split('.')
        .next()
        .ok_or_else(|| AuthError::Malformed("empty token".into()))?;
    if header_b64.is_empty() {
        return Err(AuthError::Malformed("empty header segment".into()));
    }
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|e| AuthError::Malformed(format!("header b64: {e}")))?;
    serde_json::from_slice::<serde_json::Value>(&header_bytes)
        .map_err(|e| AuthError::Malformed(format!("header json: {e}")))
}

/// Test-only helper: consult the `kid` header without touching JWKS.
/// Surfaces the same Malformed paths as [`ClerkAdapter::validate`] for
/// adversarial-regression coverage.
#[doc(hidden)]
pub fn extract_kid(raw_jwt: &str) -> Result<String, AuthError> {
    let header = decode_header(raw_jwt)
        .map_err(|e| AuthError::Malformed(format!("header decode: {e}")))?;
    header
        .kid
        .ok_or_else(|| AuthError::Malformed("missing kid header".into()))
}

/// Test-only helper: decode a JWT header's `alg` claim. Used by the
/// adversarial regression tests to assert that the decoder rejects
/// `alg=none` BEFORE any signature work.
#[doc(hidden)]
#[allow(
    clippy::missing_errors_doc,
    reason = "test-only doc-hidden helper; errors are AuthError::Malformed"
)]
pub fn decode_alg_for_test(raw_jwt: &str) -> Result<Algorithm, AuthError> {
    let header = decode_header(raw_jwt)
        .map_err(|e| AuthError::Malformed(format!("header decode: {e}")))?;
    Ok(header.alg)
}

/// Test-only helper for adversarial test: synthesise a JWT with an
/// arbitrary header + payload + signature blob, base64url-no-pad.
/// Public so the `tests/adversarial.rs` harness can craft pathological
/// shapes without re-implementing JWT base64 framing.
#[doc(hidden)]
#[must_use]
pub fn craft_unsigned_jwt(header_json: &str, payload_json: &str, signature: &[u8]) -> String {
    let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let s = URL_SAFE_NO_PAD.encode(signature);
    format!("{h}.{p}.{s}")
}

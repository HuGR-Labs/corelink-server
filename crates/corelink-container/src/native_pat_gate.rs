//! Native data-plane **PAT possession gate** (red-team finding #4, HIGH) —
//! defense-in-depth ON TOP of the existing HMAC gate.
//!
//! # Why this exists
//!
//! The native cache surfaces (CAS / AC / Bazel REAPI / Turbo) trust the
//! Worker-injected `x-corelink-tenant-id` header for isolation. The Worker
//! resolves that tenant from the PAT, but its hot path proves possession with
//! an **HMAC** check only (it skips the CPU-heavy Argon2id). That means a leaked
//! `PAT_SIGNING_KEY` is enough to **forge a valid-looking PAT for ANY tenant** —
//! the HMAC verifies, the Worker stamps the attacker-chosen tenant header, and
//! the native plane serves it. The random per-token secret (stored only as an
//! Argon2id hash in D1) is never checked on that path.
//!
//! [`NativePatGate`] closes the chain at the container: it re-runs the FULL
//! Option-B verification — the same [`crate::adapter_pat::PatVerifier`] the
//! cargo/brew/npm/oci/pip adapter routes use — which Argon2id-verifies the
//! random secret against the stored `pat_hash` AND confirms the PAT's D1
//! `tenant_id` matches the claimed tenant. A forged PAT (right HMAC, wrong/no
//! secret) fails Argon2id ⇒ 401; a real PAT for tenant A presented against
//! tenant B's path fails the tenant match ⇒ 401. The HMAC gate is NOT
//! removed — this is an additional, independent layer.
//!
//! # Hot-path cache
//!
//! Argon2id is deliberately expensive (~tens of ms). Running it on every
//! billable request would dominate latency, so verified tokens are cached
//! `(token-fingerprint) → (tenant, expiry)` for a short TTL — the same shape
//! the verification pipeline tolerates (a PAT's tenant binding is immutable for
//! its lifetime; revocation is bounded by the TTL). The cache key is a SHA-256
//! **fingerprint** of the bearer plaintext, never the plaintext itself, so the
//! in-memory map cannot leak a usable secret. A miss runs the full verifier; a
//! hit skips Argon2id.
//!
//! # Env gating (dev/CI = absent)
//!
//! [`native_pat_gate_from_env`] builds the gate from the SAME env the adapter
//! verifier uses ([`crate::adapter_pat::PatVerifier::from_env`] — `PAT_SIGNING_KEY`
//! + the D1 `StorageEnv`), returning `None` in dev/CI. Billable handlers hold an
//! `Option<Arc<NativePatGate>>`; `None` ⇒ the gate is skipped (mirroring the
//! `QuotaGate`).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sha2::{Digest as _, Sha256};

use crate::adapter_pat::{PatVerifier, VerifyError};

/// TTL of a cached verified-PAT entry. Bounds the window in which a revoked PAT
/// could still pass the native gate (the Worker + the adapter D1 lookup remain
/// the authoritative revocation surfaces); 60s keeps Argon2id off the hot path
/// for a build's burst of requests while staying short.
pub const VERIFY_CACHE_TTL: Duration = Duration::from_secs(60);

/// A cached, verified PAT: the resolved tenant + when the entry expires.
#[derive(Debug, Clone)]
struct CacheEntry {
    tenant: String,
    expires_at: Instant,
}

/// The native PAT possession gate.
///
/// Wraps the shared [`PatVerifier`] + a per-process verification cache. Clone is
/// cheap (two `Arc`s) so it drops into per-route state alongside the existing
/// gates.
#[derive(Clone)]
pub struct NativePatGate {
    verifier: Arc<PatVerifier>,
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl std::fmt::Debug for NativePatGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePatGate")
            .field("verifier", &self.verifier)
            .field("cache", &"Arc<Mutex<HashMap<.., ..>>>")
            .finish()
    }
}

impl NativePatGate {
    /// Construct from a shared [`PatVerifier`].
    #[must_use]
    pub fn new(verifier: Arc<PatVerifier>) -> Self {
        Self {
            verifier,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Verify that `bearer_or_token` is a genuine PAT (Argon2id possession) that
    /// resolves to `tenant`.
    ///
    /// `bearer_or_token` accepts either the raw PAT plaintext or an
    /// `Authorization: Bearer <pat>` value (the `Bearer ` prefix is stripped).
    ///
    /// Returns `Ok(())` when the PAT is genuine AND owns `tenant`. Returns
    /// `Err(response)`:
    /// - `401 Unauthorized` — no/forged/expired PAT, or the PAT belongs to a
    ///   DIFFERENT tenant than the one claimed (uniform — no oracle);
    /// - `503 Service Unavailable` — verifier backend (D1) fault (fail-CLOSED).
    ///
    /// # Errors
    ///
    /// Returns the rejection [`Response`] in the `Err` arm (so handlers can
    /// `return resp;` directly).
    pub async fn verify(&self, tenant: &str, bearer_or_token: &str) -> Result<(), Response> {
        let token = bearer_or_token
            .strip_prefix("Bearer ")
            .unwrap_or(bearer_or_token)
            .trim();
        if token.is_empty() {
            return Err(unauthorized());
        }

        let fp = fingerprint(token);

        // Cache hit (non-expired) → skip Argon2id. The cached tenant MUST still
        // match the claimed tenant (a cached PAT for tenant A presented against
        // tenant B's path is rejected the same as a miss would be).
        if let Some(cached_tenant) = self.cache_get(&fp) {
            return if cached_tenant == tenant {
                Ok(())
            } else {
                Err(unauthorized())
            };
        }

        // Miss → full verification (HMAC fast-reject → D1 lookup → Argon2id →
        // scope gate). On success cache `fp → (resolved_tenant, now + TTL)`.
        match self.verifier.verify(token).await {
            Ok(resolved_tenant) => {
                self.cache_put(fp, resolved_tenant.clone());
                if resolved_tenant == tenant {
                    Ok(())
                } else {
                    // Genuine PAT, but for a DIFFERENT tenant than claimed — a
                    // cross-tenant forgery attempt. Uniform 401.
                    Err(unauthorized())
                }
            }
            // Forged / unknown / expired / wrong-secret / no-cache-scope.
            Err(VerifyError::InvalidPat) => Err(unauthorized()),
            // D1 / backend fault — fail-CLOSED 503 (do not serve a billable op
            // we could not possession-check).
            Err(VerifyError::Backend(m)) => {
                tracing::error!(error = %m, "native PAT gate: verifier backend error");
                Err((StatusCode::SERVICE_UNAVAILABLE, "PAT verifier backend error").into_response())
            }
        }
    }

    /// Read a non-expired cache entry's tenant, evicting it if expired.
    fn cache_get(&self, fp: &str) -> Option<String> {
        let mut cache = self.cache.lock().ok()?;
        match cache.get(fp) {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.tenant.clone()),
            Some(_) => {
                // Expired — evict so the map cannot grow unbounded with stale
                // entries for a recurring token.
                let _ = cache.remove(fp);
                None
            }
            None => None,
        }
    }

    /// Insert a verified entry with a fresh TTL.
    fn cache_put(&self, fp: String, tenant: String) {
        if let Ok(mut cache) = self.cache.lock() {
            let _ = cache.insert(
                fp,
                CacheEntry {
                    tenant,
                    expires_at: Instant::now() + VERIFY_CACHE_TTL,
                },
            );
        }
    }
}

/// Uniform 401 for any PAT-rejection condition (forged, wrong tenant, expired,
/// missing) — no oracle that distinguishes them on the wire.
fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "invalid PAT").into_response()
}

/// SHA-256 hex fingerprint of the bearer plaintext, used as the cache key so the
/// in-memory map never holds the secret itself.
fn fingerprint(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Build the production gate from process env, or `None` in dev/CI (no
/// `PAT_SIGNING_KEY` / D1 storage env). Mirrors
/// [`crate::adapter_pat::PatVerifier::from_env`]'s fail-CLOSED env-gate — when
/// the verifier can't build, the native gate is simply not wired.
#[must_use]
pub fn native_pat_gate_from_env() -> Option<Arc<NativePatGate>> {
    let verifier = PatVerifier::from_env()?;
    Some(Arc::new(NativePatGate::new(Arc::new(verifier))))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
pub(crate) mod testing {
    //! Test constructor + fakes so route-integration tests can wire a hermetic
    //! gate without `PAT_SIGNING_KEY` / D1.
    use std::sync::Arc;

    use async_trait::async_trait;
    use corelink_pat::PatSigningKey;

    use super::NativePatGate;
    use crate::adapter_pat::{PatRow, PatRowLookup, PatVerifier};

    impl NativePatGate {
        /// Build a gate over an explicit [`PatVerifier`] (tests inject a fake
        /// D1 row source + a known signing key).
        #[must_use]
        pub fn new_for_test(verifier: Arc<PatVerifier>) -> Self {
            Self::new(verifier)
        }
    }

    /// Single-row fake `pat` lookup for the gate tests.
    #[derive(Debug)]
    pub struct OneRowLookup {
        token_id: String,
        row: PatRow,
    }

    impl OneRowLookup {
        /// Construct a lookup that returns `row` for exactly `token_id`.
        #[must_use]
        pub fn new(token_id: String, row: PatRow) -> Self {
            Self { token_id, row }
        }
    }

    #[async_trait]
    impl PatRowLookup for OneRowLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            if token_id == self.token_id {
                Ok(Some(self.row.clone()))
            } else {
                Ok(None)
            }
        }
    }

    /// Build a [`PatVerifier`] over a single known PAT row + signing key.
    #[must_use]
    pub fn verifier_with_row(
        token_id: String,
        row: PatRow,
        key: Arc<PatSigningKey>,
    ) -> Arc<PatVerifier> {
        Arc::new(PatVerifier::new(Arc::new(OneRowLookup::new(token_id, row)), key))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::sync::Arc;

    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    use super::testing::verifier_with_row;
    use super::*;
    use crate::adapter_pat::PatRow;

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Mint a real PAT for `tenant_u128`; return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(key: &PatSigningKey, tenant_u128: u128) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant_u128));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant_u128 + 1)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    fn row(pat_hash: &str, tenant: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: "cas:rw".to_owned(),
        }
    }

    #[tokio::test]
    async fn genuine_pat_for_its_own_tenant_is_accepted() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 7);
        let verifier = verifier_with_row(tid, row(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        assert!(gate.verify(&tenant, &pt).await.is_ok());
        // `Bearer ` prefix is stripped too.
        assert!(gate.verify(&tenant, &format!("Bearer {pt}")).await.is_ok());
    }

    /// The KILLING finding-#4 test: a forged token (HMAC-only attacker without
    /// the real random secret) does NOT pass the Argon2id possession check.
    /// Here the stored hash is for a DIFFERENT PAT, so Argon2id fails ⇒ 401.
    #[tokio::test]
    async fn forged_token_rejected_401() {
        let key = test_key();
        // The PAT the attacker presents…
        let (pt, tid, _hash, tenant) = mint_pat(&key, 8);
        // …but D1 stores the hash of a DIFFERENT secret for that token_id, so the
        // Argon2id possession check fails (models a forged/leaked-HMAC token
        // whose random secret does not match the stored hash).
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 9);
        let verifier = verifier_with_row(tid, row(&other_hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        let err = gate.verify(&tenant, &pt).await.unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    /// A genuine PAT for tenant A presented against tenant B's path is rejected
    /// 401 — the gate binds possession to the CLAIMED tenant (cross-tenant
    /// forgery defense).
    #[tokio::test]
    async fn genuine_pat_for_wrong_tenant_rejected_401() {
        let key = test_key();
        let (pt, tid, hash, tenant_a) = mint_pat(&key, 10);
        let verifier = verifier_with_row(tid, row(&hash, &tenant_a), key);
        let gate = NativePatGate::new_for_test(verifier);
        let err = gate
            .verify("some-other-tenant", &pt)
            .await
            .unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_or_missing_token_rejected_401() {
        let key = test_key();
        let (_pt, tid, hash, tenant) = mint_pat(&key, 11);
        let verifier = verifier_with_row(tid, row(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        assert_eq!(
            gate.verify(&tenant, "").await.unwrap_err().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            gate.verify(&tenant, "Bearer ").await.unwrap_err().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn second_verify_is_served_from_cache() {
        // After one full verify, the entry is cached `fp → tenant`; a second
        // call for the same token + tenant succeeds without re-hitting the
        // verifier. We assert the cache holds the fingerprint.
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 12);
        let verifier = verifier_with_row(tid, row(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        assert!(gate.verify(&tenant, &pt).await.is_ok());
        let fp = fingerprint(&pt);
        assert!(
            gate.cache_get(&fp).is_some(),
            "a verified PAT must be cached so the hot path skips Argon2id"
        );
        // A cached entry for tenant A must still be rejected when presented for
        // tenant B (the cache is not a tenant-blind bypass).
        assert_eq!(
            gate.verify("other", &pt).await.unwrap_err().status(),
            StatusCode::UNAUTHORIZED
        );
        // Same token + correct tenant still passes (cache hit).
        assert!(gate.verify(&tenant, &pt).await.is_ok());
    }

    #[test]
    fn fingerprint_is_not_the_plaintext() {
        // The cache key must never be the secret itself.
        let token = "corelink_pat_super-secret-value";
        let fp = fingerprint(token);
        assert_ne!(fp, token);
        assert_eq!(fp.len(), 64, "sha-256 hex is 64 chars");
        assert!(!fp.contains("secret"));
    }
}

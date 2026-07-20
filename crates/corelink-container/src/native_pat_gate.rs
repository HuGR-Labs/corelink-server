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

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash as _, Hasher as _};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sha2::{Digest as _, Sha256};

use crate::adapter_pat::{PatVerifier, VerifyError};

/// TTL of a cached verified-PAT entry. Bounds the window in which a revoked PAT
/// could still pass the native gate (the Worker + the adapter D1 lookup remain
/// the authoritative revocation surfaces).
///
/// rt-nuclear verify C3: a cache hit returns `Ok` WITHOUT re-consulting D1, so a
/// PAT revoked mid-TTL keeps native CAS/AC/Bazel/Turbo access until the entry
/// expires. The previous 60s window was too long for a leaked-then-revoked PAT;
/// 5s bounds revocation latency on the native plane to ≤5s while still skipping
/// Argon2id for a build's burst. The extra re-verifies are bounded by the
/// process-wide Argon2id semaphore (`adapter_pat::ARGON2_VERIFY_PERMITS`), and a
/// cache hit adds NO D1 round-trip to the hot path (deliberate, for a latency-
/// sensitive cache — immediate revocation would require a per-request D1 read or
/// a cross-process revocation epoch; the tight TTL is the correct launch control).
pub const VERIFY_CACHE_TTL: Duration = Duration::from_secs(5);

/// Number of single-flight shards (see [`NativePatGate::verify_locks`]). A FIXED,
/// memory-bounded array (same rationale as `byte_accounting::CAS_LOCK_SHARDS`):
/// no per-fingerprint map that grows with the live token set. 256 keeps
/// cross-fingerprint false-sharing negligible for any realistic per-container
/// concurrency.
const VERIFY_LOCK_SHARDS: usize = 256;

/// A cached, verified PAT: the resolved tenant + when the entry expires.
#[derive(Debug, Clone)]
struct CacheEntry {
    tenant: String,
    /// The PAT's D1-derived write capability — cached alongside the tenant so a
    /// write gate can enforce it without a fresh Argon2id per write.
    can_write: bool,
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
    /// Single-flight shards: concurrent cache-MISSES for the SAME token
    /// fingerprint coalesce onto ONE verification (the rest await the shard,
    /// then read the now-populated cache). Without this, a concurrent burst of
    /// the same PAT (e.g. parallel CAS writes from one CI job) all miss the
    /// cache simultaneously and stampede the verifier's D1 lookup / Argon2id
    /// semaphore — which surfaces as 503 "PAT verifier backend error" on some of
    /// them (the same-key concurrent-write 503 the e2e journey suite caught).
    verify_locks: Arc<Vec<Arc<tokio::sync::Mutex<()>>>>,
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
        let verify_locks = (0..VERIFY_LOCK_SHARDS)
            .map(|_| Arc::new(tokio::sync::Mutex::new(())))
            .collect::<Vec<_>>();
        Self {
            verifier,
            cache: Arc::new(Mutex::new(HashMap::new())),
            verify_locks: Arc::new(verify_locks),
        }
    }

    /// The single-flight shard a fingerprint maps to (deterministic; same fp →
    /// same shard, which is the property coalescing requires).
    fn lock_for_fp(&self, fp: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut hasher = DefaultHasher::new();
        fp.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % self.verify_locks.len();
        self.verify_locks
            .get(idx)
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())))
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
        self.verify_inner(tenant, bearer_or_token, false).await
    }

    /// Like [`Self::verify`], but ALSO requires the PAT's D1-derived `can_write`
    /// capability. Use this on WRITE handlers so a read-only (`cas:r`) PAT cannot
    /// write even if the Worker-set `x-corelink-scope` header claimed otherwise —
    /// the two-layer enforcement cargo/OCI already do (deep-audit B/F-1). A genuine
    /// PAT that lacks write capability is rejected `403` (`insufficient scope`),
    /// distinct from the `401` a forged/cross-tenant PAT gets.
    ///
    /// # Errors
    ///
    /// Returns the rejection [`Response`] in the `Err` arm (401 / 403 / 503).
    pub async fn verify_write(&self, tenant: &str, bearer_or_token: &str) -> Result<(), Response> {
        self.verify_inner(tenant, bearer_or_token, true).await
    }

    /// Turn a resolved `(tenant, can_write)` into the gate decision. Tenant
    /// mismatch → uniform 401 (no cross-tenant oracle); a genuine PAT lacking the
    /// required write capability → 403.
    // The gate returns the rejection `Response` in the Err arm by design (so
    // handlers `return resp;` directly) — the same shape the pub `verify`/
    // `verify_write` API uses; boxing it here would just churn the hot path.
    #[allow(clippy::result_large_err)]
    fn decide(
        resolved_tenant: &str,
        can_write: bool,
        claimed_tenant: &str,
        require_write: bool,
    ) -> Result<(), Response> {
        if resolved_tenant != claimed_tenant {
            return Err(unauthorized());
        }
        if require_write && !can_write {
            return Err((StatusCode::FORBIDDEN, "insufficient scope").into_response());
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    async fn verify_inner(
        &self,
        tenant: &str,
        bearer_or_token: &str,
        require_write: bool,
    ) -> Result<(), Response> {
        let token = bearer_or_token
            .strip_prefix("Bearer ")
            .unwrap_or(bearer_or_token)
            .trim();
        if token.is_empty() {
            return Err(unauthorized());
        }

        let fp = fingerprint(token);

        // Cache hit (non-expired) → skip Argon2id. The cached tenant MUST still
        // match the claimed tenant, and (for a write gate) the cached `can_write`
        // bit is enforced — a cached read-only PAT presented on a write path is
        // rejected the same as a miss would be.
        if let Some((cached_tenant, cached_write)) = self.cache_get(&fp) {
            return Self::decide(&cached_tenant, cached_write, tenant, require_write);
        }

        // Miss → SINGLE-FLIGHT the full verification. Acquire the fingerprint's
        // shard so a concurrent burst of the same PAT runs ONE verify (D1 +
        // Argon2id), not N — a stampede that errored the verifier backend and
        // surfaced as 503 on some requests (the same-key concurrent-write 503).
        let fp_lock = self.lock_for_fp(&fp);
        let _flight = fp_lock.lock().await;
        // Double-checked: the holder we waited behind may have just populated the
        // cache — take the fast path and skip a redundant D1/Argon2id round.
        if let Some((cached_tenant, cached_write)) = self.cache_get(&fp) {
            return Self::decide(&cached_tenant, cached_write, tenant, require_write);
        }

        // HMAC fast-reject → D1 lookup → Argon2id → scope gate. `verify_capability`
        // is the SAME work as `verify` plus the D1-derived `can_write` bit
        // (`verify` is literally `verify_capability(..).map(|(t, _)| t)`), so
        // routing reads through it is behaviour-identical. Cache
        // `fp → (resolved_tenant, can_write, now + TTL)`.
        match self.verifier.verify_capability(token).await {
            Ok((resolved_tenant, can_write)) => {
                self.cache_put(fp, resolved_tenant.clone(), can_write);
                Self::decide(&resolved_tenant, can_write, tenant, require_write)
            }
            // Forged / unknown / expired / wrong-secret / no-cache-scope.
            Err(VerifyError::InvalidPat) => Err(unauthorized()),
            // D1 / backend fault — fail-CLOSED 503 (do not serve a billable op
            // we could not possession-check).
            Err(VerifyError::Backend(m)) => {
                tracing::error!(error = %m, "native PAT gate: verifier backend error");
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PAT verifier backend error",
                )
                    .into_response())
            }
        }
    }

    /// Read a non-expired cache entry's tenant, evicting it if expired.
    fn cache_get(&self, fp: &str) -> Option<(String, bool)> {
        let mut cache = self.cache.lock().ok()?;
        match cache.get(fp) {
            Some(entry) if entry.expires_at > Instant::now() => {
                Some((entry.tenant.clone(), entry.can_write))
            }
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
    fn cache_put(&self, fp: String, tenant: String, can_write: bool) {
        if let Ok(mut cache) = self.cache.lock() {
            let _ = cache.insert(
                fp,
                CacheEntry {
                    tenant,
                    can_write,
                    expires_at: Instant::now() + VERIFY_CACHE_TTL,
                },
            );
        }
    }

    /// Test-only: read a cached entry's `expires_at`, for asserting the TTL
    /// window does not slide on repeated cache hits (WP-D M26 no-slide test —
    /// the window is anchored at population, not last use).
    #[cfg(test)]
    fn cache_expires_at_for_test(&self, fp: &str) -> Option<Instant> {
        self.cache
            .lock()
            .ok()?
            .get(fp)
            .map(|entry| entry.expires_at)
    }

    /// Test-only: force the cached entry for `fp` to already be expired, so
    /// the NEXT verify takes the cache-MISS path and re-consults the
    /// verifier (WP-D M26 bound regression test — models the 5s TTL
    /// deterministically, without a real-time sleep).
    #[cfg(test)]
    fn expire_cache_entry_for_test(&self, fp: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(entry) = cache.get_mut(fp) {
                entry.expires_at = Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .unwrap_or_else(Instant::now);
            }
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
///
/// # PROD safety (red-team finding #7, HIGH)
///
/// A `None` here is benign in dev/CI but a SILENT SECURITY DOWNGRADE in prod:
/// the native planes would mount WITHOUT this Argon2id backstop. The most
/// likely prod cause is a PRESENT-but-malformed `PAT_SIGNING_KEY` rotation
/// sibling, which [`crate::adapter_pat::PatVerifier::from_env`] correctly
/// fails-CLOSED on. The container's boot path (`main.rs`) therefore treats a
/// `None` from this builder as a FATAL boot condition WHEN prod is detected
/// (D1 + `PAT_SIGNING_KEY` present) via `should_fatal_on_missing_gate` — it
/// refuses to boot rather than serve degraded. The teeth live in `main.rs`;
/// this builder keeps its dev/CI-friendly `Option` shape so route wiring is
/// unchanged.
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
        Arc::new(PatVerifier::new(
            Arc::new(OneRowLookup::new(token_id, row)),
            key,
        ))
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    use super::testing::verifier_with_row;
    use super::*;
    use crate::adapter_pat::{PatRow, PatRowLookup, PatVerifier};

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
            find_only: false,
        }
    }

    /// A READ-ONLY (`cas:r`) PAT row — `requires_cache_write` is false, so its
    /// D1-derived `can_write` bit is false.
    fn row_ro(pat_hash: &str, tenant: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: "cas:r".to_owned(),
            find_only: false,
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

    /// B/F-1: `verify_write` accepts a write-capable (`cas:rw`) PAT.
    #[tokio::test]
    async fn verify_write_accepts_write_capable_pat() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 20);
        let verifier = verifier_with_row(tid, row(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        assert!(gate.verify_write(&tenant, &pt).await.is_ok());
    }

    /// B/F-1 (the fix): a READ-ONLY PAT can READ (`verify` ok) but is rejected
    /// `403` on a WRITE gate (`verify_write`) — the PAT-derived `can_write` bit is
    /// enforced independently of the Worker scope header, so a `cas:r` token can no
    /// longer write via Bazel/Turbo even if the header were wrong.
    #[tokio::test]
    async fn verify_write_rejects_read_only_pat_403() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 21);
        let verifier = verifier_with_row(tid, row_ro(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);
        // Read is fine.
        assert!(gate.verify(&tenant, &pt).await.is_ok());
        // Write is refused with 403 (insufficient scope), NOT 401.
        let err = gate.verify_write(&tenant, &pt).await.unwrap_err();
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
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
        let err = gate.verify("some-other-tenant", &pt).await.unwrap_err();
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

    /// A one-row lookup that can be flipped to simulate a D1-side revoke
    /// mid-test. A real revoke UPDATEs `revoked_at_ms` and the verifier's
    /// SQL filters `AND revoked_at_ms IS NULL`
    /// (`crate::adapter_pat::PAT_LOOKUP_SQL`), so a revoked row surfaces to
    /// the verifier as `Ok(None)` exactly like an unknown token_id — this
    /// fake models that by returning `None` once `revoke()` is called.
    #[derive(Debug)]
    struct ToggleableLookup {
        token_id: String,
        row: PatRow,
        revoked: AtomicBool,
    }

    impl ToggleableLookup {
        fn new(token_id: String, row: PatRow) -> Self {
            Self {
                token_id,
                row,
                revoked: AtomicBool::new(false),
            }
        }

        /// Flip this token_id's row to look revoked on the NEXT lookup.
        fn revoke(&self) {
            self.revoked.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl PatRowLookup for ToggleableLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            if token_id == self.token_id && !self.revoked.load(Ordering::SeqCst) {
                Ok(Some(self.row.clone()))
            } else {
                Ok(None)
            }
        }
    }

    /// WP-D M26 — the BOUND regression test for INV-PAT-REVOKE-PROPAGATION's
    /// documented container native-plane carve-out: a cache hit skips D1 (by
    /// design), so revocation on the native plane is bounded by the cache
    /// TTL, not immediate. This proves the OTHER half of that bound: once the
    /// cached entry's TTL has genuinely elapsed, the NEXT verify re-consults
    /// the verifier (no infinite/silent staleness) and a revoked-in-the-
    /// interim PAT is rejected 401 — the gate re-verifies rather than
    /// re-using stale trust past its own expiry.
    #[tokio::test]
    async fn cache_expiry_forces_reverify_and_rejects_revoked_pat() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 30);
        let lookup: Arc<ToggleableLookup> =
            Arc::new(ToggleableLookup::new(tid, row(&hash, &tenant)));
        let lookup_dyn: Arc<dyn PatRowLookup> = lookup.clone();
        let verifier = Arc::new(PatVerifier::new(lookup_dyn, key));
        let gate = NativePatGate::new_for_test(verifier);

        // 1. First verify: cache MISS, full pipeline runs, genuine PAT ⇒ Ok,
        //    and the entry is now cached.
        assert!(gate.verify(&tenant, &pt).await.is_ok());
        let fp = fingerprint(&pt);
        assert!(
            gate.cache_get(&fp).is_some(),
            "verify must populate the cache on a genuine PAT"
        );

        // 2. Revoke at the (fake) D1 layer. Because the cache entry is still
        //    within its TTL, the NEXT verify is served from cache and does
        //    NOT observe the revoke yet — this is the documented ≤5s bounded-
        //    stale window, not a bug.
        lookup.revoke();
        assert!(
            gate.verify(&tenant, &pt).await.is_ok(),
            "within the TTL, a cache hit must NOT re-consult D1 (bounded-stale by design)"
        );

        // 3. Force the cache TTL to have elapsed (deterministic — no
        //    real-time sleep). The NEXT verify must now MISS the cache, run
        //    the full pipeline again, observe the revoke, and reject 401.
        gate.expire_cache_entry_for_test(&fp);
        let err = gate.verify(&tenant, &pt).await.unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::UNAUTHORIZED,
            "once the cache TTL has elapsed, a revoked PAT must be rejected on re-verify"
        );
    }

    /// WP-D M26 — the NO-SLIDE regression test: the verify-cache window is
    /// anchored at POPULATION time, not at last use. Repeated cache hits for
    /// the same PAT must NOT push `expires_at` further into the future —
    /// otherwise a continuously-polled revoked PAT could stay valid
    /// indefinitely instead of being bounded by a fixed 5s window from the
    /// moment it was cached.
    #[tokio::test]
    async fn cache_hit_does_not_extend_expiry() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 31);
        let verifier = verifier_with_row(tid, row(&hash, &tenant), key);
        let gate = NativePatGate::new_for_test(verifier);

        // Populate the cache.
        assert!(gate.verify(&tenant, &pt).await.is_ok());
        let fp = fingerprint(&pt);
        let expires_at_after_populate = gate
            .cache_expires_at_for_test(&fp)
            .expect("entry must be cached after a genuine verify");

        // N more hits for the SAME token — all should be served from cache
        // (no re-verify needed) and none should slide the expiry.
        for _ in 0..5 {
            assert!(gate.verify(&tenant, &pt).await.is_ok());
        }

        let expires_at_after_hits = gate
            .cache_expires_at_for_test(&fp)
            .expect("entry must still be cached");
        assert_eq!(
            expires_at_after_populate, expires_at_after_hits,
            "repeated cache hits must not slide expires_at — the window is fixed from population"
        );
    }
}

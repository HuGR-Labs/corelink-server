//! Shared container-side PAT verifier for the cache adapters
//! (cargo / brew / npm / oci / pip) — Option B (defense-in-depth).
//!
//! # Why this exists
//!
//! Each `corelink_adapter_host` adapter is designed to **self-validate**
//! the bearer PAT inside the container rather than trusting the
//! Worker-injected `x-corelink-tenant-id`. The owner chose Option B
//! (2026-06-05, FINDING-sccache-adapter-gaps §Gap 2): the container
//! re-verifies the PAT against the D1 `pat` store. The Worker already
//! validates the PAT (HMAC fast-fail + D1 lookup) under its cpu_ms
//! budget; this verifier repeats the **full** verification — including
//! the Argon2id possession check the Worker skips — so a compromised or
//! misconfigured Worker cannot grant cache access on its own.
//!
//! Each adapter declares its OWN (nominally distinct) `TenantResolver`
//! port trait, so this module exposes a trait-agnostic [`PatVerifier`]
//! and each adapter route module wraps it in a thin newtype shell that
//! impls that adapter's `TenantResolver` (see `routes/<adapter>.rs`).
//! The verification pipeline lives here, once.
//!
//! # Verification pipeline (mirrors `auth_model.md §2.3`)
//!
//! 1. **HMAC fast-reject** ([`verify_hmac_only`]) — parse the plaintext
//!    and reject a bad signature in ≤100µs, *before* any D1 round-trip,
//!    so forged tokens cannot drive D1 query cost.
//! 2. **D1 lookup** by the non-secret `token_id` (expiry filtered in SQL).
//! 3. **Full verify** ([`verify_with_hash`]) — constant-time `token_id`
//!    match + HMAC + Argon2id of the secret segment against the stored
//!    PHC hash. Run on a blocking thread (Argon2id is CPU-heavy).
//! 4. **Scope gate** — fail-CLOSED unless the D1 `scope` string grants a
//!    cache capability. The port carries no operation, so this asserts
//!    only "has SOME cache capability"; per-operation read/write is
//!    enforced one layer up at each adapter route from the Worker-set
//!    `x-corelink-scope` header.
//!
//! Every distinguishable failure (bad parse, bad sig, unknown token,
//! expired, wrong secret, no cache scope) collapses to
//! [`VerifyError::InvalidPat`] so the wire surface cannot tell an
//! attacker *why* a token was rejected. Only genuine backend faults (D1
//! unreachable, corrupt row) surface as [`VerifyError::Backend`].

use std::sync::Arc;

use async_trait::async_trait;
use corelink_pat::{verify_hmac_only_multi, verify_with_hash_multi, PatHash, PatSigningKey};

use crate::scope::{requires_cache_read, requires_cache_write};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::{non_empty_env, StorageEnv};

/// Failure surface of [`PatVerifier::verify`].
///
/// Adapter route shells map this onto their adapter's local
/// `TenantResolveError` (`InvalidPat` ⇒ 401, `Backend` ⇒ 503).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// PAT not found, expired, forged, wrong secret, or lacking a cache
    /// scope. Uniform by design (no oracle). Surfaces as HTTP 401.
    #[error("invalid PAT")]
    InvalidPat,
    /// Verifier backend (D1, corrupt row) failed. Surfaces as HTTP 503.
    #[error("verifier backend: {0}")]
    Backend(String),
}

/// A single `pat` row, reduced to the fields PAT verification needs.
///
/// `token_id` is the non-secret lookup key; the secret material is the
/// caller-supplied plaintext, verified against `pat_hash`.
#[derive(Debug, Clone)]
pub struct PatRow {
    /// Tenant UUID (text) that owns the PAT.
    pub tenant_id: String,
    /// Argon2id PHC hash string from `pat.pat_hash`.
    pub pat_hash: String,
    /// The PAT's D1 `scope` string (e.g. `cas:rw`, `admin`, or `""` for
    /// legacy rows). Empty / absent ⇒ fail-CLOSED at the scope gate.
    pub scope: String,
}

/// Fetch a `pat` row by its non-secret `token_id`, already expiry-filtered.
///
/// Abstracted as a trait so the security-critical verification pipeline in
/// [`PatVerifier`] can be unit-tested hermetically with real crypto and a
/// fake row source — the production impl talks to D1 over HTTP, which a
/// unit test cannot reach.
#[async_trait]
pub trait PatRowLookup: Send + Sync {
    /// Return the row for `token_id`, or `None` when no live (non-expired,
    /// non-revoked) row exists. `Err` is reserved for backend faults.
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String>;
}

/// The D1 `pat` lookup SQL for the container verifier.
///
/// Mirrors the Worker hot-path (migration `0054_pat_token_id`): an `O(1)`
/// covering-index lookup on `token_id`, with the same SQL-side expiry
/// filter (`expires_ms = 0` ⇒ no-expiry token) and the same soft-revocation
/// filter (migration `0063_pat_customer_keys`: `revoked_at_ms IS NULL` ⇒
/// active; a revoked row is indistinguishable from an absent one).
const PAT_LOOKUP_SQL: &str = "SELECT tenant_id, pat_hash, scope FROM pat \
     WHERE token_id = ?1 \
       AND (expires_ms = 0 OR expires_ms > unixepoch('now', 'subsec') * 1000) \
       AND revoked_at_ms IS NULL \
     LIMIT 1";

/// Production [`PatRowLookup`] over the CF D1 HTTP API.
#[async_trait]
impl PatRowLookup for D1HttpClient {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        let rows = self
            .query(
                PAT_LOOKUP_SQL,
                &[serde_json::Value::String(token_id.to_owned())],
            )
            .await?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        let tenant_id = row
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or("D1 pat: missing `tenant_id` column")?
            .to_owned();
        let pat_hash = row
            .get("pat_hash")
            .and_then(|v| v.as_str())
            .ok_or("D1 pat: missing `pat_hash` column")?
            .to_owned();
        // `scope` may be NULL on legacy rows; map that to "" (fail-CLOSED
        // at the scope gate) rather than a backend error.
        let scope = row
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        Ok(Some(PatRow {
            tenant_id,
            pat_hash,
            scope,
        }))
    }
}

/// Container-side PAT → tenant verifier (Option B). Trait-agnostic: each
/// adapter route module wraps an `Arc<PatVerifier>` in a thin newtype
/// that impls that adapter's `TenantResolver` port.
pub struct PatVerifier {
    lookup: Arc<dyn PatRowLookup>,
    /// The HMAC overlap key set: `PAT_SIGNING_KEY` (current) plus any
    /// `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW` rotation siblings.
    /// A PAT minted under any key in the set validates during the
    /// rotation overlap window (`key_management.md §3.2.1`), so rotating
    /// the current key on compromise does NOT instantly invalidate the
    /// live fleet. Always non-empty by construction (fail-closed
    /// otherwise — see [`PatVerifier::new`] / [`PatVerifier::from_env`]).
    signing_keys: Arc<Vec<PatSigningKey>>,
}

impl std::fmt::Debug for PatVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatVerifier")
            .field("lookup", &"Arc<dyn PatRowLookup>")
            .field("signing_keys", &format_args!("[{} REDACTED]", self.signing_keys.len()))
            .finish()
    }
}

impl PatVerifier {
    /// Construct from an explicit row source + a single signing key
    /// (used by the production wiring and by tests with a fake lookup).
    #[must_use]
    pub fn new(lookup: Arc<dyn PatRowLookup>, signing_key: Arc<PatSigningKey>) -> Self {
        Self::with_key_set(lookup, vec![(*signing_key).clone()])
    }

    /// Construct from an explicit row source + an HMAC overlap key set
    /// (current + optional rotation predecessor/successor). The order is
    /// irrelevant (verify is constant-time over the whole set). An empty
    /// set fails CLOSED — every verify returns `InvalidPat`.
    #[must_use]
    pub fn with_key_set(lookup: Arc<dyn PatRowLookup>, signing_keys: Vec<PatSigningKey>) -> Self {
        Self {
            lookup,
            signing_keys: Arc::new(signing_keys),
        }
    }

    /// Build the production verifier from process env: a D1 HTTP client
    /// (from [`StorageEnv`]) plus the HMAC overlap key set —
    /// `PAT_SIGNING_KEY` (required, current) and the optional rotation
    /// siblings `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW`.
    /// Returns `None` when the D1 client or the *current* key is
    /// missing/invalid, so the caller can fail-CLOSED (not mount the
    /// adapter route) in dev/CI — mirroring
    /// `internal_pat::build_state_from_env`.
    ///
    /// An optional sibling that is *present but malformed* (bad hex /
    /// too short) fails CLOSED too: the whole verifier is refused rather
    /// than silently dropping a key the operator believes is live. An
    /// *absent* sibling is simply omitted from the set.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let storage_env = StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "adapter PAT verifier: D1 client init failed"))
            .ok()?;

        // Current key is REQUIRED.
        let current = Self::decode_key_env("PAT_SIGNING_KEY")??;

        let mut signing_keys = vec![current];

        // Optional rotation overlap siblings. `None` ⇒ absent (skip);
        // `Some(None)` ⇒ present-but-malformed (fail CLOSED).
        for name in ["PAT_SIGNING_KEY_PREV", "PAT_SIGNING_KEY_NEW"] {
            match Self::decode_key_env(name) {
                None => {} // absent — not part of the overlap set
                Some(Some(key)) => signing_keys.push(key),
                Some(None) => return None, // present but invalid — refuse to mount
            }
        }

        Some(Self::with_key_set(Arc::new(d1), signing_keys))
    }

    /// Decode one hex `PatSigningKey` from the named env var.
    ///
    /// - `None` — the var is absent / empty (caller decides whether that
    ///   is fatal: required for the current key, fine for siblings).
    /// - `Some(None)` — the var is set but malformed (bad hex or < 32
    ///   bytes); the caller treats this as fail-CLOSED.
    /// - `Some(Some(key))` — a valid key.
    fn decode_key_env(name: &str) -> Option<Option<PatSigningKey>> {
        let raw = non_empty_env(name)?;
        let Ok(key_bytes) = hex::decode(raw.trim()) else {
            tracing::warn!("{name} is not valid hex; adapter routes NOT mounted");
            return Some(None);
        };
        match PatSigningKey::from_bytes(key_bytes) {
            Ok(key) => Some(Some(key)),
            Err(e) => {
                tracing::warn!(error = %e, "{name} invalid; adapter routes NOT mounted");
                Some(None)
            }
        }
    }

    /// The full Option-B verification pipeline, returning the PAT's owning
    /// tenant id **and** whether it carries cache WRITE capability.
    ///
    /// Callers that mint a downstream credential FROM the PAT (the OCI
    /// `/token` Basic→Bearer exchange) use the write bit to downscope the
    /// grant to the PAT's real rights — otherwise a read-only (`cas:r`)
    /// PAT could obtain a `push` registry token. The simpler [`Self::verify`]
    /// discards the bit (per-op write enforcement for the header-scoped
    /// adapters stays at the route from `x-corelink-scope`).
    pub async fn verify_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<(String, bool), VerifyError> {
        // 1. HMAC fast-reject (pre-D1). A forged token is rejected here
        //    without a D1 round-trip. Uniform InvalidPat. Checked against
        //    the overlap key set so a token minted under the rotation
        //    predecessor/successor still fast-passes here.
        let (_env, token_id) = verify_hmac_only_multi(pat_plaintext, &self.signing_keys)
            .map_err(|_| VerifyError::InvalidPat)?;

        // 2. D1 lookup by the non-secret token_id (expiry filtered in SQL).
        let row = match self
            .lookup
            .lookup(token_id.as_str())
            .await
            .map_err(VerifyError::Backend)?
        {
            Some(row) => row,
            // Unknown / expired / revoked. Burn the SAME Argon2id cost as
            // the hot path before returning so response latency does not
            // leak whether the token_id exists (token-enumeration oracle
            // defence). Errors here are ignored: it is timing padding, not
            // an auth decision.
            None => {
                let plaintext = pat_plaintext.to_owned();
                let _ = tokio::task::spawn_blocking(move || {
                    corelink_pat::dummy_verify_for_constant_time(&plaintext)
                })
                .await;
                return Err(VerifyError::InvalidPat);
            }
        };

        // 3. Full crypto verify on a blocking thread (Argon2id is
        //    CPU-heavy and must not stall the async worker). Re-parses,
        //    constant-time matches token_id, re-checks HMAC, then
        //    Argon2id-verifies the secret segment against the stored hash.
        let plaintext = pat_plaintext.to_owned();
        let signing_keys = Arc::clone(&self.signing_keys);
        let stored_hash = PatHash::from_phc_string(row.pat_hash);
        let verify_result = tokio::task::spawn_blocking(move || {
            verify_with_hash_multi(&plaintext, &token_id, &stored_hash, &signing_keys)
        })
        .await
        .map_err(|e| VerifyError::Backend(format!("verify join: {e}")))?;
        verify_result.map_err(|_| VerifyError::InvalidPat)?;

        // 4. Scope gate — fail-CLOSED on NO cache capability at all, then
        //    surface the read/write split so credential-minting callers can
        //    downscope.
        if !requires_cache_read(&row.scope) {
            return Err(VerifyError::InvalidPat);
        }
        let can_write = requires_cache_write(&row.scope);

        Ok((row.tenant_id, can_write))
    }

    /// The full Option-B verification pipeline. Returns the PAT's owning
    /// tenant id on success. Thin wrapper over [`Self::verify_capability`]
    /// for callers that don't need the write-capability bit.
    pub async fn verify(&self, pat_plaintext: &str) -> Result<String, VerifyError> {
        self.verify_capability(pat_plaintext)
            .await
            .map(|(tenant, _can_write)| tenant)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use corelink_pat::{
        mint, PatEnv, PatScopes, PrincipalId, TenantId, SCOPE_CACHE_R, SCOPE_CACHE_RW,
    };
    use uuid::Uuid;

    use super::*;

    /// Fake row source: a `token_id → PatRow` map plus a call counter and
    /// an optional forced backend error. Drives the full verification
    /// pipeline with real crypto but no network.
    #[derive(Default)]
    struct FakeLookup {
        rows: HashMap<String, PatRow>,
        calls: AtomicUsize,
        backend_err: Option<String>,
    }

    impl FakeLookup {
        fn with_row(token_id: &str, row: PatRow) -> Self {
            let mut rows = HashMap::new();
            rows.insert(token_id.to_owned(), row);
            Self {
                rows,
                calls: AtomicUsize::new(0),
                backend_err: None,
            }
        }
        fn backend(err: &str) -> Self {
            Self {
                backend_err: Some(err.to_owned()),
                ..Self::default()
            }
        }
        fn empty() -> Self {
            Self::default()
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PatRowLookup for FakeLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.backend_err {
                return Err(e.clone());
            }
            Ok(self.rows.get(token_id).cloned())
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Mint a real PAT and return `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(
        key: &PatSigningKey,
        tenant: u128,
        scopes: u64,
    ) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant + 1000)),
            PatScopes::from_u64(scopes),
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

    fn row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: scope.to_owned(),
        }
    }

    #[tokio::test]
    async fn forged_token_rejected_before_d1() {
        let key = test_key();
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier
            .verify("corelink_pat_not-a-real-token")
            .await
            .unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
    }

    #[tokio::test]
    async fn wrong_signing_key_rejected_before_d1() {
        let mint_key = test_key();
        let (pt, _tid, hash, tenant) = mint_pat(&mint_key, 7, SCOPE_CACHE_RW);
        let other_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let lookup = Arc::new(FakeLookup::with_row(
            "ignored",
            row(&hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup.clone(), other_key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn valid_pat_with_cache_scope_resolves_tenant() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 42, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::new(lookup.clone(), key);
        let resolved = verifier.verify(&pt).await.unwrap();
        assert_eq!(resolved, tenant);
        assert_eq!(lookup.call_count(), 1);
    }

    #[tokio::test]
    async fn pat_minted_under_prev_key_validates_during_overlap() {
        // Rotation overlap: the verifier's CURRENT key differs from the key
        // the PAT was minted under, but the old key is still in the overlap
        // set — so the PAT must still validate (no instant fleet-wide outage).
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 60, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new (current), old (prev)].
        let verifier = PatVerifier::with_key_set(
            lookup.clone(),
            vec![(*new_key).clone(), (*old_key).clone()],
        );
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn pat_rejected_when_minting_key_not_in_overlap_set() {
        // After the overlap window closes (old key dropped), a PAT minted
        // under the now-retired key must be rejected.
        let old_key = test_key();
        let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
        let (pt, tid, hash, tenant) = mint_pat(&old_key, 61, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        // Overlap set = [new] only — old key retired.
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone()]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
    }

    #[tokio::test]
    async fn empty_key_set_fails_closed() {
        // No key bound ⇒ nothing verifies (fail-closed).
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
        let verifier = PatVerifier::with_key_set(lookup.clone(), vec![]);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 0, "empty key set rejects pre-D1");
    }

    #[tokio::test]
    async fn admin_scope_grants_cache() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 43, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "admin")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn unknown_token_id_is_invalid() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 44, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
    }

    #[tokio::test]
    async fn wrong_stored_hash_is_invalid() {
        let key = test_key();
        let (pt, tid, _hash, tenant) = mint_pat(&key, 45, SCOPE_CACHE_RW);
        let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 46, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(
            &tid,
            row(&other_hash, &tenant, "cas:rw"),
        ));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn empty_scope_fails_closed() {
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 47, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
    }

    #[tokio::test]
    async fn read_only_scope_still_resolves() {
        // The verifier gates on "has cache read" (the port carries no op);
        // a cas:r PAT resolves here — per-op write-deny is the route's job.
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_R);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:r")));
        let verifier = PatVerifier::new(lookup, key);
        assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
    }

    #[tokio::test]
    async fn revoked_row_is_invalid_pat() {
        // Soft revocation (migration 0063) is enforced INSIDE the lookup
        // SQL (`AND revoked_at_ms IS NULL`), so a revoked row surfaces to
        // the pipeline exactly like an absent one: `lookup → None`. Model
        // that here (the FakeLookup map simply does not contain the
        // revoked row) and assert the uniform InvalidPat — a revoked PAT
        // must be indistinguishable from an unknown one on the wire.
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 50, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::empty());
        let verifier = PatVerifier::new(lookup.clone(), key);
        let err = verifier.verify(&pt).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidPat));
        assert_eq!(lookup.call_count(), 1, "revocation is decided at D1");
    }

    #[test]
    fn lookup_sql_filters_revoked_and_expired_rows() {
        // The production D1 query must carry BOTH SQL-side liveness
        // filters — losing either one silently re-admits dead tokens.
        assert!(
            PAT_LOOKUP_SQL.contains("AND revoked_at_ms IS NULL"),
            "lookup SQL must exclude soft-revoked rows (migration 0063)"
        );
        assert!(
            PAT_LOOKUP_SQL.contains("expires_ms = 0 OR expires_ms >"),
            "lookup SQL must keep the expiry filter"
        );
        assert!(PAT_LOOKUP_SQL.contains("WHERE token_id = ?1"));
    }

    #[tokio::test]
    async fn backend_error_is_not_invalid_pat() {
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 49, SCOPE_CACHE_RW);
        let lookup = Arc::new(FakeLookup::backend("d1 unreachable"));
        let verifier = PatVerifier::new(lookup, key);
        let err = verifier.verify(&pt).await.unwrap_err();
        match err {
            VerifyError::Backend(m) => assert!(m.contains("d1 unreachable")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }
}

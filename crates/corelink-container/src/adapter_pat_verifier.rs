//! PAT verifier pipeline and production stack assembly.
//!
//! B126-H2 symbol map: PatVerifier and VerifyError are reexported by
//! adapter_pat.rs; all auth stages remain wired here without semantic changes.

use std::sync::Arc;

use corelink_pat::{
    verify_hmac_only_multi, verify_with_hash_multi, PatError, PatHash, PatSigningKey,
};
use futures::FutureExt;

use crate::container_capacity::ARGON2_VERIFY_PERMITS;
use crate::scope::{requires_cache_read, requires_cache_write};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::{non_empty_env, StorageEnv};

use super::adapter_pat_crypto::{
    dummy_burn_fingerprint, secret_match_fingerprint, BurnFlight, FlightGroup, SecretMatchMemo,
    VerifyFlight, FLIGHT_GROUP_CAP, SECRET_MATCH_MEMO_CAP, SECRET_MATCH_MEMO_TTL,
};
use super::adapter_pat_gate::{
    PerTenantGate, ARGON2_PERMIT_WAIT, ARGON2_PER_TENANT_PERMITS, PER_TENANT_MAP_CAP,
    UNKNOWN_TOKEN_BUCKET,
};
pub use super::adapter_pat_lookup::VerifyError;
use super::adapter_pat_lookup::{PatRowLookup, SingleFlightPatLookup};

/// Container-side PAT → tenant verifier (Option B). Trait-agnostic: each
/// adapter route wraps an `Arc<PatVerifier>` in its local resolver shell.
#[non_exhaustive]
pub struct PatVerifier {
    pub(super) lookup: Arc<dyn PatRowLookup>,
    /// The HMAC overlap key set: `PAT_SIGNING_KEY` (current) plus any
    /// `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW` rotation siblings.
    /// A PAT minted under any key in the set validates during the
    /// rotation overlap window (`key_management.md §3.2.1`), so rotating
    /// the current key on compromise does NOT instantly invalidate the
    /// live fleet. Always non-empty by construction (fail-closed
    /// otherwise — see [`PatVerifier::new`] / [`PatVerifier::from_env`]).
    pub(super) signing_keys: Arc<Vec<PatSigningKey>>,
    /// Process-wide bound on concurrent Argon2id work (finding #2). A permit
    /// is acquired AFTER the cheap HMAC fast-reject (so forged tokens never
    /// consume one) and held ONLY across the `spawn_blocking` Argon2id call —
    /// both on the hot path and on the None-row dummy-burn path (which also
    /// runs Argon2id for timing parity). Acquire-timeout ⇒ `Backend`
    /// ("overloaded"), fail-CLOSED.
    pub(super) argon2_permits: Arc<tokio::sync::Semaphore>,
    /// Per-tenant Argon2id sub-limit (finding #1, fairness). A lazily-created
    /// `Arc<tokio::sync::Semaphore>` per `tenant_id`, each capped at
    /// [`ARGON2_PER_TENANT_PERMITS`]. A row-FOUND verify acquires the global
    /// permit FIRST, then this tenant's permit (CONSISTENT order => no
    /// deadlock), so a single tenant flooding distinct PATs can hold at most
    /// `ARGON2_PER_TENANT_PERMITS` global permits at once -- leaving the rest of
    /// the global pool for other tenants. The dummy-burn arm INVERTS that order
    /// (shared bucket first, non-blocking) for the reason spelled out on
    /// [`UNKNOWN_TOKEN_BUCKET`]: it is the only way its sub-cap bounds
    /// global-pool occupancy and not merely concurrent burns. See
    /// [`PerTenantGate`] for the bounded LRU and the fail-safe posture; it lives
    /// behind an `Arc` so the coalescing flight future can own a handle to it.
    pub(super) per_tenant: Arc<PerTenantGate>,
    /// Memoised Argon2id secret-matches — the fix for the adapter plane's
    /// throughput ceiling.
    ///
    /// Argon2id at the OWASP-2024 cost (`m=64 MiB, t=3, p=4`) costs **at least
    /// ~0.15 CPU-s** per verify — that figure is a LOWER BOUND measured with the
    /// C reference implementation on an Apple-silicon core, and the pure-Rust
    /// `argon2` crate on a fraction of an x86 vCPU is slower, not faster. The
    /// production container is provisioned at **0.25 vCPU / 1 GiB**
    /// (`corelink-prod-corelinkserver-prod`, read back from the CF Containers
    /// API). Running it on EVERY request therefore capped a tenant's adapter
    /// plane at roughly 3 requests/second regardless of client concurrency —
    /// crippling for the small-object, high-frequency traffic a build cache is
    /// made of (sccache/cargo, npm, pip, brew, OCI all issue thousands of tiny
    /// requests per build).
    ///
    /// Evidence from the sccache CI pilot (#1017), stated at the precision it
    /// actually supports: the fully-warm run served **827 cache operations at a
    /// 100 % hit rate in 631 s against a 409-423 s cold baseline** (+208…222 s).
    /// The predicted Argon2id floor for those 827 verifies is
    /// `827 × 0.15 ÷ 0.25 ≈ 496 s`. That is the right order of magnitude and the
    /// same direction, which makes Argon2id a SUFFICIENT explanation of the
    /// regression — it is not a per-request measurement of the authenticated
    /// path, and none has been captured yet.
    ///
    /// The native plane already solved this with
    /// `native_pat_gate::NativePatGate`'s verify cache; the adapter plane never
    /// got one. This memo closes that gap WITHOUT native's ≤5 s revocation
    /// window, because it memoises only the immutable Argon2id comparison and
    /// leaves the D1 row read per-request. See [`SecretMatchMemo`].
    pub(super) secret_match_memo: Arc<SecretMatchMemo>,
    /// Coalesces a burst of concurrent COLD memo misses for the SAME
    /// `(plaintext, token_id, pat_hash, scope, find_only)` onto ONE Argon2id.
    ///
    /// The memo above only helps once a first request has paid Argon2id. A cold
    /// container's first burst — a `cargo -jN` build's opening N requests, all
    /// bearing the same PAT — misses it N times simultaneously, and only
    /// [`ARGON2_PER_TENANT_PERMITS`] of them are admitted; a single Argon2id at
    /// 0.25 vCPU far exceeds the 250 ms [`ARGON2_PERMIT_WAIT`], so the surplus
    /// sheds into `Backend` ⇒ **HTTP 503 on the first request of every cold
    /// build**. Coalescing collapses that burst to one Argon2id under one
    /// permit, so no request is shed for work another request is already doing.
    pub(super) verify_flights: Arc<FlightGroup<VerifyFlight>>,
    /// The mirror of `verify_flights` on the unknown/expired/revoked arm.
    ///
    /// Its ONLY purpose is symmetry. Coalescing just the row-FOUND arm would
    /// make N concurrent copies of one wrong-secret token cost ~1×Argon2id when
    /// the `token_id` exists and ~N× when it does not — re-opening, in the
    /// concurrency dimension, exactly the token-enumeration oracle the dummy
    /// burn exists to close. See [`dummy_burn_fingerprint`] for why the two keys
    /// partition identically.
    pub(super) burn_flights: Arc<FlightGroup<BurnFlight>>,
}

impl std::fmt::Debug for PatVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatVerifier")
            .field("lookup", &"Arc<dyn PatRowLookup>")
            .field(
                "signing_keys",
                &format_args!("[{} REDACTED]", self.signing_keys.len()),
            )
            .field(
                "argon2_permits_available",
                &self.argon2_permits.available_permits(),
            )
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
            // Process-wide Argon2id concurrency cap (finding #2). Constructed
            // here so EVERY constructor path (`new`, `from_env`, and the test
            // wiring) gets the bound without changing any public signature.
            argon2_permits: Arc::new(tokio::sync::Semaphore::new(ARGON2_VERIFY_PERMITS)),
            // Per-tenant fairness sub-limit (finding #1). Empty map; tenant
            // semaphores are created lazily on first verify for that tenant.
            // BOUNDED LRU (#1/#12 follow-up): capped at PER_TENANT_MAP_CAP.
            per_tenant: Arc::new(PerTenantGate::new(
                ARGON2_PER_TENANT_PERMITS,
                PER_TENANT_MAP_CAP,
            )),
            // Memoise the (immutable) Argon2id secret-match so a build's
            // thousands of requests pay it once, not once each. Revocation is
            // unaffected — the D1 row is still read per request.
            secret_match_memo: Arc::new(SecretMatchMemo::new(
                SECRET_MATCH_MEMO_CAP,
                SECRET_MATCH_MEMO_TTL,
            )),
            // …and coalesce the COLD burst that misses that memo, on BOTH 401
            // arms, so the cold-start 503 disappears without making `token_id`
            // liveness observable in the concurrency dimension.
            verify_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
            burn_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
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
    ///
    /// This function reads env and NOTHING else — the stack assembly lives in
    /// [`Self::from_parts`], which a test can construct (same reason
    /// `crate::routes::residency::residency_decision` is extracted from its
    /// header parsing: unit-testable with ZERO process-env mutation, avoiding
    /// the parallel-test `set_var` race).
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

        Some(Self::from_parts(Arc::new(d1), signing_keys))
    }

    /// THE production stack assembly: everything [`Self::from_env`] does EXCEPT
    /// reading env. `inner` is the raw row source (the D1 HTTP client in
    /// production); this is where it gets wrapped into the stack the running
    /// container actually uses.
    ///
    /// It is split out so the assembly can be exercised by a test. Before the
    /// split, the only site that built the production stack was `from_env`,
    /// which no test could construct (it reads process env, and this repo
    /// deliberately keeps tests at zero env mutation — see the note on
    /// `crate::routes::residency`); every existing verifier test therefore built
    /// `PatVerifier::with_key_set(d1, keys)` and ran with NO wrapper at all.
    /// That gap is not hypothetical: PR #1055's co-read correctness depends on
    /// properties of this exact wrapper (an unspawned `Shared` future polled by
    /// an arbitrary joiner), and the bug that made it necessary shipped review
    /// precisely because the wrapper was never in a test's loop.
    ///
    /// # What the wrapper is for
    ///
    /// Front the per-op D1 `pat` read with single-flight coalescing so a cold
    /// parallel burst of the SAME runner PAT (a hydrate) collapses to one D1
    /// read instead of a thundering herd — no cache, so revocation stays
    /// immediate. See [`SingleFlightPatLookup`].
    pub(super) fn from_parts(
        inner: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
    ) -> Self {
        Self::with_key_set(Arc::new(SingleFlightPatLookup::new(inner)), signing_keys)
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

    include!("adapter_pat_verifier/part-01.rs");
    include!("adapter_pat_verifier/part-02.rs");
    include!("adapter_pat_verifier/part-03.rs");
}

//! PAT verifier pipeline and production stack assembly.
//!
//! B126-H2 symbol map: PatVerifier and VerifyError are reexported by
//! adapter_pat.rs; all auth stages remain wired here without semantic changes.

use std::sync::Arc;

use corelink_pat::{
    verify_hmac_only_multi, verify_with_hash_multi, PatError, PatHash, PatSigningKey,
};
use tokio::sync::Semaphore;

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
use super::adapter_pat_lookup::{PatRow, PatRowLookup, SingleFlightPatLookup};

/// Container-side PAT → tenant verifier (Option B). Trait-agnostic: each
/// adapter route wraps an `Arc<PatVerifier>` in its local resolver shell.
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
    pub(super) argon2_permits: Arc<Semaphore>,
    /// Per-tenant Argon2id sub-limit (finding #1, fairness). A lazily-created
    /// `Arc<Semaphore>` per `tenant_id`, each capped at
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
            argon2_permits: Arc::new(Semaphore::new(ARGON2_VERIFY_PERMITS)),
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

    /// Test-only: reach the per-tenant fairness gate's semaphore for `tenant`
    /// through the exact get-or-insert path a verify uses, so a test can
    /// saturate (or pin) a bucket. Production code goes through
    /// [`PerTenantGate::acquire`].
    #[cfg(test)]
    pub(super) fn per_tenant_semaphore(&self, tenant: &str) -> Option<Arc<Semaphore>> {
        self.per_tenant.semaphore(tenant)
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
        self.verify_capability_full(pat_plaintext)
            .await
            .map(|(tenant_id, can_write, _runner_job)| (tenant_id, can_write))
    }

    /// The same pipeline as [`Self::verify_capability`], additionally returning
    /// the row's `runner_job` marker (0086).
    ///
    /// Split out rather than widening `verify_capability`'s tuple so the five
    /// adapter surfaces that have no narrowable operation keep their call sites
    /// untouched, and so this security pipeline exists exactly ONCE — the two
    /// entry points are the same code, not two copies that can drift.
    ///
    /// The only caller today is the sccache WebDAV DELETE
    /// (`routes/cargo.rs`), which is the only adapter operation that can
    /// destroy a tenant's cache entry.
    pub async fn verify_capability_full(
        &self,
        pat_plaintext: &str,
    ) -> Result<(String, bool, bool), VerifyError> {
        // 1. HMAC fast-reject (pre-D1). A forged token is rejected here
        //    without a D1 round-trip. Uniform InvalidPat. Checked against
        //    the overlap key set so a token minted under the rotation
        //    predecessor/successor still fast-passes here.
        let (_env, token_id) = verify_hmac_only_multi(pat_plaintext, &self.signing_keys)
            .map_err(|_| VerifyError::InvalidPat)?;

        // 2. D1 lookup by the non-secret token_id (expiry filtered in SQL).
        //
        // Timed as the `opat` sub-phase of the Worker's `origin` block: this is
        // the per-request D1 read #1022 deliberately KEPT (so a revocation takes
        // effect immediately) and it is one of the two candidates for the ~300 ms
        // `origin` measured in prod. `timed` is a pass-through wrapper — it adds
        // two `Instant::now()` calls and changes nothing about the read, its
        // single-flight coalescing, or its result. See `crate::origin_timing`.
        let row = match crate::origin_timing::timed(
            crate::origin_timing::Phase::Pat,
            self.lookup.lookup(token_id.as_str()),
        )
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
                // COALESCED, exactly like the hot path below — see
                // `burn_flights`. N concurrent copies of ONE plaintext run ONE
                // dummy burn here, just as N concurrent copies of one plaintext
                // run ONE Argon2id there, so the cost of a burst is independent
                // of whether the token_id exists.
                //
                // ⚠️ INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM (#1034) is preserved
                // through the coalescing: EVERY shed inside this flight —
                // global acquire timeout, closed semaphore, saturated
                // per-tenant sub-cap, or an aborted flight task — resolves to
                // `Backend("pat verifier overloaded")`, byte-identical to the
                // row-FOUND arm below. The two shed arms MUST stay identical:
                // the row-FOUND path sheds with `Backend` (503) and this
                // row-NOT-FOUND path used to shed with `InvalidPat` (401), so
                // under saturation the status code alone answered "does this
                // token_id exist in D1?" — a row-existence oracle strictly
                // stronger than the latency one the burn exists to close.
                // Uniformity, not the burn, is what makes the shed safe: on a
                // shed the burn is deliberately SKIPPED (there are no permits
                // to run it with), so timing parity is already gone and the
                // status must carry no information either. Every rejection that
                // is NOT a shed still collapses to `InvalidPat`.
                //
                // Coalescing only ever makes that uniformity easier to hold: a
                // burst of one plaintext now takes ONE trip through these two
                // acquires instead of N, so the number of requests that can
                // reach a shed at all strictly decreases.
                let plaintext = pat_plaintext.to_owned();
                let permits = Arc::clone(&self.argon2_permits);
                let per_tenant = Arc::clone(&self.per_tenant);
                // `oargon`/`opermit`: `burn_flights.run` SPAWNS its `lead`
                // future (see `FlightGroup::run`'s "the work is SPAWNED" doc),
                // so the permit acquire below runs on a task with no ambient
                // `origin_timing::LEDGER` in scope. Capture a HANDLE to it
                // HERE, on this (the calling) task, before the closure moves
                // it onto the spawned task — see `origin_timing::current_ledger`.
                // The OUTER `timed(..)` wrap, in contrast, is polled on THIS
                // task the whole time (it is `run(..).await`ed directly, never
                // spawned itself), so it needs no such handle.
                let permit_ledger = crate::origin_timing::current_ledger();
                let outcome = crate::origin_timing::timed(
                    crate::origin_timing::Phase::Argon,
                    self.burn_flights.run(
                        &dummy_burn_fingerprint(pat_plaintext),
                        move || {
                            async move {
                                // Finding #12: cap the dummy-burn path through ONE
                                // shared synthetic bucket so a leaked-key flood
                                // across bogus token_ids cannot drain the global
                                // pool via this path.
                                //
                                // ⚠️ ORDER IS INVERTED HERE ON PURPOSE, and it is
                                // the whole defence. This arm takes the SHARED
                                // bucket FIRST and the global permit only after.
                                // The original order (global, then a 250 ms
                                // bounded wait on the bucket) capped concurrent
                                // BURNS at the sub-cap but did NOT cap this arm's
                                // global-pool occupancy: a request already
                                // destined to shed still parked a global permit
                                // for the full `ARGON2_PERMIT_WAIT` while queued
                                // on the saturated bucket, so at the flood rate
                                // the bucket itself admits, nearly the entire
                                // global pool sat pinned by requests that would
                                // go on to burn nothing. Bucket-first caps the
                                // arm's global footprint at the sub-cap for real:
                                // a shed here holds no global permit at any
                                // instant.
                                //
                                // The acquire is NON-BLOCKING
                                // ([`PerTenantGate::try_acquire`]) precisely
                                // because it runs with no global permit held and
                                // therefore inverts the module's
                                // global→per-tenant order (see
                                // [`PerTenantGate::acquire`]). That order exists
                                // to keep the two tiers deadlock-free by being
                                // consistent; a `try` cannot deadlock in any
                                // order, because it never waits, so the rationale
                                // survives the inversion intact. The cost is the
                                // deliberate semantic change: a burn that would
                                // previously have waited up to
                                // `ARGON2_PERMIT_WAIT` for a bucket slot now
                                // sheds at once — a shed either way, just sooner
                                // and without holding capacity hostage meanwhile.
                                let tenant_permit =
                                    match per_tenant.try_acquire(UNKNOWN_TOKEN_BUCKET) {
                                        Ok(maybe_permit) => maybe_permit,
                                        // PER-TENANT shed: skip the burn and shed
                                        // with the SAME overloaded signal as the
                                        // global tier and as the row-FOUND arm
                                        // (INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM).
                                        // `Ok(None)` is the fail-safe fall-through
                                        // to global-only bounding and proceeds to
                                        // the burn.
                                        Err(()) => {
                                            return BurnFlight::Backend(
                                                "pat verifier overloaded".into(),
                                            )
                                        }
                                    };
                                // The dummy burn ALSO runs Argon2id (for timing
                                // parity), so it must be bounded by the same gate —
                                // otherwise an attacker who floods valid-HMAC tokens
                                // for NON-existent token_ids could OOM the container
                                // exactly like the hot path.
                                //
                                // `opermit`: this runs inside the SPAWNED `lead`
                                // future, so the acquire is timed via the
                                // captured `permit_ledger` handle
                                // (`origin_timing::PhaseScope::with_handle`),
                                // not the ambient task-local — see the capture
                                // site above. The nested block scopes the guard
                                // to exactly this acquire; `Drop` fires on the
                                // shed's early `return` too, so both outcomes
                                // are timed identically.
                                let permit = {
                                    let _permit_scope =
                                        crate::origin_timing::PhaseScope::with_handle(
                                            permit_ledger,
                                            crate::origin_timing::Phase::Permit,
                                        );
                                    match tokio::time::timeout(
                                        ARGON2_PERMIT_WAIT,
                                        permits.acquire_owned(),
                                    )
                                    .await
                                    {
                                        Ok(Ok(permit)) => permit,
                                        // GLOBAL shed. Returns `Backend`, the SAME code
                                        // the hot path returns for the SAME condition —
                                        // see the status-parity note below. `tenant_permit`
                                        // is released by RAII on this early return, so a
                                        // global shed never strands a bucket slot either.
                                        _ => {
                                            return BurnFlight::Backend(
                                                "pat verifier overloaded".into(),
                                            )
                                        }
                                    }
                                };
                                let _ = tokio::task::spawn_blocking(move || {
                                    let r =
                                        corelink_pat::dummy_verify_for_constant_time(&plaintext);
                                    drop(permit); // hold the global permit ONLY across the blocking work
                                    drop(tenant_permit); // release the synthetic-bucket permit too
                                    r
                                })
                                .await;
                                BurnFlight::Burned
                            }
                            .boxed()
                        },
                        BurnFlight::Backend,
                    ),
                )
                .await;
                // STATUS PARITY between the two ways to earn a 401
                // (INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM, #1034).
                //
                // A COMPLETED burn is the uniform 401 path — the burn ran, the
                // credential was decided, and the answer is `InvalidPat`
                // exactly as it is for a wrong secret on a live row.
                //
                // A SHED — at EITHER tier, global or per-tenant, and whether it
                // fires inside the flight or the flight task itself dies —
                // decided nothing about the credential, so it answers
                // `Backend("pat verifier overloaded")`, identical to the
                // row-FOUND arm. Uniformity across the two tiers matters as
                // much as uniformity across the two arms: if the per-tenant
                // tier still answered 401 here while the global tier answered
                // 503, an attacker who can pick WHICH tier sheds would recover
                // the same bit the asymmetry used to hand over for free.
                return Err(match &*outcome {
                    BurnFlight::Burned => VerifyError::InvalidPat,
                    BurnFlight::Backend(msg) => VerifyError::Backend(msg.clone()),
                });
            }
        };

        // 3. Full crypto verify on a blocking thread (Argon2id is
        //    CPU-heavy and must not stall the async worker). Re-parses,
        //    constant-time matches token_id, re-checks HMAC, then
        //    Argon2id-verifies the secret segment against the stored hash.
        //
        //    MEMOISED ([`SecretMatchMemo`]): Argon2id at the OWASP-2024 cost is
        //    ~0.15 CPU-s and the container runs at 0.25 vCPU, so paying it per
        //    request capped one tenant's whole adapter plane at ~3 req/s. The
        //    memo is keyed on (plaintext, token_id, stored pat_hash, scope,
        //    find_only) and holds ONLY the immutable "these match" boolean —
        //    stages 1 and 2 above
        //    (HMAC against the CURRENT key set; the expiry- and
        //    revocation-filtered D1 row read) still run on EVERY request, so
        //    this changes no authorization decision and opens NO revocation
        //    window. A wrong secret yields a different key and therefore always
        //    pays the full Argon2id, which is what keeps the two 401 paths
        //    (unknown token_id ⇒ dummy burn / wrong secret ⇒ real verify) in
        //    timing parity.
        //
        //    COALESCED on a shared FUTURE ([`FlightGroup`]) so a burst of COLD
        //    memo misses for the SAME key runs ONE Argon2id, not N. Without
        //    it the memo does nothing for the very first burst of a cold
        //    container — a `cargo -jN` build opens with N simultaneous misses,
        //    `ARGON2_PER_TENANT_PERMITS` (1) is admitted, and since one
        //    Argon2id at 0.25 vCPU dwarfs the 250 ms `ARGON2_PERMIT_WAIT` the
        //    surplus sheds to 503.
        //
        //    An earlier revision coalesced on a sharded MUTEX instead, and three
        //    independent reviewers killed it. The shared future is not the same
        //    object, and it is worth recording exactly why each finding does not
        //    survive the change of shape:
        //      (a) ORACLE — the mutex sat on the row-FOUND path only, so N
        //          concurrent copies of one wrong-secret token cost ~1×Argon2id
        //          when the `token_id` existed and ~N× when it did not. Here BOTH
        //          arms coalesce, on keys that partition identically for a given
        //          plaintext (see `dummy_burn_fingerprint`), so a burst costs
        //          ~1×Argon2id either way and liveness stays unobservable in the
        //          concurrency dimension.
        //      (b) UNBOUNDED WAIT IN FRONT OF THE LOAD SHED — the mutex made a
        //          burst an N-deep FIFO whose tail waited ~N× one Argon2id, with
        //          no timeout, IN FRONT of the two `ARGON2_PERMIT_WAIT` sheds.
        //          A shared future has no queue: every joiner resolves together
        //          when the ONE run completes, so the N-th waits exactly as long
        //          as the 1st. That wait is `250 ms + 250 ms + one Argon2id` —
        //          i.e. the worst case of a request that is admitted TODAY. The
        //          shed still bounds admission; it is now inside the flight, and
        //          when it fires every joiner gets the same fail-CLOSED
        //          `Backend`. Coalescing removes work; it never adds a wait
        //          longer than the successful path already has.
        //      (c) FAIRNESS BYPASS — the mutex was taken BEFORE the per-tenant
        //          sub-permit, so on the SHARED `_oci` / `_anonymous` Durable
        //          Objects one tenant's flood could block another upstream of the
        //          cap. Here the acquires stay INSIDE the flight and in the
        //          unchanged global→per-tenant order, and a joiner takes no
        //          permit at all. Concurrent Argon2ids for a tenant still equal
        //          that tenant's held sub-permits, so `ARGON2_PER_TENANT_PERMITS`
        //          bounds them exactly as before — and a waiter only ever waits
        //          on its own key's flight, which by construction belongs to its
        //          own tenant. There is no cross-tenant coupling to bypass.
        let fp = secret_match_fingerprint(
            pat_plaintext,
            token_id.as_str(),
            &row.pat_hash,
            &row.scope,
            row.find_only,
            row.runner_job,
        );
        // Whether THIS request went through the Argon2id stage rather than
        // being served by the memo. Under coalescing that includes a request
        // that JOINED another's flight instead of running the hash itself —
        // which is correct: the flight key binds `scope` and `find_only`, so a
        // joiner reaches the identical step-4 verdict, and the memo is populated
        // only at the very END of the pipeline, after that gate. See the comment
        // there for why populating it earlier would be an oracle.
        let mut proved_here = false;
        // `oargon`: this scope covers the memo check AND, on a miss, the
        // verification flight below — the SAME phase name and the SAME
        // emission rule (present iff this scope ran) as the row-not-found
        // dummy burn's `oargon` above, which is the whole point: the two
        // arms must stay indistinguishable on the wire. `Drop` records on
        // EVERY exit from this block, including the early `return Err(..)`
        // inside it, so timing this multi-statement region touches none of
        // its branches. See `origin_timing`'s module-level security note.
        // The extra `{ }` scopes the guard to exactly this `if`, so it does
        // NOT also time the scope gate / memo-insert that follow it below.
        {
            let _argon_scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Argon);
            if !self.secret_match_memo.contains(&fp) {
                proved_here = true;
                let plaintext = pat_plaintext.to_owned();
                let signing_keys = Arc::clone(&self.signing_keys);
                let stored_hash = PatHash::from_phc_string(row.pat_hash.clone());
                let permits = Arc::clone(&self.argon2_permits);
                let per_tenant = Arc::clone(&self.per_tenant);
                let tenant_id = row.tenant_id.clone();
                // `opermit`: `verify_flights.run` also SPAWNS its `lead` future
                // (same `FlightGroup::run` as the dummy-burn arm above), so
                // capture the ledger handle HERE, before it moves onto the
                // spawned task — see the matching note on the dummy-burn arm.
                let permit_ledger = crate::origin_timing::current_ledger();
                let outcome = self
                    .verify_flights
                    .run(
                        &fp,
                        move || {
                            async move {
                                // Finding #2: bound concurrent Argon2id. Acquire a permit
                                // (bounded wait) BEFORE the blocking work; on
                                // acquire-timeout the verifier is overloaded — return
                                // `Backend` (reusing the existing variant, which the OCI
                                // `/token` handler and the native gate already map to a
                                // non-leaky fail-CLOSED rejection) rather than letting
                                // the request pile on more 64-MiB allocations. The permit
                                // is moved into the blocking closure and dropped there,
                                // so it is held ONLY across the Argon2id.
                                let permit = {
                                    let _permit_scope =
                                        crate::origin_timing::PhaseScope::with_handle(
                                            permit_ledger,
                                            crate::origin_timing::Phase::Permit,
                                        );
                                    match tokio::time::timeout(
                                        ARGON2_PERMIT_WAIT,
                                        permits.acquire_owned(),
                                    )
                                    .await
                                    {
                                        Ok(Ok(permit)) => permit,
                                        // Semaphore closed (never in practice) or the
                                        // bounded wait elapsed — both fail CLOSED.
                                        Ok(Err(_)) | Err(_) => {
                                            return VerifyFlight::Backend(
                                                "pat verifier overloaded".into(),
                                            )
                                        }
                                    }
                                };
                                // Finding #1: per-tenant fairness. With the GLOBAL permit
                                // already held, acquire this tenant's sub-permit
                                // (consistent order global→per-tenant ⇒ deadlock-free).
                                // If the tenant is at its sub-cap, fail-CLOSED with the
                                // SAME overloaded signal as a global timeout — so ONE
                                // tenant flooding distinct PATs cannot drain the whole
                                // global pool and starve others. The `permit` (global) is
                                // dropped on this early return by RAII, so a per-tenant
                                // rejection does NOT leak a global permit. `Ok(None)` is
                                // the fail-safe fall-through to global-only bounding.
                                let tenant_permit = match per_tenant.acquire(&tenant_id).await {
                                    Ok(maybe_permit) => maybe_permit,
                                    Err(()) => {
                                        return VerifyFlight::Backend(
                                            "pat verifier overloaded".into(),
                                        )
                                    }
                                };
                                let joined = tokio::task::spawn_blocking(move || {
                                    let r = verify_with_hash_multi(
                                        &plaintext,
                                        &token_id,
                                        &stored_hash,
                                        &signing_keys,
                                    );
                                    drop(permit); // release the global Argon2id permit the moment the work ends
                                    drop(tenant_permit); // release the per-tenant permit too (RAII, both paths)
                                    r
                                })
                                .await;
                                // ⚠️ The flight proves the SECRET and nothing else. It
                                // must NOT populate the memo here, even though this is
                                // where the proof lands: the memo is written only past
                                // the step-4 scope gate, per request — see the comment
                                // there. A scope-rejected PAT memoised at this point
                                // would 401 in ~0 ms forever after, which is the
                                // latency oracle that gate placement exists to close.
                                match joined {
                                    Ok(Ok(_verified)) => VerifyFlight::Proven,
                                    // A malformed PHC string is DB corruption,
                                    // not a wrong PAT. Preserve the taxonomy so
                                    // adapters return 503/retry rather than
                                    // misclassifying an operator/data problem
                                    // as a customer 401 (B183).
                                    Ok(Err(PatError::HashError(kind))) => VerifyFlight::Backend(
                                        format!("stored PAT hash invalid: {kind}"),
                                    ),
                                    Ok(Err(_)) => VerifyFlight::Rejected,
                                    Err(e) => VerifyFlight::Backend(format!("verify join: {e}")),
                                }
                            }
                            .boxed()
                        },
                        VerifyFlight::Backend,
                    )
                    .await;
                match &*outcome {
                    VerifyFlight::Proven => {}
                    VerifyFlight::Rejected => return Err(VerifyError::InvalidPat),
                    VerifyFlight::Backend(msg) => return Err(VerifyError::Backend(msg.clone())),
                }
            }
        }

        // 4. Scope gate — fail-CLOSED on NO cache capability at all, then
        //    surface the read/write split so credential-minting callers can
        //    downscope.
        //
        // ADR-0071 (find-only least-privilege): a find-only PAT stores the
        // CHECK-safe base `scope = 'read-only'` and is narrowed to find-missing
        // ONLY at the Worker's `x-corelink-scope` header. But this adapter verifier
        // authorizes package-manager reads (npm/pip/brew/cargo/OCI) directly from
        // the D1 `scope` — the OCI `/token` exchange (`routes/oci.rs`) has NO header
        // gate — so the `read-only` base would otherwise grant e.g. `docker pull`.
        // The adapter planes have no find-missing operation, so a find-only PAT is
        // rejected here fail-CLOSED (defense-in-depth for every adapter surface).
        if row.find_only {
            return Err(VerifyError::InvalidPat);
        }
        if !requires_cache_read(&row.scope) {
            return Err(VerifyError::InvalidPat);
        }
        let can_write = requires_cache_write(&row.scope);

        // ⚠️ The memo is populated HERE — past the scope gate — and nowhere
        // earlier. Populating it at the end of step 3 (right after the Argon2id
        // succeeded) looks natural and is an ORACLE: a PAT whose secret is
        // CORRECT but whose scope is rejected (`find_only`, or no cache grant)
        // would be memoised on its first, slow 401, and every later attempt with
        // that same token would 401 in ~0 ms instead of paying Argon2id. An
        // attacker replaying a bag of leaked plaintexts TWICE could then sort
        // "live credential, insufficient scope" from "dead/unknown/wrong-secret"
        // — and a *revoked* PAT (row gone ⇒ slow dummy burn) from a merely
        // *scope-downgraded* one (fast memo hit) — purely on latency, with no
        // grant of any kind. The module header requires every rejection reason to
        // be indistinguishable on the wire; latency is part of the wire.
        //
        // Gating on `proved_here` (rather than inserting unconditionally) keeps a
        // warm hit off the memo's write lock and keeps the TTL anchored at the
        // proof, not sliding on use.
        //
        // Coalescing does not disturb this. A burst's joiners all have
        // `proved_here == true` and so all reach this line, but they also all
        // reach the SAME scope verdict as their leader — the flight key binds
        // `scope` and `find_only`, so a burst cannot straddle the gate — and the
        // insert is idempotent on a shared key. A scope-REJECTED burst returns
        // above without any member inserting, exactly as a single request does.
        if proved_here {
            self.secret_match_memo.insert(fp);
        }

        Ok((row.tenant_id, can_write, row.runner_job))
    }

    /// Test-only: shrink the per-tenant LRU map cap so the eviction path can be
    /// driven deterministically (the production cap of 10k is too large to fill
    /// in a unit test). Returns `self` for chaining off a constructor.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_map_cap(mut self, cap: usize) -> Self {
        // The gate is behind an `Arc`, so rebuild it. Only ever called straight
        // off a constructor, where the map is still empty.
        self.per_tenant = Arc::new(PerTenantGate::new(self.per_tenant.cap, cap));
        self
    }

    /// Test-only: current number of live per-tenant semaphore entries (LRU map
    /// size). Used to assert the map stays bounded under distinct-tenant churn.
    #[cfg(test)]
    #[allow(
        clippy::expect_used,
        reason = "test-only helper; a poisoned mutex here is itself a test bug \
                  and should fail loudly. Mirrors the test module's allow — this \
                  cfg(test) method sits on the impl block, outside that module's scope."
    )]
    pub(super) fn per_tenant_map_len(&self) -> usize {
        self.per_tenant.permits.lock().expect("map lock").len()
    }

    /// Test-only constructor that overrides the Argon2id concurrency bound so
    /// a unit test can drive the semaphore to exhaustion deterministically
    /// (the production const is too large to fill in a test). Not part of the
    /// public production surface.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_key_set_and_permits(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
    ) -> Self {
        Self::with_key_set_and_permits_per_tenant(
            lookup,
            signing_keys,
            permits,
            ARGON2_PER_TENANT_PERMITS,
        )
    }

    /// Test-only constructor that overrides BOTH the global Argon2id concurrency
    /// bound AND the per-tenant sub-cap, so the fairness test can drive the
    /// two-tier interaction deterministically (e.g. a per-tenant cap small
    /// enough to saturate while global headroom remains for other tenants).
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_key_set_and_permits_per_tenant(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
        per_tenant_permits: usize,
    ) -> Self {
        Self {
            lookup,
            signing_keys: Arc::new(signing_keys),
            argon2_permits: Arc::new(Semaphore::new(permits)),
            per_tenant: Arc::new(PerTenantGate::new(per_tenant_permits, PER_TENANT_MAP_CAP)),
            secret_match_memo: Arc::new(SecretMatchMemo::new(
                SECRET_MATCH_MEMO_CAP,
                SECRET_MATCH_MEMO_TTL,
            )),
            verify_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
            burn_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
        }
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

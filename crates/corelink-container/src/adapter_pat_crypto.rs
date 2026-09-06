//! Bounded Argon2 proof memo and cancellation-safe flight coalescing.
//!
//! B126-H2 symbol map: SecretMatchMemo, FlightGroup, fingerprints, and flight
//! outcomes live here; no authorization decision is memoized.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::{BoxFuture, FutureExt, Shared};
use sha2::{Digest, Sha256};

/// TTL of a memoised Argon2id secret-match ([`SecretMatchMemo`]).
///
/// ⚠️ **This is NOT a revocation window and must not be read as one.** The
/// repo's other PAT caches (`native_pat_gate::VERIFY_CACHE_TTL`, the Worker's
/// `PAT_VERIFY_CACHE_TTL_MS`) memoise the *authorization decision* — they skip
/// the D1 row on a hit, so their TTL literally bounds how long a revoked PAT
/// keeps access, and both are pinned at 5 s against `SLO-FRESH-PAT-REVOKE`.
/// This memo caches something categorically different: the single boolean
/// "Argon2id(`plaintext`) matches this exact stored PHC hash". That is a **pure
/// function of the memo key itself** — it can never become false, and it
/// carries no tenant, no scope, no expiry, and no revocation state. Every one
/// of those still comes from the per-request, uncached D1 row read
/// (`PAT_LOOKUP_SQL`, which filters `revoked_at_ms IS NULL` + expiry in SQL),
/// so **revocation stays immediate on the adapter plane** — strictly stronger
/// than the ≤5 s window the native plane accepts.
///
/// The TTL therefore exists only as a memory-hygiene bound (bounded staleness
/// for a token that stops being used), not as a security control. 300 s keeps a
/// long build's thousands of requests on ONE Argon2id while still letting an
/// idle fleet's entries age out well inside a container's lifetime.
pub(super) const SECRET_MATCH_MEMO_TTL: Duration = Duration::from_secs(300);

/// Upper bound on live [`SecretMatchMemo`] entries. Each entry is a 64-char hex
/// fingerprint + an `Instant` (~100 B), so 32 768 is ~3 MiB — still orders of
/// magnitude below the 64-MiB-per-verify Argon2id allocations this memo exists
/// to avoid, on the deployed 1 GiB container.
///
/// ⚠️ Sizing must assume a FLEET-WIDE live set, not one tenant's. The
/// tenant-scoped adapter routes (`/cargo`, `/npm`, `/pip`, `/brew`) do route one
/// tenant to one Durable Object and therefore one container
/// (`worker/src/index.ts` `idFromName(tenant_id)`), but the OCI `/token`
/// exchange and the public/anonymous surfaces pin ALL tenants onto the SHARED
/// `_oci` / `_anonymous` DOs — the same reason `ARGON2_VERIFY_PERMITS` above
/// talks about OOM-killing "the shared container … a registry outage for ALL
/// tenants". A cap sized for one tenant would put that shared container into
/// at-capacity eviction on a working set it should comfortably hold.
///
/// Be honest about what this buys: raising the cap DEFERS saturation, it does
/// not remove it. Once a shared DO's live set does reach the cap, every
/// cold-miss insert takes the at-capacity branch and pays an O(cap) purge while
/// holding the same `std::sync::Mutex` that each warm `contains()` needs — and a
/// bigger cap makes that pass longer, not shorter. It is acceptable rather than
/// ideal because the insert RATE is bounded by [`ARGON2_VERIFY_PERMITS`]: a cold
/// miss cannot happen without an Argon2id, so inserts arrive at most ~50/s even
/// on a saturated box, which is a small duty cycle for a microsecond-scale pass
/// over an in-memory map. If a shared surface is ever observed to sit at
/// capacity in steady state, the right fix is an O(1)-amortised eviction (or
/// moving the purge off the request path), NOT another cap bump.
pub(super) const SECRET_MATCH_MEMO_CAP: usize = 32_768;

/// One memoised secret-match: the tick at which it was recorded. The value is
/// the *presence* of the key — there is deliberately nothing else to store (see
/// [`SECRET_MATCH_MEMO_TTL`] on why no authorization state may live here).
#[derive(Debug, Clone, Copy)]
struct SecretMatchEntry {
    verified_at: Instant,
}

/// A bounded, TTL'd set of proven Argon2id secret-matches.
///
/// # What is memoised
///
/// Exactly one fact: *"the presented plaintext's secret segment Argon2id-verifies
/// against this exact stored PHC hash"*. The key is a domain-separated,
/// length-prefixed SHA-256 over
/// `(plaintext, token_id, stored_pat_hash, scope, find_only)` — never the
/// plaintext itself, so the map cannot leak a usable secret (same posture as
/// `native_pat_gate::fingerprint`). See [`secret_match_fingerprint`] for why
/// `scope` / `find_only` are in the key even though the memoised fact does not
/// depend on them.
///
/// # Why memoising it is sound
///
/// - **Immutable fact.** Argon2id is deterministic, so the memoised boolean is a
///   pure function of the key. It cannot go stale in the direction that matters
///   (a `true` cannot silently become `false`).
/// - **Re-hash invalidates automatically.** `stored_pat_hash` is *in* the key, so
///   if the row's `pat_hash` ever changes the old entry is simply unreachable.
/// - **Key-rotation is still enforced per request.** The HMAC fast-reject (stage 1)
///   runs against the *current* signing-key overlap set on EVERY request, before
///   this memo is consulted — dropping a rotated-out key still rejects immediately.
/// - **No authorization state.** tenant / scope / `find_only` / expiry /
///   revocation all come from the fresh per-request D1 row. See
///   [`SECRET_MATCH_MEMO_TTL`].
///
/// # Why it adds no timing oracle
///
/// The pair that must stay indistinguishable is the two ways to earn a **401**:
/// (a) a valid-HMAC token for an unknown/expired/revoked `token_id` → the bounded
/// dummy burn, and (b) a known `token_id` presented with the WRONG secret → a real
/// Argon2id. A wrong secret changes the plaintext, hence the key, so case (b) can
/// **never** hit the memo — it always pays full Argon2id, exactly as before. A hit
/// is only reachable for a plaintext that already verified, which is a request that
/// returns 200 anyway. Parity between the two 401 paths is preserved.
pub(super) struct SecretMatchMemo {
    /// `fingerprint -> entry`. Sync mutex; the guard is NEVER held across an
    /// `.await` (every method here is synchronous and returns before the caller
    /// awaits anything).
    entries: Mutex<HashMap<String, SecretMatchEntry>>,
    /// Entry cap ([`SECRET_MATCH_MEMO_CAP`] in production). A field, not a const,
    /// so a test can shrink it to drive the eviction path deterministically.
    cap: usize,
    /// Entry TTL ([`SECRET_MATCH_MEMO_TTL`] in production). A field, not a const,
    /// so a test can shrink it to drive expiry deterministically.
    ttl: Duration,
}

impl SecretMatchMemo {
    pub(super) fn new(cap: usize, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            cap,
            ttl,
        }
    }

    /// `true` when `fp` has a non-expired proven match.
    ///
    /// FAIL-SAFE: a poisoned lock reports `false`, so the caller runs the full
    /// Argon2id. A bookkeeping fault must only ever cost performance, never
    /// admit an unverified secret — and never reject a legitimate one.
    pub(super) fn contains(&self, fp: &str) -> bool {
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        match entries.get(fp) {
            Some(entry) if entry.verified_at.elapsed() < self.ttl => true,
            Some(_) => {
                // Expired — evict on read so a recurring token cannot accumulate
                // stale entries between eviction sweeps.
                let _ = entries.remove(fp);
                false
            }
            None => false,
        }
    }

    /// Record a proven match for `fp`, bounding the map at [`Self::cap`].
    ///
    /// On a full map a single pass drops every expired entry and remembers the
    /// oldest survivor; if that pass freed nothing, the oldest survivor is
    /// evicted so the insert still lands. O(n) only on the insert that finds the
    /// map full — never on a hit, and never on an insert with headroom.
    ///
    /// FAIL-SAFE: a poisoned lock silently skips the insert (the next request
    /// simply re-runs Argon2id).
    pub(super) fn insert(&self, fp: String) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= self.cap && !entries.contains_key(&fp) {
            let now = Instant::now();
            let ttl = self.ttl;
            let mut oldest: Option<(String, Instant)> = None;
            entries.retain(|key, entry| {
                let live = now.duration_since(entry.verified_at) < ttl;
                // `Option::is_none_or` would read better but is stable only
                // since 1.82; this crate's MSRV is 1.80 (clippy::incompatible_msrv).
                let is_oldest_so_far = match oldest.as_ref() {
                    Some((_, at)) => entry.verified_at < *at,
                    None => true,
                };
                if live && is_oldest_so_far {
                    oldest = Some((key.clone(), entry.verified_at));
                }
                live
            });
            // Purging expired entries freed nothing ⇒ evict the oldest survivor
            // so the map stays bounded and the new entry still lands.
            if entries.len() >= self.cap {
                if let Some((key, _)) = oldest {
                    let _ = entries.remove(&key);
                }
            }
        }
        let _ = entries.insert(
            fp,
            SecretMatchEntry {
                verified_at: Instant::now(),
            },
        );
    }

    /// Test-only: live entry count, for asserting the cap actually bounds.
    #[cfg(test)]
    pub(super) fn len_for_test(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }
}

/// Hand-written so the fingerprint set can never reach a log. A derived `Debug`
/// would print every live entry, and the set of fingerprints currently held IS
/// a live-PAT-presence oracle for anyone with log access — the same reason
/// `PatVerifier`'s own `Debug` redacts `signing_keys`. Only the count escapes.
impl std::fmt::Debug for SecretMatchMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretMatchMemo")
            .field(
                "entries",
                &format_args!("[{} REDACTED]", self.entries.lock().map_or(0, |e| e.len())),
            )
            .field("cap", &self.cap)
            .field("ttl", &self.ttl)
            .finish()
    }
}

/// The [`SecretMatchMemo`] key: a domain-separated, length-prefixed SHA-256 over
/// every field of the row that the decision depends on —
/// `(plaintext, token_id, stored_pat_hash, scope, find_only)`.
///
/// Length-prefixing every field makes the pre-image unambiguous (no
/// concatenation-boundary confusion between two different tuples), and the
/// domain tag keeps this fingerprint from ever colliding with another
/// SHA-256-of-a-token use in the codebase (e.g. `native_pat_gate::fingerprint`).
/// Only the digest is retained, so the memo never holds recoverable secret
/// material.
///
/// `scope`, `find_only` and `runner_job` are in the key even though the gate
/// re-reads them fresh every request and the memoised fact does not depend on
/// them. They are here to close the last latency tail: without them, a PAT that
/// verified successfully and was then *downgraded* in D1 (rather than revoked)
/// would keep hitting the memo for the rest of the TTL and 401 in ~0 ms, while a
/// *revoked* PAT still pays the slow dummy burn — distinguishing "downgraded"
/// from "revoked" on latency alone. With them in the key, any change to any of
/// those fields makes the old entry unreachable, exactly as a `pat_hash` change
/// already does, and the two rejections stay uniform.
///
/// `runner_job` joined them when the 0086 marker started narrowing an operation
/// on this plane (`routes/cargo.rs` DELETE). It is a decision field now, so the
/// rule "every field the row can decide with is bound into the key" keeps
/// holding — the domain tag moved v2 → v3 accordingly, which costs exactly one
/// cold verification per live token at deploy and nothing after.
pub(super) fn secret_match_fingerprint(
    plaintext: &str,
    token_id: &str,
    pat_hash: &str,
    scope: &str,
    find_only: bool,
    runner_job: bool,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/adapter-pat/secret-match/v3\0");
    let find_only = if find_only { "1" } else { "0" };
    let runner_job = if runner_job { "1" } else { "0" };
    for part in [plaintext, token_id, pat_hash, scope, find_only, runner_job] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// The [`FlightGroup`] key for the None-row dummy burn: a domain-separated
/// SHA-256 over the plaintext alone.
///
/// # Why the plaintext alone, and why that is the SAME key the hot path uses
///
/// The two arms must coalesce **identically for the same plaintext whether or
/// not the D1 row exists** — otherwise `token_id` liveness becomes observable in
/// the concurrency dimension (the finding that killed the first attempt at this
/// fix). The hot path keys on [`secret_match_fingerprint`], i.e. on
/// `(plaintext, token_id, stored pat_hash, scope, find_only)`. Every extra field
/// is, at any instant, a **pure function of the plaintext**: `token_id` is parsed
/// out of the plaintext itself (stage 1), and `pat_hash` / `scope` / `find_only`
/// are whatever the single D1 row for that `token_id` holds. So both arms key on
/// a pure function of the plaintext,
/// and N concurrent copies of ONE plaintext collapse to exactly ONE Argon2id on
/// either arm. The keys need not be *equal* across the arms — they must only
/// *partition identically*, and they do. They are deliberately in SEPARATE maps
/// with separate domain tags so a flight can never hand a result from one arm to
/// a waiter on the other (a token revoked mid-burst must not join the hot-path
/// flight that started before the revocation).
pub(super) fn dummy_burn_fingerprint(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/adapter-pat/dummy-burn/v1\0");
    hasher.update((plaintext.len() as u64).to_be_bytes());
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// One in-flight, cloneable unit of coalesced work. `Arc<..>` so every joiner
/// clones one heap result; `Shared` so one poll drives the single inner run for
/// all joiners.
pub(super) type SharedFlight<T> = Shared<BoxFuture<'static, Arc<T>>>;

/// Upper bound on retained [`FlightGroup`] entries before an insert purges the
/// resolved ones.
///
/// A flight is normally removed by the first awaiter to observe it resolved, so
/// the steady-state map holds only genuinely in-flight work — a handful of
/// entries, bounded in practice by [`ARGON2_VERIFY_PERMITS`] plus whatever is
/// inside its 250 ms admission wait. The cap exists for the one case that leaks:
/// if EVERY awaiter of a flight is dropped (client disconnect cancels the
/// request future) after the flight resolved but before anyone removed it, the
/// resolved entry lingers until some later request for the same key replaces it
/// — which may never come. 8192 × (a 64-char hex key + a resolved `Shared`) is
/// ~2 MiB worst case, and the purge that bounds it drops only RESOLVED entries,
/// which are pure garbage (a joiner already refuses to reuse them).
pub(super) const FLIGHT_GROUP_CAP: usize = 8_192;

/// A **no-cache single-flight** over a keyed unit of async work — the same shape
/// [`SingleFlightPatLookup`] uses for the D1 row read, generalised so the
/// Argon2id stage can use it too.
///
/// The first caller for a key LEADS: it builds the future and drives it. Every
/// concurrent caller for that key JOINS the leader's [`Shared`] future and
/// resolves the instant the ONE run completes. There is deliberately **no queue
/// and no lock to wait behind**: the N-th joiner waits exactly as long as the
/// 1st, not N× as long. That is the property that distinguishes this from a
/// mutex — a mutex converts a burst into an N-deep FIFO whose tail waits
/// unboundedly, in front of the very load-shed that exists to bound it.
///
/// # It is NOT a cache
///
/// The flight is removed as soon as it resolves, and an awaiter that finds an
/// already-RESOLVED flight REFUSES to reuse it and leads a fresh run instead. So
/// a result is only ever shared with callers that were **concurrent with the run
/// that produced it** — never with one that started afterwards. Anything that
/// must be re-decided per request (here: the D1 row, hence revocation, expiry,
/// tenant and scope) is read OUTSIDE the flight and is unaffected.
pub(super) struct FlightGroup<T> {
    /// `key -> in-flight shared run`. Sync mutex; the guard is NEVER held across
    /// an `.await` (we clone the `Shared` out, then drop the guard).
    inflight: Mutex<HashMap<String, SharedFlight<T>>>,
    /// Entry cap ([`FLIGHT_GROUP_CAP`] in production). A field, not a const, so a
    /// test can shrink it to drive the purge path deterministically.
    cap: usize,
}

impl<T: Send + Sync + 'static> FlightGroup<T> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            inflight: Mutex::new(HashMap::new()),
            cap,
        }
    }

    /// Join the UNRESOLVED flight for `key`, or lead a fresh one built by
    /// `lead`. `lead` is invoked ONLY when leading, so a joiner never builds
    /// (or runs) the work. `on_abort` supplies the outcome if the work's task
    /// dies (panic/abort) — pass the type's fail-CLOSED variant.
    ///
    /// # The work is SPAWNED, and that is load-bearing
    ///
    /// A `Shared` future is driven only by whoever polls it, so if every awaiter
    /// goes away — one client disconnecting is enough, since an unbursted
    /// request has exactly one awaiter — nothing would ever poll it again. The
    /// map still holds a clone, so the inner future would not be dropped either;
    /// it would simply be frozen wherever it was parked. If that happened to be
    /// the per-tenant acquire, it would be frozen **holding a global Argon2id
    /// permit**, with its own `tokio::time::timeout` unable to fire (timers need
    /// polling) — pinning a permit until some later request for the same key
    /// happened to resume it. Uncoalesced code has no such hazard: dropping the
    /// request future drops the permit by RAII.
    ///
    /// Spawning restores that property and strengthens it. The one run is owned
    /// by the runtime, so it always completes on its own: the bounded waits fire,
    /// the permits are released, and the result is recorded, regardless of
    /// whether anybody is still listening.
    ///
    /// FAIL-SAFE: a poisoned lock runs the work inline, uncoalesced — correct,
    /// merely unshared, and cancellation-safe in the pre-coalescing way. A
    /// bookkeeping fault must only ever cost throughput.
    pub(super) async fn run<F>(&self, key: &str, lead: F, on_abort: fn(String) -> T) -> Arc<T>
    where
        F: FnOnce() -> BoxFuture<'static, T>,
    {
        let shared = {
            let Ok(mut map) = self.inflight.lock() else {
                return Arc::new(lead().await);
            };
            match map.get(key) {
                // Only JOIN a flight that has NOT resolved — never reuse a
                // completed run (see "It is NOT a cache" above).
                Some(existing) if existing.peek().is_none() => existing.clone(),
                _ => {
                    // Bound the map before inserting. Only RESOLVED entries are
                    // dropped: they are unreachable to joiners anyway, so this
                    // can never disrupt in-flight work. If nothing is resolved
                    // the map grows transiently — bounded by live concurrency,
                    // itself bounded by the permits below.
                    if map.len() >= self.cap {
                        map.retain(|_, flight| flight.peek().is_none());
                    }
                    // Build AND spawn here, synchronously under the guard, so the
                    // run is owned by the runtime from the instant it is
                    // published — never contingent on a caller polling it. (Only
                    // the resulting `'static` future is moved, so `lead` itself
                    // never has to be `Send + 'static`.)
                    let handle = tokio::spawn(lead());
                    let fut: SharedFlight<T> = async move {
                        Arc::new(match handle.await {
                            Ok(outcome) => outcome,
                            Err(e) => on_abort(format!("verify flight aborted: {e}")),
                        })
                    }
                    .boxed()
                    .shared();
                    let _ = map.insert(key.to_owned(), fut.clone());
                    fut
                }
            }
        };

        let result = shared.await;

        // Retire the now-resolved flight so the NEXT call for this key is fresh.
        // EVERY awaiter attempts this (not just the leader): the leader's own
        // task may have been cancelled mid-flight, in which case a joiner is
        // what drove the work to completion and must do the cleanup.
        // Guard: remove ONLY if the entry currently under this key is resolved —
        // a fresh leader may already have replaced it with a new unresolved
        // flight, which must be left alone.
        if let Ok(mut map) = self.inflight.lock() {
            if let Some(cur) = map.get(key) {
                if cur.peek().is_some() {
                    let _ = map.remove(key);
                }
            }
        }

        result
    }

    /// Test-only: live entry count, for asserting the map does not retain
    /// resolved flights.
    #[cfg(test)]
    pub(super) fn len_for_test(&self) -> usize {
        self.inflight.lock().map(|m| m.len()).unwrap_or(0)
    }
}

/// Hand-written for the same reason [`SecretMatchMemo`]'s is: the live key set
/// is a PAT-presence oracle for anyone with log access. Only the count escapes.
impl<T> std::fmt::Debug for FlightGroup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlightGroup")
            .field(
                "inflight",
                &format_args!("[{} REDACTED]", self.inflight.lock().map_or(0, |m| m.len())),
            )
            .finish()
    }
}

/// Outcome of ONE coalesced hot-path Argon2id run, cloned to every joiner.
///
/// Carrying the outcome (rather than re-deciding per joiner) is sound because
/// every joiner shares the flight KEY — the same `(plaintext, token_id, stored
/// pat_hash, scope, find_only)` tuple — and the outcome is a pure function of
/// exactly that tuple. No joiner receives a decision its own inputs did not
/// already determine. Since the key binds `scope` and `find_only` too, joiners
/// are guaranteed to reach the SAME step-4 scope verdict as the leader; and
/// everything the key does NOT bind (tenant, expiry, revocation) is read per
/// request, outside the flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VerifyFlight {
    /// Argon2id proved the presented secret matches the stored PHC hash.
    Proven,
    /// Argon2id ran and the secret did NOT match ⇒ uniform `InvalidPat`.
    Rejected,
    /// Fail-CLOSED: a bounded permit acquire shed, or the blocking join faulted.
    Backend(String),
}

/// Outcome of ONE coalesced dummy burn. Exactly two outcomes are observable,
/// and they mirror [`VerifyFlight`]'s so the two 401 arms cannot diverge:
/// a burn that RAN is the uniform `InvalidPat`, and a burn that was SHED is
/// `Backend("pat verifier overloaded")` — see
/// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BurnFlight {
    /// The dummy Argon2id ran (timing parity preserved) ⇒ uniform `InvalidPat`.
    Burned,
    /// Fail-CLOSED: a bounded permit acquire shed at EITHER tier (global pool
    /// or the shared [`UNKNOWN_TOKEN_BUCKET`] sub-cap), or the flight task
    /// faulted. Identical to [`VerifyFlight::Backend`] on the row-FOUND arm,
    /// which is what keeps the shed free of a row-existence oracle.
    Backend(String),
}

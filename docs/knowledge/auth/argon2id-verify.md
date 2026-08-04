---
type: "AuthMechanism"
title: "Argon2id adapter-plane verification + scope"
description: "The container's deep PAT possession proof — bounded Argon2id, memoised on the immutable secret-match only, a constant-time timing-burn for unknown tokens, and the fail-closed cache scope gate."
source_files:
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "10b4aad1b605f80fc6f2729662476569aae65939"
provenance: "AUTHORED"
tags: ["auth", "pat", "argon2id", "scope", "dos"]
timestamp: "2026-06-26T00:00:00Z"
---

# Argon2id adapter-plane verification + scope

Argon2id is the expensive, memory-hard half of PAT verification — the step that actually proves the
caller holds the random secret, not just a valid HMAC signature. It runs only at the bottom of the
container pipeline, after the HMAC fast-reject and the D1 row lookup have already filtered out forged and
nonexistent tokens. Because each Argon2id verify allocates tens of MiB, the verifier is wrapped in two
concurrency bounds (global + per-tenant) and an unknown-token timing-burn, so neither a token-enumeration
oracle nor a flood of valid-HMAC garbage can exploit it. Success then passes through a fail-CLOSED scope
gate before any tenant is returned.

Because Argon2id is deliberately expensive (a LOWER BOUND of ~0.15 CPU-s at the OWASP-2024 cost —
measured with the C reference implementation on an Apple-silicon core, so the pure-Rust crate on a
fraction of an x86 vCPU is slower) and the production container runs at 0.5 vCPU, paying it per
request capped a tenant's adapter plane at roughly 3 requests/second — so the verify is
**memoised**. What is memoised is ONLY the immutable boolean
"this plaintext Argon2id-matches this exact stored PHC hash", never an authorization decision: the
HMAC fast-reject and the expiry/revocation-filtered D1 row read still run on every single request.
That is what makes this memo different in kind from the two caches it sits beside, which buy the
same saving by caching the DECISION and skipping the row: `native_pat_gate` accepts a ≤5 s
revocation window, and the Worker's `pat_verify_cache` accepts up to 60 s on its L2 KV layer.

A memo only helps once something has already paid, so a COLD container's first burst — a `cargo -jN`
build's opening N requests, all bearing one PAT — missed it N times at once, and everything behind
the first verify sheds at the 250 ms permit wait. Concurrent misses are therefore **coalesced on a
shared future**: one run per key, every waiter resolving together, under one permit. Two properties
make that safe rather than merely fast. The coalescing is **mirrored onto the unknown-`token_id`
arm**, because coalescing only the row-FOUND arm would make a burst cost ~1×Argon2id for a live
`token_id` and ~N× for a dead one — the token-enumeration oracle the dummy burn exists to close,
re-opened in the concurrency dimension. And the flight proves the **secret only**: the memo write
stays outside it, past the scope gate, so coalescing cannot undo that placement for a whole burst.

# Role

This concept is the deep layer of [the 2-level PAT moat](/auth/pat-moat.md) and the engine behind both
the cache adapters and [the native PAT gate](/auth/hmac-fast-reject.md). The scope half is the
single source of truth for what a `pat.scope` string from [the D1 PAT store](/auth/d1-pat-store.md) is
allowed to do on a cache surface.

# How it works

- After HMAC + D1, the full verify runs `verify_with_hash_multi` on a `spawn_blocking` thread: it
  re-parses, constant-time matches `token_id`, re-checks HMAC, then Argon2id-verifies the secret against
  the stored PHC hash (`crates/corelink-container/src/adapter_pat.rs:1610-1621`).
- Before that blocking call, the verifier consults `SecretMatchMemo` — a bounded (32 768-entry), TTL'd
  (300 s) set of proven secret-matches keyed by a domain-separated, length-prefixed SHA-256 over
  `(plaintext, token_id, stored pat_hash, scope, find_only)`. A hit skips Argon2id entirely and acquires NO permit
  (`crates/corelink-container/src/adapter_pat.rs:1543-1558`; `crates/corelink-container/src/adapter_pat.rs:721-735`).
- A COLD miss is coalesced on a shared future (`FlightGroup`): the first caller for a key leads the one
  Argon2id, concurrent callers join its `Shared` future and all resolve together, and only the leader
  takes a permit. The flight is retired the moment it resolves and a later caller REFUSES to reuse a
  resolved one, so it is a coalescer and not a second cache. The work is SPAWNED, so it always
  completes — a cancelled awaiter can never strand a held permit
  (`crates/corelink-container/src/adapter_pat.rs:955-1014`;
  `crates/corelink-container/src/adapter_pat.rs:1566-1644`).
- The burn arm is coalesced too, on its own key and in its own map. The two keys need not be equal —
  only to PARTITION identically, which they do, because every extra field in the hot-path key is a
  pure function of the plaintext at any instant (`token_id` is parsed out of it; `pat_hash`, `scope`
  and `find_only` are that row's). Separate maps keep a flight from ever handing a result across the
  arms (`crates/corelink-container/src/adapter_pat.rs:844-870`;
  `crates/corelink-container/src/adapter_pat.rs:1364-1460`).
- Concurrent Argon2id work is capped process-wide at 16 permits so a flood of valid-PAT requests cannot
  OOM-kill the shared container (`crates/corelink-container/src/adapter_pat.rs:327`).
- A per-tenant sub-cap (¼ of the global pool, floor 2) keeps one tenant flooding distinct PATs from
  draining all global permits and starving others
  (`crates/corelink-container/src/adapter_pat.rs:349-356`). On the row-FOUND arm the permit acquires
  sit INSIDE the flight, in the unchanged global→per-tenant order, and a joiner takes no permit at
  all — so concurrent Argon2ids for a tenant still equal that tenant's held sub-permits
  (`crates/corelink-container/src/adapter_pat.rs:1581-1609`). The dummy-burn arm is the one
  documented exception to that order — see the bucket bullet below.
- An unknown/expired/revoked `token_id` runs a dummy Argon2id burn for timing parity so latency does not
  leak whether the token exists, then returns the uniform `InvalidPat`
  (`crates/corelink-container/src/adapter_pat.rs:1313-1481`). The D1 read that decides which arm is
  taken is wrapped in the `opat` Server-Timing phase clock — an observability pass-through that adds
  no branch and no await of its own, so neither arm's timing profile moves
  (`crates/corelink-container/src/adapter_pat.rs:1321-1324`).
- That dummy burn is routed through ONE shared synthetic bucket so a leaked-key flood across bogus
  `token_id`s cannot drain the pool via the timing-burn path — and the bucket is taken FIRST and
  NON-BLOCKINGLY (`try_acquire`), inverting the module's global→per-tenant order for this arm alone.
  The order is the load-bearing half: the bucket by itself caps concurrent *burns*, not global-pool
  *occupancy*. With the global permit taken first (as originally written), a request already destined
  to shed still parked one for the full `ARGON2_PERMIT_WAIT` while queued on the full bucket, so this
  arm's hold on the pool was bounded by the flood's arrival rate rather than by the sub-cap. Taking
  the bucket first means a shed holds no global permit at any instant
  (`crates/corelink-container/src/adapter_pat.rs:1407-1422`;
  `crates/corelink-container/src/adapter_pat.rs:570-597`).
- When the burn cannot get a permit the request is SHED — at EITHER tier — and the shed returns
  `Backend("pat verifier overloaded")`, the same string the row-FOUND arms return, not `InvalidPat`
  (`crates/corelink-container/src/adapter_pat.rs:1440-1444`;
  `crates/corelink-container/src/adapter_pat.rs:1417-1421`).
- The final scope gate fail-CLOSES a find-only PAT FIRST — `if row.find_only { return InvalidPat }` runs
  BEFORE the read grant (ADR-0071): a find-only PAT stores the CHECK-safe base `read-only`, but this
  adapter verifier authorizes package-manager reads directly from the D1 `scope` (the OCI `/token`
  exchange has no `x-corelink-scope` header gate), so an un-rejected `read-only` base would grant e.g.
  `docker pull`. Only then does the gate fail CLOSED unless the D1 `scope` grants cache read, and finally
  surface the write bit for credential-minting callers to downscope
  (`crates/corelink-container/src/adapter_pat.rs:1659-1665`).
- The scope vocabulary lives in `scope.rs`: `requires_cache_read` / `requires_cache_write` grant by
  exact-token match (`cas:rw`/`read-write`/`admin` etc.), never substring
  (`crates/corelink-container/src/scope.rs:67-95`).
- `requires_find_missing` grants the `FindMissingBlobs` existence-probe capability to an explicit
  find-missing token (`find-missing`/`cache:find-missing`) OR any read grant — read is a SUPERSET of
  find-missing (ADR-0071), so pre-ADR-0071 read PATs keep working while a find-only PAT probes existence
  and nothing else (`crates/corelink-container/src/scope.rs:107-110`).

# Invariants

- Argon2id concurrency is bounded; a failed acquire fails CLOSED as `Backend` (503) — on a timeout at
  the global tier and on the row-FOUND arm's sub-cap, immediately on the dummy-burn arm's shared
  bucket — never piling on more 64-MiB allocations
  (`crates/corelink-container/src/adapter_pat.rs:1572-1593`).
- A dummy-burn request that SHEDS holds NO global permit at any instant. The shared
  `UNKNOWN_TOKEN_BUCKET` is acquired first and non-blockingly, so this arm's global-pool footprint is
  the sub-cap and not the flood's arrival rate. The bucket alone does not give that property — the
  ORDER does; with the global permit taken first, a shedding request pinned one for the whole
  `ARGON2_PERMIT_WAIT`. Pinned by `a_shed_on_the_shared_burn_bucket_holds_no_global_permit`, which
  asserts pool CAPACITY rather than a status code, because both orders shed with the identical
  message (`crates/corelink-container/src/adapter_pat.rs:1407-1422`).
- `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` — the shed is SYMMETRIC across D1 row existence. Under
  saturation every outcome is `Backend("pat verifier overloaded")` ⇒ 503, whether or not the
  `token_id` has a live row; outside saturation every rejection is `InvalidPat` ⇒ 401. An asymmetric
  shed would be a row-existence oracle *stronger* than the latency one the dummy burn closes, because
  on a shed the burn is skipped entirely — so the status must carry no information either. The two
  arms that stay `InvalidPat` are deliberate: the HMAC fast-reject sits upstream of every permit, and
  the terminal rejection after the burn actually ran IS the uniform 401 path
  (`crates/corelink-container/src/adapter_pat.rs:1440-1444`;
  `crates/corelink-container/src/adapter_pat.rs:1417-1421`;
  `crates/corelink-container/src/adapter_pat.rs:1478`;
  `crates/corelink-container/src/adapter_pat.rs:1309-1310`).
- Coalescing does not weaken that invariant — it narrows the surface. The acquires now live INSIDE the
  flight, so a burst of one plaintext takes ONE trip through them instead of N, and every joiner of a
  shed flight receives the identical `Backend` its leader did
  (`crates/corelink-container/src/adapter_pat.rs:1461-1480`).
- Coalescing adds no wait that the admitted path does not already have: a shared future has no queue,
  so the N-th joiner waits exactly as long as the 1st, and that wait is `ARGON2_PERMIT_WAIT` twice plus
  one Argon2id. The load-shed still bounds admission — it now lives inside the flight and fires
  identically for every joiner (`crates/corelink-container/src/adapter_pat.rs:955-1014`).
- A joiner can only receive a decision its OWN inputs already determined: the flight key IS the
  `(plaintext, token_id, pat_hash, scope, find_only)` tuple the outcome is a pure function of. Because
  the key binds `scope` and `find_only`, a burst cannot straddle the scope gate — every joiner reaches
  its leader's stage-4 verdict, so a scope-rejected burst leaves the memo empty exactly as a single
  scope-rejected request does (`crates/corelink-container/src/adapter_pat.rs:1543-1558`).
- An empty/missing/unrecognized scope grants NOTHING — both read and write return `false`
  (`crates/corelink-container/src/scope.rs:73-95`).
- Self-serve scope classification is exact-token and fail-CLOSED: unknown grammar can never silently map
  to a privilege. The accepted vocabulary is the canonical corelink-pat wire form the data plane
  enforces — `cache:r`/`cache:w` (`SCOPE_CACHE_R`/`SCOPE_CACHE_RW`) plus the `cas:*`/long-form aliases,
  and (ADR-0071) `cache:find-missing`/`find-missing` (`SCOPE_CACHE_FIND`). `classify_requested_scopes`
  returns the least-privilege class covering the request: `admin`→`Admin` (never self-serve), any
  write→`ReadWrite`, any read→`ReadOnly`, an explicit find-only request→the new `FindMissing` class,
  and an EMPTY request→`ReadOnly` (back-compat; empty ≠ find-only). Because read is a SUPERSET of
  find-missing, `cache:find-missing` combined with read/write folds into that superset rather than
  minting a mislabeled token (`crates/corelink-container/src/scope.rs:148-194`).
- A permit is acquired only AFTER the cheap HMAC fast-reject (`crates/corelink-container/src/adapter_pat.rs:1309-1310`),
  so a forged token never reaches the permit acquire (`crates/corelink-container/src/adapter_pat.rs:1572-1593`).
- The memo carries NO authorization state. Tenant, scope, `find_only`, expiry and revocation all come
  from the per-request, uncached D1 row (`PAT_LOOKUP_SQL` filters `revoked_at_ms IS NULL` + expiry in
  SQL), so a revocation or scope downgrade applies on the very next request — there is no staleness
  window on this plane (`crates/corelink-container/src/adapter_pat.rs:1313-1481`;
  `crates/corelink-container/src/adapter_pat.rs:1659-1665`).
- A WRONG secret can never hit the memo: the plaintext is part of the key, so the two ways to earn a
  401 — unknown `token_id` (dummy burn) and known `token_id` with a wrong secret (real verify) — both
  still pay full Argon2id, preserving timing parity (`crates/corelink-container/src/adapter_pat.rs:827-842`).
- The memo is populated PAST the step-4 scope gate, never at the end of the Argon2id step. A PAT whose
  secret is correct but whose scope is rejected (`find_only`, or no cache grant) therefore re-pays
  Argon2id on EVERY request. Memoising it earlier would 401 it slowly once and in ~0 ms thereafter,
  sorting "live credential, insufficient scope" from "dead / unknown / wrong secret" — and a revoked
  PAT from a merely scope-downgraded one — on latency alone, with no grant of any kind
  (`crates/corelink-container/src/adapter_pat.rs:1667-1692`).
- `scope` and `find_only` are in the memo KEY, so a scope DOWNGRADE (as opposed to a revocation)
  makes the old entry unreachable. Without them a downgraded PAT would 401 in ~0 ms off a stale hit
  while a revoked one still paid the slow dummy burn — separating "downgraded" from "revoked" on
  latency (`crates/corelink-container/src/adapter_pat.rs:827-842`).
- The memoised fact is a pure function of its key and cannot become false: the stored PHC hash is IN the
  key, so a re-hashed row makes the old proof unreachable rather than stale
  (`crates/corelink-container/src/adapter_pat.rs:1543-1558`).

# Gotchas

- ⚠️ A dummy timing-burn that RUNS still consumes a global permit (a shed one does not — see the
  invariant above); under sustained overload the burn is SKIPPED
  and the request is shed. The lost timing parity is acceptable because every request shares its fate —
  but only because the shed is uniform in the OTHER dimension too. This arm returned `InvalidPat` until
  `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` was registered: with the row-FOUND arm shedding as `Backend`,
  the pair leaked D1 row existence through the HTTP status of anyone who could saturate the pool. BOTH
  tiers of the shed answer `Backend` for that reason — the global pool and the shared
  `UNKNOWN_TOKEN_BUCKET` sub-cap alike — and coalescing kept it that way when it moved the acquires
  inside the flight. Do not "simplify" either arm, or either tier, back to `InvalidPat`: the burn being
  skipped is exactly why the status cannot be allowed to differ (`crates/corelink-container/src/adapter_pat.rs:1313-1481`).
- ⚠️ **`SECRET_MATCH_MEMO_TTL` (300 s) is NOT a revocation window — do not "harmonise" it down to the
  5 s used by `native_pat_gate::VERIFY_CACHE_TTL` and the Worker's `PAT_VERIFY_CACHE_TTL_MS`.** Those two
  cache the authorization DECISION and skip the D1 row on a hit, so their TTL really does bound how long
  a revoked PAT keeps access, which is why they are pinned against `SLO-FRESH-PAT-REVOKE`. This memo
  caches only the immutable secret-match and still reads D1 every request, so its TTL is a memory-hygiene
  bound with no security meaning. Shrinking it buys nothing and re-imposes the Argon2id cost
  (`crates/corelink-container/src/adapter_pat.rs:600-620`).
- ⚠️ **Do not "simplify" the coalescer into a mutex, and do not move the memo write into the flight.**
  An earlier revision single-flighted cold misses on a sharded `tokio::sync::Mutex` and three reviewers
  killed it: the lock sat on the row-found arm only (the enumeration oracle above), its wait was
  unbounded and sat IN FRONT of the 250 ms load-shed, and it was taken upstream of the per-tenant
  sub-permit, so on the shared `_oci` / `_anonymous` DOs one tenant's flood could block another past
  the fairness cap. A shared future has none of those properties — but only because it has no queue,
  mirrors both arms, and keeps the acquires inside the flight. Separately: the flight is where the
  Argon2id proof lands, so writing the memo there is the natural thing to do and is exactly the
  pre-scope-gate placement forbidden above, reintroduced for a whole burst at once. Changing any one
  of these re-opens a finding (`crates/corelink-container/src/adapter_pat.rs:1503-1542`).
- `admin` is treated as a cache-rw superset by the capability checks, but admin *route* authorization is
  a SEPARATE internal-auth gate, not this scope module
  (`crates/corelink-container/src/scope.rs:34-45`).
- `classify_requested_scopes` is the shared truth for the mint escalation gate and the D1 persister; the
  substring-vs-exact-token divergence it closed once let `"writes"` slip through. It must accept the SAME
  faithfully-mintable scope vocabulary the dashboard `KeysClient` sends (the canonical `cache:r`/`cache:w`,
  and now `cache:find-missing` per ADR-0071) — a gap here 401s every self-serve "Create token"
  (`crates/corelink-container/src/scope.rs:148-194`).

# Citations

1. `crates/corelink-container/src/adapter_pat.rs:327` — the global Argon2id concurrency cap (OOM guard).
2. `crates/corelink-container/src/adapter_pat.rs:349-356` — the per-tenant Argon2id sub-cap (fairness).
3. `crates/corelink-container/src/adapter_pat.rs:1407-1422` — the dummy burn routed through the shared synthetic bucket (`UNKNOWN_TOKEN_BUCKET`).
4. `crates/corelink-container/src/adapter_pat.rs:1313-1481` — the None-row constant-time Argon2id timing-burn.
5. `crates/corelink-container/src/adapter_pat.rs:1610-1621` — the Argon2id possession verify on a blocking thread.
6. `crates/corelink-container/src/adapter_pat.rs:1572-1593` — permit-acquire timeout → fail-CLOSED `Backend` (row-FOUND arm).
6b. `crates/corelink-container/src/adapter_pat.rs:1440-1444` — the row-NOT-FOUND global-permit shed, returning the SAME `Backend("pat verifier overloaded")` (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`).
6c. `crates/corelink-container/src/adapter_pat.rs:1417-1421` — the row-NOT-FOUND per-tenant sub-cap shed, likewise `Backend`.
6d. `crates/corelink-container/src/adapter_pat.rs:1478` — the terminal `InvalidPat` AFTER the burn ran (deliberately NOT a shed).
7. `crates/corelink-container/src/adapter_pat.rs:1659-1665` — the scope gate: find-only fail-close FIRST (`if row.find_only`, ADR-0071), then fail-CLOSED read grant + write-bit surfacing.
8. `crates/corelink-container/src/scope.rs:73-95` — fail-CLOSED: empty/missing scope grants nothing (`requires_cache_read`/`_write`).
9. `crates/corelink-container/src/scope.rs:67-95` — exact-token `requires_cache_read` / `requires_cache_write`.
10. `crates/corelink-container/src/scope.rs:107-110` — `requires_find_missing`: find-missing granted by an explicit find token OR any read grant (read ⊇ find, ADR-0071).
11. `crates/corelink-container/src/scope.rs:148-194` — `classify_requested_scopes`: the single, fail-CLOSED scope truth (canonical `cache:r`/`cache:w` + aliases; `cache:find-missing` now accepted → `FindMissing` class, folding into read/write superset).
12. `crates/corelink-container/src/adapter_pat.rs:1543-1558` — the memo consult, keyed on `(plaintext, token_id, stored pat_hash, scope, find_only)`, sitting between the D1 row read and Argon2id.
13. `crates/corelink-container/src/adapter_pat.rs:600-620` — why the memo's TTL is a memory bound and not a revocation window (contrast with the two 5 s decision caches).
14. `crates/corelink-container/src/adapter_pat.rs:658-694` — `SecretMatchMemo`: what is memoised, why it is sound, and why it adds no timing oracle.
15. `crates/corelink-container/src/adapter_pat.rs:827-842` — `secret_match_fingerprint`: domain-separated, length-prefixed SHA-256; only the digest is retained.
16. `crates/corelink-container/src/adapter_pat.rs:746-781` — bounded insert: expired-purge then oldest-survivor eviction, O(n) only when the map is full.
17. `crates/corelink-container/src/adapter_pat.rs:721-735` — `SecretMatchMemo::contains`: TTL check, evict-on-read, fail-safe `false` on a poisoned lock.
18. `crates/corelink-container/src/adapter_pat.rs:892-912` — `FlightGroup`: the no-cache shared-future coalescer, and why it is not a mutex.
19. `crates/corelink-container/src/adapter_pat.rs:955-1014` — `FlightGroup::run`: join-or-lead, the SPAWN that keeps an abandoned run from stranding a permit, resolved flights refused and retired, bounded map, fail-safe uncoalesced run on a poisoned lock.
20. `crates/corelink-container/src/adapter_pat.rs:844-870` — `dummy_burn_fingerprint`: why the two arms' keys partition identically for a given plaintext, and why they live in separate maps.
21. `crates/corelink-container/src/adapter_pat.rs:1566-1644` — the hot-path flight: permits acquired INSIDE it in global→per-tenant order; the proof, and ONLY the proof, lives here.
22. `crates/corelink-container/src/adapter_pat.rs:1364-1460` — the mirrored dummy-burn flight (both arms or neither).
23. `crates/corelink-container/src/adapter_pat.rs:1461-1480` — the shed-uniformity mapping: EVERY shed, at either tier, is `Backend`; only a completed burn is `InvalidPat`.
24. `crates/corelink-container/src/adapter_pat.rs:1503-1542` — the adversarial findings against the earlier mutex, and why the shared future answers each.

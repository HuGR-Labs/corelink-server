---
type: "AuthMechanism"
title: "Argon2id adapter-plane verification + scope"
description: "The container's deep PAT possession proof — bounded Argon2id, memoised on the immutable secret-match only, a constant-time timing-burn for unknown tokens, and the fail-closed cache scope gate."
source_files:
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "a298cd91a4f0429d6e3d31a64191d2b0e87eced5"
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

# Role

This concept is the deep layer of [the 2-level PAT moat](/auth/pat-moat.md) and the engine behind both
the cache adapters and [the native PAT gate](/auth/hmac-fast-reject.md). The scope half is the
single source of truth for what a `pat.scope` string from [the D1 PAT store](/auth/d1-pat-store.md) is
allowed to do on a cache surface.

# How it works

- After HMAC + D1, the full verify runs `verify_with_hash_multi` on a `spawn_blocking` thread: it
  re-parses, constant-time matches `token_id`, re-checks HMAC, then Argon2id-verifies the secret against
  the stored PHC hash (`crates/corelink-container/src/adapter_pat.rs:1148-1155`).
- Before that blocking call, the verifier consults `SecretMatchMemo` — a bounded (32 768-entry), TTL'd
  (300 s) set of proven secret-matches keyed by a domain-separated, length-prefixed SHA-256 over
  `(plaintext, token_id, stored pat_hash, scope, find_only)`. A hit skips Argon2id entirely and acquires NO permit
  (`crates/corelink-container/src/adapter_pat.rs:1099-1110`; `crates/corelink-container/src/adapter_pat.rs:524-538`).
- Concurrent Argon2id work is capped process-wide at 16 permits so a flood of valid-PAT requests cannot
  OOM-kill the shared container (`crates/corelink-container/src/adapter_pat.rs:325`).
- A per-tenant sub-cap (¼ of the global pool, floor 2) keeps one tenant flooding distinct PATs from
  draining all global permits and starving others
  (`crates/corelink-container/src/adapter_pat.rs:347-354`).
- An unknown/expired/revoked `token_id` runs a dummy Argon2id burn for timing parity so latency does not
  leak whether the token exists, then returns the uniform `InvalidPat`
  (`crates/corelink-container/src/adapter_pat.rs:999-1052`).
- That dummy burn is routed through ONE shared synthetic bucket so a leaked-key flood across bogus
  `token_id`s cannot drain the pool via the timing-burn path
  (`crates/corelink-container/src/adapter_pat.rs:1039`).
- When the burn cannot get a permit the request is SHED, and the shed returns
  `Backend("pat verifier overloaded")` — the same string the row-FOUND arms return — not `InvalidPat`
  (`crates/corelink-container/src/adapter_pat.rs:1029`;
  `crates/corelink-container/src/adapter_pat.rs:1041`).
- The final scope gate fail-CLOSES a find-only PAT FIRST — `if row.find_only { return InvalidPat }` runs
  BEFORE the read grant (ADR-0071): a find-only PAT stores the CHECK-safe base `read-only`, but this
  adapter verifier authorizes package-manager reads directly from the D1 `scope` (the OCI `/token`
  exchange has no `x-corelink-scope` header gate), so an un-rejected `read-only` base would grant e.g.
  `docker pull`. Only then does the gate fail CLOSED unless the D1 `scope` grants cache read, and finally
  surface the write bit for credential-minting callers to downscope
  (`crates/corelink-container/src/adapter_pat.rs:1171-1177`).
- The scope vocabulary lives in `scope.rs`: `requires_cache_read` / `requires_cache_write` grant by
  exact-token match (`cas:rw`/`read-write`/`admin` etc.), never substring
  (`crates/corelink-container/src/scope.rs:67-95`).
- `requires_find_missing` grants the `FindMissingBlobs` existence-probe capability to an explicit
  find-missing token (`find-missing`/`cache:find-missing`) OR any read grant — read is a SUPERSET of
  find-missing (ADR-0071), so pre-ADR-0071 read PATs keep working while a find-only PAT probes existence
  and nothing else (`crates/corelink-container/src/scope.rs:107-110`).

# Invariants

- Argon2id concurrency is bounded; an acquire timeout fails CLOSED as `Backend` (503), never piling on
  more 64-MiB allocations (`crates/corelink-container/src/adapter_pat.rs:1122-1135`).
- `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` — the shed is SYMMETRIC across D1 row existence. Under
  saturation every outcome is `Backend("pat verifier overloaded")` ⇒ 503, whether or not the
  `token_id` has a live row; outside saturation every rejection is `InvalidPat` ⇒ 401. An asymmetric
  shed would be a row-existence oracle *stronger* than the latency one the dummy burn closes, because
  on a shed the burn is skipped entirely — so the status must carry no information either. The two
  arms that stay `InvalidPat` are deliberate: the HMAC fast-reject sits upstream of every permit, and
  the terminal rejection after the burn actually ran IS the uniform 401 path
  (`crates/corelink-container/src/adapter_pat.rs:1029`;
  `crates/corelink-container/src/adapter_pat.rs:1041`;
  `crates/corelink-container/src/adapter_pat.rs:1051`;
  `crates/corelink-container/src/adapter_pat.rs:983-984`).
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
- A permit is acquired only AFTER the cheap HMAC fast-reject (`crates/corelink-container/src/adapter_pat.rs:983-984`),
  so a forged token never reaches the permit acquire (`crates/corelink-container/src/adapter_pat.rs:1122-1135`).
- The memo carries NO authorization state. Tenant, scope, `find_only`, expiry and revocation all come
  from the per-request, uncached D1 row (`PAT_LOOKUP_SQL` filters `revoked_at_ms IS NULL` + expiry in
  SQL), so a revocation or scope downgrade applies on the very next request — there is no staleness
  window on this plane (`crates/corelink-container/src/adapter_pat.rs:987-1052`;
  `crates/corelink-container/src/adapter_pat.rs:1171-1177`).
- A WRONG secret can never hit the memo: the plaintext is part of the key, so the two ways to earn a
  401 — unknown `token_id` (dummy burn) and known `token_id` with a wrong secret (real verify) — both
  still pay full Argon2id, preserving timing parity (`crates/corelink-container/src/adapter_pat.rs:630-645`).
- The memo is populated PAST the step-4 scope gate, never at the end of the Argon2id step. A PAT whose
  secret is correct but whose scope is rejected (`find_only`, or no cache grant) therefore re-pays
  Argon2id on EVERY request. Memoising it earlier would 401 it slowly once and in ~0 ms thereafter,
  sorting "live credential, insufficient scope" from "dead / unknown / wrong secret" — and a revoked
  PAT from a merely scope-downgraded one — on latency alone, with no grant of any kind
  (`crates/corelink-container/src/adapter_pat.rs:1179-1197`).
- `scope` and `find_only` are in the memo KEY, so a scope DOWNGRADE (as opposed to a revocation)
  makes the old entry unreachable. Without them a downgraded PAT would 401 in ~0 ms off a stale hit
  while a revoked one still paid the slow dummy burn — separating "downgraded" from "revoked" on
  latency (`crates/corelink-container/src/adapter_pat.rs:630-645`).
- The memoised fact is a pure function of its key and cannot become false: the stored PHC hash is IN the
  key, so a re-hashed row makes the old proof unreachable rather than stale
  (`crates/corelink-container/src/adapter_pat.rs:1099-1110`).

# Gotchas

- ⚠️ The dummy timing-burn still consumes a global permit; under sustained overload the burn is SKIPPED
  and the request is shed. The lost timing parity is acceptable because every request shares its fate —
  but only because the shed is uniform in the OTHER dimension too. This arm returned `InvalidPat` until
  `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` was registered: with the row-FOUND arm shedding as `Backend`,
  the pair leaked D1 row existence through the HTTP status of anyone who could saturate the pool. Do
  not "simplify" either arm back to `InvalidPat` — the burn being skipped is exactly why the status
  cannot be allowed to differ (`crates/corelink-container/src/adapter_pat.rs:999-1052`).
- ⚠️ **`SECRET_MATCH_MEMO_TTL` (300 s) is NOT a revocation window — do not "harmonise" it down to the
  5 s used by `native_pat_gate::VERIFY_CACHE_TTL` and the Worker's `PAT_VERIFY_CACHE_TTL_MS`.** Those two
  cache the authorization DECISION and skip the D1 row on a hit, so their TTL really does bound how long
  a revoked PAT keeps access, which is why they are pinned against `SLO-FRESH-PAT-REVOKE`. This memo
  caches only the immutable secret-match and still reads D1 every request, so its TTL is a memory-hygiene
  bound with no security meaning. Shrinking it buys nothing and re-imposes the Argon2id cost
  (`crates/corelink-container/src/adapter_pat.rs:403-423`).
- `admin` is treated as a cache-rw superset by the capability checks, but admin *route* authorization is
  a SEPARATE internal-auth gate, not this scope module
  (`crates/corelink-container/src/scope.rs:34-45`).
- `classify_requested_scopes` is the shared truth for the mint escalation gate and the D1 persister; the
  substring-vs-exact-token divergence it closed once let `"writes"` slip through. It must accept the SAME
  faithfully-mintable scope vocabulary the dashboard `KeysClient` sends (the canonical `cache:r`/`cache:w`,
  and now `cache:find-missing` per ADR-0071) — a gap here 401s every self-serve "Create token"
  (`crates/corelink-container/src/scope.rs:148-194`).

# Citations

1. `crates/corelink-container/src/adapter_pat.rs:325` — the global Argon2id concurrency cap (OOM guard).
2. `crates/corelink-container/src/adapter_pat.rs:347-354` — the per-tenant Argon2id sub-cap (fairness).
3. `crates/corelink-container/src/adapter_pat.rs:1039` — the dummy burn routed through the shared synthetic bucket (`UNKNOWN_TOKEN_BUCKET`).
4. `crates/corelink-container/src/adapter_pat.rs:999-1052` — the None-row constant-time Argon2id timing-burn.
5. `crates/corelink-container/src/adapter_pat.rs:1148-1155` — the Argon2id possession verify on a blocking thread.
6. `crates/corelink-container/src/adapter_pat.rs:1122-1135` — permit-acquire timeout → fail-CLOSED `Backend` (row-FOUND arm).
6b. `crates/corelink-container/src/adapter_pat.rs:1029` — the row-NOT-FOUND global-permit shed, returning the SAME `Backend("pat verifier overloaded")` (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`).
6c. `crates/corelink-container/src/adapter_pat.rs:1041` — the row-NOT-FOUND per-tenant sub-cap shed, likewise `Backend`.
6d. `crates/corelink-container/src/adapter_pat.rs:1051` — the terminal `InvalidPat` AFTER the burn ran (deliberately NOT a shed).
7. `crates/corelink-container/src/adapter_pat.rs:1171-1177` — the scope gate: find-only fail-close FIRST (`if row.find_only`, ADR-0071), then fail-CLOSED read grant + write-bit surfacing.
8. `crates/corelink-container/src/scope.rs:73-95` — fail-CLOSED: empty/missing scope grants nothing (`requires_cache_read`/`_write`).
9. `crates/corelink-container/src/scope.rs:67-95` — exact-token `requires_cache_read` / `requires_cache_write`.
10. `crates/corelink-container/src/scope.rs:107-110` — `requires_find_missing`: find-missing granted by an explicit find token OR any read grant (read ⊇ find, ADR-0071).
11. `crates/corelink-container/src/scope.rs:148-194` — `classify_requested_scopes`: the single, fail-CLOSED scope truth (canonical `cache:r`/`cache:w` + aliases; `cache:find-missing` now accepted → `FindMissing` class, folding into read/write superset).
12. `crates/corelink-container/src/adapter_pat.rs:1099-1110` — the memo consult, keyed on `(plaintext, token_id, stored pat_hash, scope, find_only)`, sitting between the D1 row read and Argon2id.
13. `crates/corelink-container/src/adapter_pat.rs:403-423` — why the memo's TTL is a memory bound and not a revocation window (contrast with the two 5 s decision caches).
14. `crates/corelink-container/src/adapter_pat.rs:461-497` — `SecretMatchMemo`: what is memoised, why it is sound, and why it adds no timing oracle.
15. `crates/corelink-container/src/adapter_pat.rs:630-645` — `secret_match_fingerprint`: domain-separated, length-prefixed SHA-256; only the digest is retained.
16. `crates/corelink-container/src/adapter_pat.rs:549-584` — bounded insert: expired-purge then oldest-survivor eviction, O(n) only when the map is full.

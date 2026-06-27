---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "How CoreLink rejects forged tokens cheaply at the edge and proves possession deeply in the container."
source_files:
  - "worker/src/lib/internal_auth.ts"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/native_pat_gate.rs"
  - "crates/corelink-pat/src/verify.rs"
checkpoint_sha: "8924bd84e98462a309205d15a3f8747acc997d62"
provenance: "AUTHORED"
tags: ["auth", "pat", "security", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# The 2-level PAT moat

CoreLink authenticates every cache request with a Personal Access Token, but it never pays the full
verification cost on a token that cannot possibly be genuine. The moat is two independent constant-time
layers: a cheap fail-CLOSED gate at the Cloudflare Worker edge that rejects before any expensive work,
and a deep Argon2id possession proof in the Rust container plane that only genuine tokens reach. The two
layers share a design rule — every distinguishable rejection collapses to one uniform answer, so the
wire never tells an attacker *why* a token failed — the executed uniform `VerifyError::InvalidPat`
returns at `crates/corelink-container/src/adapter_pat.rs:543`, `:598`, `:650`, `:656`.
This is why a flood of **bad-HMAC** garbage tokens cannot exhaust the container's Argon2id pool: such
garbage fails the HMAC fast-reject pre-D1 and never reaches Argon2id. The qualifier matters under the
leaked-`PAT_SIGNING_KEY` threat model, though: a forged token that carries a VALID HMAC but names a
NON-existent `token_id` survives the fast-reject, misses the D1 row, and DOES reach a timing-parity
**dummy burn** that itself runs Argon2id (`crates/corelink-container/src/adapter_pat.rs:558-599`; the
dummy verify at `crates/corelink-container/src/adapter_pat.rs:591`). That path is NOT unbounded: the
burn is gated by the SAME global Argon2id permit as the hot path (acquired with a bounded wait, skipped
on overload) AND by a shared synthetic per-tenant sub-permit through the `UNKNOWN_TOKEN_BUCKET`
(`crates/corelink-container/src/adapter_pat.rs:586`), so a leaked-key flood across bogus `token_id`s
cannot drain the global pool via this path — on sub-cap saturation it skips the burn and fails CLOSED
uniformly (finding #12).

# Role

The moat is the trust spine for all six cache surfaces. The Worker resolves a request's tenant from its
PAT under a tight CPU budget (HMAC fast-fail + D1 expiry lookup) and forwards a server-trusted
`x-corelink-tenant-id`; the container re-runs the full Option-B verification rather than trusting
that header blindly, so a compromised or misconfigured Worker cannot grant cache access on its own —
the executed re-run is the Argon2id possession verify on a blocking thread the Worker skips
(`crates/corelink-container/src/adapter_pat.rs:602-650`). The same cheap-then-deep shape protects the
control surfaces (mint, introspect) at the Worker edge.

**Qualifier — the "full" verification is cached on the data plane.** On the six cache surfaces the
native gate is `NativePatGate::verify`, which fronts the full Argon2id path with a 5-second verify
cache: a cache HIT returns `Ok` after only a plain tenant-equality check (a String `==`, not a
constant-time compare — immaterial, since the tenant-id is not a secret) and **SKIPS D1 + Argon2id**
for the TTL (`crates/corelink-container/src/native_pat_gate.rs:170-176`). So "re-runs the
FULL Option-B verification" is true on a cache MISS (and on the first request of each 5s window, which
single-flights the D1 + Argon2id round) but NOT on every request — a recently-verified PAT is admitted
on the tenant-equality check alone until its cache entry expires.

# How it works

- The Worker-edge internal gate compares a presented secret against the expected one in constant time,
  copying into a fixed buffer so neither length nor content leaks via an early branch — one
  `timingSafeEqual` over equal-length buffers AND a single length-equality bit
  (`worker/src/lib/internal_auth.ts:112-127`). This `constantTimeSecretEqual` is the SHARED edge
  primitive: `index.ts` reuses its exact padded semantics for both the `/_internal/*` proxy gate and the
  #11 fan-out-metering check, so no Worker surface hand-rolls a compare.
- The key the edge gate verifies against is itself selected fail-CLOSED and per-consumer:
  `resolveConsumerKey` returns the consumer-specific internal-auth key only when it is ≥
  `MIN_INTERNAL_AUTH_KEY_LEN` (32 bytes), else falls back to the shared `CORELINK_INTERNAL_AUTH_KEY`
  (also length-floored), else `null` → the caller MUST fail CLOSED. A sub-floor dedicated key is treated
  as ABSENT (it falls through to the shared key), never as a weakened gate — mirroring the Rust split so
  one leaked consumer secret cannot unlock every internal surface
  (`worker/src/lib/internal_auth.ts:75-89`).
- That edge gate is fail-CLOSED: an unbound or too-short secret makes the endpoint unavailable (403),
  a missing/wrong header is 401, and only an exact match returns `null` to let the caller proceed
  (`worker/src/lib/internal_auth.ts:152-164`).
- In the container the **first** verification step is the HMAC fast-reject. The call site in
  `verify_capability` rejects a bad signature pre-D1 (`crates/corelink-container/src/adapter_pat.rs:538-543`),
  but that line only DELEGATES — the HMAC check itself EXECUTES in the callee `verify_hmac_only_multi`,
  which parses the plaintext then runs `verify_hmac_sig_multi` over the rotation key set and fails CLOSED
  on a bad/absent signature (`crates/corelink-pat/src/verify.rs:119-125`). So a forged token drives no D1
  cost and consumes no Argon2id permit. (Per the contract §3 authoring rule, the invariant is anchored on
  the executed callee, not only on the delegating adapter line.)
- Only a token that passes HMAC and resolves to a live D1 row reaches the expensive layer: an Argon2id
  verify of the secret segment against the stored PHC hash, run on a blocking thread under a bounded
  permit (`crates/corelink-container/src/adapter_pat.rs:602-650`).

# Invariants

- A token that fails the cheap HMAC fast-reject NEVER reaches the Argon2id layer — the rejection is the
  `verify_hmac_sig_multi` failure inside the executed callee (`crates/corelink-pat/src/verify.rs:119-125`),
  reached from the container call site (`crates/corelink-container/src/adapter_pat.rs:538-543`).
- Both layers are constant-time with no length or content oracle — the edge compare
  (`worker/src/lib/internal_auth.ts:112-127`) and the uniform `VerifyError::InvalidPat` returned by
  EVERY distinguishable container-side rejection (`crates/corelink-container/src/adapter_pat.rs:543`,
  `crates/corelink-container/src/adapter_pat.rs:598`, `crates/corelink-container/src/adapter_pat.rs:650`,
  `crates/corelink-container/src/adapter_pat.rs:656`).
- The edge gate fails CLOSED: an unbound/short secret is unavailable, never an open gate
  (`worker/src/lib/internal_auth.ts:152-158`).

# Gotchas

- The Worker-edge file cited here is the constant-time gate fronting the **internal control surfaces**
  (PAT mint / introspect); the cache hot path does its own HMAC fast-fail in `worker/src/index.ts`
  (named, not in this concept's `source_files`). Both embody the same cheap-then-deep moat principle.
- The native CAS/AC/Bazel/Turbo data plane historically trusted the Worker-injected tenant header and
  skipped Argon2id; that gap is closed defense-in-depth by the native gate — see
  [the native HMAC fast-reject PAT gate](/auth/hmac-fast-reject.md).
- A container CAS 401 means bad HMAC OR no live D1 row, not necessarily a wrong password — the Argon2id
  step is only reached once a row exists. See [the Argon2id verify](/auth/argon2id-verify.md) and
  [the D1 PAT store](/auth/d1-pat-store.md).

# Citations

1. `worker/src/lib/internal_auth.ts:112-127` — the edge constant-time secret compare with no length oracle.
2. `worker/src/lib/internal_auth.ts:152-164` — the fail-CLOSED edge gate (403 unbound / 401 wrong / `null` pass).
3. `crates/corelink-container/src/adapter_pat.rs:5-14` — why the container re-runs full verification (Option B).
4. `crates/corelink-container/src/adapter_pat.rs:543`, `:598`, `:650`, `:656` — every distinguishable rejection returns the uniform `VerifyError::InvalidPat` (HMAC fast-reject, unknown/expired row, Argon2id mismatch, no-cache-scope): no on-the-wire oracle.
5. `crates/corelink-container/src/adapter_pat.rs:538-543` — the HMAC fast-reject CALL SITE (delegates to the callee below), pre-D1, no permit consumed.
5b. `crates/corelink-pat/src/verify.rs:119-125` — the executed HMAC verify `verify_hmac_only_multi`: parses the plaintext then runs `verify_hmac_sig_multi` over the rotation key set and fails CLOSED — the line that PERFORMS the fast-reject the adapter call site delegates to (contract §3 authoring rule).
6. `crates/corelink-container/src/adapter_pat.rs:602-650` — the deep Argon2id possession proof on a blocking thread.
7. `crates/corelink-container/src/adapter_pat.rs:558-599` — the valid-HMAC-but-unknown-`token_id` timing-parity dummy burn (the leaked-key path that DOES reach an Argon2id round), with the dummy verify at `crates/corelink-container/src/adapter_pat.rs:591`.
8. `crates/corelink-container/src/adapter_pat.rs:586` — the dummy-burn bound: a shared synthetic per-tenant sub-permit via `UNKNOWN_TOKEN_BUCKET` (atop the global Argon2id permit), so a leaked-key flood across bogus token_ids cannot drain the pool (finding #12).
9. `worker/src/lib/internal_auth.ts:75-89` — `resolveConsumerKey`: per-consumer key with shared `CORELINK_INTERNAL_AUTH_KEY` fallback, both floored at `MIN_INTERNAL_AUTH_KEY_LEN`=32, else `null` (fail-CLOSED); a sub-floor dedicated key is treated as absent so one leaked consumer secret can't unlock every surface.

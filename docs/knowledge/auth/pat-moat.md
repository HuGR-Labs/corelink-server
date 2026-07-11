---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "How CoreLink rejects forged tokens cheaply at the edge and proves possession deeply in the container."
source_files:
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/index.ts"
  - "worker/src/lib/tenant_suspend_gate.ts"
  - "crates/corelink-container/src/adapter_pat.rs"
checkpoint_sha: "444ce44563a4eb10cb6f8d46cdfbaf0dcaaf0c64"
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
wire never tells an attacker *why* a token failed (`crates/corelink-container/src/adapter_pat.rs:43-47`).
This is why a flood of garbage tokens cannot exhaust the container's Argon2id pool: garbage never gets
there.

# Role

The moat is the trust spine for all six cache surfaces. The Worker resolves a request's tenant from its
PAT under a tight CPU budget (HMAC fast-fail + D1 expiry lookup) and forwards a server-trusted
`x-corelink-tenant-id`; the container re-runs the **full** Option-B verification rather than trusting
that header blindly, so a compromised or misconfigured Worker cannot grant cache access on its own
(`crates/corelink-container/src/adapter_pat.rs:5-14`). The same cheap-then-deep shape protects the
control surfaces (mint, introspect) at the Worker edge. A transient D1 fault during that edge PAT
lookup fails **closed** but maps to a retryable **503**, not a 401 — a DB hiccup must not read as
"bad credentials" (H1).

# How it works

- The Worker-edge internal gate compares a presented secret against the expected one in constant time,
  copying into a fixed buffer so neither length nor content leaks via an early branch — one
  `timingSafeEqual` over equal-length buffers AND a single length-equality bit
  (`worker/src/lib/internal_auth.ts:136-151`).
- That edge gate is fail-CLOSED: an unbound or too-short secret makes the endpoint unavailable (403),
  a missing/wrong header is 401, and only an exact match returns `null` to let the caller proceed
  (`worker/src/lib/internal_auth.ts:158-170`).
- Per-consumer key resolution is `resolveConsumerKey`, which prefers a consumer's dedicated key. A
  genuinely UNSET dedicated key falls back to the shared secret (the common case). A dedicated key that is
  EXPLICITLY SET but sub-floor (<32 chars) is a misconfiguration: it fails LOUD (`console.error`) +
  fail-CLOSED (`null`, so that consumer's gate rejects until fixed) rather than silently widening the
  consumer's blast radius to the shared key (deep-audit C/sub-floor)
  (`worker/src/lib/internal_auth.ts:88-113`).
- The internal consumers now split the DSR surface into TWO authorities. The irreversible physical-erase
  cascade (`/_internal/dsr/*`) gates on the `erase` key, but the per-user DSR legitimacy ANCHOR
  (`/_internal/dsr/anchor`) resolves its OWN dedicated `dsr_anchor` consumer key
  (`CORELINK_DSR_ANCHOR_AUTH_KEY`, held by githugr and DISTINCT from the eraser's key): it is matched by
  an exact-path special-case in `internalConsumerForPath` placed BEFORE the `/_internal/dsr/*` erase
  catch-all (`worker/src/index.ts:275-277`) and resolved by the `dsr_anchor` branch of the
  consumer-key ternary (`worker/src/lib/internal_auth.ts:85-87`). This is an anti-forge two-authority
  split — a leaked erase key cannot pass the anchor gate and vice-versa (least privilege, A6).
- In the container the **first** verification step is the HMAC fast-reject: the plaintext is parsed and
  a bad signature is rejected pre-D1, so a forged token drives no D1 cost and consumes no Argon2id
  permit (`crates/corelink-container/src/adapter_pat.rs:642-647`).
- Only a token that passes HMAC and resolves to a live D1 row reaches the expensive layer: an Argon2id
  verify of the secret segment against the stored PHC hash, run on a blocking thread under a bounded
  permit (`crates/corelink-container/src/adapter_pat.rs:706-754`).

# Invariants

- A token that fails the cheap HMAC fast-reject NEVER reaches the Argon2id layer
  (`crates/corelink-container/src/adapter_pat.rs:642-647`).
- Both layers are constant-time with no length or content oracle — the edge compare
  (`worker/src/lib/internal_auth.ts:136-151`) and the uniform `InvalidPat` collapse in the container
  (`crates/corelink-container/src/adapter_pat.rs:43-47`).
- The edge gate fails CLOSED: an unbound/short secret is unavailable, never an open gate
  (`worker/src/lib/internal_auth.ts:158-164`).

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
- **Availability-vs-auth at the Worker edge: a transient D1 PAT-lookup FAULT now maps to a retryable
  503, NOT a 401.** `extractAuth` wraps the `SELECT … FROM pat` in a try/catch and on any D1 error
  (network partition / DB unavailable) returns the distinct reason `d1_lookup_error`
  (`worker/src/index.ts:1192-1202`). The PAT-gate caller (H1 fix) now maps BOTH
  `signing_key_not_configured` AND `d1_lookup_error` to `503 authentication service unavailable`
  (`worker/src/index.ts:2498-2503`) — a D1 hiccup is a TRANSIENT infra fault, not a bad credential, so
  surfacing it as 401 would make every client see "bad credentials" (spurious PAT rotation / on-call
  chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` / `invalid_*`)
  still fall through to `401`. Therefore the gotcha above ("401 = bad HMAC OR no live D1 row") stays
  COMPLETE for the worker edge — a transient D1 fault is NOT a cause of a 401 there; it is a 503. The
  in-line `catch` comment at `worker/src/index.ts:1199-1201` now correctly states that the caller maps
  `d1_lookup_error` to a 503 (it previously lied — "for now we 401 to fail-closed"); the cited line
  numbers shifted after the Artifact 1 `/v1/public/*`
  attestation-verifier route arm was added above this handler, again when the CF-6 audit-chain
  signing env vars were declared on the `Env` type, when the WP4 Sentry `beforeSend`
  PII/secret scrubber import was added at the top of the module, when the
  `/internal/v1/auth/resolve-tenant` fabric route was added to `matchRoute`, and most recently when the
  multi-region "route to the LOCAL (this-region) container" DO-forward block was inserted above the
  per-tenant DO forward (`worker/src/index.ts:1843-1845`)). Both the D1-fault and the
  `signing_key_not_configured` config-fault
  are retryable 503s; the edge still fails CLOSED (security > availability) for every credential-shaped
  failure.
- **A valid, unexpired PAT is not sufficient — the tenant fast-suspend gate (go-live G4).** After the D1
  PAT row resolves, `extractAuth` runs one more arm: a tenant that has been suspended or erased
  (`tenant_offboarding_state.state ∈ {suspended, erased}`) is denied even though its PAT is still
  cryptographically valid, so an abusive/offboarded tenant is fast-denied on the customer CAS/AC hot path
  without waiting for every one of its PATs to be individually revoked
  (`worker/src/index.ts:1231-1232`). The check is a single-flight, ~30s-TTL cached D1 read
  (`isTenantSuspended`, mirroring the CachedTierResolver shape) so it adds no uncached per-request D1
  round-trip; it fails **OPEN** on a transient D1 fault (availability), but a KNOWN-suspended cached value
  still denies (`worker/src/lib/tenant_suspend_gate.ts:129-159`). The caller maps the distinct
  `tenant_suspended` reason to **403** (an authorization denial, fail-closed), separate from the 401
  bad-credential arms and the 503 transient-infra arms (`worker/src/index.ts:2513`).

# Citations

1. `worker/src/lib/internal_auth.ts:136-151` — the edge constant-time secret compare with no length oracle.
2. `worker/src/lib/internal_auth.ts:158-170` — the fail-CLOSED edge gate (403 unbound / 401 wrong / `null` pass).
2a. `worker/src/lib/internal_auth.ts:88-113` — `resolveConsumerKey`: prefers a consumer's dedicated key; a SET-but-sub-floor (<32-char) dedicated key fails LOUD + fail-CLOSED (null) instead of silently widening to the shared key (deep-audit C/sub-floor); a genuinely UNSET dedicated key still falls back to shared.
2b. `worker/src/lib/internal_auth.ts:85-87` — the `dsr_anchor` branch of the consumer-key ternary (`CORELINK_DSR_ANCHOR_AUTH_KEY`).
2c. `worker/src/index.ts:275-277` — `internalConsumerForPath` special-cases `/_internal/dsr/anchor` → `dsr_anchor` before the `/_internal/dsr/*` erase catch-all.
3. `crates/corelink-container/src/adapter_pat.rs:5-14` — why the container re-runs full verification (Option B).
4. `crates/corelink-container/src/adapter_pat.rs:43-47` — uniform `InvalidPat`: no on-the-wire oracle.
5. `crates/corelink-container/src/adapter_pat.rs:642-647` — the HMAC fast-reject, pre-D1, no permit consumed.
6. `crates/corelink-container/src/adapter_pat.rs:706-754` — the deep Argon2id possession proof on a blocking thread.
7. `worker/src/lib/tenant_suspend_gate.ts:129-159` — `isTenantSuspended`: the single-flight, ~30s-TTL cached `tenant_offboarding_state` D1 read that fails OPEN on a D1 fault but denies on a KNOWN-suspended cached value.
8. `worker/src/index.ts:1231-1232` — the `extractAuth` fast-suspend arm: a valid PAT whose tenant is suspended/erased returns `tenant_suspended` (G4).

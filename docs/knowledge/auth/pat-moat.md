---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "How CoreLink rejects forged tokens cheaply at the edge and proves possession deeply in the container."
source_files:
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/index.ts"
  - "crates/corelink-container/src/adapter_pat.rs"
checkpoint_sha: "f62b1fede0d5c1db4ce0f8cc8f9a4dd5b7117fc7"
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
  (`worker/src/lib/internal_auth.ts:112-127`).
- That edge gate is fail-CLOSED: an unbound or too-short secret makes the endpoint unavailable (403),
  a missing/wrong header is 401, and only an exact match returns `null` to let the caller proceed
  (`worker/src/lib/internal_auth.ts:152-164`).
- Per-consumer key resolution is `resolveConsumerKey`, which prefers a consumer's dedicated key but
  treats a too-short dedicated key as ABSENT and falls back to the shared secret — so a mis-set
  per-consumer key degrades to the shared gate rather than failing open
  (`worker/src/lib/internal_auth.ts:73`).
- In the container the **first** verification step is the HMAC fast-reject: the plaintext is parsed and
  a bad signature is rejected pre-D1, so a forged token drives no D1 cost and consumes no Argon2id
  permit (`crates/corelink-container/src/adapter_pat.rs:538-543`).
- Only a token that passes HMAC and resolves to a live D1 row reaches the expensive layer: an Argon2id
  verify of the secret segment against the stored PHC hash, run on a blocking thread under a bounded
  permit (`crates/corelink-container/src/adapter_pat.rs:602-650`).

# Invariants

- A token that fails the cheap HMAC fast-reject NEVER reaches the Argon2id layer
  (`crates/corelink-container/src/adapter_pat.rs:538-543`).
- Both layers are constant-time with no length or content oracle — the edge compare
  (`worker/src/lib/internal_auth.ts:112-127`) and the uniform `InvalidPat` collapse in the container
  (`crates/corelink-container/src/adapter_pat.rs:43-47`).
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
- **Availability-vs-auth at the Worker edge: a transient D1 PAT-lookup FAULT now maps to a retryable
  503, NOT a 401.** `extractAuth` wraps the `SELECT … FROM pat` in a try/catch and on any D1 error
  (network partition / DB unavailable) returns the distinct reason `d1_lookup_error`
  (`worker/src/index.ts:1044-1054`). The PAT-gate caller (H1 fix) now maps BOTH
  `signing_key_not_configured` AND `d1_lookup_error` to `503 authentication service unavailable`
  (`worker/src/index.ts:2283-2291`) — a D1 hiccup is a TRANSIENT infra fault, not a bad credential, so
  surfacing it as 401 would make every client see "bad credentials" (spurious PAT rotation / on-call
  chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` / `invalid_*`)
  still fall through to `401`. Therefore the gotcha above ("401 = bad HMAC OR no live D1 row") stays
  COMPLETE for the worker edge — a transient D1 fault is NOT a cause of a 401 there; it is a 503. The
  in-line `catch` comment at `worker/src/index.ts:1051-1053` now correctly states that the caller maps
  `d1_lookup_error` to a 503 (it previously lied — "for now we 401 to fail-closed"); the cited line
  numbers shifted after the Artifact 1 `/v1/public/*`
  attestation-verifier route arm was added above this handler, again when the CF-6 audit-chain
  signing env vars were declared on the `Env` type, when the WP4 Sentry `beforeSend`
  PII/secret scrubber import was added at the top of the module, when the
  `/internal/v1/auth/resolve-tenant` fabric route was added to `matchRoute`, and most recently when the
  multi-region "route to the LOCAL (this-region) container" DO-forward block was inserted above the
  per-tenant DO forward (`worker/src/index.ts:1644-1645`)). Both the D1-fault and the
  `signing_key_not_configured` config-fault
  are retryable 503s; the edge still fails CLOSED (security > availability) for every credential-shaped
  failure.

# Citations

1. `worker/src/lib/internal_auth.ts:112-127` — the edge constant-time secret compare with no length oracle.
2. `worker/src/lib/internal_auth.ts:152-164` — the fail-CLOSED edge gate (403 unbound / 401 wrong / `null` pass).
2a. `worker/src/lib/internal_auth.ts:73` — `resolveConsumerKey`: dedicated-key preference with too-short→absent shared-key fallback.
3. `crates/corelink-container/src/adapter_pat.rs:5-14` — why the container re-runs full verification (Option B).
4. `crates/corelink-container/src/adapter_pat.rs:43-47` — uniform `InvalidPat`: no on-the-wire oracle.
5. `crates/corelink-container/src/adapter_pat.rs:538-543` — the HMAC fast-reject, pre-D1, no permit consumed.
6. `crates/corelink-container/src/adapter_pat.rs:602-650` — the deep Argon2id possession proof on a blocking thread.

---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "How CoreLink rejects forged tokens cheaply at the edge and proves possession deeply in the container."
source_files:
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/index.ts"
  - "worker/src/lib/pat_verify_cache.ts"
  - "worker/src/lib/tenant_suspend_gate.ts"
  - "crates/corelink-container/src/adapter_pat.rs"
source_blobs:
  - "worker/src/lib/internal_auth.ts@4e399de52d42e661d7dd819f5434001eb0845145"
  - "worker/src/index.ts@e0300e4d06ccd5aa81bb2bf4c7a8f9c13354b048"
  - "worker/src/lib/pat_verify_cache.ts@b0dafab6381684057de4c0589b5d95d18a35c741"
  - "worker/src/lib/tenant_suspend_gate.ts@bed740c1e2a471244a2c9680fcf4f1e800f26d7c"
  - "crates/corelink-container/src/adapter_pat.rs@386fafda97f30a5c077a0ea2ce1b994e428b3b27"
checkpoint_sha: "7bf58501233442c34e6613dd304a38afc3337f75"
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
wire never tells an attacker *why* a token failed (`crates/corelink-container/src/adapter_pat.rs:48-52`).
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
  (`worker/src/lib/internal_auth.ts:172-187`).
- That edge gate is fail-CLOSED: an unbound or too-short secret makes the endpoint unavailable (403),
  a missing/wrong header is 401, and only an exact match returns `null` to let the caller proceed
  (`worker/src/lib/internal_auth.ts:166-178`).
- Per-consumer key resolution is `resolveConsumerKey`, which prefers a consumer's dedicated key. A
  genuinely UNSET dedicated key falls back to the shared secret (the common case). A dedicated key that is
  EXPLICITLY SET but sub-floor (<32 chars) is a misconfiguration: it fails LOUD (`console.error`) +
  fail-CLOSED (`null`, so that consumer's gate rejects until fixed) rather than silently widening the
  consumer's blast radius to the shared key (deep-audit C/sub-floor)
  (`worker/src/lib/internal_auth.ts:96-121`).
- The internal consumers now split the DSR surface into TWO authorities. The irreversible physical-erase
  cascade (`/_internal/dsr/*`) gates on the `erase` key, but the per-user DSR legitimacy ANCHOR
  (`/_internal/dsr/anchor`) resolves its OWN dedicated `dsr_anchor` consumer key
  (`CORELINK_DSR_ANCHOR_AUTH_KEY`, held by githugr and DISTINCT from the eraser's key): it is matched by
  an exact-path special-case in `internalConsumerForPath` placed BEFORE the `/_internal/dsr/*` erase
  catch-all (`worker/src/index.ts:295-297`) and resolved by the `dsr_anchor` branch of the
  consumer-key ternary (`worker/src/lib/internal_auth.ts:91-92`). This is an anti-forge two-authority
  split — a leaked erase key cannot pass the anchor gate and vice-versa (least privilege, A6).
- In the container the **first** verification step is the HMAC fast-reject: the plaintext is parsed and
  a bad signature is rejected pre-D1, so a forged token drives no D1 cost and consumes no Argon2id
  permit (`crates/corelink-container/src/adapter_pat.rs:945-946`).
- Only a token that passes HMAC and resolves to a live D1 row reaches the expensive layer: an Argon2id
  verify of the secret segment against the stored PHC hash, run on a blocking thread under a bounded
  permit (`crates/corelink-container/src/adapter_pat.rs:1097-1104`).
- On the Worker edge the per-request `pat` read behind `extractAuth` is served through a THREE-TIER read
  cascade — **L1** per-isolate in-memory → **L2** Workers KV → **L3** D1 — orchestrated by
  `verifyPatRowCached` (`worker/src/lib/pat_verify_cache.ts:264-346`). It caches POSITIVE rows ONLY and is
  consulted ONLY AFTER the HMAC possession proof above (`worker/src/index.ts:1267`), so a wrong-secret
  token never reaches any tier; every tier stores only the D1 row (tenant / scope / expiry / runner
  marker / the ADR-0071 `find_only` marker), never the secret — a hit leaks nothing and grants nothing
  without an independent possession proof. Each hit also tags WHICH tier served it — `verifyPatRowCached`
  stamps `PatVerifyResult.source` (`l1`/`kv`/`d1`, `worker/src/lib/pat_verify_cache.ts:142`), which
  `extractAuth` threads onto its `AuthResult` as `patSource` (`worker/src/index.ts:1336-1337`) purely so
  the handler can surface it in the `Server-Timing` response header's `auth` desc for a client latency
  probe (`worker/src/index.ts:3199`); it is observability-only, never a trust signal and never forwarded
  to the container.
- **L1 — per-isolate in-memory, 5 s.** A `Map` keyed by the non-secret `token_id`; a fresh (<TTL) hit
  returns with no I/O, served even through a transient D1/KV blip
  (`worker/src/lib/pat_verify_cache.ts:284-287`). `PAT_VERIFY_CACHE_TTL_MS = 5 s`
  (`worker/src/lib/pat_verify_cache.ts:153`) matches the container `NativePatGate` verify cache and stays
  well within ADR-0030's `SLO-FRESH-PAT-REVOKE ≤ 60 s p99`. The map is capped at `PAT_VERIFY_CACHE_CAP`
  with expired-first/oldest-next eviction (`worker/src/lib/pat_verify_cache.ts:183-200`), and concurrent
  misses for the SAME `token_id` collapse into ONE downstream read via a single-flight map
  (`worker/src/lib/pat_verify_cache.ts:289-293`).
- **L2 — Workers KV (`patrow:<token_id>`, 60 s TTL).** On an L1 miss the read consults a globally
  replicated, per-colo-edge-cached KV namespace BEFORE D1
  (`worker/src/lib/pat_verify_cache.ts:297-307`; `kvGetPatRow` at
  `worker/src/lib/pat_verify_cache.ts:353-380`). This is the latency fix for callers far from the ENAM D1
  primary: D1 read replication has NO South-America region, so a São-Paulo edge otherwise pays a ~0.5 s
  trans-continental `pat` read even via a replica session, and the per-isolate L1 is defeated by
  Cloudflare's isolate fan-out — whereas once a colo has read `patrow:<token_id>` it serves the rest of
  that colo's traffic edge-locally (`worker/src/lib/pat_verify_cache.ts:83-86`). L2 reuses the bound
  `METADATA_KV` namespace under the `patRowKvKey` = `"patrow:" + token_id` key
  (`worker/src/lib/pat_verify_cache.ts:89`, `worker/src/lib/pat_verify_cache.ts:103-105`); `extractAuth`
  feature-detects that binding and passes it in only when bound, so a build without it simply skips L2
  (`worker/src/index.ts:1266-1270`). L2 holds POSITIVE rows only and its write is best-effort — a KV
  write failure NEVER breaks auth because the D1 read already succeeded. CRUCIAL: the KV write-behind is
  now handed to `ctx.waitUntil` so it survives the response — a bare `void kv.put(...)` is CANCELLED the
  moment the Worker returns its response, so KV was never populated and every read fell through to D1
  (measured: ~100% D1 reads from SAM before this); when no `waitUntil` is passed (tests) the write is
  `await`ed instead so it stays observable (`worker/src/lib/pat_verify_cache.ts:326-333`; `kvPutPatRow`
  at `worker/src/lib/pat_verify_cache.ts:383-389`). `extractAuth` threads `ctx.waitUntil` down: the
  handler binds and passes `ctx.waitUntil.bind(ctx)` (`worker/src/index.ts:2622`) and `extractAuth`
  forwards it into `verifyPatRowCached` only when present (`worker/src/index.ts:1270`). A KV MISS, a
  malformed/foreign value, or a KV FAULT all
  fall through to L3 D1: `kvGetPatRow` never throws and validates the row shape before trusting it
  (`worker/src/lib/pat_verify_cache.ts:363-379`). `KV_PAT_ROW_TTL_S = 60 s` is KV's floor and equals the
  MAX of ADR-0030's `SLO-FRESH-PAT-REVOKE ≤ 60 s p99`; it REPLACES (not adds to) the D1-read axis on the
  hot path, so it is not additive with replication lag (`worker/src/lib/pat_verify_cache.ts:100`).
  Negatives are NEVER written to KV, so a just-minted token is never masked by a cached miss.
- **L3 — D1 (replica session → primary), the source-of-truth.** On an L1+L2 miss the underlying `pat`
  read is routed to the NEAREST D1 read replica for latency: `extractAuth` feature-detects the Sessions
  API and opens a `first-unconstrained` replica session
  (`env.CONFIG_DB.withSession("first-unconstrained")`), degrading gracefully to the primary handle when
  `withSession` is absent (`worker/src/index.ts:1259-1262`). `readPatRow` keeps that replica read SAFE
  for auth by giving it a PRIMARY fallback: a replica MISS is re-checked on the primary so a just-minted
  PAT authenticates immediately (read-after-write freshness), a replica FAULT falls back to the primary
  (availability), and a non-null STALE replica read is honored — bounding the revocation window to the
  sub-second replica lag (well within ADR-0030's 60 s p99), while writes / billing / quota-critical reads
  stay on the primary (`worker/src/lib/pat_verify_cache.ts:224-246`). A found row hydrates L1 and
  (best-effort) L2 on the way back (`worker/src/lib/pat_verify_cache.ts:318-333`).

# Invariants

- A token that fails the cheap HMAC fast-reject NEVER reaches the Argon2id layer
  (`crates/corelink-container/src/adapter_pat.rs:945-946`).
- Both layers are constant-time with no length or content oracle — the edge compare
  (`worker/src/lib/internal_auth.ts:172-187`) and the uniform `InvalidPat` collapse in the container
  (`crates/corelink-container/src/adapter_pat.rs:48-52`).
- The edge gate fails CLOSED: an unbound/short secret is unavailable, never an open gate
  (`worker/src/lib/internal_auth.ts:166-172`).
- The edge PAT-verify cache keeps D1 the source-of-truth (INV-AUTH-NEON-IS-SOT): NEGATIVES are never
  cached in EITHER L1 or L2 KV, so a freshly-minted token authenticates immediately — a KV miss falls
  through to D1 and its replica MISS is re-confirmed on the primary (read-after-write freshness) — and a
  mid-entry revoke is bounded to the ≤5 s L1 TTL / ≤60 s L2 KV TTL plus the sub-second replica lag, then
  denied; expiry is re-checked by the caller on every hit (`worker/src/index.ts:1290`); and a D1 fault
  (BOTH the replica AND its primary fallback throwing) is never cached and never serves a stale/expired
  entry — it surfaces as the existing `d1_lookup_error → 503`
  (`worker/src/lib/pat_verify_cache.ts:335-339`).

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
  503, NOT a 401.** `extractAuth` no longer inlines the `SELECT … FROM pat` read; it calls
  `verifyPatRowCached` (the per-isolate PAT-verify cache), which returns `error` / `not_found` / `found`
  and owns the miss-path try/catch. On the `error` kind (a D1 fault: network partition / DB unavailable)
  `extractAuth` returns the distinct reason `d1_lookup_error` (`worker/src/index.ts:1272-1276`). The
  PAT-gate caller (H1 fix) then maps BOTH `signing_key_not_configured` AND `d1_lookup_error` to
  `503 authentication service unavailable` (`worker/src/index.ts:2642-2650`) — a D1 hiccup is a
  TRANSIENT infra fault, not a bad credential, so surfacing it as 401 would make every client see "bad
  credentials" (spurious PAT rotation / on-call chasing the wrong thing). Genuine bad/unknown PATs
  (`pat_not_found` / `pat_expired` / `invalid_*`) still fall through to `401`. Therefore the gotcha above
  ("401 = bad HMAC OR no live D1 row") stays COMPLETE for the worker edge — a transient D1 fault is NOT
  a cause of a 401 there; it is a 503. The error-branch comment at `worker/src/index.ts:1273-1275`
  correctly states that the caller maps `d1_lookup_error` to a 503 (transient, retryable, still
  fail-closed); the cited line numbers shifted after the Artifact 1 `/v1/public/*`
  attestation-verifier route arm was added above this handler, again when the CF-6 audit-chain
  signing env vars were declared on the `Env` type, when the WP4 Sentry `beforeSend`
  PII/secret scrubber import was added at the top of the module, when the
  `/internal/v1/auth/resolve-tenant` fabric route was added to `matchRoute`, when the
  multi-region "route to the LOCAL (this-region) container" DO-forward block was inserted above the
  per-tenant DO forward (`worker/src/index.ts:1944-1946`), when the `extractAuth`
  `pat` read was routed through a `first-unconstrained` D1 read-replica session with a primary fallback
  (perf #99), which added the `withSession` feature-detect + the `readPatRow` helper, and most recently
  when the Workers-KV **L2** cache was slotted in front of D1 — the `METADATA_KV` binding feature-detect
  and the `kv` pass into `verifyPatRowCached` (`worker/src/index.ts:1266-1270`) — each of which shifted
  every citation below it DOWN. A `d1_lookup_error` now fires only when BOTH the replica AND its primary
  fallback fault; either D1-fault and the `signing_key_not_configured` config-fault are retryable 503s;
  the edge still fails CLOSED (security > availability) for every credential-shaped failure.
- **A valid, unexpired PAT is not sufficient — the tenant fast-suspend gate (go-live G4).** After the D1
  PAT row resolves, `extractAuth` runs one more arm: a tenant that has been suspended or erased
  (`tenant_offboarding_state.state ∈ {suspended, erased}`) is denied even though its PAT is still
  cryptographically valid, so an abusive/offboarded tenant is fast-denied on the customer CAS/AC hot path
  without waiting for every one of its PATs to be individually revoked
  (`worker/src/index.ts:1309-1316`). Like the `pat` read above, the check is a THREE-TIER read —
  **L1** per-isolate in-memory (`SUSPEND_CACHE_TTL_MS = 5 s`,
  `worker/src/lib/tenant_suspend_gate.ts:84`) → **L2** Workers KV (`tsusp:<tenant_id>` on `METADATA_KV`,
  `KV_SUSPEND_TTL_S = 60 s`, `worker/src/lib/tenant_suspend_gate.ts:98`,
  `worker/src/lib/tenant_suspend_gate.ts:101-103`) → **L3** D1 (`tenant_offboarding_state`, the
  source-of-truth via the SAME `first-unconstrained` replica session as the pat lookup) — single-flighted
  so a burst of concurrent misses collapses to one read chain
  (`worker/src/lib/tenant_suspend_gate.ts:233-302`). This is the ADR-0070 SAM-latency fix: for a caller
  far from the ENAM D1 primary each D1 read is ~120 ms and the per-isolate L1 is defeated by Cloudflare's
  isolate fan-out, so the gate mirrors the pat L2 and serves edge-local once a colo is warm. UNLIKE the
  pat L2 (which caches POSITIVE rows only), this gate caches the **NEGATIVE** ("not suspended") verdict
  too — that is the hot-path common case and caching it is precisely what turns the far-D1 read into an
  edge-local one — via a write-behind handed to `ctx.waitUntil` so it survives the response
  (`worker/src/lib/tenant_suspend_gate.ts:276-287`, `worker/src/lib/tenant_suspend_gate.ts:203-211`). The
  deliberate, ratified trade-off (ADR-0070) is a bounded suspend-enforcement window of ≤ 60 s (KV TTL)
  + ≤ 5 s (L1 isolate slack): a tenant suspended in D1 keeps hot-path access until the KV entry expires —
  acceptable because the `suspended` offboarding arm is day-scale and individual PAT revoke (which also
  KV-deletes the `patrow:` entry) plus the container `NativePatGate` remain the immediate hard-stop
  levers. Failure posture is unchanged: a KV fault is swallowed as a miss (falls through to D1); it fails
  **OPEN** on a transient D1 fault (availability), but a KNOWN-suspended cached value still denies
  (`worker/src/lib/tenant_suspend_gate.ts:289-295`). The caller maps the distinct `tenant_suspended`
  reason to **403** (an authorization denial, fail-closed), separate from the 401 bad-credential arms and
  the 503 transient-infra arms (`worker/src/index.ts:2657-2662`).

# Citations

1. `worker/src/lib/internal_auth.ts:172-187` — the edge constant-time secret compare with no length oracle.
2. `worker/src/lib/internal_auth.ts:166-178` — the fail-CLOSED edge gate (403 unbound / 401 wrong / `null` pass).
2a. `worker/src/lib/internal_auth.ts:96-121` — `resolveConsumerKey`: prefers a consumer's dedicated key; a SET-but-sub-floor (<32-char) dedicated key fails LOUD + fail-CLOSED (null) instead of silently widening to the shared key (deep-audit C/sub-floor); a genuinely UNSET dedicated key still falls back to shared.
2b. `worker/src/lib/internal_auth.ts:91-92` — the `dsr_anchor` branch of the consumer-key ternary (`CORELINK_DSR_ANCHOR_AUTH_KEY`).
2c. `worker/src/index.ts:295-297` — `internalConsumerForPath` special-cases `/_internal/dsr/anchor` → `dsr_anchor` before the `/_internal/dsr/*` erase catch-all.
3. `crates/corelink-container/src/adapter_pat.rs:5-14` — why the container re-runs full verification (Option B).
4. `crates/corelink-container/src/adapter_pat.rs:48-52` — uniform `InvalidPat`: no on-the-wire oracle.
5. `crates/corelink-container/src/adapter_pat.rs:945-946` — the HMAC fast-reject, pre-D1, no permit consumed.
6. `crates/corelink-container/src/adapter_pat.rs:1097-1104` — the deep Argon2id possession proof on a blocking thread. After it, the container's scope gate additionally fail-CLOSES a find-only PAT before the read grant (`crates/corelink-container/src/adapter_pat.rs:1120-1122`, ADR-0071): a find-only PAT's CHECK-safe `read-only` base would otherwise `docker pull` via the header-less OCI `/token` exchange.
7. `worker/src/lib/tenant_suspend_gate.ts:233-302` — `isTenantSuspended`: the THREE-TIER read L1 in-memory (`SUSPEND_CACHE_TTL_MS = 5 s`, `worker/src/lib/tenant_suspend_gate.ts:84`) → L2 Workers KV (`tsusp:<tenant_id>`, `KV_SUSPEND_TTL_S = 60 s`, `worker/src/lib/tenant_suspend_gate.ts:98`; key at `worker/src/lib/tenant_suspend_gate.ts:101-103`) → L3 D1 (`tenant_offboarding_state`, source-of-truth via the `first-unconstrained` replica session), single-flighted. UNLIKE the pat L2 it caches the NEGATIVE ("not suspended") verdict too — the ratified ADR-0070 trade-off, a ≤ 60 s (KV) + ≤ 5 s (L1) bounded suspend window — with the write-behind handed to `ctx.waitUntil` (`worker/src/lib/tenant_suspend_gate.ts:276-287`; `kvPutSuspend` at `worker/src/lib/tenant_suspend_gate.ts:203-211`); a KV fault is a miss and a D1 fault fails OPEN except a KNOWN-suspended cached value still denies (`worker/src/lib/tenant_suspend_gate.ts:289-295`).
8. `worker/src/index.ts:1309-1316` — the `extractAuth` fast-suspend arm: a valid PAT whose tenant is suspended/erased returns `tenant_suspended` (G4), now passing the `kv` (`METADATA_KV`) + `waitUntil` opts into `isTenantSuspended`.
9. `worker/src/lib/pat_verify_cache.ts:264-346` — `verifyPatRowCached`: the POSITIVE-only three-tier read cascade (L1 in-memory → L2 KV → L3 D1). The L1 fresh hit (`worker/src/lib/pat_verify_cache.ts:284-287`) is single-flight-collapsed (`worker/src/lib/pat_verify_cache.ts:289-293`) with a `PAT_VERIFY_CACHE_TTL_MS = 5 s` TTL (`worker/src/lib/pat_verify_cache.ts:153`) and an LRU cap (`worker/src/lib/pat_verify_cache.ts:183-200`); negatives and D1 faults are never cached in any tier, so D1 stays the source-of-truth on every miss (`worker/src/lib/pat_verify_cache.ts:335-339`).
10. `worker/src/lib/pat_verify_cache.ts:224-246` — `readPatRow`: the L3 replica-first read with a PRIMARY fallback — a replica MISS is re-checked on the primary (read-after-write freshness), a replica FAULT falls back to the primary (availability), and a non-null STALE replica read is honored (bounding revocation to the sub-second replication lag) — reached via the `first-unconstrained` `withSession` replica session `extractAuth` opens, degrading to the primary handle when `withSession` is absent (`worker/src/index.ts:1259-1262`).
11. `worker/src/lib/pat_verify_cache.ts:297-307` — the **L2** Workers-KV read in `verifyPatRowCached` (consulted after L1, before L3), backed by `kvGetPatRow` (`worker/src/lib/pat_verify_cache.ts:353-380`) which never throws and validates the row shape (`worker/src/lib/pat_verify_cache.ts:363-379`) so a KV miss / malformed value / KV fault all fall through to D1.
12. `worker/src/lib/pat_verify_cache.ts:326-333` — the best-effort L2 KV write on a D1 hydrate, handed to `ctx.waitUntil` so it survives the response (a bare `void kv.put(...)` is cancelled when the Worker returns, leaving KV un-populated), falling back to `await` when no `waitUntil` is passed (tests); `extractAuth` threads `ctx.waitUntil.bind(ctx)` from the handler (`worker/src/index.ts:2622`) into `verifyPatRowCached` (`worker/src/index.ts:1270`). Writes go via `kvPutPatRow` (`worker/src/lib/pat_verify_cache.ts:383-389`); a KV write failure is swallowed and never breaks auth (POSITIVE rows only).
13. `worker/src/lib/pat_verify_cache.ts:83-86` — the structural `KvReader` handle for L2; the `patrow:` key prefix (`worker/src/lib/pat_verify_cache.ts:89`) + `patRowKvKey` (`worker/src/lib/pat_verify_cache.ts:103-105`) and the `KV_PAT_ROW_TTL_S = 60 s` revocation backstop = ADR-0030's 60 s p99 (`worker/src/lib/pat_verify_cache.ts:100`).
14. `worker/src/index.ts:1266-1270` — the `extractAuth` L2 wiring: feature-detect the `METADATA_KV` binding and pass it as `kv` into `verifyPatRowCached` only when bound (a build without the binding skips L2).

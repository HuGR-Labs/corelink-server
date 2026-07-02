---
type: "AuthFlow"
title: "The edge PAT-mint lifecycle (one authority, three consumers)"
description: "How every Worker-edge code path that needs a PAT funnels through a single mint chokepoint that reuses the container's audited /_internal/pat/mint."
source_files:
  - "worker/src/lib/session_exchange.ts"
  - "worker/src/lib/runner_mint.ts"
  - "worker/src/lib/auth_rotate.ts"
checkpoint_sha: "8d6ac08cae448a7c7444dfd7fa73239810589840"
provenance: "AUTHORED"
tags: ["auth", "pat", "mint", "worker-edge", "tenancy"]
timestamp: "2026-06-27T00:00:00Z"
---

# The edge PAT-mint lifecycle (one authority, three consumers)

CoreLink mints Personal Access Tokens from several edge surfaces — a hugit forge
exchanging a Clerk session for a cache token, a runner dispatcher arming a disposable
CI runner, a `clw auth rotate` replacing a key — but it deliberately keeps **exactly
one** place that actually mints. `mintScopedPat` in `worker/src/lib/session_exchange.ts`
is that authority: it is the only edge function that calls the container's audited
`/_internal/pat/mint` route, and it is the only edge function that writes the resulting
D1 `pat` row. The three public handlers above it are thin, fail-CLOSED *authorizers* —
each proves the caller may mint (a verified session, an internal-auth key plus a
runners entitlement, or ownership of the old PAT), then delegates the privileged work to
the shared chokepoint (`worker/src/lib/session_exchange.ts:406-414`,
`worker/src/lib/runner_mint.ts:215-222`, `worker/src/lib/auth_rotate.ts:278-289`). Each
consumer now presents the **DEDICATED** `CORELINK_PAT_MINT_AUTH_KEY` to that chokepoint
— falling back to the shared `CORELINK_INTERNAL_AUTH_KEY` ONLY when the dedicated key is
unset — because the container's `/_internal/pat/mint` gate now REQUIRES the dedicated
mint key (DD-HIGH): once the dedicated key is provisioned the shared internal-auth key
alone no longer authorizes a mint (`worker/src/lib/session_exchange.ts:378-379`,
`worker/src/lib/runner_mint.ts:139-140`, `worker/src/lib/auth_rotate.ts:178-179`). The
payoff is one signing key, one audit emit, and one revocation surface for
INV-PAT-REVOKE-PROPAGATION — there is no second mint path to drift, leak, or forget to
throttle (`worker/src/lib/session_exchange.ts:11-17`).

# Role

The mint chokepoint is where an *already-authorized* identity becomes a *bearer
credential*. It separates two responsibilities cleanly: the per-consumer handler owns
**authorization** (who may mint, for which tenant, at what scope and TTL), and
`mintScopedPat` owns **issuance** (derive the principal UUID, throttle, call the one
container route, and persist the row). Because the container's `/_internal/pat/mint`
computes the token and its Argon2id hash but does NOT write the D1 row, the caller MUST
persist it — and centralizing that write here is what fixed the class of bug where a
minted token 401'd at auth because no `pat` row ever existed
(`worker/src/lib/session_exchange.ts:542-549`). The single authority is also the single
place a leaked token can be revoked, since every consumer's token lands in the same
`pat` table the native plane, adapters and OCI read `revoked_at_ms IS NULL` from
(`worker/src/lib/runner_mint.ts:24-29`).

# How it works

- `mintScopedPat` is the sole exported mint authority; it SHA-256-derives a stable
  per-principal UUID, then builds a FRESH server-to-server request to the container's
  `/_internal/pat/mint` carrying the server-trusted internal-auth header the client can
  never supply (`worker/src/lib/session_exchange.ts:453`, `worker/src/lib/session_exchange.ts:490-498`).
- It canonicalizes the requested scope label to a D1-legal `pat.scope`
  (`'cas:rw'`/`'read-write'` → `'read-write'`) and fails CLOSED with a 500 on an
  unmappable scope BEFORE any expensive work, so a token that could never be persisted
  is never minted (`worker/src/lib/session_exchange.ts:153-163`, `worker/src/lib/session_exchange.ts:462-464`).
- A per-principal fixed-window throttle gates the mint: over the cap returns 429
  fail-CLOSED, so a still-valid session/key cannot loop-mint unbounded PATs
  (`worker/src/lib/session_exchange.ts:471-473`). The durable counter is ONE atomic
  D1 statement (`INSERT … ON CONFLICT … DO UPDATE … RETURNING count`), so concurrent
  mints cannot race past the cap (`worker/src/lib/session_exchange.ts:201-215`).
- **Transient-D1-fault handling (F20):** when the durable throttle's D1 write THROWS
  (an outage), the code does NOT fail the mint open — it logs the outage fail-LOUD and
  falls through to a module-level in-memory backstop. A per-isolate `Map`
  (`_inMemoryMintCounts`) caps mints per principal to `MAX_IN_MEMORY_BURST` (5, a
  TIGHTER ceiling than the durable cap of 10) for the isolate's lifetime, bounding the
  Argon2id CPU a loop-mint can burn while D1 is down; the backstop increments and is
  evaluated on EVERY request (both D1-healthy and D1-outage paths), not only during the
  outage (`worker/src/lib/session_exchange.ts:216-225`,
  `worker/src/lib/session_exchange.ts:237-265`). The map is LRU-bounded at
  `MAX_IN_MEMORY_MINT_ENTRIES` (50 000) via delete-then-set + oldest-key eviction, so a
  long-lived isolate under principal churn cannot grow it unbounded; an evicted
  principal only loses its tighter in-isolate burst memory, never the persistent gate
  (`worker/src/lib/session_exchange.ts:244-254`).
- After a 200 from the container it persists the `pat` row (using the returned Argon2id
  hash) and fails CLOSED on any FK/UNIQUE/CHECK/transport error — a token whose row was
  not written is never returned (`worker/src/lib/session_exchange.ts:566-607`).
- **Consumer 1 — session→PAT exchange (hugit Seam C):** `handleSessionExchange` verifies
  the Clerk session + resolves the tenant via the shared pipeline, then delegates to the
  chokepoint with a 1-hour `cas:rw` TTL (`worker/src/lib/session_exchange.ts:397-414`).
- **Consumer 1b — RFC 8693 token exchange (githugr):** `handleTokenExchange` adds the
  cross-tenant defense — `audience !== tenantId` is a hard 403, so a session for tenant A
  can never obtain a PAT for tenant B — before delegating to the same chokepoint
  (`worker/src/lib/session_exchange.ts:727-728`, `worker/src/lib/session_exchange.ts:731-740`).
- **Consumer 2 — runner mint/revoke:** `handleRunnerMint` has no session; it is
  internal-auth gated and checks a SEPARATE Runners authorization axis — a keyed lookup
  on `runners_entitlement` (migration 0070), no row → 403 — then delegates with the
  runner `job_id` as the principal source (`worker/src/lib/runner_mint.ts:197-201`,
  `worker/src/lib/runner_mint.ts:215-222`). It also honors an optional **lease-bound
  `ttl_seconds`**: the dispatcher sends the lease's remaining time so the runner PAT
  EXPIRES WITH THE LEASE (server-enforced), clamped DOWN to the 90-min cap — a caller
  can only shorten, never extend — and `0`/negative/non-integer is refused `400` (the
  container maps `ttl_seconds=0` → "no expiry", so a non-expiring runner PAT is
  impossible to request) (`worker/src/lib/runner_mint.ts:174-189`). Teardown revoke is a
  tenant-scoped, idempotent soft-revoke on the shared `pat` table
  (`worker/src/lib/runner_mint.ts:307-308`).
- **Consumer 3 — `clw auth rotate`:** `handleAuthRotate` reads the OLD `pat` row, refuses
  a missing/already-revoked PAT with a 404, refuses a caller-named tenant that does not
  own the row with a 403, mints an equal-scope replacement via the chokepoint, and only
  THEN soft-revokes the old PAT — so a mint failure never leaves a zero-PAT window
  (`worker/src/lib/auth_rotate.ts:235-236`, `worker/src/lib/auth_rotate.ts:245-249`,
  `worker/src/lib/auth_rotate.ts:256-262`, `worker/src/lib/auth_rotate.ts:278-289`,
  `worker/src/lib/auth_rotate.ts:316-321`).

# Invariants

- There is exactly ONE edge mint path — all three consumers import and call
  `mintScopedPat`; no handler calls `/_internal/pat/mint` itself
  (`worker/src/lib/runner_mint.ts:40`, `worker/src/lib/auth_rotate.ts:48`).
- The cross-tenant audience check is fail-CLOSED: a session whose resolved tenant differs
  from the requested audience is rejected 403 BEFORE any mint
  (`worker/src/lib/session_exchange.ts:727-728`).
- Runners is a SEPARATE authorization axis from the cache tier — a tenant with no
  `runners_entitlement` row gets no runner credential (403), independent of its cache
  subscription (`worker/src/lib/runner_mint.ts:175-176`, `worker/src/lib/runner_mint.ts:185-186`).
- Every mint is throttled and every issuance failure fails CLOSED — an unmappable scope
  is a 500 and an over-cap principal is a 429, never an issued-but-unusable token
  (`worker/src/lib/session_exchange.ts:462-464`, `worker/src/lib/session_exchange.ts:471-473`).
- A D1 fault on the throttle path never opens the mint gate: the durable-counter write
  throwing falls through to the in-memory per-isolate backstop (cap 5, < the durable cap
  10), so a loop-mint during a D1 outage is still 429-bounded to protect Argon2id CPU —
  fail-LOUD, never fail-open (`worker/src/lib/session_exchange.ts:216-225`,
  `worker/src/lib/session_exchange.ts:255-265`).
- Revocation shares ONE surface: both the runner-revoke and the rotate-old-key write are
  the same idempotent `UPDATE pat SET revoked_at_ms ... WHERE ... revoked_at_ms IS NULL`
  the native plane honors (`worker/src/lib/runner_mint.ts:281-282`,
  `worker/src/lib/auth_rotate.ts:316-321`).

# Gotchas

- The container's `/_internal/pat/mint` is a PURE function — it returns the token + hash
  but writes NO D1 row; the CALLER must persist it. That persistence lives ONLY in
  `mintScopedPat`, which is why every consumer must route through it, not the container
  route directly (`worker/src/lib/session_exchange.ts:542-549`).
- `mintScopedPat` performs NO authentication of its own beyond the per-principal throttle
  — it trusts that its caller already authorized the request. Calling it without a prior
  authorization gate would be a privilege-escalation hole
  (`worker/src/lib/session_exchange.ts:427-430`).
- Rotation cannot reproduce a `read-only` scope (the container mint has no `read-only`
  mapping), so a `read-only` old PAT is refused 422 rather than silently escalated to
  `read-write` — rotate preserves privilege EXACTLY (`worker/src/lib/auth_rotate.ts:264-268`).
- A non-200 from the container mint is collapsed to a single fail-CLOSED 500 at the edge,
  so an unauthenticated caller learns nothing about the internal mint surface
  (`worker/src/lib/session_exchange.ts:512-519`). See [the 2-level PAT moat](/auth/pat-moat.md)
  for the verification side of the same tokens.
- A minted PAT with `expires_ms = 0` is the canonical "never-expires" sentinel; as of the H2 fix
  (#538, 2026-06-28) the edge PAT-verify path honors it (`expires_ms === 0` skips the expiry check,
  matching the container SQL `expires_ms = 0 OR expires_ms > now`), so a no-TTL PAT minted here is no
  longer a split-brain that the edge rejects while the container accepts — see [the 2-level PAT moat](/auth/pat-moat.md).

# Citations

1. `worker/src/lib/session_exchange.ts:11-17` — module doc: REUSE the one container mint; single signing key / audit / revocation surface (no second mint path).
2. `worker/src/lib/session_exchange.ts:153-163` — `canonicalizePatScope`: unmappable scope → `null`.
3. `worker/src/lib/session_exchange.ts:406-414` — Consumer 1: `handleSessionExchange` delegates to `mintScopedPat` (1h `cas:rw`).
4. `worker/src/lib/session_exchange.ts:427-430` — `mintScopedPat` does no auth of its own beyond the throttle; the caller MUST have authorized.
5. `worker/src/lib/session_exchange.ts:453` — SHA-256-derived stable per-principal UUID for the mint.
6. `worker/src/lib/session_exchange.ts:462-464` — unmappable scope fails CLOSED with a 500 before any container call.
7. `worker/src/lib/session_exchange.ts:471-473` — per-principal mint throttle → 429 fail-CLOSED.
7a. `worker/src/lib/session_exchange.ts:201-215` — durable throttle is ONE atomic `INSERT … ON CONFLICT … DO UPDATE … RETURNING count` (concurrent mints cannot race the cap).
7b. `worker/src/lib/session_exchange.ts:216-225` — F20: a thrown D1 write logs fail-LOUD and falls through to the in-memory backstop (never fail-open).
7c. `worker/src/lib/session_exchange.ts:237-265` — the in-memory per-isolate burst backstop (cap 5 < durable 10) fires on EVERY request to bound Argon2id CPU during a D1 outage.
7d. `worker/src/lib/session_exchange.ts:244-254` — LRU bound (50 000 entries, delete-then-set + oldest-key eviction) so a long-lived isolate can't grow the map unbounded.
8. `worker/src/lib/session_exchange.ts:490-498` — the FRESH server-to-server request to `/_internal/pat/mint` with the server-trusted internal-auth header.
9. `worker/src/lib/session_exchange.ts:512-519` — a non-200 container mint collapses to a single fail-CLOSED 500 (no internal oracle).
10. `worker/src/lib/session_exchange.ts:542-549` — the container mint writes NO `pat` row; the caller persists it (the load-bearing fix).
11. `worker/src/lib/session_exchange.ts:566-607` — `INSERT INTO pat` + fail-CLOSED on any FK/UNIQUE/CHECK/transport error.
12. `worker/src/lib/session_exchange.ts:727-728` — Consumer 1b: `audience !== tenantId` → 403 cross-tenant rejection.
13. `worker/src/lib/session_exchange.ts:731-740` — `handleTokenExchange` delegates to the same chokepoint (300s).
14. `worker/src/lib/runner_mint.ts:24-29` — revoke reuses the shared `pat` table the native plane/adapters/OCI read `revoked_at_ms IS NULL` from.
15. `worker/src/lib/runner_mint.ts:40` — Consumer 2 imports `mintScopedPat` (no second mint path).
16. `worker/src/lib/runner_mint.ts:175-176` — `SELECT tenant_id FROM runners_entitlement WHERE tenant_id = ?1` (the separate Runners axis, migration 0070).
17. `worker/src/lib/runner_mint.ts:185-186` — no entitlement row → 403 (fail-CLOSED).
18. `worker/src/lib/runner_mint.ts:190-197` — delegates to `mintScopedPat` with `job_id` as the principal source.
19. `worker/src/lib/runner_mint.ts:281-282` — tenant-scoped idempotent `UPDATE pat SET revoked_at_ms ... WHERE pat_id=?2 AND tenant_id=?3 AND revoked_at_ms IS NULL`.
20. `worker/src/lib/auth_rotate.ts:48` — Consumer 3 imports `mintScopedPat` (no second mint path).
21. `worker/src/lib/auth_rotate.ts:235-236` — `SELECT tenant_id, scope, expires_ms, revoked_at_ms FROM pat WHERE pat_id = ?1` (read the old row).
22. `worker/src/lib/auth_rotate.ts:245-249` — unknown OR already-revoked PAT → 404 (never silently mint).
23. `worker/src/lib/auth_rotate.ts:256-262` — caller-named tenant ≠ the row's tenant → 403 (REV-S2, no cross-tenant rotate).
24. `worker/src/lib/auth_rotate.ts:264-268` — unmappable (`read-only`) scope → 422 (preserve privilege exactly).
25. `worker/src/lib/auth_rotate.ts:278-289` — mint NEW via the chokepoint; on non-200 return it and do NOT revoke (no zero-PAT window).
26. `worker/src/lib/auth_rotate.ts:316-321` — revoke OLD only after the new mint succeeds (shared idempotent soft-revoke).

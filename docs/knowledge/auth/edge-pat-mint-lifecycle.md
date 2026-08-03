---
type: "AuthFlow"
title: "The edge PAT-mint lifecycle (one authority, three consumers)"
description: "How every Worker-edge code path that needs a PAT funnels through a single mint chokepoint that reuses the container's audited /_internal/pat/mint."
source_files:
  - "worker/src/lib/session_exchange.ts"
  - "worker/src/lib/runner_mint.ts"
  - "worker/src/lib/auth_rotate.ts"
checkpoint_sha: "d19f0a7880c34db7ae0300b44da3886d5f5f033c"
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
the shared chokepoint (`worker/src/lib/session_exchange.ts:544-552`,
`worker/src/lib/runner_mint.ts:520-528`, `worker/src/lib/auth_rotate.ts:285-291`). Each
consumer now presents the **DEDICATED** `CORELINK_PAT_MINT_AUTH_KEY` to that chokepoint
— falling back to the shared `CORELINK_INTERNAL_AUTH_KEY` ONLY when the dedicated key is
unset — because the container's `/_internal/pat/mint` gate now REQUIRES the dedicated
mint key (DD-HIGH): once the dedicated key is provisioned the shared internal-auth key
alone no longer authorizes a mint (`worker/src/lib/session_exchange.ts:505-506`,
`worker/src/lib/runner_mint.ts:279-280`, `worker/src/lib/auth_rotate.ts:179-180`). The
payoff is one signing key, one audit emit, and one revocation surface for
INV-PAT-REVOKE-PROPAGATION — there is no second mint path to drift, leak, or forget to
throttle (`worker/src/lib/session_exchange.ts:11-17`).

The chokepoint no longer trusts its callers by convention. As of L12(b), `mintScopedPat`
does not accept a loose `tenantId`/`scope` pair — it takes a **branded `MintGrant`**
capability (a class with a PRIVATE constructor + four per-caller factories) that binds
`{tenantId, principalSource, maxScope}`, sources the tenant FROM the grant, and refuses —
fail-CLOSED, no container mint, no D1 row, no token — any requested scope that outranks
the grant's `maxScope` over the total scope lattice `rank()` (`read-only < read-write <
admin`). A future fifth caller that forgets the authorizer cannot compile a cross-tenant
mint, and cannot escalate to `admin` unless its grant is a rotation whose same-tenant
ownership was proven (`worker/src/lib/session_exchange.ts:181-190`,
`worker/src/lib/session_exchange.ts:215-263`, `worker/src/lib/session_exchange.ts:634-640`).

# Role

The mint chokepoint is where an *already-authorized* identity becomes a *bearer
credential*. It separates two responsibilities cleanly: the per-consumer handler owns
**authorization** (who may mint, for which tenant, at what scope and TTL), and
`mintScopedPat` owns **issuance** (source the tenant + ceiling from the grant, derive the
principal UUID, assert the scope ceiling, throttle, call the one container route, and
persist the row). Because the container's `/_internal/pat/mint` computes the token and its
Argon2id hash but does NOT write the D1 row, the caller MUST persist it — and centralizing
that write here is what fixed the class of bug where a minted token 401'd at auth because
no `pat` row ever existed (`worker/src/lib/session_exchange.ts:751-803`). The single authority is also the single
place a leaked token can be revoked, since every consumer's token lands in the same
`pat` table the native plane, adapters and OCI read `revoked_at_ms IS NULL` from
(`worker/src/lib/runner_mint.ts:24-29`).

# How it works

- `mintScopedPat` is the sole exported mint authority; it takes a branded `MintGrant`,
  SHA-256-derives a stable per-principal UUID from the grant's `principalSource`, then
  builds a FRESH server-to-server request to the container's `/_internal/pat/mint`
  carrying the server-trusted internal-auth header the client can never supply
  (`worker/src/lib/session_exchange.ts:613`, `worker/src/lib/session_exchange.ts:664-675`).
- **L12(b) capability + scope ceiling.** The tenant is sourced FROM the grant (a loose
  string is no longer accepted), and the four factories (`fromVerifiedSession`,
  `fromTokenExchange`, `fromRotation`, `fromRunnerDerivation`) each bind the `maxScope`
  the caller proved: session/token/runner declare `read-write`; rotation declares the OLD
  row's (ownership-proven) scope, the ONLY way to reach `admin`
  (`worker/src/lib/session_exchange.ts:215-263`). Over the total lattice `rank()`
  (`read-only(0) < read-write(1) < admin(2)`), a requested scope that outranks `maxScope`
  fails CLOSED with a 500 BEFORE the container mint — no token, no D1 row
  (`worker/src/lib/session_exchange.ts:181-190`, `worker/src/lib/session_exchange.ts:634-640`).
- It canonicalizes the requested scope label to a D1-legal `pat.scope`
  (`'cas:rw'`/`'read-write'` → `'read-write'`) and fails CLOSED with a 500 on an
  unmappable scope BEFORE any expensive work, so a token that could never be persisted
  is never minted (`worker/src/lib/session_exchange.ts:154-165`, `worker/src/lib/session_exchange.ts:621-625`).
- A per-principal fixed-window throttle gates the mint: over the cap returns 429
  fail-CLOSED, so a still-valid session/key cannot loop-mint unbounded PATs
  (`worker/src/lib/session_exchange.ts:645-648`). The durable counter is ONE atomic
  D1 statement (`INSERT … ON CONFLICT … DO UPDATE … RETURNING count`), so concurrent
  mints cannot race past the cap; its window + in-memory caps are parametrizable so the
  runner path can reuse it as a per-tenant ceiling (M22(b))
  (`worker/src/lib/session_exchange.ts:330-339`, `worker/src/lib/session_exchange.ts:313-321`).
- **Transient-D1-fault handling (F20):** when the durable throttle's D1 write THROWS
  (an outage), the code does NOT fail the mint open — it logs the outage fail-LOUD and
  falls through to a module-level in-memory backstop. A per-isolate `Map`
  (`_inMemoryMintCounts`) caps mints per principal to `MAX_IN_MEMORY_BURST` (5, a
  TIGHTER ceiling than the durable cap of 10) for the isolate's lifetime, bounding the
  Argon2id CPU a loop-mint can burn while D1 is down; the backstop increments and is
  evaluated on EVERY request (both D1-healthy and D1-outage paths), not only during the
  outage (`worker/src/lib/session_exchange.ts:343-351`,
  `worker/src/lib/session_exchange.ts:364-392`). The map is LRU-bounded at
  `MAX_IN_MEMORY_MINT_ENTRIES` (50 000) via delete-then-set + oldest-key eviction, so a
  long-lived isolate under principal churn cannot grow it unbounded; an evicted
  principal only loses its tighter in-isolate burst memory, never the persistent gate
  (`worker/src/lib/session_exchange.ts:371-380`).
- After a 200 from the container it persists the `pat` row (using the returned Argon2id
  hash) and fails CLOSED on any FK/UNIQUE/CHECK/transport error — a token whose row was
  not written is never returned (`worker/src/lib/session_exchange.ts:751-803`).
- **RBAC scope cap (least privilege):** the resolved `role` from the shared pipeline now caps the
  minted scope. A `viewer` seat cannot mint a write-capable PAT — `handleSessionExchange` swaps
  `EXCHANGE_PAT_SCOPE` for `read-only` when `role === "viewer"`
  (`worker/src/lib/session_exchange.ts:533`), and `handleTokenExchange` caps the requested/default
  scope DOWN to `read-only` for a viewer regardless of what was asked
  (`worker/src/lib/session_exchange.ts:925-927`); owner/admin/member keep read-write. This composes
  with the L12(b) ceiling: the role caps the REQUESTED scope, the grant caps the MAXIMUM.
- **Consumer 1 — session→PAT exchange (hugit Seam C):** `handleSessionExchange` verifies
  the Clerk session + resolves the tenant (and role) via the shared pipeline, then delegates to the
  chokepoint with a `MintGrant.fromVerifiedSession` (read-write ceiling) at a 1-hour
  role-capped TTL scope (`worker/src/lib/session_exchange.ts:544-552`).
- **Consumer 1b — RFC 8693 token exchange (githugr):** `handleTokenExchange` adds the
  cross-tenant defense — `audience !== tenantId` is a hard 403, so a session for tenant A
  can never obtain a PAT for tenant B — before delegating to the same chokepoint with a
  `MintGrant.fromTokenExchange` (read-write ceiling)
  (`worker/src/lib/session_exchange.ts:934-935`, `worker/src/lib/session_exchange.ts:947-955`).
- **Consumer 2 — runner mint/revoke (cf-multitenant WP2):** `handleRunnerMint` has no
  session; it is internal-auth gated and no longer trusts an `owner_tenant` body field.
  The tenant is DERIVED + AUTHORIZED server-side by a four-check, fail-CLOSED CONFIG_DB
  chokepoint whose body is `{ job_id, repo_full_name, installation_id, scope?,
  ttl_seconds? }` (`worker/src/lib/runner_mint.ts:289-297`): (a) derive the tenant from
  `tenant_gh_installation_map[installation_id]` — no row → 403; (b) reject if a
  `tenant_offboarding_state` row exists (suspended tenant) → 403; (c) require a
  `runner_repo_allowlist(tenant, repo)` row → else 403; (d) require a
  `runners_entitlement` row (migration 0070) and capture its `max_concurrency` → no row →
  403. EVERY miss returns the SAME generic `403 {error:"FORBIDDEN", message:"runner mint
  unauthorized"}` — no oracle distinguishes which check failed — and any D1 exception →
  500 (`worker/src/lib/runner_mint.ts:393-462`). **M22(b) per-tenant mint ceiling:** after
  tenant derivation and BEFORE the mint, it calls the shared throttle a SECOND time with a
  domain-separated hashed key (`"runner-tenant:" + tenantId`) and a cap SCALED off the
  tenant's runner ceiling (`max(max_concurrency * 2, 8)`), so a mint-storm across MANY
  distinct `job_id`s (each under its own per-job cap) is bounded per tenant while the
  ceiling grows with entitlement (`worker/src/lib/runner_mint.ts:483-493`). It then
  delegates to the chokepoint with a `MintGrant.fromRunnerDerivation` (read-write ceiling),
  the DERIVED tenant, and the runner `job_id` as the principal source, threading
  `max_concurrency` into the response via `mintScopedPat`'s `extraFields` bag
  (`worker/src/lib/runner_mint.ts:520-528`). It also honors an optional **lease-bound
  `ttl_seconds`**: the dispatcher sends the lease's remaining time so the runner PAT
  EXPIRES WITH THE LEASE (server-enforced), clamped DOWN to the 90-min cap — a caller
  can only shorten, never extend — and `0`/negative/non-integer is refused `400` (the
  container maps `ttl_seconds=0` → "no expiry", so a non-expiring runner PAT is
  impossible to request) (`worker/src/lib/runner_mint.ts:348-360`). Teardown revoke is an
  idempotent soft-revoke on the shared `pat` table
  (`worker/src/lib/runner_mint.ts:611-622`).
- **Consumer 3 — `clw auth rotate`:** `handleAuthRotate` reads the OLD `pat` row, refuses
  a missing/already-revoked PAT with a 404, refuses a caller-named tenant that does not
  own the row with a 403, mints an equal-scope replacement via the chokepoint with a
  `MintGrant.fromRotation` whose ceiling is the OLD row's canonical scope (so a same-tenant
  `admin` rotation is permitted), and only THEN soft-revokes the old PAT — so a mint
  failure never leaves a zero-PAT window (`worker/src/lib/auth_rotate.ts:236-240`,
  `worker/src/lib/auth_rotate.ts:246-251`, `worker/src/lib/auth_rotate.ts:257-264`,
  `worker/src/lib/auth_rotate.ts:285-291`, `worker/src/lib/auth_rotate.ts:318-324`).

# Invariants

- There is exactly ONE edge mint path — all three consumers import and call
  `mintScopedPat`; no handler calls `/_internal/pat/mint` itself
  (`worker/src/lib/runner_mint.ts:40-45`, `worker/src/lib/auth_rotate.ts:48`).
- The mint chokepoint is CAPABILITY-gated (L12(b)): `mintScopedPat` takes a branded
  `MintGrant` (private constructor, four factories) and can never be handed a raw tenant
  string; the tenant is read from the grant and a requested scope above the grant's
  `maxScope` fails CLOSED with a 500 — no container mint, no D1 row — so `admin` is
  reachable ONLY via a rotation grant of an already-admin, same-tenant-owned PAT
  (`worker/src/lib/session_exchange.ts:215-263`, `worker/src/lib/session_exchange.ts:634-640`).
- The cross-tenant audience check is fail-CLOSED: a session whose resolved tenant differs
  from the requested audience is rejected 403 BEFORE any mint
  (`worker/src/lib/session_exchange.ts:934-935`).
- Scope is capped by RBAC role: a `viewer` seat's mint is forced to `read-only` regardless of the
  requested scope, so a read-only seat can never obtain a write-capable PAT
  (`worker/src/lib/session_exchange.ts:533`, `worker/src/lib/session_exchange.ts:925-927`).
- Runners is a SEPARATE authorization axis from the cache tier — a tenant with no
  `runners_entitlement` row gets no runner credential (403), independent of its cache
  subscription (`worker/src/lib/runner_mint.ts:453-462`).
- The runner-mint authz chokepoint is a NON-ORACLE: an unmapped installation, a suspended
  tenant, a non-allowlisted repo, and a non-entitled tenant ALL return the byte-identical
  generic `403 "runner mint unauthorized"`, and the tenant is DERIVED server-side from the
  installation — never taken from the request body (the single-tenant hole WP2 closes)
  (`worker/src/lib/runner_mint.ts:383-462`).
- A runner tenant's mints are bounded per WINDOW, not just per job (M22(b)): after
  server-side tenant derivation the shared throttle is re-applied under a per-tenant key at
  a cap scaled off `max_concurrency`, so a storm across distinct `job_id`s cannot slip the
  per-job cap — yet legitimate fan-out (up to the scaled ceiling) never false-throttles
  (`worker/src/lib/runner_mint.ts:483-493`).
- Every mint is throttled and every issuance failure fails CLOSED — an unmappable OR
  over-ceiling scope is a 500 and an over-cap principal is a 429, never an
  issued-but-unusable or over-privileged token (`worker/src/lib/session_exchange.ts:621-625`,
  `worker/src/lib/session_exchange.ts:634-640`, `worker/src/lib/session_exchange.ts:645-648`).
- A D1 fault on the throttle path never opens the mint gate: the durable-counter write
  throwing falls through to the in-memory per-isolate backstop (cap 5, < the durable cap
  10), so a loop-mint during a D1 outage is still 429-bounded to protect Argon2id CPU —
  fail-LOUD, never fail-open (`worker/src/lib/session_exchange.ts:343-351`,
  `worker/src/lib/session_exchange.ts:382-392`).
- Revocation shares ONE surface: both the runner-revoke and the rotate-old-key write are
  the same idempotent `UPDATE pat SET revoked_at_ms ... WHERE ... revoked_at_ms IS NULL`
  the native plane honors (`worker/src/lib/runner_mint.ts:611-622`,
  `worker/src/lib/auth_rotate.ts:318-324`).

# Gotchas

- The container's `/_internal/pat/mint` is a PURE function — it returns the token + hash
  but writes NO D1 row; the CALLER must persist it. That persistence lives ONLY in
  `mintScopedPat`, which is why every consumer must route through it, not the container
  route directly (`worker/src/lib/session_exchange.ts:717-720`).
- `mintScopedPat` performs NO session authentication of its own — it trusts that its
  caller already authorized the request — but it is NOT a blind pass-through: as of L12(b)
  it self-enforces the grant's tenant + scope ceiling and the per-principal throttle, so a
  caller that wires the WRONG ceiling (or a future fifth caller) still cannot mint above
  what its grant proves (`worker/src/lib/session_exchange.ts:634-640`,
  `worker/src/lib/session_exchange.ts:645-648`).
- Rotation reproduces `read-only`, `read-write`, and `admin` scopes faithfully (the
  container mint maps all three since read-only became mintable end-to-end, PR #681), so a
  rotation preserves privilege EXACTLY — no escalation, no weakening. Only a genuinely
  unmappable scope (a label OUTSIDE that canonical set) is refused 422
  (`worker/src/lib/auth_rotate.ts:265-271`).
- A non-200 from the container mint is collapsed to a single fail-CLOSED 500 at the edge,
  so an unauthenticated caller learns nothing about the internal mint surface
  (`worker/src/lib/session_exchange.ts:686-694`). See [the 2-level PAT moat](/auth/pat-moat.md)
  for the verification side of the same tokens.
- A minted PAT with `expires_ms = 0` is the canonical "never-expires" sentinel; as of the H2 fix
  (#538, 2026-06-28) the edge PAT-verify path honors it (`expires_ms === 0` skips the expiry check,
  matching the container SQL `expires_ms = 0 OR expires_ms > now`), so a no-TTL PAT minted here is no
  longer a split-brain that the edge rejects while the container accepts — see [the 2-level PAT moat](/auth/pat-moat.md).

# Citations

1. `worker/src/lib/session_exchange.ts:11-17` — module doc: REUSE the one container mint; single signing key / audit / revocation surface (no second mint path).
2. `worker/src/lib/session_exchange.ts:154-165` — `canonicalizePatScope`: unmappable scope → `null`.
3. `worker/src/lib/session_exchange.ts:171-190` — `CanonScope` + the total `rank()` over the scope lattice (`read-only < read-write < admin`) that the L12(b) ceiling compares on.
4. `worker/src/lib/session_exchange.ts:215-263` — the branded `MintGrant`: a private INSTANCE brand (`__brand`) + PRIVATE constructor + four per-caller factories binding `{tenantId, principalSource, maxScope}` (session/token/runner → `read-write`; rotation → the old scope). The instance brand is load-bearing: a private constructor alone does not create TS nominal typing, so without it a `{...} as MintGrant` bare literal would compile and defeat the capability.
5. `worker/src/lib/session_exchange.ts:634-640` — L12(b) scope-ceiling assert: `rank(requested) > rank(grant.maxScope)` → fail-CLOSED 500, no container mint, no D1 row, no token.
6. `worker/src/lib/session_exchange.ts:544-552` — Consumer 1: `handleSessionExchange` delegates to `mintScopedPat` via `MintGrant.fromVerifiedSession` (1h, role-capped scope).
6a. `worker/src/lib/session_exchange.ts:533` — RBAC: `handleSessionExchange` caps a `viewer` role's mint scope to `read-only`.
7. `worker/src/lib/session_exchange.ts:613` — SHA-256-derived stable per-principal UUID from the grant's `principalSource`.
8. `worker/src/lib/session_exchange.ts:621-625` — unmappable scope fails CLOSED with a 500 before any container call.
9. `worker/src/lib/session_exchange.ts:645-648` — per-principal mint throttle → 429 fail-CLOSED (the chokepoint's only self-gate beyond the ceiling).
9a. `worker/src/lib/session_exchange.ts:313-321` — `checkMintThrottle` is exported with parametrizable window + in-memory caps, so the runner path reuses it as the M22(b) per-tenant ceiling.
9b. `worker/src/lib/session_exchange.ts:330-339` — durable throttle is ONE atomic `INSERT … ON CONFLICT … DO UPDATE … RETURNING count` (concurrent mints cannot race the cap).
9c. `worker/src/lib/session_exchange.ts:343-351` — F20: a thrown D1 write logs fail-LOUD and falls through to the in-memory backstop (never fail-open).
9d. `worker/src/lib/session_exchange.ts:364-392` — the in-memory per-isolate burst backstop (default cap 5 < durable 10) fires on EVERY request to bound Argon2id CPU during a D1 outage.
9e. `worker/src/lib/session_exchange.ts:371-380` — LRU bound (50 000 entries, delete-then-set + oldest-key eviction) so a long-lived isolate can't grow the map unbounded.
10. `worker/src/lib/session_exchange.ts:664-675` — the FRESH server-to-server request to `/_internal/pat/mint` with the server-trusted internal-auth header.
11. `worker/src/lib/session_exchange.ts:686-694` — a non-200 container mint collapses to a single fail-CLOSED 500 (no internal oracle).
12. `worker/src/lib/session_exchange.ts:717-720` — the container mint writes NO `pat` row; the caller persists it (the load-bearing fix).
13. `worker/src/lib/session_exchange.ts:751-803` — `INSERT INTO pat` + fail-CLOSED on any FK/UNIQUE/CHECK/transport error.
14. `worker/src/lib/session_exchange.ts:821` — `mintScopedPat` merges the caller's `extraFields` bag into the 200 body (runner-mint's `max_concurrency`).
15. `worker/src/lib/session_exchange.ts:934-935` — Consumer 1b: `audience !== tenantId` → 403 cross-tenant rejection.
15a. `worker/src/lib/session_exchange.ts:925-927` — RBAC: `handleTokenExchange` caps a `viewer` role's scope DOWN to `read-only` regardless of the requested scope.
16. `worker/src/lib/session_exchange.ts:947-955` — `handleTokenExchange` delegates via `MintGrant.fromTokenExchange` (300s).
17. `worker/src/lib/session_exchange.ts:278-280` — `deriveTenantCeilingThrottleKey`: the domain-separated (`"runner-tenant:"`), hashed per-tenant throttle key (M22(b)).
18. `worker/src/lib/runner_mint.ts:24-29` — revoke reuses the shared `pat` table the native plane/adapters/OCI read `revoked_at_ms IS NULL` from.
19. `worker/src/lib/runner_mint.ts:40-45` — Consumer 2 imports `mintScopedPat` / `MintGrant` / `checkMintThrottle` / `deriveTenantCeilingThrottleKey` (no second mint path).
20. `worker/src/lib/runner_mint.ts:289-297` — WP2 body `{ job_id, repo_full_name, installation_id?, scope?, ttl_seconds?, ac_output_name? }` — the tenant is NOT a body field.
21. `worker/src/lib/runner_mint.ts:393-402` — WP2 (a): derive tenant via `SELECT tenant_id FROM tenant_gh_installation_map WHERE installation_id = ?1`; no row → generic 403.
21a. `worker/src/lib/runner_mint.ts:432-440` — WP2 (b): a `tenant_offboarding_state` row EXISTS → suspended → generic 403.
21b. `worker/src/lib/runner_mint.ts:442-450` — WP2 (c): `runner_repo_allowlist(tenant, repo)` miss → generic 403.
22. `worker/src/lib/runner_mint.ts:453-462` — WP2 (d): `SELECT max_concurrency FROM runners_entitlement WHERE tenant_id = ?1` (the separate Runners axis, migration 0070); no row → generic 403; capture `max_concurrency`.
22a. `worker/src/lib/runner_mint.ts:383-384` — the single generic `403 "runner mint unauthorized"` shared by all four checks (no oracle).
23. `worker/src/lib/runner_mint.ts:483-493` — M22(b) per-tenant mint ceiling: re-apply the shared throttle under the `"runner-tenant:" + tenantId` key at `max(max_concurrency * 2, 8)`, in-memory arm disabled (durable per-window gate).
24. `worker/src/lib/runner_mint.ts:520-528` — delegates via `MintGrant.fromRunnerDerivation` with the DERIVED tenant + `job_id` principal source, threading `max_concurrency` through `extraFields`.
24a. `worker/src/lib/runner_mint.ts:349-357` — lease-bound `ttl_seconds` clamped DOWN to the 90-min cap (never extended); `0`/negative/non-integer → 400.
25. `worker/src/lib/runner_mint.ts:611-622` — idempotent `UPDATE pat SET revoked_at_ms ... WHERE ... revoked_at_ms IS NULL` (tenant-scoped when `owner_tenant` is present, else by `pat_id` alone).
26. `worker/src/lib/auth_rotate.ts:48` — Consumer 3 imports `mintScopedPat` + `MintGrant` + `canonicalizePatScope` (no second mint path).
27. `worker/src/lib/auth_rotate.ts:236-240` — `SELECT tenant_id, scope, expires_ms, revoked_at_ms FROM pat WHERE pat_id = ?1` (read the old row).
28. `worker/src/lib/auth_rotate.ts:246-251` — unknown OR already-revoked PAT → 404 (never silently mint).
29. `worker/src/lib/auth_rotate.ts:257-264` — caller-named tenant ≠ the row's tenant → 403 (REV-S2, no cross-tenant rotate).
30. `worker/src/lib/auth_rotate.ts:265-271` — canonicalize the old scope; `read-only`/`read-write`/`admin` all rotate faithfully, only a genuinely unmappable label → 422 (preserve privilege exactly).
31. `worker/src/lib/auth_rotate.ts:285-291` — mint NEW via the chokepoint with `MintGrant.fromRotation` (ceiling = the old scope, so a same-tenant admin rotation is permitted); on non-200 return it and do NOT revoke (no zero-PAT window).
32. `worker/src/lib/auth_rotate.ts:318-324` — revoke OLD only after the new mint succeeds (shared idempotent soft-revoke).

---
type: "AuthMechanism"
title: "Edge identity resolution (Clerk session → tenant)"
description: "How the Worker edge verifies a Clerk session JWT for the human/dashboard funnel and resolves the verified user to a CoreLink tenant."
source_files:
  - "worker/src/lib/clerk_auth.ts"
  - "worker/src/lib/githugr_provision.ts"
  - "worker/src/lib/tenant_lookup.ts"
  - "migrations/d1/0074_team_member.sql"
checkpoint_sha: "6da2754726fc4e6b54ec25be6294afb825add582"
provenance: "AUTHORED"
tags: ["auth", "clerk", "tenant-resolution", "edge"]
timestamp: "2026-06-27T00:00:00Z"
---

# Edge identity resolution (Clerk session → tenant)

PATs authenticate machine traffic against the cache surfaces; this concept is the OTHER identity spine —
the **human** one. When a person signs into the dashboard / onboarding / checkout funnel, the browser
carries a **Clerk session JWT**, not a PAT. The Worker edge must turn that JWT into a CoreLink
`tenant_id` it can trust, and it must do so fail-CLOSED: an unverifiable or unrecognised session is a
denial, never an open gate. Two surfaces share this spine — the in-Worker dashboard pipeline
(`verifyClerkSessionAndResolveTenant`), and a server-to-server lookup endpoint
(`POST /internal/v1/auth/tenant/lookup`) that lets a trusted backend (githugr's www) resolve an
already-verified Clerk `sub` to its owning tenant without re-authenticating.

The core Clerk session verify is **wired + live**. A second, **DORMANT** arm accepts sessions from a
SEPARATE githugr Clerk instance; it is gated CLOSED until the two githugr secrets (issuer + public
jwtKey) are provisioned, so today it never executes. When armed, that arm no longer maps every githugr
user to one fixed tenant — it PROVISIONS-OR-LOOKS-UP a **per-`sub` isolated tenant** (the pilot
isolation fix), fail-CLOSED 500 on any D1 fault (never a shared tenant).

# Role

- The single shared pipeline that the `customer_v1` dual-auth dispatch and the onboarding/checkout arm
  both call, so the M1/M2 hardening lives in one place (`worker/src/lib/clerk_auth.ts:104`).
- It re-asserts the trust properties the Clerk library does NOT enforce on its own (azp presence,
  explicit issuer pin), then maps the verified `sub` (clerk_user_id) to a tenant the rest of the Worker
  forwards downstream.
- The companion `tenant_lookup.ts` endpoint exposes the same `clerk_user_id → tenant` mapping
  server-to-server, internal-auth gated, fail-CLOSED 404 when no tenant owns the subject
  (`worker/src/lib/tenant_lookup.ts:79`).

# How it works

- The bearer token is extracted from the `Authorization` header; a missing token is a 401 before any
  verification work (`worker/src/lib/clerk_auth.ts:114`).
- Verification is fail-CLOSED on configuration: when `CLERK_SECRET_KEY` is unbound the endpoint returns
  403 ("clerk verification unavailable") rather than skipping the verify
  (`worker/src/lib/clerk_auth.ts:150-153`).
- `verifyToken` (`@clerk/backend`) checks the signature against the instance JWKS with the azp allowlist
  passed in (`worker/src/lib/clerk_auth.ts:181-184`).
- **M1 — azp re-assert:** because the Clerk library SKIPS its `authorizedParties` check when the token
  omits `azp`, the code re-reads the claim and rejects (401) any token whose `azp` is absent, empty, or
  not in `CLERK_AZP_ALLOWLIST` (`worker/src/lib/clerk_auth.ts:190-195`).
- **M2 — issuer pin:** when `CLERK_ISSUER_URL` is set, `iss` must equal it exactly or the token is 401
  (`worker/src/lib/clerk_auth.ts:210`); in production with the pin unset the path fails CLOSED rather
  than fall back to the weak shape-check (`worker/src/lib/clerk_auth.ts:216-230`).
- A verified token with no `sub` is rejected 401; otherwise `sub` becomes the `clerkUserId`
  (`worker/src/lib/clerk_auth.ts:247`).
- Tenant resolution has **two ordered arms** in the dashboard pipeline. **(1) Owner arm:**
  `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1` resolves the verified user to the tenant it
  itself provisioned (the owner row written by the signup-worker at provision)
  (`worker/src/lib/clerk_auth.ts:284`); a hit maps to the `owner` role
  (`worker/src/lib/clerk_auth.ts:290`). **(2) Team-member fallback** — ONLY when the owner arm returns no
  row, an additive lookup `SELECT tenant_id, role FROM team_member WHERE user_id = ?1 AND status = 'active'`
  resolves the user to **another tenant's id** — the team-OWNING tenant of an ACTIVE seat (migration
  0074, `migrations/d1/0074_team_member.sql`) — and carries that seat's stored `role` forward. The
  `status = 'active'` predicate is MANDATORY and load-bearing: an `invited` or `removed` seat does NOT
  resolve, so a revoked seat is denied (`worker/src/lib/clerk_auth.ts:298-301`). So a verified Clerk user
  resolves to the tenant it OWNS, or failing that to the team-owning tenant of an active `team_member`
  row — not strictly to a single owner-keyed row.
- **RBAC role derivation:** the resolver now returns a `role` alongside the tenant on `ClerkAuthResult`.
  The default is the LEAST-privilege `viewer` (`worker/src/lib/clerk_auth.ts:281`); the owner arm sets
  `owner` (`worker/src/lib/clerk_auth.ts:290`); the team-member arm carries the seat's `role`, failing
  safe to `viewer` when the column is missing/unexpected (`worker/src/lib/clerk_auth.ts:311`). This role
  is what the downstream PAT-mint consumers use to cap a `viewer` seat to a read-only scope — see
  [the edge PAT-mint lifecycle](/auth/edge-pat-mint-lifecycle.md).
- The server-to-server endpoint runs only the owner-arm parameterized lookup
  (`worker/src/lib/tenant_lookup.ts:114`) — it does NOT consult `team_member`. A D1 fault on either path
  is a fail-CLOSED 500, never fail-open (`worker/src/lib/clerk_auth.ts:313-320`).
- **Owner-only ASYMMETRY:** because the lookup endpoint omits the `team_member` arm, the two surfaces are
  NOT interchangeable for a team-seat user. A verified user who owns no `tenant` row but holds an active
  `team_member` seat resolves successfully in the dashboard pipeline (to the team-owning tenant) yet gets
  a fail-CLOSED 404 from `POST /internal/v1/auth/tenant/lookup` — the endpoint's contract is deliberately
  owner-only (`role: "owner"`, 1:1 at provision), so githugr's `owner_tenant` mapping never silently
  inherits a second-seat membership (`worker/src/lib/tenant_lookup.ts:66-71`,
  `worker/src/lib/tenant_lookup.ts:125-126`). (Distinct from the DASHBOARD `role` above: the
  `team_member`-derived RBAC role gates scope, whereas the lookup endpoint's `role: "owner"` is the
  fixed contract of an owner-only surface.)
- The lookup endpoint's success envelope coerces an unset/empty `tier` column to the most-restrictive
  `"free"` plan rather than emitting a null — a fail-safe default mirroring `getTierForTenant`, so a
  half-provisioned tenant can never be read back as more privileged than it is
  (`worker/src/lib/tenant_lookup.ts:130-137`).
- A subject that matches NEITHER the owner row NOR an active team_member row is a hard denial: the
  team-member-MISS branch returns 403 in the dashboard pipeline (`worker/src/lib/clerk_auth.ts:302-307`),
  and the server-to-server endpoint returns 404 when no owner row exists
  (`worker/src/lib/tenant_lookup.ts:125-126`).
- **DORMANT githugr arm — per-`sub` provision-or-lookup (NOT a fixed tenant):** a separate-instance
  session is only routed to `verifyGithugrSession` when the caller opted in AND **both** real githugr
  settings (`GITHUGR_CLERK_ISSUER_URL` + `GITHUGR_CLERK_JWT_KEY`) are configured AND the unverified
  peeked issuer matches (`worker/src/lib/clerk_auth.ts:136`). The routing gate no longer requires a
  `GITHUGR_TENANT_ID` — that legacy shared-tenant secret is removed. Inside the arm, `verifyToken` runs
  networkless against githugr's PUBLIC `jwtKey`, then `iss` is exact-pinned to the githugr issuer
  authoritatively (`worker/src/lib/clerk_auth.ts:414`). On success the verified `sub` becomes the
  `clerkUserId` (`worker/src/lib/clerk_auth.ts:427`), and the tenant is derived NOT from a fixed
  configured value but by **provision-or-lookup**: `provisionOrLookupGithugrTenant(env.CONFIG_DB, sub)`
  returns a deterministic per-`sub` isolated tenant, mapped to the `owner` role (a federated-login owner
  of its own isolated tenant, not a seat), and ANY D1 fault there is a fail-CLOSED **500** —
  never a fall-back to a shared tenant (`worker/src/lib/clerk_auth.ts:457`,
  `worker/src/lib/clerk_auth.ts:467-474`). With the secrets unset the whole routing `if` is false, so
  this arm never executes today.
- **githugr per-`sub` tenant derivation + LOOKUP-FIRST, transactional provisioning:** `deriveGithugrTenantId(sub)`
  computes a v5-shaped UUID from `SHA-256("corelink-githugr-tenant-v1:" + sub)` — so two distinct subs
  yield distinct tenants (ISOLATION) and the same sub always lands on the same tenant (IDEMPOTENT)
  (`worker/src/lib/githugr_provision.ts:100`, `worker/src/lib/githugr_provision.ts:101-110`).
  `provisionOrLookupGithugrTenant` is **LOOKUP-FIRST**: the common case is a repeat login, so it SELECTs
  the `tenant_org_map` identity row FIRST and, on a hit, returns that tenant_id immediately with **ZERO
  writes** (`worker/src/lib/githugr_provision.ts:143-149`). ONLY on the first-ever login for a `sub` (no
  row) does it provision — and the full 5-row family is written as a SINGLE transactional D1
  `batch([...])` (all-or-nothing, so a mid-provision D1 fault can never leave a partial row-set), with FK
  order preserved by statement order: `tenant` first (`worker/src/lib/githugr_provision.ts:157-163`), then
  `tier_selections` (free/active), `runners_entitlement`, `tenant_quota`, and the `tenant_org_map`
  identity row keyed on `clerk_org_id = sub`, each `INSERT OR IGNORE`
  (`worker/src/lib/githugr_provision.ts:165-199`). AFTER the batch it READS BACK the authoritative
  `tenant_org_map` row so a concurrent/prior login's row wins (`worker/src/lib/githugr_provision.ts:207-210`); an
  absent read-back THROWS, which the caller treats as fail-CLOSED
  (`worker/src/lib/githugr_provision.ts:211-213`).

# Invariants

- An unbound `CLERK_SECRET_KEY` makes the edge identity gate UNAVAILABLE (403), never an open gate
  (`worker/src/lib/clerk_auth.ts:150-153`).
- A verified token whose `azp` claim is absent/empty/not-allowlisted is REJECTED (401), closing the
  Clerk-library azp-skip gap so a token from a different app on the same instance cannot pass
  (`worker/src/lib/clerk_auth.ts:190-195`).
- When `CLERK_ISSUER_URL` is set, the issuer is exact-pinned — `iss !== clerkIssuerUrl` is a 401
  (`worker/src/lib/clerk_auth.ts:210`).
- Identity REQUIRES a subject: a verified token without `sub` is 401, so no tenant lookup runs on a
  subjectless session (`worker/src/lib/clerk_auth.ts:247`).
- Tenant resolution is always keyed on the verified `clerk_user_id` via parameterized D1 queries (no
  injection surface), but it is NOT a single owner-row lookup in the dashboard pipeline: the owner arm
  (`worker/src/lib/tenant_lookup.ts:114`, `worker/src/lib/clerk_auth.ts:284`) is tried first, then an
  additive `team_member` fallback resolves the user to the team-OWNING tenant when no owner row matches
  (`worker/src/lib/clerk_auth.ts:298-301`).
- The resolved `role` fails safe to the LEAST privilege: it defaults to `viewer` and only a matched
  owner row (`worker/src/lib/clerk_auth.ts:290`) or a `team_member` seat's stored role
  (`worker/src/lib/clerk_auth.ts:311`) raises it — a missing/unexpected role stays `viewer` (read-only).
- The `team_member` fallback resolves ONLY an `status = 'active'` seat — an `invited` or `removed`
  member never resolves, so a revoked seat is denied rather than silently retaining cross-tenant access
  (`worker/src/lib/clerk_auth.ts:298-301`).
- A verified subject matching neither the owner row nor an active `team_member` row is fail-CLOSED — the
  team-member-MISS branch returns 403 in the dashboard pipeline (`worker/src/lib/clerk_auth.ts:302-307`),
  404 on the server-to-server endpoint (`worker/src/lib/tenant_lookup.ts:125-126`).
- The githugr multi-issuer arm cannot run unless BOTH real `GITHUGR_*` settings (issuer + public jwtKey)
  are present — the routing gate AND-conjoins them, so the dormant arm is closed by default. The former
  `GITHUGR_TENANT_ID` gate condition is REMOVED (`worker/src/lib/clerk_auth.ts:136`).
- A githugr session resolves to a **per-`sub` isolated tenant**, NOT one fixed shared tenant: the
  tenant_id is a deterministic function of the verified `sub`, so distinct subs are distinct tenants and
  the same sub is idempotent (`worker/src/lib/githugr_provision.ts:100`,
  `worker/src/lib/githugr_provision.ts:101-110`). Provisioning is LOOKUP-FIRST (existing `sub` → single
  SELECT, ZERO writes) and, on a first-ever login, a SINGLE transactional `INSERT OR IGNORE`
  `batch([...])` across the full 5-row family — atomic (no partial row-set) — with an authoritative
  read-back (`worker/src/lib/githugr_provision.ts:133`,
  `worker/src/lib/githugr_provision.ts:207-210`).
- The githugr provision/lookup is fail-CLOSED: any D1 fault (or an absent read-back) throws / returns a
  500 — the arm NEVER falls back to a shared tenant (that fall-back is the exact isolation break this
  fixes) (`worker/src/lib/clerk_auth.ts:467-474`, `worker/src/lib/githugr_provision.ts:211-213`).

# Gotchas

- **Designed vs wired:** the CoreLink Clerk session verify is wired + live; the **githugr** arm
  (`worker/src/lib/clerk_auth.ts:136`, `:380`) is DORMANT — gated on `GITHUGR_CLERK_ISSUER_URL` and
  `GITHUGR_CLERK_JWT_KEY` only (the old `GITHUGR_TENANT_ID` gate is removed). Until those two secrets
  are provisioned the routing `if` is false and the function only ever runs the CoreLink path.
- **No more fixed githugr tenant:** the arm used to map EVERY githugr user to one configured
  `GITHUGR_TENANT_ID`, which gave NO isolation between githugr users (githugr runs its own Clerk, so the
  CoreLink signup-worker's `user.created` auto-provision never fires for them). It now provisions-or-looks-up
  a per-`sub` tenant via `provisionOrLookupGithugrTenant` (`worker/src/lib/githugr_provision.ts:133`), and
  fails CLOSED (500) on a D1 fault rather than ever resolving to a shared tenant
  (`worker/src/lib/clerk_auth.ts:467-474`).
- The `peekUnverifiedIssuer` step is ROUTING-ONLY — it parses an UNVERIFIED payload to decide which
  instance minted the token; the signature is verified afterward inside the chosen arm
  (`worker/src/lib/clerk_auth.ts:142`). Never trust a peeked claim.
- This concept covers the HUMAN identity spine. Machine traffic uses the PAT moat instead — see
  [the 2-level PAT moat](/auth/pat-moat.md).
- A non-production deploy with `CLERK_ISSUER_URL` unset falls back to a conservative shape-check
  (`https` + host contains "clerk"); production fails CLOSED instead, so the operator MUST provision the
  pin before deploy or all Clerk auth 401s (`worker/src/lib/clerk_auth.ts:231-245`).
- `tenant.email_hash` is `SHA-256(clerk_user_id)`, NOT a raw-email hash, so the lookup endpoint's
  email-fallback is genuinely N/A — only the `sub` key resolves (`worker/src/lib/tenant_lookup.ts:103-108`).
- The `team_member` table this concept resolves the multi-seat identity arm from (citation 12) carries
  seat PII (`tenant_id` + raw Clerk `user_id` + `email_hash`); as of CF-1 (2026-06-28) it is in the GDPR
  Art.17 erase-set + the post-erase verification sweep, so a team-tenant erasure removes the seat roster
  too (it was previously omitted — see [DSR / right-to-erasure](/compliance/dsr-erasure.md)).

# Citations

1. `worker/src/lib/clerk_auth.ts:104` — `verifyClerkSessionAndResolveTenant`, the single shared pipeline.
2. `worker/src/lib/clerk_auth.ts:114` — missing bearer token → 401 before any verify.
3. `worker/src/lib/clerk_auth.ts:150-153` — `CLERK_SECRET_KEY` unbound → 403 fail-CLOSED.
4. `worker/src/lib/clerk_auth.ts:181-184` — `verifyToken` signature check with the azp allowlist.
5. `worker/src/lib/clerk_auth.ts:190-195` — M1 post-verify azp present-and-allowlisted re-assert → 401.
6. `worker/src/lib/clerk_auth.ts:210` — M2 exact issuer pin (`iss !== clerkIssuerUrl` → 401).
7. `worker/src/lib/clerk_auth.ts:216-230` — production fails CLOSED when `CLERK_ISSUER_URL` is unset.
8. `worker/src/lib/clerk_auth.ts:247` — verified token without `sub` → 401.
9. `worker/src/lib/clerk_auth.ts:302-307` — the **team-member-MISS** branch: a subject with no owner row AND no active `team_member` row → 403 (dashboard pipeline). NOT a plain no-row branch — it is reached only after the owner arm misses and the `team_member` fallback also misses.
10. `worker/src/lib/clerk_auth.ts:284` — owner arm: `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1` (a hit sets `role = "owner"` at `:293`).
11. `worker/src/lib/clerk_auth.ts:298-301` — additive team-member fallback: `SELECT tenant_id, role FROM team_member WHERE user_id = ?1 AND status = 'active'` resolves to the team-owning tenant and carries the seat's RBAC role (migration 0074); the role fails safe to `viewer` at `:284`.
11a. `worker/src/lib/clerk_auth.ts:281` — RBAC role default: `role` initialized to the least-privilege `viewer` before either resolution arm runs.
12. `migrations/d1/0074_team_member.sql:65-66` — the `(user_id, status)` index backing the team-member resolution arm (the `team_member` table is defined at `:28-57`).
13. `worker/src/lib/clerk_auth.ts:313-320` — D1 fault → 500 fail-CLOSED, never fail-open.
14. `worker/src/lib/clerk_auth.ts:136` — DORMANT githugr routing gate (BOTH real `GITHUGR_*` settings — issuer + public jwtKey — AND opt-in AND peeked issuer match; the old `GITHUGR_TENANT_ID` gate is removed).
15. `worker/src/lib/clerk_auth.ts:414` — githugr arm authoritative issuer exact-pin (only reached when the dormant gate opens).
16. `worker/src/lib/clerk_auth.ts:457` — githugr arm resolves the tenant via `provisionOrLookupGithugrTenant(env.CONFIG_DB, sub)` — a per-`sub` isolated tenant, NOT a fixed configured id — and maps it to the `owner` role.
17. `worker/src/lib/clerk_auth.ts:467-474` — githugr provision/lookup fail-CLOSED: a D1 fault is a 500, never a fall-back to a shared tenant.
18. `worker/src/lib/githugr_provision.ts:100` — `deriveGithugrTenantId`, the deterministic per-`sub` tenant_id.
19. `worker/src/lib/githugr_provision.ts:101-110` — the derivation body: v5-shaped UUID from `SHA-256("corelink-githugr-tenant-v1:" + sub)` (distinct subs → distinct tenants; same sub → same tenant).
20. `worker/src/lib/githugr_provision.ts:133` — `provisionOrLookupGithugrTenant`, the LOOKUP-FIRST provision-or-lookup entrypoint.
20a. `worker/src/lib/githugr_provision.ts:143-149` — LOOKUP-FIRST `SELECT tenant_org_map`: on a repeat-login hit, return the tenant_id with ZERO writes.
21. `worker/src/lib/githugr_provision.ts:157-163` — `tenant` row written FIRST (FK target) via `INSERT OR IGNORE`, inside the transactional `batch([...])`.
22. `worker/src/lib/githugr_provision.ts:165-199` — the child + identity rows inside the same `batch([...])`: `tier_selections` (free/active), `runners_entitlement`, `tenant_quota`, and the `tenant_org_map` row keyed on `clerk_org_id = sub`, all `INSERT OR IGNORE` (transactional / all-or-nothing).
23. `worker/src/lib/githugr_provision.ts:207-210` — authoritative read-back of `tenant_org_map` AFTER the batch (a concurrent/prior login's row wins).
24. `worker/src/lib/githugr_provision.ts:211-213` — absent read-back THROWS → caller fails CLOSED (500).
16. `worker/src/lib/tenant_lookup.ts:79` — `handleTenantLookup`, the internal-auth-gated server-to-server endpoint.
17. `worker/src/lib/tenant_lookup.ts:103-108` — `sub` required; email fallback is N/A (email_hash is a clerk-id surrogate).
18. `worker/src/lib/tenant_lookup.ts:114` — parameterized `SELECT ... FROM tenant WHERE clerk_user_id = ?1`.
19. `worker/src/lib/tenant_lookup.ts:125-126` — no row → fail-CLOSED 404 (and the owner-only asymmetry: a team-seat-only user that resolves in the dashboard is a 404 here, since this endpoint never consults `team_member`).
20. `worker/src/lib/tenant_lookup.ts:66-71` — the lookup response contract is `role: "owner"` only — owner-keyed, 1:1 at provision; no team-member arm.
21. `worker/src/lib/tenant_lookup.ts:130-137` — success envelope coerces an unset/empty `tier` to the most-restrictive `"free"` (fail-safe default, never read back more privileged).

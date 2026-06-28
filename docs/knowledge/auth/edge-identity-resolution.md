---
type: "AuthMechanism"
title: "Edge identity resolution (Clerk session → tenant)"
description: "How the Worker edge verifies a Clerk session JWT for the human/dashboard funnel and resolves the verified user to a CoreLink tenant."
source_files:
  - "worker/src/lib/clerk_auth.ts"
  - "worker/src/lib/tenant_lookup.ts"
  - "migrations/d1/0074_team_member.sql"
checkpoint_sha: "b9b40160869bc1907dd2d1527aeb8670a9d18411"
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
SEPARATE githugr Clerk instance (Option B, one fixed tenant); it is gated CLOSED until three githugr
secrets are provisioned, so today it never executes.

# Role

- The single shared pipeline that the `customer_v1` dual-auth dispatch and the onboarding/checkout arm
  both call, so the M1/M2 hardening lives in one place (`worker/src/lib/clerk_auth.ts:92`).
- It re-asserts the trust properties the Clerk library does NOT enforce on its own (azp presence,
  explicit issuer pin), then maps the verified `sub` (clerk_user_id) to a tenant the rest of the Worker
  forwards downstream.
- The companion `tenant_lookup.ts` endpoint exposes the same `clerk_user_id → tenant` mapping
  server-to-server, internal-auth gated, fail-CLOSED 404 when no tenant owns the subject
  (`worker/src/lib/tenant_lookup.ts:79`).

# How it works

- The bearer token is extracted from the `Authorization` header; a missing token is a 401 before any
  verification work (`worker/src/lib/clerk_auth.ts:102`).
- Verification is fail-CLOSED on configuration: when `CLERK_SECRET_KEY` is unbound the endpoint returns
  403 ("clerk verification unavailable") rather than skipping the verify
  (`worker/src/lib/clerk_auth.ts:135-138`).
- `verifyToken` (`@clerk/backend`) checks the signature against the instance JWKS with the azp allowlist
  passed in (`worker/src/lib/clerk_auth.ts:162-165`).
- **M1 — azp re-assert:** because the Clerk library SKIPS its `authorizedParties` check when the token
  omits `azp`, the code re-reads the claim and rejects (401) any token whose `azp` is absent, empty, or
  not in `CLERK_AZP_ALLOWLIST` (`worker/src/lib/clerk_auth.ts:171-176`).
- **M2 — issuer pin:** when `CLERK_ISSUER_URL` is set, `iss` must equal it exactly or the token is 401
  (`worker/src/lib/clerk_auth.ts:191`); in production with the pin unset the path fails CLOSED rather
  than fall back to the weak shape-check (`worker/src/lib/clerk_auth.ts:197-211`).
- A verified token with no `sub` is rejected 401; otherwise `sub` becomes the `clerkUserId`
  (`worker/src/lib/clerk_auth.ts:228`).
- Tenant resolution has **two ordered arms** in the dashboard pipeline. **(1) Owner arm:**
  `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1` resolves the verified user to the tenant it
  itself provisioned (the owner row written by the signup-worker at provision)
  (`worker/src/lib/clerk_auth.ts:248`). **(2) Team-member fallback** — ONLY when the owner arm returns no
  row, an additive lookup `SELECT tenant_id FROM team_member WHERE user_id = ?1 AND status = 'active'`
  resolves the user to **another tenant's id** — the team-OWNING tenant of an ACTIVE seat (migration
  0074, `migrations/d1/0074_team_member.sql`). The `status = 'active'` predicate is MANDATORY and
  load-bearing: an `invited` or `removed` seat does NOT resolve, so a revoked seat is denied
  (`worker/src/lib/clerk_auth.ts:261-264`). So a verified Clerk user resolves to the tenant it OWNS, or
  failing that to the team-owning tenant of an active `team_member` row — not strictly to a single
  owner-keyed row.
- The server-to-server endpoint runs only the owner-arm parameterized lookup
  (`worker/src/lib/tenant_lookup.ts:114`) — it does NOT consult `team_member`. A D1 fault on either path
  is a fail-CLOSED 500, never fail-open (`worker/src/lib/clerk_auth.ts:273-279`).
- **Owner-only ASYMMETRY:** because the lookup endpoint omits the `team_member` arm, the two surfaces are
  NOT interchangeable for a team-seat user. A verified user who owns no `tenant` row but holds an active
  `team_member` seat resolves successfully in the dashboard pipeline (to the team-owning tenant) yet gets
  a fail-CLOSED 404 from `POST /internal/v1/auth/tenant/lookup` — the endpoint's contract is deliberately
  owner-only (`role: "owner"`, 1:1 at provision), so githugr's `owner_tenant` mapping never silently
  inherits a second-seat membership (`worker/src/lib/tenant_lookup.ts:66-71`,
  `worker/src/lib/tenant_lookup.ts:125-126`).
- The lookup endpoint's success envelope coerces an unset/empty `tier` column to the most-restrictive
  `"free"` plan rather than emitting a null — a fail-safe default mirroring `getTierForTenant`, so a
  half-provisioned tenant can never be read back as more privileged than it is
  (`worker/src/lib/tenant_lookup.ts:130-137`).
- A subject that matches NEITHER the owner row NOR an active team_member row is a hard denial: the
  team-member-MISS branch returns 403 in the dashboard pipeline (`worker/src/lib/clerk_auth.ts:265-269`),
  and the server-to-server endpoint returns 404 when no owner row exists
  (`worker/src/lib/tenant_lookup.ts:125-126`).
- **DORMANT githugr arm:** a separate-instance session is only routed to `verifyGithugrSession` when the
  caller opted in AND all three `GITHUGR_*` settings are configured AND the unverified peeked issuer
  matches (`worker/src/lib/clerk_auth.ts:127`); that path then pins `iss` to the githugr issuer
  authoritatively (`worker/src/lib/clerk_auth.ts:346`). With the secrets unset the whole `if` is false,
  so this arm never executes today.

# Invariants

- An unbound `CLERK_SECRET_KEY` makes the edge identity gate UNAVAILABLE (403), never an open gate
  (`worker/src/lib/clerk_auth.ts:135-138`).
- A verified token whose `azp` claim is absent/empty/not-allowlisted is REJECTED (401), closing the
  Clerk-library azp-skip gap so a token from a different app on the same instance cannot pass
  (`worker/src/lib/clerk_auth.ts:171-176`).
- When `CLERK_ISSUER_URL` is set, the issuer is exact-pinned — `iss !== clerkIssuerUrl` is a 401
  (`worker/src/lib/clerk_auth.ts:191`).
- Identity REQUIRES a subject: a verified token without `sub` is 401, so no tenant lookup runs on a
  subjectless session (`worker/src/lib/clerk_auth.ts:228`).
- Tenant resolution is always keyed on the verified `clerk_user_id` via parameterized D1 queries (no
  injection surface), but it is NOT a single owner-row lookup in the dashboard pipeline: the owner arm
  (`worker/src/lib/tenant_lookup.ts:114`, `worker/src/lib/clerk_auth.ts:248`) is tried first, then an
  additive `team_member` fallback resolves the user to the team-OWNING tenant when no owner row matches
  (`worker/src/lib/clerk_auth.ts:261-264`).
- The `team_member` fallback resolves ONLY an `status = 'active'` seat — an `invited` or `removed`
  member never resolves, so a revoked seat is denied rather than silently retaining cross-tenant access
  (`worker/src/lib/clerk_auth.ts:261-264`).
- A verified subject matching neither the owner row nor an active `team_member` row is fail-CLOSED — the
  team-member-MISS branch returns 403 in the dashboard pipeline (`worker/src/lib/clerk_auth.ts:265-269`),
  404 on the server-to-server endpoint (`worker/src/lib/tenant_lookup.ts:125-126`).
- The githugr multi-issuer arm cannot run unless ALL three `GITHUGR_*` settings are present — the routing
  gate AND-conjoins them, so the dormant arm is closed by default
  (`worker/src/lib/clerk_auth.ts:127`).

# Gotchas

- **Designed vs wired:** the CoreLink Clerk session verify is wired + live; the **githugr** arm
  (`worker/src/lib/clerk_auth.ts:127`, `:346`) is DORMANT — gated on `GITHUGR_CLERK_ISSUER_URL`,
  `GITHUGR_CLERK_JWT_KEY`, and `GITHUGR_TENANT_ID`. Until those secrets are provisioned the routing `if`
  is false and the function only ever runs the CoreLink path.
- The `peekUnverifiedIssuer` step is ROUTING-ONLY — it parses an UNVERIFIED payload to decide which
  instance minted the token; the signature is verified afterward inside the chosen arm
  (`worker/src/lib/clerk_auth.ts:127`). Never trust a peeked claim.
- This concept covers the HUMAN identity spine. Machine traffic uses the PAT moat instead — see
  [the 2-level PAT moat](/auth/pat-moat.md).
- A non-production deploy with `CLERK_ISSUER_URL` unset falls back to a conservative shape-check
  (`https` + host contains "clerk"); production fails CLOSED instead, so the operator MUST provision the
  pin before deploy or all Clerk auth 401s (`worker/src/lib/clerk_auth.ts:197-211`).
- `tenant.email_hash` is `SHA-256(clerk_user_id)`, NOT a raw-email hash, so the lookup endpoint's
  email-fallback is genuinely N/A — only the `sub` key resolves (`worker/src/lib/tenant_lookup.ts:103-108`).

# Citations

1. `worker/src/lib/clerk_auth.ts:92` — `verifyClerkSessionAndResolveTenant`, the single shared pipeline.
2. `worker/src/lib/clerk_auth.ts:102` — missing bearer token → 401 before any verify.
3. `worker/src/lib/clerk_auth.ts:135-138` — `CLERK_SECRET_KEY` unbound → 403 fail-CLOSED.
4. `worker/src/lib/clerk_auth.ts:162-165` — `verifyToken` signature check with the azp allowlist.
5. `worker/src/lib/clerk_auth.ts:171-176` — M1 post-verify azp present-and-allowlisted re-assert → 401.
6. `worker/src/lib/clerk_auth.ts:191` — M2 exact issuer pin (`iss !== clerkIssuerUrl` → 401).
7. `worker/src/lib/clerk_auth.ts:197-211` — production fails CLOSED when `CLERK_ISSUER_URL` is unset.
8. `worker/src/lib/clerk_auth.ts:228` — verified token without `sub` → 401.
9. `worker/src/lib/clerk_auth.ts:265-269` — the **team-member-MISS** branch: a subject with no owner row AND no active `team_member` row → 403 (dashboard pipeline). NOT a plain no-row branch — it is reached only after the owner arm misses and the `team_member` fallback also misses.
10. `worker/src/lib/clerk_auth.ts:248` — owner arm: `SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1`.
11. `worker/src/lib/clerk_auth.ts:261-264` — additive team-member fallback: `SELECT tenant_id FROM team_member WHERE user_id = ?1 AND status = 'active'` resolves to the team-owning tenant (migration 0074).
12. `migrations/d1/0074_team_member.sql:65-66` — the `(user_id, status)` index backing the team-member resolution arm (the `team_member` table is defined at `:28-57`).
13. `worker/src/lib/clerk_auth.ts:273-279` — D1 fault → 500 fail-CLOSED, never fail-open.
14. `worker/src/lib/clerk_auth.ts:127` — DORMANT githugr routing gate (all three `GITHUGR_*` settings AND opt-in AND peeked issuer match).
15. `worker/src/lib/clerk_auth.ts:346` — githugr arm authoritative issuer exact-pin (only reached when the dormant gate opens).
16. `worker/src/lib/tenant_lookup.ts:79` — `handleTenantLookup`, the internal-auth-gated server-to-server endpoint.
17. `worker/src/lib/tenant_lookup.ts:103-108` — `sub` required; email fallback is N/A (email_hash is a clerk-id surrogate).
18. `worker/src/lib/tenant_lookup.ts:114` — parameterized `SELECT ... FROM tenant WHERE clerk_user_id = ?1`.
19. `worker/src/lib/tenant_lookup.ts:125-126` — no row → fail-CLOSED 404 (and the owner-only asymmetry: a team-seat-only user that resolves in the dashboard is a 404 here, since this endpoint never consults `team_member`).
20. `worker/src/lib/tenant_lookup.ts:66-71` — the lookup response contract is `role: "owner"` only — owner-keyed, 1:1 at provision; no team-member arm.
21. `worker/src/lib/tenant_lookup.ts:130-137` — success envelope coerces an unset/empty `tier` to the most-restrictive `"free"` (fail-safe default, never read back more privileged).

# DECIDED PACKET → corelink-server TL — the CF multi-tenant authorization surface (the go-live gargalo) + per-tenant fairness (no-waiver)

> **From:** clw coordinator (go-live lead) · **To:** corelink-server TL · **cc** runners TL, owner · **Date:** 2026-07-03
> Architecture RATIFIED by the lead (design plan grounded in your code). This is a decided spec — implement, don't re-design. The one thing that makes the runner-CI path multi-tenant-safe.

## Why (grounded)
The deployed CF Worker `/webhook` autoscaler is structurally SINGLE-TENANT: it reads the firing repo's
`full_name` (`deploy/cloudflare/src/index.ts:351`) only to mint a JIT, then mints the per-job CAS PAT under a
HARDCODED `env.CLW_TENANT` (`wrangler.jsonc:16` = `ee30f7ba…`, threaded `lib.ts:125,178`) regardless of which
tenant's repo fired. No repo→tenant binding, no allowlist, no suspend on that plane (C1's `leases.rs:372` is the
Rust fabric plane, never invoked here). An arbitrary user cannot get their own isolated runner tenant.

## Ratified decisions (yours to implement)
- **DP1 — authorize SERVER-SIDE in `handleRunnerMint` (worker/src/lib/runner_mint.ts:112-224).** The Worker
  STOPS sending `owner_tenant`; the mint endpoint DERIVES the tenant from the GitHub installation and authorizes
  there — the single chokepoint (mirrors how the fabric funnels through `leases.rs:372`). New request body:
  `{ job_id, scope?, ttl_seconds?, repo_full_name, installation_id }`; response unchanged
  (`{token_plaintext, pat_id, token_id, tenant, expires_ms}`) — the Worker reads `tenant` from it as authoritative.
- **DP2 — key the identity map on `installation_id`** (immutable, on every payload). NET-NEW D1 table
  `tenant_gh_installation_map(installation_id PK, tenant_id, created_at_ms)` modeled EXACTLY on
  `migrations/d1/0083_tenant_org_map.sql` (Clerk-keyed 0083 cannot be reused). Add a `resolve_tenant_for_installation`
  mirroring `resolve_tenant_for_org` (`auth_introspect.rs:689-720`), **lookup-only, 403 on unmapped (no oracle)**.
- **DP3 — provision on GitHub `installation:created`** (signup-worker), gated by an authenticated CoreLink
  identity (the install callback carries the Clerk session/`state`). NOT lazy-provision (that fail-opens the
  `runners_entitlement` gate at `runner_mint.ts:210`).
- **DP4 — suspend check folded into the same mint authorization** (reuse the fabric semantic `leases.rs:225`).
- **DP5 — one D1 `runner_repo_allowlist(tenant_id, repo_full_name)`** that BOTH the Rust fabric and the CF mint
  read (no divergent copies). The mint authz checks, in one place: (a) installation mapped, (b) not suspended,
  (c) repo on tenant's allowlist, (d) entitlement (already there) → any miss = **403 generic** (no existence oracle).
- **NO-WAIVER: per-tenant FAIRNESS folds in here.** Since the mint now knows the tenant, add a per-tenant
  concurrency/admission bound at mint time (or expose the tenant so the Worker's per-tenant DO counter can gate)
  — the owner refused the "single-tenant, defer fairness" waiver, so max_instances:2 must be paired with a
  per-tenant admission cap once multi-tenant. Recommend the mint endpoint returns/enforces the tenant's
  concurrency ceiling (from `runners_entitlement`).

## The seam I own (runners/CF Worker) — for your awareness, being packeted to runners
The Worker changes (extract `installation.id` at `index.ts:351`; pass `repo_full_name`+`installation_id`
through `spawnRunner → buildContainerEnv → mintCasPat`; read the server-returned `tenant`; **reorder mint AFTER
authz so `mintJit` never runs for an unauthorized repo**; and CRITICALLY **treat a 403 as a hard deny — do NOT
let `buildContainerEnv`'s fail-open-to-cold at `lib.ts:186-190` swallow a 403 into a wrong-tenant cold spawn**;
5xx may still fail-open). That half is agent-implementable ONCE your seam is frozen.

## Reply with
The frozen `handleRunnerMint` request/response shape + the new-table migrations + the `installation:created`
provisioning + the per-tenant concurrency source. Then I dispatch the Worker half against it. This is THE
biggest go-live gap and ~70% of it is your surface — flagging it as such.

— clw coordinator

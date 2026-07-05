# NUDGE → corelink-server TL — all 4 cf-multitenant WPs are merged; I verified prod read-only. The cutover is one grounded step from fire. Here's the exact condition.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner · **Date:** 2026-07-04
> Poll-tick finding. No new ask beyond confirming ONE thing — grounded in a read-only prod probe, not a guess.

## What I confirmed this tick (read-only, no mutation)
- ✅ **WP1 (0084/0085 tables)** merged (#615) · **WP2** (derive + 4-check authz + `max_concurrency`) merged ·
  **WP3** (Rust resolver + single-source allowlist) merged · **WP4** (identity-gated `installation.created`
  provisioning primitive) **now merged** — I saw it land. All four are on `main`.
- ✅ githugr **#63 + #64 merged** (B2 per-tenant write authz + GDPR1 consume) — the consume side is in.
- ⚠️ **But prod `CONFIG_DB` does NOT yet have the tables applied:** a read-only
  `SELECT COUNT(*) FROM tenant_gh_installation_map` returns **`no such table`**. The migration is *merged in
  code*, not *applied to the remote D1*. It gets applied by the cutover `cf-deploy-prod` migration leg (same as
  0083 in LEG 1) — expected, flagging so we're aligned on sequencing.

## Why I am STILL holding the fire (this is the whole point)
The mint half (WP2 `handleRunnerMint` derive) reads `tenant_gh_installation_map`. Two failure shapes if I fire blind:
1. **Deploy before the migration applies** → the Worker queries a non-existent table → **500 → runner-side
   fail-open-COLD → the fleet stops spawning.** (The cf-deploy-prod leg applies 0084/0085 first, so this is
   avoided *by sequencing*, not by luck — I want your confirm that the deploy's migration leg is in the same run.)
2. **Deploy with the table present but EMPTY** → the dogfood installation resolves to no tenant → the 4-check
   authz returns the generic **403 → spawn abort, fleet-wide.** This is the real gate.

## The single fire-condition I need from you (then I fire same-window)
> **Confirm the dogfood installation is provisioned into `tenant_gh_installation_map` (or will be seeded as part
> of the cutover) so the map is NON-EMPTY the instant the derive path goes live.**

WP4 gives the *primitive* (write-on-`installation.created`); it does not by itself mean a row exists. Either:
- **(a)** the dogfood GitHub App install has fired `installation.created` post-WP4-deploy and I can see a row, **or**
- **(b)** you hand me a one-line seed (tenant_id + installation_id + repo) I apply in the same cutover window
  *before* signaling runners.

## The atomic window I run on your green (unchanged from the coord doc, now fully armed)
1. You confirm (a) or (b) above.
2. I re-dispatch **`cf-deploy-prod`** → applies 0084/0085 + deploys the derive/mint Worker + `max_concurrency`.
3. I verify the map is non-empty (read-only) **before** step 4.
4. I signal **runners TL to deploy #283** (the Docker-free `--containers-rollout=none` one-liner) — same window.
5. G4 fairness closes (mint returns `max_concurrency`); the gargalo is retired.

**Everything is merged. The only thing between us and firing is your confirm on the map row.** Ping me (a) or (b)
and I run the window end-to-end.

— clw coordinator

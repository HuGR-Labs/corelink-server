# FIRED → corelink-server TL — step 1 dispatched (`cf-deploy-prod` run 28741116392). Execute step 2 (App create + install) the instant it's green. #283 stays HELD until I verify the map row.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner ("fire") · **Date:** 2026-07-05
> Acting on your `2026-07-05-GO-SIGNAL` path (a). Pre-fire rigor done; firing back-to-back to zero the gap.

## Step 1 — FIRED ✅
- `cf-deploy-prod.yml` dispatched, **run 28741116392** (`main`, in_progress, all 5 envs, blank env selector).
- **Migrations-first confirmed** (H3): the `migrate` job runs `scripts/apply-d1-migrations-prod.sh --apply`
  (idempotent) BEFORE any deploy → applies 0083 (already in) + **0084 map / 0085 allowlist / 0086 ac-key /
  0087 runner_billing**, then the 5-env Worker deploy + container re-pin (LEG 3 folds in).
- **Pre-fire checks I ran (so this leg is safe standalone):** the secrets gate requires only the CORE set
  (`CORELINK_INTERNAL_AUTH_KEY`, `PAT_SIGNING_KEY`, `R2_S3_*`) + prod billing/auth (`STRIPE_*`, `CLERK_SECRET_KEY`,
  `PAGERDUTY_ROUTING_KEY`) — all already populated — and does NOT gate on the new `GITHUB_APP_*` install-flow
  secrets, so the deploy won't chicken-and-egg on the secrets you bind in step 2. Install-flow ships inert (503)
  until then, as you said.

## Your step 2 — GO the instant run 28741116392 is green
Create the private "CoreLink Runners" App (`/install/github/app/new?setup_token=…`) → bind the signup-worker +
admin-ui secrets → install on the dogfood org **tenant `d863fafb-17c3-4ec3-92f6-b5a85c27d7bd`**. The signed-state
callback (#627 `writeInstallationProvision`) auto-writes `tenant_gh_installation_map` + `runner_repo_allowlist`
under `d863fafb`. **Ping me the moment the install fires** (or just tell me it's done — I'll read the map).

## Step 3 — mine, the fleet-safety gate (unchanged)
The instant you confirm the install: I read `tenant_gh_installation_map` (read-only) and **only if the `d863fafb`
row exists** do I signal the runners TL to deploy #283 → then smoke (dogfood job mints under its REAL tenant =
200+spawn; a non-allowlisted repo = 403, no spawn). **#283 does NOT go out on an empty map — that guard holds.**

Monitoring run 28741116392 to green now; I'll post the deploy-green marker here + tell you to go.

— clw coordinator

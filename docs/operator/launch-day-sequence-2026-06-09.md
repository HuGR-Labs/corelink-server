# Launch-day sequence (operator runbook) — pre-flight 2026-06-09

**Status:** pre-flight complete. Prod currently runs the **STALE** container
`f9859a59-r1` (pre-security-audit). All money-path code is merged; the flat-host
configs (PR #183) and the cutover-runbook landmines (this PR) are fixed. This is
the consolidated, verified-as-of-today launch sequence; it references the canonical
scripts + `specs/_runbooks/RB-GA-CUTOVER.md`.

## What only the operator can do

`.env.local` holds **TEST** keys (`sk_test_…`). The **LIVE** Clerk/Stripe keys are
set at launch from the operator dashboards. The Worker deploy gate
(`cf-deploy-prod.yml`) **hard-requires** these in Cloudflare before it will deploy:
`STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_ID_TEAM`,
`STRIPE_PRICE_ID_PRO`, `CLERK_SECRET_KEY`, `PAGERDUTY_ROUTING_KEY`.

## Sequence (Phase D → H, per `cutover-checklist-prod.sh`)

Confirm each script's exact flags via `bash scripts/<name> --help` before running.

**0. Pre-reqs** — `.env.local` backed up (CF API token, `CORELINK_INTERNAL_AUTH_KEY`,
`PAT_SIGNING_KEY`, `CORELINK_OCI_TOKEN_KEY`); R2 AC/CAS buckets provisioned.

**D1 — LIVE secrets into Cloudflare** (the deploy gate depends on these):
- `bash scripts/put-secrets-prod.sh --mvp-only` — reads `scripts/secrets-mvp-allowlist.txt`
  (CF×3, Clerk×2, Stripe×6 incl. `STRIPE_WEBHOOK_SECRET` + `STRIPE_PRICE_ID_TEAM/_PRO`,
  `CORELINK_INTERNAL_AUTH_KEY` + `CORELINK_DPA_VERSION` (no key → **no signups**),
  `R2_S3_*`, BetterStack, PagerDuty, Resend, `PAT_SIGNING_KEY`, `CORELINK_OCI_TOKEN_KEY`).
- Verify: `bash scripts/verify-secrets-deployed.sh` → exit 0.

**D2 — D1 migrations** (runner now accepts the real count = **60**, was hard-paused at 52):
- Dry-run: `bash scripts/apply-d1-migrations-prod.sh` (lists 60 + runs the additive guard).
- Apply: `bash scripts/apply-d1-migrations-prod.sh --apply`.

**E — Container** (replaces the stale `f9859a59-r1` with today's audited code):
- `bash scripts/build-container-prod.sh` → `bash scripts/e-day-container-pre-push-scan.sh`
  → `bash scripts/push-container-prod.sh` (or `push-container-multiregion.sh`).
- Canary: `bash scripts/e-day-container-canary-promote.sh` (5% → watch → 100%).
- Verify: `bash scripts/verify-container-prod.sh`.

**F — Pages** — already live (`corelink-docs/app`). Re-deploy only if docs/admin-ui
changed: `deploy-pages-docs-prod.sh` / `deploy-pages-admin-ui-prod.sh`.

**G — DNS** — already live (flat scheme). **Verify-only:**
`bash scripts/dns-prod-verify.sh --post-apply` (reconciled to flat in this PR).

**H — Worker deploy + cutover:**
- Worker: trigger the `cf-deploy-prod` workflow (manual dispatch, type `deploy-prod`).
- Smoke: `bash scripts/smoke-prod-corelink.sh` (18 checks, flat hosts).
- Cutover: `bash scripts/cutover-checklist-prod.sh` (15 items; final sign-off = type
  `CUTOVER`; auto-rollback on post-cutover smoke fail).

**Rollback (any time):** `bash scripts/rollback-prod-corelink.sh`.

## ⚠️ Owner-flags to resolve BEFORE / AT launch

1. **Clerk CSP host (AUTH-critical).** `clerk.corelink.humangr.com` (in `admin-ui` CSP)
   does not resolve. Confirm the Clerk custom-domain CNAME is set at launch, **or** the
   CSP must list Clerk's default Frontend-API domain — else the sign-in widget is
   CSP-blocked. See `docs/operator/host-scheme-canonical-2026-06-09.md`.

   > **CORRECTION 2026-08-22 (RESOLVED — read this, not the flag above).** The host
   > named above is wrong and was never the CSP host. The live, auth-critical Clerk
   > production Frontend API is **`clerk.corelink-app.humangr.com`** — baked into the
   > `pk_live_` publishable key and hardcoded in `apps/admin-ui/src/lib/csp.ts:137,166,169`
   > (`script-src` / `connect-src` / `frame-src`). `clerk.corelink.humangr.com` is
   > NXDOMAIN and always was; checking it during an auth incident proves nothing.
   > `clerk.corelink-app.humangr.com` is a SEPARATE DNS record from the retired
   > `corelink-app.humangr.com` app subdomain — do not remove it.

2. **`get.corelink.io`** — install host does not resolve; `e2e-prod`'s install-script
   test stays red until it is provisioned (or that test is gated).
3. **Billing portal is a STUB** — the customer self-service "Manage billing" returns a
   fake Stripe URL (`InMemoryCustomerHandler::portal_url`); wire to the real Stripe
   portal post-launch (checkout + webhook provisioning are NOT affected).

## Already verified (pre-flight, 2026-06-09)

- **Money-path code:** all 7 audit-#27 launch-blockers merged (#170–#181). The scheduled
  `e2e-stripe-checkout` + `e2e-clerk-signup` smokes are **GREEN** on current `main`.
- **317 container lib tests green** (quiet-window run).
- **Flat-host configs fixed** (PR #183); **cutover-runbook landmines fixed** (this PR:
  migration count 52→60, `dns-prod-plan/verify` dotted→flat).
- Prod hosts live: `corelink-{api,app,docs,signup,admin}.humangr.com`.

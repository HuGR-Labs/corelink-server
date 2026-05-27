---
id: "AUDIT-2026-05-27-PHASE-0-PAGES-SECRETS-RUNBUNK"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "READY-FOR-EXECUTION"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter (operator) / CF-PAGES-SECRETS-PREP agent (author)"
tags: ["runbook", "phase-0", "cf-pages", "secrets", "admin-ui", "launch-blocker"]
references:
  - "specs/_audits/2026-05-27-phase-0-execution-plan.md"
  - "specs/_audits/2026-05-27-launch-readiness-check.md"
  - "specs/_audits/sealed/2026-05-26-w32-phaseF-apply-admin-ui-closure.md"
---

# Phase 0 — CF Pages Secrets Runbook (drop-in, ≤ 5 min)

> **Mandate.** Apply the three blocking Cloudflare Pages secrets on the `corelink-admin-ui` Pages project so that the `[locale]/*`, `/sign-up`, and `/api/clerk/webhook` routes stop returning HTTP 500. This is the BLOCKING follow-up flagged by `2026-05-26-w32-phaseF-apply-admin-ui-closure.md` §8 and the RED item L2 in `2026-05-27-launch-readiness-check.md`.
>
> **Who runs this.** Gustavo (operator). The Phase 0 agent author of this doc never executes wrangler commands — secrets live only in Gustavo's local `.env.local` and in the dashboards.
>
> **Budget.** ≤ 5 min start-to-finish, including the re-deploy trigger and the four smoke curls.
>
> **Scope.** Only the three secrets called out as BLOCKING. Non-blocking optional secrets (Sentry DSN, PostHog key, etc.) are out of scope for this runbook.

---

## §1 Pre-flight (≤ 60 s)

Open a terminal at the repo root (or anywhere — wrangler is global). Confirm CLI auth + project visibility:

```bash
# 1.1 Confirm wrangler is authenticated as the right CF account.
wrangler whoami

# 1.2 Confirm the Pages project exists and is named exactly `corelink-admin-ui`.
wrangler pages project list | grep corelink-admin-ui
```

**Expected.**

- `wrangler whoami` prints your Cloudflare email and the account that owns the `corelink-admin-ui` Pages project. If it prints a different account, run `wrangler logout && wrangler login` and re-auth into the correct account before continuing.
- `wrangler pages project list | grep corelink-admin-ui` prints a single row with project name `corelink-admin-ui` and a "Last modified" timestamp. Project ID for reference: `36d65631-252a-4f76-8ca7-8fc4c1c555a9` (from `2026-05-26-w32-phaseF-apply-admin-ui-closure.md` §5.1).

If either check fails, **STOP** — do not proceed to §2 until pre-flight is clean.

---

## §2 Apply the three secrets (≤ 3 min)

Each command opens an interactive prompt: wrangler writes `?` and waits for the secret value on stdin. **Paste from your local `.env.local`** — never type the value plainly into a shared chat, screen share, or commit it anywhere. Hit Enter; wrangler confirms `✨ Success! Uploaded secret <NAME>.`

```bash
# 2.1 Clerk — server-side session validation + JWT verification.
wrangler pages secret put CLERK_SECRET_KEY --project-name=corelink-admin-ui
# (paste CLERK_SECRET_KEY value from .env.local at the prompt)

# 2.2 Stripe — Checkout Session creation + webhook signature verification.
wrangler pages secret put STRIPE_SECRET_KEY --project-name=corelink-admin-ui
# (paste STRIPE_SECRET_KEY value from .env.local at the prompt)

# 2.3 Resend — transactional email (invites, password resets, billing receipts).
wrangler pages secret put RESEND_API_KEY --project-name=corelink-admin-ui
# (paste RESEND_API_KEY value from .env.local at the prompt)
```

**Note on environments.** `wrangler pages secret put` defaults to the `production` environment. If you also want the same secret available on Preview deployments, append `--env preview` and re-run. For Phase 0 launch unblock, **production only** is sufficient — preview environments are not customer-facing.

**Verification (optional, 10 s).** List the secret names now stored (values are never echoed back):

```bash
wrangler pages secret list --project-name=corelink-admin-ui
```

Expected output includes `CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY` as `secret_text` entries.

---

## §3 Source of each secret (where to fetch / rotate from)

| Secret | Dashboard URL | Where to copy |
|---|---|---|
| `CLERK_SECRET_KEY` | <https://dashboard.clerk.com/> → select the **corelink-app.humangr.com** instance → **API keys** (left nav) → **Secret keys** tab | Click "Show" next to the `sk_live_...` key, copy. If rotating: click "Add new key", copy the new value, paste here, then revoke the old key after §5 smoke passes. |
| `STRIPE_SECRET_KEY` | <https://dashboard.stripe.com/apikeys> (toggle to **Live mode** in the top-right) | Restricted key recommended (scope: `Checkout Sessions` write + `Webhook Endpoints` write + `Customers` write); copy `rk_live_...`. Standard `sk_live_...` also works. **Never use the test-mode key in production.** |
| `RESEND_API_KEY` | <https://resend.com/api-keys> | Click "Create API key" if no production key exists; scope `sending_access` for the `humangr.com` domain. Copy `re_...`. |

**Rotation cadence.** Per Wave-32 §10.1 (CTRL-CRED-001) all three secrets should be rotated:
- on operator handoff,
- on any suspected leak (push to public repo, screen-share frame leak, etc.),
- at minimum every 90 days.

Each rotation = re-run §2 with the new value, then revoke the prior value in the source dashboard once §5 smoke passes on the new value.

**Never** commit any of these values to the repo. `.env.local` is already gitignored (verified by Wave-32 §10.1).

---

## §4 Trigger a re-deploy (so the new secrets take effect) (≤ 60 s)

CF Pages does **not** automatically restart workers when a secret changes. You must trigger a new deployment to bind the secrets into the running worker. Pick one of the two paths below — they are equivalent.

### §4.A — Recommended: redeploy the current production commit via dashboard

1. Open <https://dash.cloudflare.com/> → **Workers & Pages** → **corelink-admin-ui** → **Deployments** tab.
2. Find the most recent **Production** deployment (top of the list, branch `main`).
3. Click the `⋯` menu on that row → **Retry deployment**. CF rebuilds + redeploys with the new secrets bound.
4. Wait for the deployment status to flip to **Success** (~60–90 s).

### §4.B — Alternative: re-deploy from local build via wrangler

Use this if §4.A is unavailable or if you have a newer local build you want shipped:

```bash
cd apps/admin-ui
pnpm run build:cf
npx wrangler@latest pages deploy \
  .vercel/output/static \
  --project-name=corelink-admin-ui \
  --branch=main \
  --commit-dirty=true \
  --no-bundle
```

(The `--no-bundle` flag is required — see `2026-05-26-w32-phaseF-apply-admin-ui-closure.md` §5.1 for the rationale.)

### §4.C — Alternative: trigger via git push

If you have an in-flight commit that needs to ship anyway, simply `git push` it to the branch CF Pages watches (`main` for production deployments). The push triggers a fresh CF Pages build that picks up the new secrets.

---

## §5 Post-apply smoke (≤ 60 s)

Run these four `curl -I` checks against the production custom domain. They confirm: (a) Clerk-gated routes render instead of 500ing, (b) the public health endpoint stays green, (c) the Clerk webhook requires auth instead of crashing.

```bash
# 5.1 Public sign-up page — must render (Clerk publishable key + secret combo OK).
curl -I https://corelink-app.humangr.com/sign-up

# 5.2 Onboarding entry — was 500 pre-secrets; must be 200 post-secrets.
curl -I https://corelink-app.humangr.com/en/onboarding

# 5.3 Health endpoint — sanity check the Pages worker is alive at all.
curl -I https://corelink-app.humangr.com/api/health

# 5.4 Clerk webhook receiver — must require auth (401) or accept (200) instead of crashing.
curl -I https://corelink-app.humangr.com/api/clerk/webhook
```

**Expected results.**

| # | URL | Expected status | Meaning if matched | Meaning if not matched |
|---|---|---|---|---|
| 5.1 | `/sign-up` | **HTTP/2 200** | Clerk `<SignUp />` component rendered successfully (publishable key + secret key both present). | If 500: secret didn't bind — re-trigger §4. If 404: Pages routing broken (unrelated, escalate). If 200 but content is the "Clerk publishable key not configured" placeholder: build needs `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` rebaked; not a §2 fix. |
| 5.2 | `/en/onboarding` | **HTTP/2 200** (was 500 per `2026-05-27-launch-readiness-check.md` §2) | Locale-gated route rendered; Clerk middleware accepted the secret. | If still 500: open `https://dash.cloudflare.com/?to=/:account/workers-and-pages/view/corelink-admin-ui/deployments` → click the active deployment → **Functions** tab → tail Real-time logs while re-running the curl. Look for "Missing CLERK_SECRET_KEY" or similar — confirms §2 didn't bind. |
| 5.3 | `/api/health` | **HTTP/2 200** | Worker process is alive. Should already have been 200 pre-secrets — this is a control. | If non-200: Pages worker itself is down — unrelated to secrets, escalate before continuing. |
| 5.4 | `/api/clerk/webhook` | **HTTP/2 401** (no Svix signature header) **or HTTP/2 200** | Webhook handler reached; auth check ran. The 401 is the healthy "no signature on this GET" path. | If 500: webhook handler crashed before auth check, likely because `CLERK_SECRET_KEY` is still unbound — re-trigger §4. |

If all four match the expected band, **the launch-readiness L2 RED is resolved.** Mark §2.B in `2026-05-27-phase-0-execution-plan.md` as complete and re-run launch-readiness smoke to lift L2 from RED to GREEN.

---

## §6 Rollback (if a bad value was pasted)

If a secret was pasted incorrectly (e.g. test-mode key in production, truncated paste, wrong account's key), roll it back in two steps:

```bash
# 6.1 Delete the bad value.
wrangler pages secret delete CLERK_SECRET_KEY --project-name=corelink-admin-ui
# (replace CLERK_SECRET_KEY with whichever of the three was bad)

# 6.2 Re-paste the correct value from .env.local.
wrangler pages secret put CLERK_SECRET_KEY --project-name=corelink-admin-ui
# (paste the correct value at the prompt)

# 6.3 Re-trigger the deploy (so the corrected secret binds).
# Repeat §4.A (or §4.B / §4.C).

# 6.4 Re-smoke (so you confirm the fix).
# Repeat §5.
```

**Rollback budget.** ≤ 2 min per secret. If multiple secrets need rollback, batch the deletes + re-puts before triggering a single re-deploy in §6.3 (one rebuild covers all corrected secrets).

**Last-resort full rollback.** If the new secrets caused worse behaviour than the pre-§2 state (e.g. all routes 500 instead of just the gated ones), revert to the previous deployment from the CF dashboard: **Workers & Pages → corelink-admin-ui → Deployments → previous Production row → ⋯ menu → Rollback to this deployment.** This restores the prior worker bundle without changing the secrets (so subsequent re-deploys will pick the new secrets back up).

---

## §7 Sign-off checklist

Operator (Gustavo) confirms each line after execution:

- [ ] §1 pre-flight: `wrangler whoami` shows correct account; `wrangler pages project list | grep corelink-admin-ui` returns one row.
- [ ] §2 all three secrets uploaded; `wrangler pages secret list` shows all three names.
- [ ] §4 re-deploy triggered and reached **Success** state in CF dashboard.
- [ ] §5.1 `/sign-up` → 200.
- [ ] §5.2 `/en/onboarding` → 200 (was 500).
- [ ] §5.3 `/api/health` → 200.
- [ ] §5.4 `/api/clerk/webhook` → 401 or 200 (not 500).
- [ ] Launch-readiness §2 L2 lifted from RED to GREEN; Phase 0 execution plan §2.B marked complete.

Once all boxes are ticked, this runbook's `audit_status` flips to **EXECUTED-VERIFIED** and the doc moves to `specs/_audits/sealed/` per the standard SEAL cadence.

---

*End of Phase 0 CF Pages secrets runbook.*

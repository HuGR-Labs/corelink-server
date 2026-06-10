# Runbook — corelink-app.humangr.com domain flip (Pages → Worker)

**Status:** config fix in this PR (`apps/admin-ui/wrangler.toml` route added);
the actual flip is **owner-gated** — it changes what production serves on the
public app hostname.
**Discovered:** pre-existing since ≥ 2026-05-27 (launch-readiness audit §2),
root-caused 2026-06-10.
**Blocks:** the public launch flip — docs pricing CTAs (`apps/docs/src/pages/pricing.tsx`
`APP_BASE`), all 6 compare pages, quickstarts (4 locales), legal/terms and the
docs navbar all target `corelink-app.humangr.com`, and the Clerk **production**
Frontend API is `clerk.corelink-app.humangr.com` (it is Clerk's home domain).

## TL;DR — root cause

`corelink-app.humangr.com` is still a **custom domain of the CF Pages project
`corelink-admin-ui`**, whose last build (commit `3daebca6`, 2026-05-29) is the
broken `@cloudflare/next-on-pages` one: `/` → literal `"Not Found"` (HTTP 200,
`text/plain`), **every SSR route → 500** (`/sign-up`, `/en/onboarding`,
`/en/legal/*`, …). next-on-pages 1.13.7 cannot render Next 15 SSR with
next-intl + middleware + ClerkProvider — that is *why* the app was migrated to
the `corelink-admin-ui` **Worker** (`@opennextjs/cloudflare`) on 2026-05-30
(`035f10ec` / `5435fd8a`). The migration moved **only**
`corelink-admin.humangr.com` to the Worker; `corelink-app.humangr.com` was left
behind on the dead Pages project.

It is **not** a code, env or secret problem: the same build serves
`corelink-admin.humangr.com` → `/` 200 (38 KB HTML), `/sign-up` 200, with the
correct CSP (`clerk.corelink-app.humangr.com`). The stale Pages build even
carries the pre-Wave-32 **dotted** hostnames (`clerk.corelink.humangr.com`,
`api.corelink.humangr.com`) in its CSP — both dead.

## Evidence (live prod, read-only, 2026-06-10)

| Probe | Result |
|---|---|
| `GET https://corelink-app.humangr.com/` | 200, body = `Not Found` (9 bytes, `text/plain`) |
| `GET https://corelink-app.humangr.com/sign-up` | **500**, CSP references dead `clerk.corelink.humangr.com`, `x-matched-path: /sign-up/[[...sign-up]]` |
| `GET https://corelink-admin.humangr.com/sign-up` | **200**, CSP references live `clerk.corelink-app.humangr.com` |
| `GET https://corelink-admin-ui.pages.dev/sign-up` | **500** (the Pages build itself is broken, independent of DNS) |
| `wrangler pages project list` | `corelink-admin-ui` domains = `corelink-admin-ui.pages.dev, corelink-app.humangr.com` |
| `wrangler pages deployment list --project-name corelink-admin-ui` | last Production deploys = commit `3daebca6` (pre-migration next-on-pages era) |
| `wrangler deployments list --name corelink-admin-ui` (Worker) | 7 deploys, latest **2026-06-10T06:01:35Z** — fresh and healthy |

## The flip (owner-gated, ~10 min, no build required)

All credentials from repo-root `.env.local` (`CLOUDFLARE_API_TOKEN`,
`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_ZONE_ID_HUMANGR`). Token scopes needed:
**Pages:Edit** (step 1), **Zone DNS:Edit** (step 2), **Workers Custom
Domains/Scripts:Edit** (step 3). Use
`worker/node_modules/.bin/wrangler` (4.95.0) — bare `npx wrangler` at repo root
resolves a broken v3.

### 0. Preflight (read-only)

```sh
curl -sS -o /dev/null -w '%{http_code}\n' https://corelink-admin.humangr.com/sign-up   # expect 200
curl -sS -o /dev/null -w '%{http_code}\n' https://corelink-app.humangr.com/sign-up     # expect 500 (the bug)
```

### 1. Detach the domain from the Pages project

```sh
curl -sS -X DELETE \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/pages/projects/corelink-admin-ui/domains/corelink-app.humangr.com"
```

### 2. Delete the stale DNS record (CNAME → corelink-admin-ui.pages.dev)

```sh
# find it
curl -sS -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/zones/$CLOUDFLARE_ZONE_ID_HUMANGR/dns_records?name=corelink-app.humangr.com"
# delete it (id from the previous response)
curl -sS -X DELETE -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/zones/$CLOUDFLARE_ZONE_ID_HUMANGR/dns_records/<RECORD_ID>"
```

### 3. Attach the domain to the Worker (pick ONE)

**A — CF API (recommended: no local build, exactly the 2026-05-30 migration
recipe):**

```sh
curl -sS -X PUT \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" -H "Content-Type: application/json" \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/workers/domains" \
  -d "{\"zone_id\":\"$CLOUDFLARE_ZONE_ID_HUMANGR\",\"hostname\":\"corelink-app.humangr.com\",\"service\":\"corelink-admin-ui\",\"environment\":\"production\"}"
```

**B — wrangler config-only push** (applies the `routes` block from
`apps/admin-ui/wrangler.toml` — this PR — without uploading code; the
subcommand is marked *experimental* in wrangler 4.95, so prefer A if it
misbehaves):

```sh
cd apps/admin-ui && ../../worker/node_modules/.bin/wrangler triggers deploy
```

**C — full redeploy** (only if you want fresh code anyway; the Mac is shared —
run a SINGLE build, no parallel storms):

```sh
cd apps/admin-ui && pnpm cf:build && pnpm cf:deploy
```

### 4. Post-flip smoke (the gate)

```sh
curl -sS -o /dev/null -w '/         %{http_code}\n' https://corelink-app.humangr.com/            # 200, HTML shell (NOT "Not Found")
curl -sS https://corelink-app.humangr.com/ | grep -c '<body'                                      # ≥ 1
curl -sS -o /dev/null -w '/sign-up  %{http_code}\n' https://corelink-app.humangr.com/sign-up      # 200
curl -sS -o /dev/null -w '/sign-in  %{http_code}\n' https://corelink-app.humangr.com/sign-in      # 200
curl -sS -o /dev/null -w '/health   %{http_code}\n' https://corelink-app.humangr.com/api/health   # 200
curl -sS -o /dev/null -w '/terms    %{http_code}\n' https://corelink-app.humangr.com/en/legal/terms  # 200
curl -sSI https://corelink-app.humangr.com/sign-up | grep -o 'clerk.corelink-app.humangr.com' | head -1  # live Clerk FAPI in CSP
curl -sS -o /dev/null -w 'admin still %{http_code}\n' https://corelink-admin.humangr.com/sign-up  # 200 (unchanged)
```

Then a real browser sign-up against the Clerk widget (Clerk's home domain IS
this hostname, so the widget must load).

## Env / secret checklist (names only — nothing new required)

- Worker secrets (already set; verify names only:
  `wrangler secret list --name corelink-admin-ui`): `CLERK_SECRET_KEY`,
  `STRIPE_SECRET_KEY`, optional `SENTRY_AUTH_TOKEN`.
- Build-time `NEXT_PUBLIC_*` (`NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY`,
  `NEXT_PUBLIC_CORELINK_API_URL`, `NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY`,
  `NEXT_PUBLIC_SENTRY_DSN`, `CSP_ENFORCEMENT`) are baked into the deployed
  Worker bundle — proven correct by `corelink-admin.humangr.com` serving the
  live CSP. The flip adds a hostname; it does not touch the bundle.
- Clerk dashboard: allowed origins already include `corelink-app` (Wave-32
  allowlist, CHANGELOG `corelink-admin`/`corelink-app`/`corelink-docs`).

## Known issues this flip does NOT fix (separate tasks)

1. **`/upgrade?plan=<tier>` → 404 even on the healthy Worker.** Docs pricing
   CTAs (`pricing.tsx`, `calculator.tsx`) link to a route that does not exist
   in `apps/admin-ui` (only `[locale]/upgraded` + `api/checkout/session`
   exist). Launch blocker for the paid CTAs; belongs with the `tier_select.rs`
   checkout keystone.
2. **`/en/welcome` → 500** — pre-existing, known follow-up from `5435fd8a`
   (server-side `currentUser()` without ClerkProvider wrap suspected).
3. The Pages project `corelink-admin-ui` still exists and its
   `corelink-admin-ui.pages.dev` URL still serves the broken build. Optional
   post-flip cleanup: delete the Pages project (owner call; nothing references
   it after the detach).

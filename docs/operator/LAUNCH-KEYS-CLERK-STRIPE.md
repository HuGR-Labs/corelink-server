# Launch keys — Clerk + Stripe (owner fill-in sheet)

**No secrets in this file** — placeholders only. You paste live values into
`.env.local` (gitignored) or hand them to the coordinator; binding to Cloudflare
is done via `scripts/put-secrets-prod.sh` (write-only, you never touch wrangler).
Code-verified against `main` 2026-07-10. Prod host: `corelink-signup.humangr.com`.

---

## CLERK — you provide 3 secrets + set 2 dashboard configs

> Going live = a **Production** Clerk instance (separate from the Development/test one).
> New instance ⇒ new keys, new issuer, its own webhook signing secret.

**Secrets (from Clerk → your Production instance):**
| CF secret name | Where in Clerk | Format |
|---|---|---|
| `CLERK_SECRET_KEY` | API Keys → Secret key | `sk_live_…` |
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` (admin-ui) | API Keys → Publishable key | `pk_live_…` |
| `CLERK_WEBHOOK_SECRET` | Webhooks → (create endpoint below) → Signing Secret | `whsec_…` |

**Dashboard config (not a secret, but launch-blocking):**
1. **Webhook endpoint** → URL `https://corelink-signup.humangr.com/webhooks/clerk`,
   subscribe to **`user.created`** (this is what auto-provisions the tenant). Its
   signing secret = `CLERK_WEBHOOK_SECRET` above.
2. **Allowed origins / redirect URLs + Frontend-API domain** → the flat prod hosts
   (`corelink-app.humangr.com`, `corelink-admin.humangr.com`). The admin-ui CSP must
   list Clerk's Frontend-API host or the sign-in widget is CSP-blocked.

**One caveat I handle at wiring time:** `CLERK_ISSUER_URL` is shared with the githugr
login path — I repin it carefully (not blindly) to the Production issuer. You don't
need to do anything here; just flag me when the Production instance exists.

---

## STRIPE — you provide 2 secrets + create products/prices + 1 webhook

**Secrets (from Stripe → Live mode):**
| CF secret name | Where in Stripe | Format |
|---|---|---|
| `STRIPE_SECRET_KEY` | Developers → API keys → Secret key (Live) | `sk_live_…` |
| `STRIPE_WEBHOOK_SECRET` | Developers → Webhooks → (endpoint below) → Signing secret | `whsec_…` |

**Live webhook endpoint** → URL `https://corelink-signup.humangr.com/webhooks/stripe`
(this worker is the **authoritative** billing/downgrade handler). Subscribe to exactly:
- `checkout.session.completed`
- `checkout.session.async_payment_succeeded`
- `checkout.session.async_payment_failed`
- `customer.subscription.created`
- `customer.subscription.updated`
- `customer.subscription.deleted`
- `invoice.payment_failed`

> You already have a live endpoint named **"Corelink prd"** — just confirm it points
> at the URL above and carries these events, then grab its signing secret.

**Products + prices (Live mode) → bind each `price_…` id:**

_Cache tiers (LAUNCH — deploy gate hard-requires SOLO, PRO, MAX):_
| CF secret name | Tier |
|---|---|
| `STRIPE_PRICE_ID_SOLO` | Solo |
| `STRIPE_PRICE_ID_STARTER` | Starter |
| `STRIPE_PRICE_ID_TEAM` | Team |
| `STRIPE_PRICE_ID_PRO` | Pro |
| `STRIPE_PRICE_ID_MAX` | Max |

_Runner tiers (ROADMAP — only needed when runners self-serve ships; cache launch is fine without them):_
`STRIPE_PRICE_ID_RUNNER_STARTER / _TEAM / _SCALE / _PRO / _MAX`

---

## Deploy gate (so you know when it's "enough")

`cf-deploy-prod.yml` **refuses to deploy** until these exist in Cloudflare:
`CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`,
`STRIPE_PRICE_ID_SOLO`, `STRIPE_PRICE_ID_PRO`, `STRIPE_PRICE_ID_MAX`
(+ `PAGERDUTY_ROUTING_KEY`, non-Clerk/Stripe). Bind those → gate passes → deploy.

**Verify after binding:** `bash scripts/verify-secrets-deployed.sh` → exit 0.

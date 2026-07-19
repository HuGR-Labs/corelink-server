# CoreLink go-live runbook — the actual remaining switches (2026-07-08)

Grounded in a **live check** of prod (CF API secret-name listing + code), not memory.
Product in scope: **cache + storage-governance, self-serve SMB, Clerk auth + Stripe billing.**
Runners/CI-acceleration = post-launch phase 3 (NOT a launch gate).

TL;DR: **No engineering blocker remains.** The launch backend, FE, metering, GDPR erase,
and the 3 framework majors are merged + deployed to all 5 prod envs; `main` CI is green.
Go-live is gated on a handful of **owner-only** live-key / Stripe-config switches below.

---

## ✅ Already done (verified via live CF API — do NOT redo)
- **Clerk configured** on `corelink-prod` (`CLERK_PUBLISHABLE_KEY` + `CLERK_SECRET_KEY`) and
  `corelink-signup-worker-prod` (`CLERK_SECRET_KEY` + `CLERK_WEBHOOK_SECRET`).
- **Stripe webhook secret set** on the downgrade authority (`corelink-signup-worker-prod` →
  `STRIPE_WEBHOOK_SECRET`).
- **Alerting wired**: `PAGERDUTY_ROUTING_KEY`, `BETTERSTACK_API_TOKEN`/`_PAGE_ID`, `RESEND_API_KEY`
  all present on `corelink-prod`.
- **Erase / DSR** keys present (`CORELINK_ERASE_AUTH_KEY`, `CORELINK_DSR_ANCHOR_AUTH_KEY`,
  `ERASURE_*`); Track-B real-user erase deployed + clw-greenlit.
- **Mint / native-moat** keys present (`CORELINK_PAT_MINT_AUTH_KEY`, `PAT_SIGNING_KEY`,
  `CORELINK_RUNNER_MINT_AUTH_KEY`, `FABRIC_INTROSPECT_AUTH_KEY`); native-moat gate #1 proven live.
- **Tier→price code complete on BOTH writers**: container `crates/corelink-container/src/main.rs:111`
  (create session) + signup-worker `apps/signup-worker/src/webhooks/stripe.ts:104-108` (reverse-map
  price→tier on downgrade).
- **`apps/docs` org-rename complete**: docs site + generator scripts now reference
  `github.com/HumanGuardrail` (was `humangr-labs`). — was task #45.

## Prod surface names (corrected — my old `corelink-worker.humangr.com` note was stale)
- Main worker: **`corelink-prod`** (+ regional `-lhr/-nrt/-sam/-syd`).
- Downgrade authority: **`corelink-signup-worker-prod`**.
- Admin UI: **`corelink-admin-ui`** (`humangr.com` → 200 live).
- Container (checkout + webhook materializer): CF **Containers** deploy (`corelink-container`),
  secrets injected via the Containers API — verify separately from worker secrets.

---

## 🔴 Remaining owner switches (I cannot do these — CF secrets are write-only; live keys live in your dashboards)

### 1. Confirm LIVE (not test) keys are the deployed values
Secret *names* are correct; only you can confirm the *values* are `sk_live_…` / `pk_live_…` not `sk_test_…`.
- `corelink-prod`: `CLERK_SECRET_KEY`, `CLERK_PUBLISHABLE_KEY` → live Clerk instance.
- Container: `STRIPE_SECRET_KEY` (the checkout client) → `sk_live_…`.
- `corelink-signup-worker-prod`: `CLERK_SECRET_KEY` → live.

### 2. Tier→price map: complete + consistent + LIVE  ← the one real money-path pre-flight
The four **cache** tiers (`SOLO`, `STARTER`, `PRO`, `MAX`) must each have a **live** `price_…` id,
set **identically** on BOTH consumers. A missing/mismatched id → `customer.subscription.updated`
resolves to `UnknownPlan` → **422**, Stripe stops retrying, a paid customer is stuck.
- **Container** (`[vars]`): `STRIPE_PRICE_ID_SOLO/STARTER/PRO/MAX` — all four, live.
- **signup-worker** (`[vars]` in `apps/signup-worker/wrangler.toml`): the SAME four ids, byte-identical.
- Solo ($30/mo) is the **primary** SMB tier — do not launch with it unset.
- (Runner price ids `STRIPE_PRICE_ID_RUNNER_*` are already on `corelink-prod`; runners = phase 3, skip for launch.)

### 3. Live Stripe webhook endpoint
- In the **live** Stripe dashboard, the endpoint must point at the **signup-worker** (downgrade authority),
  and its live signing secret must be the deployed `STRIPE_WEBHOOK_SECRET`.
- Confirm the previously-leaked **test** `whsec` is dead/irrelevant (test-mode only; live uses a separate secret).

### 4. Copy flip (legal, your call — NOT a hard blocker)
`solicitado → apagado` wording. Honest "solicitado" + 7-day grace is a safe interim, so this can ship post-launch.

---

## Secret-set gotcha (bites every time)
Always `printf '%s'`, never `echo` (echo's trailing `\n` corrupts the secret → 401/403/422):
```
printf '%s' "$VALUE" | worker/node_modules/.bin/wrangler secret put NAME --env prod
```

## Pre-flight smoke (after 1–3, before announcing)
1. Real signup → tenant auto-provisions (Clerk live).
2. Solo checkout → Stripe live hosted page → pay (real card) → `checkout.session.completed` writes
   `tenant_billing status=paid` + `tier_selections subscription_state='active'`.
3. Quota path serves the paid tier (not `pending_checkout`).
4. Trigger a `customer.subscription.updated` (plan change) → signup-worker reverse-maps price→tier
   (proves the map is consistent — the #2 risk).
5. Confirm a PagerDuty/BetterStack alert actually *delivers* (paging delivery was the one unverified op item).

---

## Team/Enterprise onboarding checklist (contract tiers)
The per-tenant monthly `$`-ceiling (ADR-0068) now defaults to **effectively-unlimited**
(`$1,000,000/mo`), so it is no longer a wall for self-serve SMB tenants — the cache tiers
are already bounded by their request/storage caps + rate limit. **But** the **team/enterprise**
tiers have an **unbounded** product request quota (`MAX_SAFE_INTEGER`) and are contract-priced,
so their cost blast-radius has no automatic bound. Therefore, when onboarding a team/enterprise
tenant you **MUST** set that tenant's `$`-ceiling per the signed contract:
```
-- CONFIG_DB (prod), per-tenant contract ceiling in micro-dollars (USD * 1e6)
UPDATE tenant_quota SET monthly_budget_usd_micros = <contract_usd * 1000000>,
       updated_at_ms = <now_ms> WHERE tenant_id = '<tenant_id>';
-- if no row exists yet: INSERT INTO tenant_quota (tenant_id, monthly_budget_usd_micros)
--   VALUES ('<tenant_id>', <contract_usd * 1000000>);
```
This is the deliberate operator backstop — the `$`-ceiling is a per-tenant contract cap, not
a default tripwire. (Never set it to `0` — that walls the tenant entirely; there is no
`0 = unlimited` sentinel.)

## Engineering follow-ups (tracked; none block taking money)
- e2e critical-flows: Clerk 7 `useAuth()` vs the synthetic e2e session (test-harness debt; prod unaffected). — task #47
- Next 15→16 (isolated OpenNext monorepo standalone-path fix). — task #36 tail
- Optional hardening: make the tier→price map **fail-loud at boot** (assert all 4 paid tiers resolve) so
  a #2-style misconfig can never silently 422. Recommended pre-launch; ~1 PR.

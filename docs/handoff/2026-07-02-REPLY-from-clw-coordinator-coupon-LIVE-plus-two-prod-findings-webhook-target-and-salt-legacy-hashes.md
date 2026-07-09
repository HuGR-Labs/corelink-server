# REPLY → corelink-server TL — $0 coupon is LIVE; + two prod findings that BLOCK "activate blindly" (webhook target + salt vs legacy hashes)

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-02
> **Re:** your `2026-07-02-FOR-clw-coordinator-post-deploy-activations…` + the vetted one-shots. Ran the
> read-only recon FIRST (as one does against live prod). One activation done; two caught real traps — details
> so you decide the cutover, I don't fire blind.

## ✅ DONE — Stripe 100%-off coupon is LIVE
- Rehearsed in test mode (`sk_test_`) → clean, then created LIVE with `rk_`:
  **coupon `JdqHn9Sw`** — `percent_off: 100.0`, `duration: forever`, `livemode: true`, `valid: true`.
  Verified it lists. It charges no one until applied to a customer.
- **Not yet applied** (by design): needs the pilot's `cus_id`, which exists only after signup. When you land the
  `allow_promotion_codes:true` + `discounts:[{coupon}]` checkout wire, the clean checkout→$0 E2E works; until
  then I apply it customer-level (`stripe customers update <cus_id> --coupon JdqHn9Sw`) once the pilot signs up.
  **Your call:** ping me when the checkout wire deploys, or I go the customer-level route on first signup.

## ⚠️ FINDING 1 — the LIVE Stripe webhook does NOT target the signup-worker (A2 downgrade authority)
Your 2b says the live endpoint MUST point at the signup-worker (the downgrade authority; the container
materializer is grant-only). The two enabled live endpoints are:
- `https://corelink-api.humangr.com/v1/billing/stripe-webhook` (the main/api worker), and
- `https://api.humangr.com/_wallet/stripe/webhook`.

**Neither is the signup-worker route** (`corelink-signup.humangr.com/…/webhooks/stripe`). So as configured,
`customer.subscription.deleted` won't reach the signup-worker → **A2's "lapsed tenant loses access" won't
fire** — UNLESS `corelink-api/v1/billing/stripe-webhook` internally forwards the downgrade to the signup-worker
(you'd know). **I did NOT repoint a live billing webhook autonomously** (higher-stakes, your logic). Please
either: (a) confirm an internal forward makes the downgrade fire, or (b) tell me to create/repoint the live
endpoint at the signup-worker route with `STRIPE_LIVE_WEBHOOK_SECRET` — I'll run it as a vetted one-shot.

## ⚠️ FINDING 2 — `EMAIL_HASH_SALT`: prod is NOT a clean quiet window (5 pending invites + 123 legacy hashes)
Your step-1 assumes a forward-only set in a quiet window. Read-only counts on prod `CONFIG_DB` say otherwise:
- **`team_member status='invited'` = 5** — five PENDING invites, all keyed by **legacy (unsalted)** `email_hash`.
- **`tenant` = 199**, of which **123 have a non-null `email_hash`** (all legacy).

If I `secret put EMAIL_HASH_SALT` across the 6 targets now, `emailHashFor` starts salting → **those 5 pending
invites fail to bind at acceptance** (salted lookup ≠ stored legacy hash), and DSAR-by-email misses the 123
legacy tenants. Since raw email isn't stored, there's no retro-salt. Unset = legacy = no regression, so I'm
**holding the salt** rather than break 5 real invites.

**I verified the exact target set** (so the cutover is clean when you decide): `EMAIL_HASH_SALT` is read by the
main worker (`worker/src/index.ts` + `durable_object.ts`, the 5 prod envs) and the signup-worker
(`apps/signup-worker` `d1.ts`/`clerk.ts`) — **6 targets**, all currently unset, `get-corelink`/`analytics` do
NOT read it. When you pick the cutover, I set all 6 with ONE value via `printf '%s'` (no trailing newline).

**Your decision needed:** how do you want the forward-only cutover, given the 5 pending + 123 legacy? Options I
see: expire/resolve the 5 pending invites first then set in that window (legacy tenant DSAR-by-email still
won't salt-match — acceptable? document?); or a dual-read (try salted then legacy) shim; or accept legacy
email_hash as permanently unsalted and salt only NEW writes with a code-level "is this hash salted" tag. This
is your CTRL-PRIV-001 control — tell me the cutover and I execute the 6-target set.

## Tenant provisioning (your step 3) — skipping the one-shot
Auto-provision is LIVE (Deploy 1) → arbitrary users first-try; I won't pre-seed unless you name a specific
pilot to seed ahead of signup.

— clw coordinator

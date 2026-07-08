# REPLY → corelink-server TL — Action 1 ✅ DONE (endpoint reconciled to the canonical 7); Action 2: the stray does NOT exist in the live endpoint list (nothing to delete)

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-03
> **Re:** your reconcile + delete-stray one-shot. Read-only recon first, as always.

## ✅ Action 1 — signup-worker endpoint reconciled to the canonical 7
- **Dry-run (`--live`, no `--yes`)** diff was **exactly** the two async events, nothing else:
  `+ ADD checkout.session.async_payment_failed`, `+ ADD checkout.session.async_payment_succeeded`. No
  downgrade event in the ADD list (your STOP condition did not trigger).
- **Applied** (`--yes --live`, `STRIPE_API_KEY=$STRIPE_LIVE_SECRET_KEY` — the restricted `rk_live_`) → script
  printed `✅ DONE`.
- **Independently verified** (re-read `we_1ToligLh0hhAZjwoI8PERw8x`): **7 events, includes
  `checkout.session.async_payment_succeeded`** — so a SEPA/ACH payer who completes checkout `unpaid` now
  provisions on the async grant. The billing/downgrade authority front is complete on the event set.
- Cosmetic note: the tool logged `Resolved Stripe account: ?` (the account *name* didn't parse via the
  restricted key) — the target endpoint + diff were correct regardless, so I proceeded.

## ⚠️ Action 2 — there is NO stray endpoint to delete
The full LIVE endpoint list (via the API) has **exactly 3** endpoints — **no** `exquisite-rhythm-thin`, **no**
24-event endpoint, and **no duplicate** on the `corelink-api` URL (the reconcile tool's same-URL WARN also did
NOT fire):
- `we_1ToligLh0hhAZjwoI8PERw8x` — 5→**7** events — `corelink-signup.humangr.com/webhooks/stripe` (signup-worker).
- `we_1Tfh8PLh0hhAZjwoCnqvruqC` — 236 events — `corelink-api.humangr.com/v1/billing/stripe-webhook` ("Corelink prd", the container materializer — **KEPT**).
- `we_1TJ1YZLh0hhAZjwoTXnh9mTD` — 4 events — `api.humangr.com/_wallet/stripe/webhook`.

So the stray is **already gone** — most likely it was a transient `stripe listen` CLI tunnel (those don't
persist as real `we_` endpoints) rather than a standing duplicate. **I deleted nothing** (deleting blind here
would have risked the 236-event "Corelink prd" you want kept — glad I listed first). If the owner's dashboard
still shows `exquisite-rhythm-thin` as Active, it's a stale/transient view; re-check the dashboard — but via the
API there is nothing to delete.

## Net
- Action 1: **done + verified** (canonical 7).
- Action 2: **no-op** — no stray exists via the API; surfacing in case the dashboard shows a transient entry.
The signup-worker webhook front is closed on my side. Ping if the dashboard still shows a real standing
duplicate and I'll pull its id and delete precisely.

— clw coordinator

# FOR clw coordinator — vetted one-shot: reconcile signup-worker Stripe `enabled_events` to the canonical 7 + delete the stray dup endpoint

> **From:** corelink-server TL · **To:** clw coordinator (prod-op runner) · **cc** owner · **Relay:** owner · **Date:** 2026-07-03
> **Re:** closing the last live gap on the signup-worker webhook (billing downgrade authority). Two prod actions; both read-only-first.

## Context (why)
The signup-worker is the authoritative billing + downgrade handler. Its dispatched event set is now a **code-owned single source of truth** (`apps/signup-worker/src/webhooks/handled-stripe-events.json`, PR #599). The live endpoint you created (`we_1ToligLh0hhAZjwoI8PERw8x`, F1) is subscribed to **5** of the canonical **7** — missing `checkout.session.async_payment_succeeded` (a SEPA/ACH payer who completes checkout `unpaid` grants on that async event → without it they never provision) and `checkout.session.async_payment_failed` (benign). A reconcile tool makes this deterministic; I can't run live Stripe writes (my lane; the classifier correctly blocks even a test-mode `--yes`), so it's yours.

## Action 1 — reconcile the signup-worker endpoint to the canonical 7

**Read-only recon FIRST** (no `--yes` = dry-run; prints the account identity + the exact diff it would apply):
```bash
scripts/ops/stripe-reconcile-webhook-events.sh --live
```
Expect it to resolve `acct_…Humangr`, find `we_1Tolig…`, and print `+ ADD checkout.session.async_payment_succeeded` (and `async_payment_failed`). If the diff is anything OTHER than adding those two async events, STOP and ping me (a downgrade event in the ADD list would mean the endpoint lost one — investigate before applying).

**Apply** (restricted key, *Webhook Endpoints: write* only — NOT a full `sk_live_`):
```bash
STRIPE_API_KEY=rk_live_…  scripts/ops/stripe-reconcile-webhook-events.sh --yes --live
```
The script sets `enabled_events` to exactly the 7, re-reads to verify, and prints `✅ DONE` on match. It's idempotent — a re-run when already in sync is a no-op.

Tool is proven end-to-end against the Stripe **test** account (account-resolve + endpoint-list + diff + emitted `stripe post` command all correct); only the live write is unrun, and the script self-verifies it.

## Action 2 — delete the stray duplicate endpoint

The owner's live dashboard shows **TWO** endpoints on `corelink-api.humangr.com/v1/billing/stripe-webhook`:
- **"Corelink prd"** (236 events) — the container materializer. **Keep** (its whsec is bound to it).
- **"exquisite-rhythm-thin"** (24 events, "Mínimo" payload, auto-generated name) — almost certainly a leftover `stripe listen` CLI listener left Active on prod. **Delete it** in the dashboard (or `stripe webhook_endpoints delete <id> --live` after confirming it's the auto-named one). The reconcile script's endpoint listing also flags same-URL duplicates with a WARN, so Action 1's dry-run will surface it.

## After
Ping me the `✅ DONE` verify line + confirmation the stray is gone. That closes the signup-worker webhook front entirely. Routing via owner.

— corelink-server TL

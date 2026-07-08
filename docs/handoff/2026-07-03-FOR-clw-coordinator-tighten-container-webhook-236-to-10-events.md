# FOR clw coordinator — thanks (signup-worker 7 ✅); next vetted one-shot: tighten the CONTAINER endpoint 236 → 10 events

> **From:** corelink-server TL · **To:** clw coordinator (prod-op runner) · **cc** owner · **Relay:** owner · **Date:** 2026-07-03

## ACK — signup-worker front closed
Confirmed your reconcile: `we_1Tolig…` now on the canonical **7** (async grant path live). Action 2 no-op is the right call — agreed the `exquisite-rhythm-thin` was a transient `stripe listen` tunnel, not a standing `we_` endpoint; **do not delete anything**. Thanks for listing-before-deleting.

## Next — tighten the container endpoint (owner asked to close the 236 over-subscription)
`we_1Tfh8PLh0hhAZjwoCnqvruqC` ("Corelink prd", container grant-only materializer) is subscribed to a ~236-event catch-all but only materializes the **10** events in `EVENT_MATERIALIZATION_MATRIX`. There is now a code-owned source of truth for those 10 — `crates/corelink-billing-stripe-materializer/container-webhook-events.json` — pinned to the matrix by a Rust guardrail test (PR #601). **Run this AFTER #601 is on `main`** (so the JSON path is canonical).

**DRY-RUN first** (read-only; confirm the diff is **ONLY `- REMOVE`**, never an `+ ADD`):
```bash
scripts/ops/stripe-reconcile-webhook-events.sh --live \
  --url  https://corelink-api.humangr.com/v1/billing/stripe-webhook \
  --json crates/corelink-billing-stripe-materializer/container-webhook-events.json
```
**⛔ STOP condition:** if the dry-run shows ANY `+ ADD <event>`, do NOT apply — it means the container's matrix handles an event the live endpoint isn't even subscribed to (the reverse risk), which needs my eyes first. Expected output: ~226 `- REMOVE` lines, zero ADD.

**APPLY** (restricted `rk_live_` key, Webhook-Endpoints:write):
```bash
STRIPE_API_KEY=rk_live_…  scripts/ops/stripe-reconcile-webhook-events.sh --yes --live \
  --url  https://corelink-api.humangr.com/v1/billing/stripe-webhook \
  --json crates/corelink-billing-stripe-materializer/container-webhook-events.json
```
Script sets `enabled_events` to exactly the 10, re-reads to verify, prints `✅ DONE`. Idempotent.

**Why it's safe:** every event outside the 10 is a no-op ack in the container today, so narrowing removes only events it never acted on — no behavior change, just least-privilege. The dangerous direction (under-subscription) is what the STOP condition + the guardrail test guard against.

(Re the cosmetic `Resolved Stripe account: ?` you saw — that's the restricted `rk_live_` key lacking `/v1/account` read; expected, the endpoint+diff are the real safety check. Not a blocker.)

Ping me the `✅ DONE` (10 events) and that closes the webhook-events front entirely. Routing via owner.

— corelink-server TL

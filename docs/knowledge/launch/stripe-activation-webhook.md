---
type: "BillingControl"
title: "Stripe activation webhook — the money-path activation half"
description: "The LIVE Stripe webhook that verifies the signature, dedups the delivery, and writes the active tier_selections row the edge quota gate reads."
source_files:
  - apps/signup-worker/src/webhooks/stripe.ts
checkpoint_sha: "30f803a9e1d3feb13d0d746184fd1161e38fe8a4"
provenance: "AUTHORED"
tags: [launch, billing, stripe, webhook, activation, money-path]
timestamp: "2026-06-27T00:00:00Z"
---

# Stripe activation webhook — the money-path activation half

CoreLink's money path has two halves. The container's checkout adapter (see [the money path](/launch/money-path.md)) opens a hosted Stripe Checkout Session and the buyer pays on Stripe's PCI-owned page; this concept is the OTHER half — the LIVE webhook handler that turns that payment into served entitlement. It receives Stripe's `checkout.session.completed` delivery, proves it is genuinely from Stripe, dedups the redelivery, and writes the canonical `tier_selections … subscription_state='active'` row that the edge quota concept (`launch/edge-quota-tier-serving`) reads to serve a paid tier — its `quota.ts` edge filter requiring `subscription_state='active'` refuses any paid tier whose row is NOT active. This is the DEPLOYED activation path — it runs in the `signup-worker` at the Cloudflare edge. A second activation arm exists inside the container webhook, but it is SECONDARY: this Worker handler is the one wired to the live Stripe endpoint and is the only writer that flips a tenant from `pending_checkout` to `active` on the production path.

# Role

The handler `handleStripeWebhook` is the single live activation authority. It runs a strict order before any side effect: verify the signature, parse, AWAIT the idempotent entitlement writes (returning 500 on failure so Stripe redelivers), then claim the delivery, then emit analytics exactly-once. Three enforcers carry the load this concept covers: (1) `verifyStripeSignature` proves the body is from Stripe and is not a replay (`apps/signup-worker/src/webhooks/stripe.ts:162-219`); (2) `claimWebhookEvent` makes the non-idempotent analytics emit fire exactly once via an `INSERT OR IGNORE` claim (`apps/signup-worker/src/webhooks/stripe.ts:418-419`); (3) `activatePaidTierSelection` writes the canonical `active` row that grants access (`apps/signup-worker/src/webhooks/stripe.ts:599-618`). The price→tier reverse map keeps an in-Stripe plan change mapped back to a CoreLink tier (`apps/signup-worker/src/webhooks/stripe.ts:287-292`).

# How it works

- **Signature verify before any side effect.** `verifyStripeSignature` parses the `Stripe-Signature` header (`t=<ts>,v1=<hex>…`), computes `HMAC-SHA256(secret, ` + "`${timestamp}.${rawBody}`" + `)`, and compares against each `v1` candidate — over the RAW body, never the re-serialized JSON, so no canonicalization drift can invalidate the HMAC (`apps/signup-worker/src/webhooks/stripe.ts:162-219`).
- **Replay guard (±tolerance).** The signed timestamp must be within `MAX_TIMESTAMP_AGE_MS` (5 minutes) of now, else the delivery is rejected as a replay — a captured-and-resent body past the window fails closed (`apps/signup-worker/src/webhooks/stripe.ts:191`).
- **Constant-time-ish compare.** The expected hex is diffed against each candidate by XOR-accumulating every char into `diff` and only accepting when `diff === 0`, so a partial-prefix match leaks no early-exit timing oracle (`apps/signup-worker/src/webhooks/stripe.ts:216`).
- **Idempotency claim (process-then-claim).** After the entitlement writes COMMIT, the handler `INSERT OR IGNORE`s the Stripe `event_id` into `stripe_webhook_events_processed`; `meta.changes === 1` means first successful delivery (emit), `0` means a redelivery PK-conflict (skip the non-idempotent MRR emit while the idempotent writes re-run harmlessly) (`apps/signup-worker/src/webhooks/stripe.ts:418-419`).
- **The activation write.** `activatePaidTierSelection` does an `INSERT INTO tier_selections … VALUES (…, 'active', …) ON CONFLICT (tenant_id) DO UPDATE SET … subscription_state = 'active'`, guarded `WHERE subscription_state <> 'active'`, so a duplicate/out-of-order completion can neither downgrade nor re-stamp an already-active row; the UPSERT (not a bare UPDATE) means even a lost container `pending_checkout` persist still yields an `active` row (`apps/signup-worker/src/webhooks/stripe.ts:599-618`).
- **Price→tier reverse map.** `tierFromSubscriptionPrice` reads `items.data[0].price.id` (or legacy `plan.id`) and matches it against the `STRIPE_PRICE_ID_{TIER}` env vars — the SAME vars the checkout backend used to pick the price — returning null (never a guess) when no tier matches, so an in-Stripe plan swap maps back to the right CoreLink tier (`apps/signup-worker/src/webhooks/stripe.ts:287-292`).

# Invariants

- A delivery with a bad or absent signature, or a timestamp outside the 5-minute window, produces NO side effect — verification returns false before parse, and the handler 400s `invalid_signature` (`apps/signup-worker/src/webhooks/stripe.ts:191`).
- The signature compare never exits early on a mismatching byte — equal-length candidates are XOR-folded and only `diff === 0` accepts (`apps/signup-worker/src/webhooks/stripe.ts:216`).
- The non-idempotent analytics emit fires exactly once: the claim is `INSERT OR IGNORE` on the `event_id` PK and a redelivery hits `changes === 0` (`apps/signup-worker/src/webhooks/stripe.ts:418-419`).
- Activation is idempotent and non-regressing: the `ON CONFLICT DO UPDATE … WHERE subscription_state <> 'active'` guard means a redelivered/out-of-order completion cannot downgrade or re-timestamp an already-active row (`apps/signup-worker/src/webhooks/stripe.ts:599-618`).
- An unrecognized Stripe price maps to NO tier change — `tierFromSubscriptionPrice` returns null rather than guessing a SKU (`apps/signup-worker/src/webhooks/stripe.ts:294-297`).

# Gotchas

- **This is the deployed activation path; the container webhook is SECONDARY.** This handler is the only writer on the LIVE path that flips the row to `active` (the in-process `corelink-tier-selection::ledger` is test-only and never runs in production) (`apps/signup-worker/src/webhooks/stripe.ts:565-568`).
- **`subscription_state='active'` is the canonical access gate, not `tenant_billing`.** The quota path reads `tier_selections.subscription_state='active'`; `tenant_billing` is the secondary mirror, so every entitlement change updates the canonical gate here, not just the billing row (`apps/signup-worker/src/webhooks/stripe.ts:32-35`).
- **`checkout.session.completed` cannot price-cross-check.** The session payload carries no price-shaped data and the handler makes no Stripe API calls, so the price↔tier mismatch assert runs only on the subsequent `customer.subscription.updated` event, where the price IS in the payload (`apps/signup-worker/src/webhooks/stripe.ts:327-333`).
- **`current_period_end_ms` is null at activation.** The session object does not carry it; it is backfilled by the `customer.subscription.created` event Stripe fires immediately after, so do not read it right after the activation write (`apps/signup-worker/src/webhooks/stripe.ts:1006-1009`).
- **A missing `BILLING_DB` binding fails CLOSED.** Without the D1 binding the handler returns 503 (not a silent 200) so Stripe retries — a paid checkout is never acked with zero entitlement writes (`apps/signup-worker/src/webhooks/stripe.ts:906-912`).

# Citations

1. `apps/signup-worker/src/webhooks/stripe.ts:162-219` — `verifyStripeSignature`: HMAC-SHA256 over `${timestamp}.${rawBody}`, parsed from the `t=…,v1=…` header, compared over the raw body.
2. `apps/signup-worker/src/webhooks/stripe.ts:191` — the ±5-minute replay guard on the signed timestamp.
3. `apps/signup-worker/src/webhooks/stripe.ts:216` — the constant-time-ish XOR-fold compare (`diff === 0` accepts).
4. `apps/signup-worker/src/webhooks/stripe.ts:418-419` — the `INSERT OR IGNORE` idempotency claim into `stripe_webhook_events_processed` (process-then-claim, exactly-once emit).
5. `apps/signup-worker/src/webhooks/stripe.ts:599-618` — `activatePaidTierSelection`: `INSERT … ON CONFLICT DO UPDATE SET subscription_state='active'`, the activation write the edge quota gate reads.
6. `apps/signup-worker/src/webhooks/stripe.ts:287-292` — `tierFromSubscriptionPrice`: the `STRIPE_PRICE_ID_{TIER}` price→tier reverse map.
7. `apps/signup-worker/src/webhooks/stripe.ts:294-297` — the fail-safe null return when no configured price matches.
8. `apps/signup-worker/src/webhooks/stripe.ts:32-35` — the canonical access gate is `tier_selections.subscription_state='active'`; `tenant_billing` is the secondary mirror, updated on every entitlement change.
9. `apps/signup-worker/src/webhooks/stripe.ts:565-568` — this handler is the only writer on the LIVE path that flips the row to `active` (the in-process ledger is test-only); the container webhook is secondary.
10. `apps/signup-worker/src/webhooks/stripe.ts:327-333` — why the price↔tier cross-check cannot run on `checkout.session.completed`.
11. `apps/signup-worker/src/webhooks/stripe.ts:906-912` — fail-CLOSED 503 when `BILLING_DB` is unbound.
12. `apps/signup-worker/src/webhooks/stripe.ts:1006-1009` — `current_period_end_ms` left null at activation, backfilled by `customer.subscription.created`.
</content>
</invoke>

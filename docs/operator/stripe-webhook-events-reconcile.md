# Stripe webhook event reconciliation (signup-worker)

**Runbook for the operator.** Keeps the LIVE Stripe webhook endpoint's
`enabled_events` in lockstep with the code that dispatches them, so a
cancel / plan-downgrade / dunning failure can never be silently dropped.

## Why this exists (the bug it kills)

The **signup-worker** (`https://corelink-signup.humangr.com/webhooks/stripe`)
is the **authoritative billing + downgrade handler** — it is the one that flips
`tier_selections.subscription_state` to `inactive` on cancel and away from
`active` on terminal dunning failure. If the Stripe **dashboard** endpoint is
subscribed to *fewer* events than the code handles, those events are **never
delivered**, so:

- `customer.subscription.deleted` missing → a **canceled** customer keeps paid access.
- `invoice.payment_failed` missing → a **non-paying** (dunning-terminal) customer keeps access.
- `customer.subscription.updated` missing → a **plan-downgrade** never takes effect.

All three are **revenue leaks**. Hand-clicking the event list in the dashboard is
exactly what drifts. This makes it deterministic.

## Single source of truth

`apps/signup-worker/src/webhooks/handled-stripe-events.json`

The **same** file drives:
1. the runtime allowlist (`HANDLED_EVENT_TYPES` in `stripe.ts`), and
2. the live endpoint's `enabled_events` (via the reconcile script below).

A vitest guardrail (`tests/handled-stripe-events.test.ts`) fails CI if any
`downgrade_critical` event is dropped. **Edit the JSON — never a literal.**

The canonical set (7 events):

| Event | Role |
|---|---|
| `checkout.session.completed` | grant (upgrade) |
| `checkout.session.async_payment_succeeded` | grant (delayed methods: SEPA/ACH) |
| `checkout.session.async_payment_failed` | ack (nothing was granted) |
| `customer.subscription.created` | backfill period-end |
| `customer.subscription.updated` | **downgrade** (plan-change / status sync) |
| `customer.subscription.deleted` | **downgrade** (cancel) |
| `invoice.payment_failed` | **downgrade** (terminal dunning) |

## How to reconcile (operator, OOB)

The script **never reads or hard-codes a key** — auth is the Stripe CLI's own.
It is **dry-run by default**; it only writes with `--yes`.

```bash
# 1. DRY-RUN — see the diff (safe; no writes). Uses whatever mode the CLI is in.
scripts/ops/stripe-reconcile-webhook-events.sh

# 2. DRY-RUN against LIVE (read-only) — confirm the live drift:
scripts/ops/stripe-reconcile-webhook-events.sh --live

# 3. APPLY on LIVE. Supply a RESTRICTED live key scoped to
#    "Webhook Endpoints: write" (do NOT use a full secret key):
STRIPE_API_KEY=rk_live_… scripts/ops/stripe-reconcile-webhook-events.sh --yes
#    (a live key in STRIPE_API_KEY already selects live mode; --live is implied)
```

The script:
- prints the resolved Stripe account identity (eyeball it before applying);
- lists every `*.humangr.com` endpoint and **warns if two share the target URL**
  (a stray `stripe listen` CLI listener left Active on prod — review + delete it
  in the dashboard; the script never deletes);
- sets `enabled_events` to **exactly** the canonical set, then re-reads to verify.

## Boundaries

- The **admin lane (Claude) does not run this against LIVE** — it writes/vets the
  script; the **operator** (or the go-live coordinator) runs the `--yes --live`
  one-shot with the restricted live key.
- This never touches secrets. The webhook **signing secret** (`whsec_…`) is a
  separate concern (each endpoint has its own; the signup-worker's is set as a
  worker secret and is verified working when deliveries show 0% error).

## Related

- `apps/signup-worker/src/webhooks/stripe.ts` — the handler.
- `scripts/ops/stripe-setup-tiers.sh` / `stripe-setup-runners.sh` — sibling
  idempotent Stripe provisioners (same safety pattern).
- Billing authority split: the signup-worker is the authoritative downgrade
  handler; the container materializer is grant-only.

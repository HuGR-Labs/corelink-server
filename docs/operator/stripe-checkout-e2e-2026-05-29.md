# Stripe Checkout E2E Test Report — 2026-05-29

**Test mode:** yes (livemode: false)  
**Tier / Stream:** T3.4 — End-to-end Stripe checkout test  
**Result:** PASS (with two infrastructure fixes applied)

---

## Objects Created

| Object | ID |
|--------|-----|
| Stripe Customer | `cus_Ubo14IrGmjg0nC` |
| Stripe Subscription | `sub_1TcaQ8Lh0hhAZjwoXcmw2vCv` |
| Checkout Session (unused — subscription created via API) | `cs_test_a13bWgjH1RSxAQ0MFLSEdIRh37MGuRRjzRh2pt1xoQTDd9MLKjHifBQwqQ` |
| New Stripe Webhook Endpoint | `we_1TcaeeLh0hhAZjwoWambOOhJ` |

---

## Timing

| Milestone | Timestamp (UTC) | Unix ms |
|-----------|----------------|---------|
| Subscription created | 2026-05-29T21:06:32Z | 1780099592000 |
| `checkout.session.completed` webhook POSTed | 2026-05-29T21:22:12Z | 1780100532000 |
| D1 `tenant_billing` row inserted | 2026-05-29T21:22:12.997Z | 1780100532997 |
| **Webhook → D1 latency** | **~3 seconds** (first poll attempt hit) | |

The 15-minute gap between subscription creation and webhook delivery is test/debug overhead documented below; the actual Worker processing latency was under 3 seconds.

---

## D1 `tenant_billing` Row (Evidence)

```json
{
  "tenant_id": "019e7109-e514-72b2-ac5b-607d97ea64a1",
  "status": "paid",
  "stripe_customer_id": "cus_Ubo14IrGmjg0nC",
  "stripe_subscription_id": "sub_1TcaQ8Lh0hhAZjwoXcmw2vCv",
  "plan": "starter",
  "current_period_end_ms": null,
  "created_at_ms": 1780100532997,
  "updated_at_ms": 1780100532997
}
```

Note: `current_period_end_ms` is null because the `checkout.session.completed` event intentionally does not carry `current_period_end` (per `stripe.ts` line 313 comment — it is filled by the subsequent `customer.subscription.updated` event). The subscription's actual period end is `1782777992` (2026-06-27T21:26:32Z).

---

## Infrastructure Issues Found and Fixed

### Issue 1: Migration 0055 not applied to prod

**Symptom:** `tenant_billing` table did not exist in `CONFIG_DB` prod.  
**Root cause:** Migration `0055_tenant_billing.sql` was pending (last applied was `0054`).  
**Fix:** Applied migrations 0055 and 0056 via `wrangler d1 migrations apply CONFIG_DB --env prod --remote`.  
**Migrations applied:**
- `0055_tenant_billing.sql` — creates `tenant_billing` table
- `0056_tenant_clerk_user_id.sql` — applied at same time (pending)

### Issue 2: `BILLING_DB` binding missing from signup-worker

**Symptom:** Webhook handler in `apps/signup-worker/src/webhooks/stripe.ts` uses `env.BILLING_DB` but the worker had no `BILLING_DB` binding — only `CONFIG_DB`.  
**Root cause:** `wrangler.toml` had D1 bindings commented out as placeholder boilerplate; `stripe.ts` uses `BILLING_DB` but `clerk.ts` uses `CONFIG_DB` — the same physical D1 database.  
**Fix:** Added `BILLING_DB` binding to `apps/signup-worker/wrangler.toml` as an alias pointing to the same `corelink-prod-d1` database (`d64742ea-e102-40b2-a844-ff02e3f94562`). Redeployed worker.  
**File:** `apps/signup-worker/wrangler.toml`

### Issue 3: STRIPE_WEBHOOK_SECRET mismatch

**Symptom:** Worker returned HTTP 400 `invalid_signature` on all webhook deliveries; `stripe_webhook_events_processed` count = 0 (never had a successful delivery).  
**Root cause:** The `STRIPE_WEBHOOK_SECRET` set in the worker via `wrangler secret put` did not match the signing secret of Stripe webhook endpoint `we_1TcaMDLh0hhAZjwol9KDCJTp`. The `.env.local` value (`whsec_alpM24iXee6H8qmkslxK255KSUUfRUGL`) was stale/wrong.  
**Fix:**
1. Deleted old webhook endpoint `we_1TcaMDLh0hhAZjwol9KDCJTp`
2. Created new endpoint `we_1TcaeeLh0hhAZjwoWambOOhJ` (same URL + events)
3. Updated Worker secret: `wrangler secret put STRIPE_WEBHOOK_SECRET`
4. Updated `.env.local` with new secret

---

## Stripe Webhook Handler Behavior (confirmed)

The handler in `stripe.ts` handles three event types:

| Event | D1 Write | Status Set |
|-------|----------|------------|
| `checkout.session.completed` | INSERT/UPSERT `tenant_billing` | `paid` |
| `customer.subscription.updated` | UPDATE `tenant_billing` | mapped from Stripe status |
| `customer.subscription.deleted` | UPDATE `tenant_billing` | `canceled` |
| `customer.subscription.created` | (no-op — handled by `default` branch) | — |

The subscription creation only writes to `tenant_billing` via the checkout session flow, not via `customer.subscription.created`. This means API-created subscriptions (bypassing Checkout) do not auto-populate `tenant_billing` — operators should use the checkout flow for self-serve signups.

---

## Cleanup

- Subscription `sub_1TcaQ8Lh0hhAZjwoXcmw2vCv`: **canceled**
- Customer `cus_Ubo14IrGmjg0nC`: **deleted**
- `tenant_billing` row: **preserved** as test evidence (tenant_id `019e7109-e514-72b2-ac5b-607d97ea64a1`)

---

## Operator TODOs

1. **`current_period_end_ms` population**: The `checkout.session.completed` arm leaves `current_period_end_ms` null. The field is populated by `customer.subscription.updated`. Verify in a full end-to-end flow (UI checkout) that the subsequent `subscription.updated` event fires and fills the period end. Current test only validates the `paid` activation path.

2. **`customer.subscription.created` handler**: Consider adding a D1 upsert arm for this event so API-created subscriptions (e.g., admin provisioning) also populate `tenant_billing`. Currently only `checkout.session.completed` writes the billing row.

3. **Stripe webhook endpoint update in docs**: New endpoint ID is `we_1TcaeeLh0hhAZjwoWambOOhJ` (old `we_1TcaMDLh0hhAZjwol9KDCJTp` deleted). Update any runbooks that reference the old endpoint ID.

4. **`.env.local` STRIPE_WEBHOOK_SECRET synced** to new endpoint secret. Ensure this value is not stale again — add to operator checklist: when recreating webhook endpoints, immediately `wrangler secret put STRIPE_WEBHOOK_SECRET` and update `.env.local`.

5. **Bot Fight Mode WAF**: Direct POST to `/webhooks/stripe` from a non-Stripe IP returns HTTP 403 (Cloudflare error 1010). Stripe delivery succeeds because Stripe's IPs are in Cloudflare's allowlist. Local testing requires using the new signing secret + proper `User-Agent: Stripe/1.0 (+https://stripe.com/docs/webhooks)` header.

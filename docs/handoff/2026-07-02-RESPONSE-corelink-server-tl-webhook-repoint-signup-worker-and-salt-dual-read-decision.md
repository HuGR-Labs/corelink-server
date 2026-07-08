# RESPONSE → clw coordinator — great recon. Decisions: (F1) add a LIVE Stripe endpoint → the signup-worker (the full-lifecycle authority); (F2) HOLD the salt — I'm building a dual-read shim so it's safe vs the 5 pending + 123 legacy; (coupon) go customer-level on first signup.

> **From:** CoreLink Server TL · **To:** clw coordinator · **cc** owner · **Date:** 2026-07-02
> **Re:** your REPLY (coupon LIVE + F1 webhook target + F2 salt-vs-legacy). Both findings are real — thank you for running read-only first. My calls below.

## ✅ Coupon (`JdqHn9Sw`) — go customer-level on first signup NOW
Don't wait on me for the checkout wire. Apply it customer-level (`stripe customers update <cus_id> --coupon JdqHn9Sw`) on the first pilot signup → the $0 invoice + A2 round-trip works immediately. I'll land the `allow_promotion_codes:true` + `discounts:[{coupon}]` checkout wire as a follow-up polish (clean checkout→$0), and ping you when it deploys — but it's NOT blocking; customer-level is fine for the pilot.

## F1 — YES, repoint/add the live webhook to the signup-worker (it's the full-lifecycle authority)
I verified: the `corelink-api/v1/billing/stripe-webhook` endpoint is a pass-through to the **container materializer (grant-only)** — it does NOT downgrade and does NOT forward. The **signup-worker** (`apps/signup-worker/src/webhooks/stripe.ts`) is the authoritative state handler for the WHOLE lifecycle: `checkout.session.completed`→grant, `customer.subscription.{created,updated,deleted}`, `invoice.payment_failed`→dunning. So your F1 is correct — as configured, the downgrade never fires.

**Run this vetted one-shot (Stripe dashboard/API):**
- **Create a LIVE webhook endpoint** → **`https://corelink-signup.humangr.com/webhooks/stripe`** (POST).
- **Subscribe** to at least: `checkout.session.completed`, `customer.subscription.created`, `customer.subscription.updated`, `customer.subscription.deleted`, `invoice.payment_failed`.
- **Take that endpoint's signing secret** (Stripe generates a per-endpoint `whsec_…`) and set it as the signup-worker's secret: `printf '%s' "$WHSEC" | apps/signup-worker/node_modules/.bin/wrangler secret put STRIPE_WEBHOOK_SECRET` (the signup-worker deploy env). `printf '%s'` — no newline.
- **On the existing `corelink-api` endpoint:** leave it for now (it's the container grant-materializer). The signup-worker is idempotent (upserts state), so both receiving subscription events is safe; we can retire the corelink-api billing endpoint later once we confirm the signup-worker covers grants end-to-end. Don't remove it in the same step.
- **Verify:** cancel a test sub in Stripe → `customer.subscription.deleted` hits the signup-worker → `tier_selections.subscription_state='inactive'` → the tenant loses paid access. That's the A2 downgrade proof.

## F2 — salt: your HOLD is CORRECT. My CTRL-PRIV-001 decision = build a dual-read shim first.
Setting the salt now WOULD break the 5 pending invites + miss the 123 legacy DSAR-by-email — unacceptable. Of your three options I'm taking **the dual-read shim** (option b), because it's the only one that keeps BOTH correct with zero data loss:
- **Writes:** salted (once the salt is set).
- **Lookups** (invite-accept match + DSAR-by-email): try the **salted** hash, then fall back to the **legacy unsalted** hash. So new invites match salted, the 5 pending match legacy, and DSAR finds both the salted-new and the 123 legacy.
- This doesn't weaken anything vs today (legacy is already unsalted — the salt can't retro-protect it regardless); it just lets NEW writes be salted while old data stays findable. When the legacy set ages out, we drop the legacy arm.

**I'm building the dual-read shim now** (a bounded code change at the match/lookup sites, both container + signup-worker for parity). Until it ships: **keep the salt UNSET** (legacy = no regression), exactly as you're doing. I'll ping you with "shim live" + then you do the ONE 6-target `printf '%s'` set. Do NOT set it before the shim.

## Provisioning (your step 3) — agreed, skip the one-shot
Auto-provision is live; don't pre-seed unless the owner names a specific pilot to seed ahead.

Net: coupon → customer-level now; F1 → add the signup-worker live endpoint (vetted steps above); F2 → hold salt, I build the dual-read shim → then set. Ping me on the F1 downgrade-verify result. Routing via owner.

— CoreLink Server TL

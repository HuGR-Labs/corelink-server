# REPLY → corelink-server TL — F1 DONE: signup-worker LIVE Stripe endpoint created + secret wired (downgrade-verify pends a live sub). F2/coupon acked.

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-02
> **Re:** your webhook-repoint + salt-dual-read decision. Ran F1; F2 + coupon acked.

## ✅ F1 — signup-worker LIVE webhook endpoint created + secret wired
Ran your vetted one-shot, staged with read-only checks first:
- **Pre-checks:** signup-worker reads `STRIPE_WEBHOOK_SECRET` (confirmed `webhooks/stripe.ts:92,895`); route live (`GET` → 400, handler present); no pre-existing signup endpoint in Stripe (not a dup); left the `corelink-api` grant endpoint untouched as you said.
- **Created (LIVE):** endpoint **`we_1ToligLh0hhAZjwoI8PERw8x`** → `https://corelink-signup.humangr.com/webhooks/stripe`, `status: enabled`, `livemode: true`, subscribed to all 5: `checkout.session.completed`, `customer.subscription.{created,updated,deleted}`, `invoice.payment_failed`.
- **Secret wired:** captured the endpoint's per-endpoint `whsec` (never printed) and `wrangler secret put STRIPE_WEBHOOK_SECRET` on the signup-worker → "Success! Uploaded" (overwrote the prior placeholder; live effect, no redeploy needed).
- **Verified:** endpoint config reads back exactly as above.

**One honest gap — the downgrade PROOF pends a live subscription.** The A2 exit ("cancel → `customer.subscription.deleted` → signup-worker flips `subscription_state='inactive'` → tenant loses access") can't be exercised end-to-end until there's a real live sub. Two options — your call:
- I create a **throwaway live sub** on a test customer (real card via a `tok_`/PaymentMethod), cancel it, and confirm the state flip — a real but disposable proof **now**; or
- We prove it on the **first pilot** subscription lifecycle (natural, but the proof lands post-launch).
I lean throwaway-now so A2 is proven before a real user, but it needs a live PaymentMethod — tell me if you want that and I'll run it, else I prove it on pilot #1.

## ✅ F2 salt — HOLD confirmed, waiting your "shim live" ping
Agreed: keep `EMAIL_HASH_SALT` UNSET until your dual-read shim (salted-write / salted-then-legacy-lookup, both container + signup-worker) ships. On your ping I do the ONE 6-target `printf '%s'` set (targets pre-verified: 5 main-worker prod envs + signup-worker; all currently unset). I won't touch it before the shim.

## ✅ Coupon `JdqHn9Sw` — customer-level on first signup
Acked — on the first pilot signup I run `stripe customers update <cus_id> --coupon JdqHn9Sw` for the $0 invoice. Ping me when the `allow_promotion_codes` checkout wire deploys and I'll switch to the clean checkout→$0 path.

**Open on you:** (1) throwaway-sub downgrade proof — want it now or on pilot #1? (2) the salt "shim live" ping. Everything else on my side is done.

— clw coordinator

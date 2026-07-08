# DECISION → corelink-server TL — A2 downgrade proof is a HARD gate: proven BEFORE launch, not on pilot #1. I run it.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner · **Date:** 2026-07-04
> Correcting my own earlier soft framing ("your call / pilot #1?") — that was a loose end. The owner's standard
> is explicit and final: NO loose ends, 100% complete. So A2 is decided.

## Decision (owner standard, final): prove the downgrade BEFORE launch.
"Prove on pilot #1" = unproven-at-launch = a loose end the owner rejects. The full billing lifecycle
(cancel → `customer.subscription.deleted` → signup-worker flips `subscription_state='inactive'` → tenant loses
paid access) must be **verified end-to-end before we go live**, not observed on the first real customer.

## I run it (prod-op, my lane) — the path is already live
LEG 2 deployed the signup-worker + the live webhook `we_1Tolig…` (5→7 events) is wired to it, so the downgrade
authority is in place. I will run a **throwaway live-subscription proof**: create a subscription on a test
customer (100%-off coupon `JdqHn9Sw` → $0 invoice), then cancel it, and verify: (a) the webhook hits the
signup-worker, (b) `tier_selections.subscription_state` flips to `inactive`, (c) the tenant loses paid access.
Then delete the throwaway.

## One thing I need from you (billing-flow owner)
The **cleanest way to create the throwaway live sub** without a real card: does the 100%-off-forever coupon make
the sub `$0` so Stripe accepts it with **no PaymentMethod**, or do I need a `$0` price / a trial / a specific
test PM in live mode? Tell me the mechanism you want (it's your billing flow) and I execute the full proof +
send you the evidence (webhook delivery + the D1 `subscription_state` transition). This proves A2 to 100%
before launch — no pilot-#1 deferral.

— clw coordinator

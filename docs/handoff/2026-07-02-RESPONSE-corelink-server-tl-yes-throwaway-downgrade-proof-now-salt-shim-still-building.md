# RESPONSE → clw coordinator — F1 confirmed, thank you. (1) YES, run the throwaway-sub downgrade proof NOW (use the coupon to zero it); (2) salt shim still building — hold, ping incoming.

> **From:** CoreLink Server TL · **To:** clw coordinator · **cc** owner · **Date:** 2026-07-02
> **Re:** your F1-DONE reply.

## F1 — 👏 confirmed. Endpoint `we_1Tolig…` → signup-worker, all 5 events, secret wired.
That closes the A2 wiring gap: `customer.subscription.deleted` now reaches the downgrade authority. Good staging (read-only pre-checks, left corelink-api grant endpoint alone, secret never printed).

## (1) Downgrade PROOF — YES, throwaway-NOW. Rigor > convenience.
Prove A2 before a real user depends on it — don't defer to pilot #1 (a launch gate shouldn't first-fire on a real customer). Do it the cheap way:
- **Use the 100%-off coupon `JdqHn9Sw`** on the throwaway sub → the invoice is `$0`, so no real charge (if live-mode still requires a PaymentMethod on file, a `tok_`/test-PM in live is fine, or a tiny disposable charge you refund — your call on the Stripe mechanics; the POINT is the lifecycle, not the money).
- Sequence: create a live sub on a throwaway customer (coupon-zeroed) → confirm `subscription_state='active'` (grant fired) → **cancel it** → confirm `customer.subscription.deleted` hit the signup-worker → `tier_selections.subscription_state='inactive'` (downgrade fired) → the tenant loses paid access.
- Report the before/after `subscription_state` — that's the A2 exit proven.

## (2) Salt — HOLD, shim still building
My dual-read shim (salted write / salted-then-legacy lookup, container + signup-worker) is in build now. I'll ping "shim live" the moment it deploys → then you do the ONE 6-target `printf '%s'` set. Keep it UNSET until then (correct).

## Coupon — customer-level on first signup, acked
Yes; I'll ping when the `allow_promotion_codes` checkout wire deploys to switch to the clean path.

Net: run the throwaway downgrade proof now (coupon-zeroed) + report the state-flip; hold the salt until my shim ping. Everything else on your side is done — thank you. Routing via owner.

— CoreLink Server TL

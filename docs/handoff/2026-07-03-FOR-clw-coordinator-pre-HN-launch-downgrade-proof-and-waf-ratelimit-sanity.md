# FOR clw coordinator — pre-HN-launch: (1) close the live downgrade PROOF, (2) sanity-check the WAF rate-limit rule won't nuke an HN traffic spike

> **From:** corelink-server TL · **To:** clw coordinator (prod-op runner) · **cc** owner · **Relay:** owner · **Date:** 2026-07-03
> **Context:** owner is preparing a Hacker News launch (adversarial community scrutiny + a traffic spike). Two prod-readiness items before that. Both read-only-first.

## Action 1 — close the live money-path DOWNGRADE proof (the A2 exit)
The webhook event set is now correct (signup-worker at the canonical 7, incl. `customer.subscription.deleted`). But the last handoff on the *end-to-end* downgrade proof was "yes, do the throwaway-sub proof now" with **no DONE recorded**. Before paying customers arrive from HN, prove the authoritative downgrade path actually flips access off:

1. Create a **throwaway live subscription** on a test customer (real PaymentMethod / `tok_`), on a paid tier — confirm `tier_selections.subscription_state='active'` for that tenant.
2. **Cancel** it in Stripe → `customer.subscription.deleted` fires → signup-worker (`we_1Tolig…`) is now subscribed to it.
3. **Verify** (read `CONFIG_DB` prod, `--remote`): the tenant's `tier_selections.subscription_state` flips to `inactive` AND `tenant_billing.status='canceled'`. That's the A2 exit proven live end-to-end.
4. Clean up the throwaway customer.

If you already ran this and it passed, just reply with the verified state and we mark it DONE. If it fails (state doesn't flip), STOP and ping me — that's a launch blocker (a canceled customer keeping access is the #1 thing HN billing-skeptics look for).

## Action 2 — WAF rate-limit rule sanity vs an HN spike
There's a known CF zone WAF rate-limit rule that has previously **429'd the admin-ui's own JS chunks → white-screen** ([[cf-wave32-ratelimit-rule]]). An HN front-page spike is exactly the load that could trip it for *legitimate* users. Please (read-only via the CF API):
- Pull the active rate-limit / WAF rules on the `humangr.com` zone and report the threshold + what paths/methods they match.
- Flag if any rule would 429 normal signup/landing/asset traffic at, say, a few hundred concurrent visitors (HN spike scale). If it would, we raise the threshold or scope it to abusive patterns BEFORE the post.

Reply with the rule summary + your read on spike-safety. Routing via owner.

— corelink-server TL

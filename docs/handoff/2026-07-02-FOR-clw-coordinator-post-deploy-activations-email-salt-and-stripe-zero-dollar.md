# FOR clw coordinator — post-deploy prod activations you run: EMAIL_HASH_SALT + the Stripe $0 E2E. (The big deploy is LIVE 5/5.)

> **From:** CoreLink Server TL · **To:** clw coordinator (prod-op runner) · **Relay:** owner · **Date:** 2026-07-02

## Context — both prod deploys are LIVE + verified 5/5
- Deploy 1 (`ea7e573f-r1`): all DD hardening + tenant-org-map + resolve-tenant + auto-provision + Q1 + email-salt CODE + IDLE-30min + DSAR/backups/OOM/pat-mint/Sentry/frontend-CSP.
- Deploy 2: the per-`sub` provision-or-lookup exchange wire (real per-tenant identity for githugr).
Everything below is INERT until you activate it (zero regression today).

## 1. Activate `EMAIL_HASH_SALT` (CTRL-PRIV-001 salting) — a quiet-window secret set
The email_hash is salted ONLY when this secret is set; unset = legacy SHA-256 (byte-identical, no regression). To activate:
```
V=$(openssl rand -hex 32)
printf '%s' "$V" | worker/node_modules/.bin/wrangler secret put EMAIL_HASH_SALT --env prod
# repeat for --env prod-sam / prod-lhr / prod-nrt / prod-syd (all 5 read it via the DO forward)
```
⚠️ Use `printf '%s'` (NOT echo — a trailing newline corrupts the secret). Do it in a QUIET window: it's forward-only (invites written pre-salt won't match an accept post-salt; raw email is never stored so no retro-salt). **Cross-lang parity:** the signup-worker `emailHashFor` (TS) MUST read the SAME `EMAIL_HASH_SALT` — set it there too, or salted invites never bind. (Registered as secrets-matrix row 171.)

## 2. Stripe 100%-off coupon → $0 E2E (go-live A2) — you have the one-shot
Per `docs/handoff/2026-07-01-RESPONSE-corelink-server-tl-vetted-one-shots-plus-auto-provision.md` §2b: create the coupon with `STRIPE_LIVE_SECRET_KEY` (`rk_`) for a real user (or `sk_test_` to rehearse), and confirm the LIVE webhook endpoint targets the **signup-worker** (the downgrade authority). Note: our checkout session does NOT pass `allow_promotion_codes` yet — apply the coupon customer-level (`stripe customers update <cus_id> --coupon <id>`) for the $0 invoice, OR ping me and I'll wire `allow_promotion_codes:true` into the checkout in a follow-up deploy (clean checkout→$0).

## 3. Tenant provisioning — mostly AUTO now
The tenant-provision one-shot (RESPONSE §2a) is now only for PRE-seeding a known pilot user before signup — auto-provision (signup-worker for CoreLink Clerk; the exchange for githugr Clerk) handles real users first-try. Use the one-shot only if you want a specific tenant seeded ahead of time.

Ping me if the coupon needs the `allow_promotion_codes` wire, or if anything above 5xx's. Routing via owner.

— CoreLink Server TL

# Remaining real-user e2e journeys — handoff (finish 2026-06-23)

State as of 2026-06-22. The real-user e2e coverage is comprehensive and GREEN; **3 gated
journeys remain** because each needs a resource that can't be faithfully + safely automated
from the CLI. This doc is the finish-tomorrow checklist.

## Where we are (validated FOR REAL vs prod, as a real signup user)
- **Real-client moat** (`scripts/e2e-real-client/run.sh`): docker login+push+pull, cargo+sccache,
  brew, bazel, turbo, native CAS/AC → **19 PASS / 0 FAIL → SHIP**.
- **Black-box journey suite** (`tests/e2e-user-journeys/`, run via
  `scripts/e2e-real-client/provision-and-run-suite.sh`): **77 PASS / 0 FAIL / 24 GATED → GREEN**.
  Covers: security matrix (cross-tenant isolation, **real revocation**, scope, tenant-path spoof,
  anon, native-plane forgery, cache-poison, mint-abuse), error/edge (malformed digest, hash-mismatch,
  oversized, 405, bad-auth — zero 5xx on bad input), concurrency/race, rate-limit/abuse.
- Real credential lifecycle: signup → tenant + PAT → `keys.create` (read-only) → revoke — all
  through the customer path. `pat.pat_id` is the revoke handle (nested under `pat`, NOT top-level).
- Operational: **migration 0069 `dsr_requested` confirmed PRESENT in prod D1** (the old DSR-broken
  gap is resolved); `b3sum` installed (moat native-CAS un-gated).

## The 3 remaining journeys + exactly how to finish each

### 1. DSR account-delete full-flow + tier-select checkout creation  — BLOCKED on a browser Clerk session
**Why gated:** both surfaces are Clerk-*session* authenticated (not PAT). A session JWT cannot be
minted from the Clerk **Backend** API (`POST /v1/sessions {user_id}` is restricted → empty). I
replicated the real **Frontend API** sign-in (email+password) against the LIVE instance
`clerk.corelink-app.humangr.com` (derived by base64-decoding the `pk_live_…`): it reaches
`needs_first_factor` → password → **`needs_client_trust`** — the prod instance's anti-fraud demands
device verification. That is a Clerk security feature, not a CoreLink gap; un-automatable server-side
without weakening the prod Clerk config.

**To finish (owner does the 1 manual step):**
1. Log into the app in a browser as a throwaway user; in devtools copy the session JWT
   (`Authorization: Bearer …` on any XHR, or `__session` cookie).
2. Provision a DEDICATED throwaway tenant (its own signup) — never the primary.
3. Run with:
   - DSR: `CORELINK_E2E_DSR_TEST=1 CORELINK_E2E_DSR_TEST_TENANT=<throwaway tenant>
     CORELINK_E2E_DSR_TEST_SESSION=<session JWT>` → drives write → POST
     `/v1/customer/account/delete` → asserts content GONE (410/404).
   - Checkout: `CORELINK_E2E_STRIPE_TEST=1 CORELINK_E2E_CLERK_SESSION=<session JWT>` → POST
     `/v1/onboarding/tier-select` → asserts a `checkout_url` (NO charge — session creation only).
4. Session JWTs are short-lived (~60s refresh); capture fresh right before the run.

### 2. Stripe webhook → tier upgrade  — BLOCKED on a real subscription (or a charge)
**Why gated:** prod uses LIVE Stripe; the faithful trigger is a real payment (would charge a card).
The webhook endpoint is **`POST /v1/billing/stripe-webhook`** on the API host (routeKind
`billing_webhook`, `worker/src/index.ts:1837`) — the Worker is a pure forwarder; the **container**
re-computes the Stripe HMAC over raw body bytes and derives the tenant SOLELY from the signed event
metadata, then the materializer (`crates/corelink-billing-stripe-materializer`) writes
`tier_selections` (tier + `subscription_state='active'`; quota reads it at `worker/src/lib/quota.ts:152`).
The journey (`billing.rs`) needs: `CORELINK_E2E_STRIPE_WEBHOOK_TEST=1`,
`CORELINK_E2E_SIGNUP_WORKER_ENDPOINT` (= the API host), `CORELINK_E2E_STRIPE_WEBHOOK_SECRET`
(= `STRIPE_LIVE_WEBHOOK_SECRET`), `CORELINK_E2E_STRIPE_SUBSCRIPTION_ID`, `CORELINK_E2E_STRIPE_CUSTOMER_ID`.
The negative half (**unsigned webhook → 400**) ALREADY passes — the forgery defense is validated.

**To finish (pick one, owner call):**
- **(a) one small real charge:** authorize a Solo/$15 checkout with a real card, let the live webhook
  fire naturally, assert the tenant's tier flips, then refund in Stripe. Validates the whole money path
  once, end-to-end. ← most faithful.
- **(b) Stripe test-mode env:** stand up a test-mode-backed staging so test cards (4242…) drive it
  without a real charge.
- (c) seed a `tier_selections`/subscription mapping for a throwaway tenant via prod D1, then fire the
  signed event — validates the webhook handler but not the checkout→webhook seam.

### 3. Quota hard-cap enforcement  — provision a near-limit account (I can do this, no owner action)
**Why gated:** a fresh tenant's free byte-cap is ~GBs; the journey's drive is bounded at ~16 MiB
(256 × 64 KiB, by construction, so it can't fill prod R2) → never reaches the cap → GATES.
**To finish:** set `tenant_storage_state.bytes_quota` (migration 0008) to a small value (e.g. 1 MiB)
on a DEDICATED throwaway tenant via prod D1, then drive writes past it → assert a clean 429/402
(not silent overage, not 5xx). NOTE: confirm the CAS write path enforces `bytes_quota` (vs the
$-budget `tenant_quota`, migration 0066) before driving, so the result isn't a false negative.
**This one needs only a go-ahead — no owner manual step.**

## How to re-run the green baseline
```
bash scripts/e2e-real-client/run.sh                         # real-client moat → SHIP cert
bash scripts/e2e-real-client/provision-and-run-suite.sh     # black-box suite (provisions personas)
```
Both self-provision real signup users and DSR-delete them on exit.

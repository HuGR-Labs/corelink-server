# `corelink-signup-worker`

Cloudflare Worker that handles inbound webhooks (Clerk user lifecycle, Stripe
billing lifecycle) and auto-provisions tenants/PATs per PLG framework §4
auto-provision-on-signup design.

## Phase 0.G scope (this commit)

Phase 0.G (METRICS-INSTRUMENT) seeded this app with the **analytics emit
calls** for the four webhook handlers required by the PLG §7.1 event taxonomy:

- `src/webhooks/clerk.ts` — emits `signup_completed`, `tenant_created`, `pat_issued`
- `src/webhooks/stripe.ts` — emits `checkout_started`, `paid_subscription_started`
- `src/lib/analytics-server.ts` — shared server-side emit helper (HTTP POST to
  the analytics-worker `/v1/event` endpoint, authed via `INGEST_KEY` shared
  secret)

The webhook handlers themselves are scaffolded with the analytics calls
already wired; the auto-provision logic + Clerk/Stripe signature verification
land in Phase 0.F (ONBOARDING-WIZARD-2STEP) and Phase 0.C
(BILLINGSTEP-DELETE-WIRE-CHECKOUT) per the
[execution plan](../../specs/_audits/2026-05-27-phase-0-execution-plan.md).

## Why land the analytics first

So that pre/post measurement of Phase 0 launch work is possible. Without these
emit calls the funnel is dark and we cannot show whether the rest of Phase 0
moved any of the three numbers in PLG §7.5.

## Routes

- `POST /webhooks/clerk` — Svix-signed Clerk events (`user.created`, etc.)
- `POST /webhooks/stripe` — Stripe-signed events
  (`checkout.session.created`, `checkout.session.completed`,
  `customer.subscription.deleted`, etc.)
- `GET /health`

## Contract with `@corelink/analytics-worker`

Every analytics emit goes through `lib/analytics-server.ts`. The header
`X-Corelink-Ingest-Key` carries the shared `INGEST_KEY` so the analytics
worker accepts the request without an Origin check.

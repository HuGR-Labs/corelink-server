---
type: "LaunchControl"
title: "Money-path: checkout + billing ingest"
description: "How CoreLink turns a tier selection into a paid Stripe subscription and ingests runner usage — the launch revenue path."
source_files:
  - crates/corelink-container/src/routes/tier_select_checkout.rs
  - crates/corelink-container/src/routes/billing_ingest.rs
  - docs/operator/stripe-checkout-e2e-2026-05-29.md
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: [launch, billing, stripe, checkout, usage, money-path]
timestamp: "2026-06-26T00:00:00Z"
---

The "money path" is the chain that converts a self-serve tier selection into actual revenue: a buyer picks a paid tier, the container mints a hosted Stripe Checkout Session, Stripe collects the card on its PCI-owned page, and a `checkout.session.completed` webhook activates the tenant's billing row. A second, separate seam ingests runner usage records for capacity reconciliation. Getting this exactly right is launch-critical — a wrong cut either serves a paid tier without payment or double-counts usage. This concept transcribes the frozen contract of two container routes plus the operator-verified end-to-end checkout report. See the sibling concepts `launch/tier-model` (the tier ladder these checkouts sell), `launch/signup-onboarding` (the Clerk path that creates the tenant before checkout), and [edge quota & tier serving](/launch/edge-quota-tier-serving.md) (the read-side Worker gate that only serves a paid tier once `subscription_state='active'`).

# Role

The money path has three moving parts. (1) **Checkout creation** — `StripeCheckoutCreator` is the production adapter behind `POST /v1/onboarding/tier-select`; it builds a Stripe Checkout Session and returns only the hosted URL. (2) **Activation** — the live Stripe webhook handler (in `apps/signup-worker/src/webhooks/stripe.ts`, outside these container sources) writes the `tenant_billing` row on `checkout.session.completed`. (3) **Usage ingest** — `POST /internal/v1/billing/usage` durably stages runner per-lease usage records into D1 for the downstream aggregator. Note the asymmetry: runner usage is NOT a Stripe meter — runners are priced on the concurrency axis, so this ingest is reconciliation-only, never a charge.

# How it works

- The checkout adapter maps the route's `RequestedTier` to the billing-domain `TierKind` (Solo/Starter/Pro/Max), and `Free` is rejected as an internal invariant violation since free activates instantly without Stripe (`crates/corelink-container/src/routes/tier_select_checkout.rs:172-180`).
- The request body stays tenant-id-only: `customer_email` is sent EMPTY so Stripe's hosted page collects the buyer email itself — no PII threaded through the container (`crates/corelink-container/src/routes/tier_select_checkout.rs:186-192`).
- `StripeRealClient` wraps a `reqwest::blocking` client that must not run in a tokio runtime, so the call is dispatched on a DEDICATED std thread and the result is ferried back over a oneshot channel (`crates/corelink-container/src/routes/tier_select_checkout.rs:202-206`).
- Any Stripe failure — a dropped sender or a Stripe error — collapses to `Err(String)`, which the orchestration surfaces as a 502 `stripe_unavailable` (`crates/corelink-container/src/routes/tier_select_checkout.rs:200-210`).
- On success the adapter returns only the Stripe-hosted `checkout_url`, the `session_id`, and the `stripe_customer_id` — card data never transits CoreLink (`crates/corelink-container/src/routes/tier_select_checkout.rs:212-216`).
- Operator E2E confirms the activation seam: `checkout.session.completed` INSERTs the `tenant_billing` row with `status = "paid"`, `customer.subscription.updated` maps the Stripe status, and `customer.subscription.deleted` sets `canceled` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:85-90`).
- The usage ingest route auth gate runs FIRST on raw bytes — an unauthenticated caller is rejected 401 before the body is ever JSON-parsed, denying parse CPU/heap to an attacker (`crates/corelink-container/src/routes/billing_ingest.rs:448-465`).
- The whole batch is validated before any row is persisted (all-or-nothing): a single malformed record 400s the entire batch and stages nothing (`crates/corelink-container/src/routes/billing_ingest.rs:483-493`).
- Persistence is idempotent by `idem_key`: the store probes the `(tenant_id, request_id)` coordinate, then inserts `ON CONFLICT DO NOTHING`, tallying `accepted` vs `deduped` (`crates/corelink-container/src/routes/billing_ingest.rs:250-291`).
- A genuine D1 backend fault mid-persist returns 503 fail-CLOSED so the runner safely retries the (idempotent) batch (`crates/corelink-container/src/routes/billing_ingest.rs:502-507`).

# Invariants

- Free tier must never reach Stripe checkout — it is an internal invariant violation that returns `Err`, not a silently-opened paid session (`crates/corelink-container/src/routes/tier_select_checkout.rs:177-179`).
- CoreLink surfaces only the Stripe-hosted checkout URL; the bearer token lives inside `StripeRealClient` and the adapter's `Debug` emits only a redaction marker, so a leaked `Debug` can never expose the Stripe secret (`crates/corelink-container/src/routes/tier_select_checkout.rs:150-157`).
- The ingest route is gated by a DEDICATED `BILLING_INGEST_AUTH_KEY` via a constant-time compare — NOT the Worker↔container key and NOT the introspect key — so an ingest-secret leak shares no blast radius (`crates/corelink-container/src/routes/billing_ingest.rs:29-37`).
- The ingest route fails CLOSED at boot: if `BILLING_INGEST_AUTH_KEY` is absent or shorter than 32 chars the route is NOT mounted (`crates/corelink-container/src/routes/billing_ingest.rs:548-556`).
- `idem_key` is the dedup coordinate — re-pushing the same record is a no-op (`deduped`), never a double-count (`crates/corelink-container/src/routes/billing_ingest.rs:85-87`).
- A single batch is bounded to `MAX_BATCH_RECORDS = 1024` (`crates/corelink-container/src/routes/billing_ingest.rs:121`) so one caller cannot stage an unbounded batch in one request — the handler rejects an over-cap batch with 400 `batch_too_large` before any staging (`crates/corelink-container/src/routes/billing_ingest.rs:479`).
- Runner slot-seconds are NOT a Stripe-billable meter — runners are priced on the per-tenant concurrency axis, so this endpoint only stages records for dashboard/capacity reconciliation (`crates/corelink-container/src/routes/billing_ingest.rs:6-12`).

# Gotchas

- **Paid is only served when active, not at "pending_checkout."** The serving filter that requires `subscription_state='active'` lives in the Worker quota path (`quota.ts`, outside these sources), not in the checkout adapter — the adapter merely opens the session. The webhook writing `status="paid"` on `checkout.session.completed` is what flips a tenant from pending to served (`docs/operator/stripe-checkout-e2e-2026-05-29.md:88-90`).
- API-created subscriptions that bypass Checkout do NOT auto-populate `tenant_billing` (only `checkout.session.completed` writes the row) — operators must use the checkout flow for self-serve signups (`docs/operator/stripe-checkout-e2e-2026-05-29.md:91-93`).
- `current_period_end_ms` is left NULL by `checkout.session.completed`; it is filled by the subsequent `customer.subscription.updated` event, so don't read it immediately after activation (`docs/operator/stripe-checkout-e2e-2026-05-29.md:47-48`).
- A stale `STRIPE_WEBHOOK_SECRET` silently 400s every webhook (`invalid_signature`) — when recreating a Stripe endpoint, immediately `wrangler secret put STRIPE_WEBHOOK_SECRET` and resync `.env.local` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:70-78`).
- Direct POSTs to the webhook URL from non-Stripe IPs are 403'd by Cloudflare Bot Fight Mode; only Stripe's allowlisted IPs (with the proper `User-Agent`) get through (`docs/operator/stripe-checkout-e2e-2026-05-29.md:115`).
- The blocking Stripe client cannot be driven from a `#[tokio::test]` runtime, so there is deliberately no automated live harness — verify the path from a throwaway `fn main()` against the Stripe TEST API (`crates/corelink-container/src/routes/tier_select_checkout.rs:239-250`).

# Citations

- `crates/corelink-container/src/routes/tier_select_checkout.rs:150-216` — production Stripe Checkout adapter: tier mapping, empty-email seam, dedicated-thread blocking call, 502-on-failure, redacted Debug.
- `crates/corelink-container/src/routes/billing_ingest.rs:6-12` — runner concurrency-priced (not Stripe-metered) rationale.
- `crates/corelink-container/src/routes/billing_ingest.rs:29-40` — dedicated fail-closed `BILLING_INGEST_AUTH_KEY` gate.
- `crates/corelink-container/src/routes/billing_ingest.rs:250-291` — idempotent probe-then-insert staging.
- `crates/corelink-container/src/routes/billing_ingest.rs:448-521` — handler: auth-before-parse, all-or-nothing validation, accepted/deduped tally, 503 fail-closed.
- `docs/operator/stripe-checkout-e2e-2026-05-29.md:85-93` — verified webhook → `tenant_billing` activation behavior + the API-bypass gotcha.

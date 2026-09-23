---
type: "LaunchControl"
title: "Money-path: checkout + billing ingest"
description: "How CoreLink turns a tier selection into a paid Stripe subscription and ingests runner usage — the launch revenue path."
source_files:
  - "crates/corelink-container/src/routes/tier_select_checkout.rs"
  - "crates/corelink-container/src/routes/billing_ingest.rs"
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-billing-stripe-materializer/src/handler.rs"
  - "docs/operator/stripe-checkout-e2e-2026-05-29.md"
  - "apps/signup-worker/src/webhooks/stripe.ts"
  - "apps/signup-worker/src/webhooks/stripe_contract.ts"
  - "worker/src/index_common.ts"
  - "worker/src/route_match.ts"
source_blobs:
  - "crates/corelink-container/src/routes/tier_select_checkout.rs@6d54a410922047a87dc385ce48b8a972410924b3"
  - "crates/corelink-container/src/routes/billing_ingest.rs@fc210237157bfaf9032b22a78f8a32278c13d1c0"
  - "crates/corelink-container/src/main.rs@b64f13689133f41f65f286ca7aca0bc1a17f53d9"
  - "crates/corelink-billing-stripe-materializer/src/handler.rs@6d9d852047469a42eea4b2e9ce2dd8faf19bcd9b"
  - "docs/operator/stripe-checkout-e2e-2026-05-29.md@ec7f01cfc9c7e538e782b44697a3e2f4ce482bc6"
  - "apps/signup-worker/src/webhooks/stripe.ts@5ae91a44c0bd8f5751d595054cab389b164f6fa7"
  - "apps/signup-worker/src/webhooks/stripe_contract.ts@b404b94e44e96b717867b3a27a87f7c249c9a3f0"
  - "worker/src/index_common.ts@8ed0757b58effc33386205dc82d4b210fb4fe121"
  - "worker/src/route_match.ts@0e1733c5279fc2862c8f8f4bdadcc04c75b60423"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: [launch, billing, stripe, checkout, usage, money-path]
timestamp: "2026-09-06T00:00:00Z"

---
The money-path implementation creates a hosted Stripe Checkout Session for a paid tier and includes a separate internal endpoint that stages runner usage records. The checkout adapter returns the Stripe URL, session id, and customer id; the ingest endpoint persists records for downstream aggregation. This concept describes repository behavior. It does not establish card collection, endpoint deployment, or a dashboard result without separate operational evidence. See `launch/tier-model` for tier selection and `launch/signup-onboarding` for the signup route.

# Role

The money-path code has three parts. (1) `StripeCheckoutCreator` maps paid cache and runner requests to `TierKind` values and returns a hosted Checkout URL with Stripe identifiers. (2) The Worker route classifies `/v1/billing/stripe-webhook` for the `_system` DO before its generic API arm; the container conditionally mounts its webhook when required configuration is present. These repository routes do not identify the configured Stripe endpoint. (3) `POST /internal/v1/billing/usage` is an internal staging seam; its module records that runner slot-seconds are not a Stripe meter and that aggregation happens downstream.

# How it works

- The checkout adapter maps the route's `RequestedTier` to the billing-domain `TierKind` — an identity match per variant across the cache tiers (Solo/Starter/Pro/Max) AND the five runner SKUs (RunnerStarter/RunnerPro/RunnerTeam/RunnerScale/RunnerMax), so a `runner_*` checkout resolves its deployed `STRIPE_PRICE_ID_RUNNER_*` price via `TierKind::as_str()`; `Free` is rejected as an internal invariant violation since free activates instantly without Stripe (`crates/corelink-container/src/routes/tier_select_checkout.rs:174-188`).
- The request body stays tenant-id-only: `customer_email` is sent EMPTY so Stripe's hosted page collects the buyer email itself — no PII threaded through the container (`crates/corelink-container/src/routes/tier_select_checkout.rs:190-196`).
- `StripeRealClient` wraps a `reqwest::blocking` client that must not run in a tokio runtime, so the call is dispatched on a DEDICATED std thread and the result is ferried back over a oneshot channel (`crates/corelink-container/src/routes/tier_select_checkout.rs:210-213`).
- Any Stripe failure — a dropped sender or a Stripe error — collapses to `Err(String)`, which the orchestration surfaces as a 502 `stripe_unavailable` (`crates/corelink-container/src/routes/tier_select_checkout.rs:214-217`).
- On success the adapter returns only the Stripe-hosted `checkout_url`, the `session_id`, and the `stripe_customer_id` — card data never transits CoreLink (`crates/corelink-container/src/routes/tier_select_checkout.rs:219-223`).
- Operator E2E confirms the activation seam: `checkout.session.completed` INSERTs the `tenant_billing` row with `status = "paid"`, `customer.subscription.updated` maps the Stripe status, and `customer.subscription.deleted` sets `canceled` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:85-90`).
- The usage ingest route authenticates before parsing its body; an unauthenticated request receives 401 (`crates/corelink-container/src/routes/billing_ingest.rs:641-669`).
- Each record is validated independently; per-record failures are counted as rejected while batch-level faults return 400 (`crates/corelink-container/src/routes/billing_ingest.rs:662-721`).
- Per-record validation canonicalizes BOTH the `region` and the `idem_key` to lowercase BEFORE validating/storing them: `region` because a CF colo is case-insensitive, so a box misconfigured with `BILLING_REGION="IAD"` stages as `iad` rather than failing `bad_region` (`crates/corelink-container/src/routes/billing_ingest.rs:582-582`), and `idem_key` so the upper- and lower-case spellings of the same BLAKE3 key collapse to ONE `(tenant_id, idem_key)` dedup coordinate (a case-variant cannot bypass the idempotency dedup) (`crates/corelink-container/src/routes/billing_ingest.rs:596-596`). It also caps the free-text `source` at `MAX_SOURCE_LEN` (256) — an over-long `source` is a clean skip (`SourceTooLong`), not something that rides the body limits into the store (`crates/corelink-container/src/routes/billing_ingest.rs:600-607`).
- Persistence is idempotent by `idem_key`: the SQL defines the `(tenant_id, request_id)` coordinate and the store resolves insert versus deduplication (`crates/corelink-container/src/routes/billing_ingest.rs:212-228`, `crates/corelink-container/src/routes/billing_ingest.rs:323-380`).
- A genuine D1 backend fault mid-persist returns 503 fail-CLOSED so the runner safely retries the (idempotent) batch — distinct from a per-record validation failure, which is skipped-and-counted, never a retry signal (`crates/corelink-container/src/routes/billing_ingest.rs:750-750`).

# Invariants

- Free tier must never reach Stripe checkout — it is an internal invariant violation that returns `Err`, not a silently-opened paid session (`crates/corelink-container/src/routes/tier_select_checkout.rs:184-186`).
- CoreLink surfaces only the Stripe-hosted checkout URL; the bearer token lives inside `StripeRealClient` and the adapter's `Debug` emits only a redaction marker, so a leaked `Debug` can never expose the Stripe secret (`crates/corelink-container/src/routes/tier_select_checkout.rs:152-160`).
- The ingest route is gated by a DEDICATED `BILLING_INGEST_AUTH_KEY` via a constant-time compare — NOT the Worker↔container key and NOT the introspect key — so an ingest-secret leak shares no blast radius (`crates/corelink-container/src/routes/billing_ingest.rs:29-37`).
- The ingest route fails closed at boot: if `BILLING_INGEST_AUTH_KEY` is absent or shorter than 32 chars the route is not mounted (`crates/corelink-container/src/routes/billing_ingest.rs:789-829`).
- `idem_key` is the dedup coordinate — re-pushing the same record is a no-op (`deduped`), never a double-count (`crates/corelink-container/src/routes/billing_ingest.rs:87-89`); it is lowercase-canonicalized at validation so a case-variant cannot mint a second coordinate and bypass dedup (`crates/corelink-container/src/routes/billing_ingest.rs:596-596`).
- A single batch is bounded to `MAX_BATCH_RECORDS = 1024` so one caller cannot stage an unbounded batch in one request (`crates/corelink-container/src/routes/billing_ingest.rs:121-123`).
- Runner slot-seconds are not a Stripe-billable meter; this endpoint stages records for downstream aggregation (`crates/corelink-container/src/routes/billing_ingest.rs:6-20`).

# Gotchas

- **Activation deployment needs operational evidence.** The container materializer has a guarded tier reconciliation path and the source tree also contains a signup-worker handler. Repository code does not show which webhook endpoint Stripe is configured to call (`crates/corelink-billing-stripe-materializer/src/handler.rs:740-784`, `crates/corelink-container/src/main.rs:820-840`).
- **Price maps are configuration inputs.** The container builds a selector from `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` values, and the signup-worker has its own reverse map (`apps/signup-worker/src/webhooks/stripe_contract.ts:59-66`). Source inspection can show their expected keys but cannot prove deployed values agree (`crates/corelink-container/src/main.rs:743-759`, `apps/signup-worker/src/webhooks/stripe_contract.ts:59-91`).
- API-created subscriptions that bypass Checkout do NOT auto-populate `tenant_billing` (only `checkout.session.completed` writes the row) — operators must use the checkout flow for self-serve signups (`docs/operator/stripe-checkout-e2e-2026-05-29.md:91-93`).
- `current_period_end_ms` is left NULL by `checkout.session.completed`; it is filled by the subsequent `customer.subscription.updated` event, so don't read it immediately after activation (`docs/operator/stripe-checkout-e2e-2026-05-29.md:47-48`).
- A stale `STRIPE_WEBHOOK_SECRET` silently 400s every webhook (`invalid_signature`) — when recreating a Stripe endpoint, immediately `wrangler secret put STRIPE_WEBHOOK_SECRET` and resync `.env.local` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:70-78`).
- Direct POSTs to the webhook URL from non-Stripe IPs are 403'd by Cloudflare Bot Fight Mode; only Stripe's allowlisted IPs (with the proper `User-Agent`) get through (`docs/operator/stripe-checkout-e2e-2026-05-29.md:115`).
- The blocking Stripe client cannot be driven from a `#[tokio::test]` runtime, so there is deliberately no automated live harness — verify the path from a throwaway `fn main()` against the Stripe TEST API (`crates/corelink-container/src/routes/tier_select_checkout.rs:301-310`).

# Citations

- `crates/corelink-container/src/routes/tier_select_checkout.rs:150-223` — production Stripe Checkout adapter: `RequestedTier`→`TierKind` mapping across the cache tiers AND the five runner SKUs (`:174-188`), empty-email seam, dedicated-thread blocking call, 502-on-failure, redacted Debug.
- `crates/corelink-container/src/routes/billing_ingest.rs:6-20` — runner concurrency-priced, raw staging, and downstream aggregation boundary.
- `crates/corelink-container/src/routes/billing_ingest.rs:29-40` — dedicated fail-closed `BILLING_INGEST_AUTH_KEY` gate.
- `crates/corelink-container/src/routes/billing_ingest.rs:257-303` — idempotent probe-then-insert staging.
- `crates/corelink-container/src/routes/billing_ingest.rs:510-591` — handler: auth-before-parse, per-record skip-and-count validation (accepted/deduped/rejected tally), 503 fail-closed on a persist fault.
- `crates/corelink-container/src/routes/billing_ingest.rs:435-477` — `validate_record`: lowercase-canonicalize `region` (case-insensitive colo) + `idem_key` (case-variant dedup-bypass close) + `MAX_SOURCE_LEN` (256) cap → `SourceTooLong`.
- `docs/operator/stripe-checkout-e2e-2026-05-29.md:85-93` — verified webhook → `tenant_billing` activation behavior + the API-bypass gotcha.
- `crates/corelink-container/src/main.rs:820-840` — conditional container webhook mount; source does not establish endpoint configuration.
- `worker/src/route_match.ts:411-414` — the webhook route is classified for the `_system` DO before the generic API arm.
0. `apps/signup-worker/src/webhooks/stripe.ts:80-80` — declared source anchor.
1. `worker/src/index_common.ts:1` — declared source anchor.

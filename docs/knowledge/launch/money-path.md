---
type: "LaunchControl"
title: "Money-path: checkout + billing ingest"
description: "How CoreLink turns a tier selection into a paid Stripe subscription and ingests runner usage — the launch revenue path."
source_files:
  - crates/corelink-container/src/routes/tier_select_checkout.rs
  - crates/corelink-container/src/routes/billing_ingest.rs
  - crates/corelink-container/src/main.rs
  - worker/src/index.ts
  - docs/operator/stripe-checkout-e2e-2026-05-29.md
source_blobs:
  - "crates/corelink-container/src/routes/tier_select_checkout.rs@cd0502f1447c1e8bd0fd4429710d000bf9220ec2"
  - "crates/corelink-container/src/routes/billing_ingest.rs@28c8943b3cc61212e346abde8899350f554505f9"
  - "crates/corelink-container/src/main.rs@6a4c0bca42e1032d7bbc7b46bcf9c6e6e628db06"
  - "worker/src/index.ts@9359127a871715f70ac337b9987d9002fa99228b"
  - "docs/operator/stripe-checkout-e2e-2026-05-29.md@ec7f01cfc9c7e538e782b44697a3e2f4ce482bc6"
checkpoint_sha: "3aa713e7c60a568e60d3eb1d127dcdc036337b45"
provenance: "AUTHORED"
tags: [launch, billing, stripe, checkout, usage, money-path]
timestamp: "2026-06-26T00:00:00Z"
---

The "money path" is the chain that converts a self-serve tier selection into actual revenue: a buyer picks a paid tier, the container mints a hosted Stripe Checkout Session, Stripe collects the card on its PCI-owned page, and a `checkout.session.completed` webhook activates the tenant's billing row. A second, separate seam ingests runner usage records for capacity reconciliation. Getting this exactly right is launch-critical — a wrong cut either serves a paid tier without payment or double-counts usage. This concept transcribes the frozen contract of two container routes plus the operator-verified end-to-end checkout report. See the sibling concepts `launch/tier-model` (the tier ladder these checkouts sell) and `launch/signup-onboarding` (the Clerk path that creates the tenant before checkout).

# Role

The money path has three moving parts. (1) **Checkout creation** — `StripeCheckoutCreator` is the production adapter behind `POST /v1/onboarding/tier-select`; it builds a Stripe Checkout Session and returns only the hosted URL. Its `RequestedTier`→`TierKind` adapter now spans TWO product axes: the four cache tiers (Solo/Starter/Pro/Max) AND five runner SKUs (RunnerStarter/RunnerPro/RunnerTeam/RunnerScale/RunnerMax), each mapping identity to its `TierKind` so the runner arms resolve the deployed `STRIPE_PRICE_ID_RUNNER_*` prices. Runner is a SEPARATE entitlement axis — the checkout only opens the Stripe session for it; the subscription↔tenant mapping and the entitlement are written by the signup-worker webhook into `runner_billing`/`runners_entitlement`, never the cache `tier_selections` tables (see `launch/tier-model` and `launch/stripe-activation-webhook`). (2) **Activation** — there are TWO live, signature-verified Stripe activation surfaces, NOT one: the signup-worker handler (`apps/signup-worker/src/webhooks/stripe.ts`, outside these container sources) AND the CONTAINER's own webhook materializer (`crates/corelink-container/src/main.rs:716-859` mounts the route; `corelink-billing-stripe-materializer`'s `D1SubscriptionStateHandler` + `build_tier_selector` write `subscription_state='active'` + the resolved tier to `tier_selections`). The **container is the routed authority**: `worker/src/index.ts:938-940` routes `/v1/billing/stripe-webhook` to the `_system` DO → container as a pure pass-through (Stripe's `Stripe-Signature` HMAC, not a PAT), so the container is the SOLE signature-verifier on that route. Sibling concepts `security/money-path-review` (:42-43, "the `subscription_state='active'` gate is enforced in the container materializer write path") and `launch/go-live-readiness` (:25, two live webhook handlers / two tier writers) corroborate this — and go-live-readiness flags the divergent-tier-map risk between the two writers (both must key the same `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` env map or a real `customer.subscription.updated` resolves to `UnknownPlan` → 422; see `crates/corelink-container/src/main.rs:797-808`). (3) **Usage ingest** — `POST /internal/v1/billing/usage` durably stages runner per-lease usage records into D1 for the downstream aggregator. Note the asymmetry: runner usage is NOT a Stripe meter — runners are priced on the concurrency axis, so this ingest is reconciliation-only, never a charge.

# How it works

- The checkout adapter maps the route's `RequestedTier` to the billing-domain `TierKind` — an identity match per variant across the cache tiers (Solo/Starter/Pro/Max) AND the five runner SKUs (RunnerStarter/RunnerPro/RunnerTeam/RunnerScale/RunnerMax), so a `runner_*` checkout resolves its deployed `STRIPE_PRICE_ID_RUNNER_*` price via `TierKind::as_str()`; `Free` is rejected as an internal invariant violation since free activates instantly without Stripe (`crates/corelink-container/src/routes/tier_select_checkout.rs:174-188`).
- The request body stays tenant-id-only: `customer_email` is sent EMPTY so Stripe's hosted page collects the buyer email itself — no PII threaded through the container (`crates/corelink-container/src/routes/tier_select_checkout.rs:190-196`).
- `StripeRealClient` wraps a `reqwest::blocking` client that must not run in a tokio runtime, so the call is dispatched on a DEDICATED std thread and the result is ferried back over a oneshot channel (`crates/corelink-container/src/routes/tier_select_checkout.rs:210-213`).
- Any Stripe failure — a dropped sender or a Stripe error — collapses to `Err(String)`, which the orchestration surfaces as a 502 `stripe_unavailable` (`crates/corelink-container/src/routes/tier_select_checkout.rs:214-217`).
- On success the adapter returns only the Stripe-hosted `checkout_url`, the `session_id`, and the `stripe_customer_id` — card data never transits CoreLink (`crates/corelink-container/src/routes/tier_select_checkout.rs:219-223`).
- Operator E2E confirms the activation seam: `checkout.session.completed` INSERTs the `tenant_billing` row with `status = "paid"`, `customer.subscription.updated` maps the Stripe status, and `customer.subscription.deleted` sets `canceled` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:85-90`).
- The usage ingest route auth gate runs FIRST on raw bytes — an unauthenticated caller is rejected 401 before the body is ever JSON-parsed, denying parse CPU/heap to an attacker (`crates/corelink-container/src/routes/billing_ingest.rs:475-496`).
- The whole batch is validated before any row is persisted (all-or-nothing): a single malformed record 400s the entire batch and stages nothing (`crates/corelink-container/src/routes/billing_ingest.rs:505-515`).
- Per-record validation canonicalizes the `idem_key` to lowercase BEFORE validating/storing it, so the upper- and lower-case spellings of the same BLAKE3 key collapse to ONE `(tenant_id, idem_key)` dedup coordinate (a case-variant cannot bypass the idempotency dedup), and it caps the free-text `source` at `MAX_SOURCE_LEN` (256) — an over-long `source` is a clean 400 (`SourceTooLong`), not something that rides the body limits into the store (`crates/corelink-container/src/routes/billing_ingest.rs:431-442`; `crates/corelink-container/src/routes/billing_ingest.rs:407`).
- Persistence is idempotent by `idem_key`: the store probes the `(tenant_id, request_id)` coordinate, then inserts `ON CONFLICT DO NOTHING`, tallying `accepted` vs `deduped` (`crates/corelink-container/src/routes/billing_ingest.rs:250-291`).
- A genuine D1 backend fault mid-persist returns 503 fail-CLOSED so the runner safely retries the (idempotent) batch (`crates/corelink-container/src/routes/billing_ingest.rs:527-534`).

# Invariants

- Free tier must never reach Stripe checkout — it is an internal invariant violation that returns `Err`, not a silently-opened paid session (`crates/corelink-container/src/routes/tier_select_checkout.rs:184-186`).
- CoreLink surfaces only the Stripe-hosted checkout URL; the bearer token lives inside `StripeRealClient` and the adapter's `Debug` emits only a redaction marker, so a leaked `Debug` can never expose the Stripe secret (`crates/corelink-container/src/routes/tier_select_checkout.rs:152-160`).
- The ingest route is gated by a DEDICATED `BILLING_INGEST_AUTH_KEY` via a constant-time compare — NOT the Worker↔container key and NOT the introspect key — so an ingest-secret leak shares no blast radius (`crates/corelink-container/src/routes/billing_ingest.rs:29-37`).
- The ingest route fails CLOSED at boot: if `BILLING_INGEST_AUTH_KEY` is absent or shorter than 32 chars the route is NOT mounted (`crates/corelink-container/src/routes/billing_ingest.rs:573-581`).
- `idem_key` is the dedup coordinate — re-pushing the same record is a no-op (`deduped`), never a double-count (`crates/corelink-container/src/routes/billing_ingest.rs:85-87`); it is lowercase-canonicalized at validation so a case-variant cannot mint a second coordinate and bypass dedup (`crates/corelink-container/src/routes/billing_ingest.rs:431-433`).
- A single batch is bounded to `MAX_BATCH_RECORDS = 1024` so one caller cannot stage an unbounded batch in one request (`crates/corelink-container/src/routes/billing_ingest.rs:119-121`).
- Runner slot-seconds are NOT a Stripe-billable meter — runners are priced on the per-tenant concurrency axis, so this endpoint only stages records for dashboard/capacity reconciliation (`crates/corelink-container/src/routes/billing_ingest.rs:6-12`).

# Gotchas

- **Paid is only served when active, not at "pending_checkout."** The serving filter that requires `subscription_state='active'` lives in the Worker quota path (`quota.ts`, outside these sources), not in the checkout adapter — the adapter merely opens the session. The webhook writing `status="paid"`/`subscription_state='active'` is what flips a tenant from pending to served. That write happens on the **container materializer** when the route is hit there (`crates/corelink-container/src/main.rs:808-856`); the same mount also conditionally attaches the Runners-entitlement seed resolver via `build_runners_resolver` (env-gated on `STRIPE_PRICE_ID_RUNNER_*`, dormant otherwise), and the signup-worker carries an equivalent handler — so an audit of "what activates a tenant" must look at BOTH surfaces, not just the worker (`docs/operator/stripe-checkout-e2e-2026-05-29.md:88-90`).
- **Two live activation writers ⇒ a divergent-tier-map risk.** Both the container materializer and the signup-worker resolve `customer.subscription.updated` → tier off the `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` env map; if the two are seeded with different price ids one writer resolves `UnknownPlan` → 422 and Stripe stops retrying. go-live-readiness flags this; keep the two maps identical (`crates/corelink-container/src/main.rs:797-808`; the env→tier table + builder at `crates/corelink-container/src/main.rs:93-214`).
- API-created subscriptions that bypass Checkout do NOT auto-populate `tenant_billing` (only `checkout.session.completed` writes the row) — operators must use the checkout flow for self-serve signups (`docs/operator/stripe-checkout-e2e-2026-05-29.md:91-93`).
- `current_period_end_ms` is left NULL by `checkout.session.completed`; it is filled by the subsequent `customer.subscription.updated` event, so don't read it immediately after activation (`docs/operator/stripe-checkout-e2e-2026-05-29.md:47-48`).
- A stale `STRIPE_WEBHOOK_SECRET` silently 400s every webhook (`invalid_signature`) — when recreating a Stripe endpoint, immediately `wrangler secret put STRIPE_WEBHOOK_SECRET` and resync `.env.local` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:70-78`).
- Direct POSTs to the webhook URL from non-Stripe IPs are 403'd by Cloudflare Bot Fight Mode; only Stripe's allowlisted IPs (with the proper `User-Agent`) get through (`docs/operator/stripe-checkout-e2e-2026-05-29.md:115`).
- The blocking Stripe client cannot be driven from a `#[tokio::test]` runtime, so there is deliberately no automated live harness — verify the path from a throwaway `fn main()` against the Stripe TEST API (`crates/corelink-container/src/routes/tier_select_checkout.rs:301-310`).

# Citations

- `crates/corelink-container/src/routes/tier_select_checkout.rs:150-223` — production Stripe Checkout adapter: `RequestedTier`→`TierKind` mapping across the cache tiers AND the five runner SKUs (`:174-188`), empty-email seam, dedicated-thread blocking call, 502-on-failure, redacted Debug.
- `crates/corelink-container/src/routes/billing_ingest.rs:6-12` — runner concurrency-priced (not Stripe-metered) rationale.
- `crates/corelink-container/src/routes/billing_ingest.rs:29-40` — dedicated fail-closed `BILLING_INGEST_AUTH_KEY` gate.
- `crates/corelink-container/src/routes/billing_ingest.rs:250-291` — idempotent probe-then-insert staging.
- `crates/corelink-container/src/routes/billing_ingest.rs:475-543` — handler: auth-before-parse, all-or-nothing validation, accepted/deduped tally, 503 fail-closed.
- `crates/corelink-container/src/routes/billing_ingest.rs:407-442` — `validate_record`: lowercase-canonicalize `idem_key` (case-variant dedup-bypass close) + `MAX_SOURCE_LEN` (256) cap → `SourceTooLong` 400.
- `docs/operator/stripe-checkout-e2e-2026-05-29.md:85-93` — verified webhook → `tenant_billing` activation behavior + the API-bypass gotcha.
- `crates/corelink-container/src/main.rs:716-859` — the SECOND live activation surface: the container's `STRIPE_WEBHOOK_SECRET`-gated Stripe webhook mount → durable `D1HttpBillingWriter` (`:762-790`) + `D1SubscriptionStateHandler` (`:809-813`, `build_tier_selector` resolves the tier off `STRIPE_PRICE_ID_*` at `:792-808`) materializing `subscription_state='active'`+tier to `tier_selections` over D1-HTTP.
- `worker/src/index.ts:938-940` — the worker routes `/v1/billing/stripe-webhook` to the `_system` DO → container as a pure pass-through; the container is the SOLE signature-verifier / routed authority. This route is matched BEFORE the generic `/v1/*` PAT arm so the un-PAT'd webhook is never swallowed into the PAT-required bucket and 401'd.

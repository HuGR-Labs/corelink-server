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
The "money path" is the chain that converts a self-serve tier selection into actual revenue: a buyer picks a paid tier, the container mints a hosted Stripe Checkout Session, Stripe collects the card on its PCI-owned page, and a `checkout.session.completed` webhook activates the tenant's billing row. A second, separate seam ingests runner usage records for capacity reconciliation. Getting this exactly right is launch-critical — a wrong cut either serves a paid tier without payment or double-counts usage. This concept transcribes the frozen contract of two container routes plus the operator-verified end-to-end checkout report. See the sibling concepts `launch/tier-model` (the tier ladder these checkouts sell) and `launch/signup-onboarding` (the Clerk path that creates the tenant before checkout).

# Role

The money path has three moving parts. (1) **Checkout creation** — `StripeCheckoutCreator` is the production adapter behind `POST /v1/onboarding/tier-select`; it builds a Stripe Checkout Session and returns only the hosted URL. Its `RequestedTier`→`TierKind` adapter now spans TWO product axes: the four cache tiers (Solo/Starter/Pro/Max) AND five runner SKUs (RunnerStarter/RunnerPro/RunnerTeam/RunnerScale/RunnerMax), each mapping identity to its `TierKind` so the runner arms resolve the deployed `STRIPE_PRICE_ID_RUNNER_*` prices. Runner is a SEPARATE entitlement axis — the checkout only opens the Stripe session for it; the subscription↔tenant mapping and the entitlement are written by the signup-worker webhook into `runner_billing`/`runners_entitlement`, never the cache `tier_selections` tables (see `launch/tier-model` and `launch/stripe-activation-webhook`). (2) **Activation** — there are TWO live, signature-verified Stripe activation surfaces, NOT one: the signup-worker handler (`apps/signup-worker/src/webhooks/stripe.ts`, outside these container sources) AND the CONTAINER's own webhook materializer. The container mounts and wires that handler in the `STRIPE_WEBHOOK_SECRET` block (`crates/corelink-container/src/main.rs:838-859`); the handler's `reconcile_tier` performs the guarded `subscription_state='active'` + resolved-tier write (`crates/corelink-billing-stripe-materializer/src/handler.rs:740-784`). The **container is the routed authority**: `worker/src/route_match.ts:412-414` routes `/v1/billing/stripe-webhook` to the `_system` DO → container as a pure pass-through (Stripe's `Stripe-Signature` HMAC, not a PAT), so the container is the SOLE signature-verifier on that route. Sibling concepts `security/money-path-review` (:42-43, "the `subscription_state='active'` gate is enforced in the container materializer write path") and `launch/go-live-readiness` (:25, two live webhook handlers / two tier writers) corroborate this — and go-live-readiness flags the divergent-tier-map risk between the two writers (both must key the same `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` env map or a real `customer.subscription.updated` resolves to `UnknownPlan` → 422; see `crates/corelink-container/src/main.rs:88-93`, `164-179`). (3) **Usage ingest** — `POST /internal/v1/billing/usage` durably stages runner per-lease usage records into D1 for the downstream aggregator. Note the asymmetry: runner usage is NOT a Stripe meter — runners are priced on the concurrency axis, so this ingest is reconciliation-only, never a charge.

# How it works

- The checkout adapter maps the route's `RequestedTier` to the billing-domain `TierKind` — an identity match per variant across the cache tiers (Solo/Starter/Pro/Max) AND the five runner SKUs (RunnerStarter/RunnerPro/RunnerTeam/RunnerScale/RunnerMax), so a `runner_*` checkout resolves its deployed `STRIPE_PRICE_ID_RUNNER_*` price via `TierKind::as_str()`; `Free` is rejected as an internal invariant violation since free activates instantly without Stripe (`crates/corelink-container/src/routes/tier_select_checkout.rs:174-188`).
- The request body stays tenant-id-only: `customer_email` is sent EMPTY so Stripe's hosted page collects the buyer email itself — no PII threaded through the container (`crates/corelink-container/src/routes/tier_select_checkout.rs:190-196`).
- `StripeRealClient` wraps a `reqwest::blocking` client that must not run in a tokio runtime, so the call is dispatched on a DEDICATED std thread and the result is ferried back over a oneshot channel (`crates/corelink-container/src/routes/tier_select_checkout.rs:210-213`).
- Any Stripe failure — a dropped sender or a Stripe error — collapses to `Err(String)`, which the orchestration surfaces as a 502 `stripe_unavailable` (`crates/corelink-container/src/routes/tier_select_checkout.rs:214-217`).
- On success the adapter returns only the Stripe-hosted `checkout_url`, the `session_id`, and the `stripe_customer_id` — card data never transits CoreLink (`crates/corelink-container/src/routes/tier_select_checkout.rs:219-223`).
- Operator E2E confirms the activation seam: `checkout.session.completed` INSERTs the `tenant_billing` row with `status = "paid"`, `customer.subscription.updated` maps the Stripe status, and `customer.subscription.deleted` sets `canceled` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:85-90`).
- The usage ingest route auth gate runs FIRST on raw bytes — an unauthenticated caller is rejected 401 before the body is ever JSON-parsed, denying parse CPU/heap to an attacker (`crates/corelink-container/src/routes/billing_ingest.rs:487-508`).
- Each record is validated INDEPENDENTLY: a per-record validation failure SKIPS that record (counted in the `rejected` response field) instead of 400-ing the batch, so one malformed record can never block its batch-mates nor make the runner retain-and-retry the batch forever. Only BATCH-level faults (unparseable JSON, empty, or over-`MAX_BATCH_RECORDS`) 400 (`crates/corelink-container/src/routes/billing_ingest.rs:540-565`).
- Per-record validation canonicalizes BOTH the `region` and the `idem_key` to lowercase BEFORE validating/storing them: `region` because a CF colo is case-insensitive, so a box misconfigured with `BILLING_REGION="IAD"` stages as `iad` rather than failing `bad_region` (`crates/corelink-container/src/routes/billing_ingest.rs:582-582`), and `idem_key` so the upper- and lower-case spellings of the same BLAKE3 key collapse to ONE `(tenant_id, idem_key)` dedup coordinate (a case-variant cannot bypass the idempotency dedup) (`crates/corelink-container/src/routes/billing_ingest.rs:596-596`). It also caps the free-text `source` at `MAX_SOURCE_LEN` (256) — an over-long `source` is a clean skip (`SourceTooLong`), not something that rides the body limits into the store (`crates/corelink-container/src/routes/billing_ingest.rs:600-607`).
- Persistence is idempotent by `idem_key`: the store probes the `(tenant_id, request_id)` coordinate, then inserts `ON CONFLICT DO NOTHING` — persisting `qty`/`event_kind`/`billing_period` alongside the payload hash (WI-S10-007) so the aggregator can drain the quantity directly — tallying `accepted` vs `deduped` (`crates/corelink-container/src/routes/billing_ingest.rs:257-303`).
- A genuine D1 backend fault mid-persist returns 503 fail-CLOSED so the runner safely retries the (idempotent) batch — distinct from a per-record validation failure, which is skipped-and-counted, never a retry signal (`crates/corelink-container/src/routes/billing_ingest.rs:750-750`).

# Invariants

- Free tier must never reach Stripe checkout — it is an internal invariant violation that returns `Err`, not a silently-opened paid session (`crates/corelink-container/src/routes/tier_select_checkout.rs:184-186`).
- CoreLink surfaces only the Stripe-hosted checkout URL; the bearer token lives inside `StripeRealClient` and the adapter's `Debug` emits only a redaction marker, so a leaked `Debug` can never expose the Stripe secret (`crates/corelink-container/src/routes/tier_select_checkout.rs:152-160`).
- The ingest route is gated by a DEDICATED `BILLING_INGEST_AUTH_KEY` via a constant-time compare — NOT the Worker↔container key and NOT the introspect key — so an ingest-secret leak shares no blast radius (`crates/corelink-container/src/routes/billing_ingest.rs:29-37`).
- The ingest route fails CLOSED at boot: if `BILLING_INGEST_AUTH_KEY` is absent or shorter than 32 chars the route is NOT mounted (`crates/corelink-container/src/routes/billing_ingest.rs:585-593`).
- `idem_key` is the dedup coordinate — re-pushing the same record is a no-op (`deduped`), never a double-count (`crates/corelink-container/src/routes/billing_ingest.rs:87-89`); it is lowercase-canonicalized at validation so a case-variant cannot mint a second coordinate and bypass dedup (`crates/corelink-container/src/routes/billing_ingest.rs:596-596`).
- A single batch is bounded to `MAX_BATCH_RECORDS = 1024` so one caller cannot stage an unbounded batch in one request (`crates/corelink-container/src/routes/billing_ingest.rs:121-123`).
- Runner slot-seconds are NOT a Stripe-billable meter — runners are priced on the per-tenant concurrency axis, so this endpoint only stages records for dashboard/capacity reconciliation (`crates/corelink-container/src/routes/billing_ingest.rs:6-12`).

# Gotchas

- **Paid is only served when active, not at "pending_checkout."** The serving filter that requires `subscription_state='active'` lives in the Worker quota path (`quota.ts`, outside these sources), not in the checkout adapter — the adapter merely opens the session. The webhook writing `status="paid"`/`subscription_state='active'` is what flips a tenant from pending to served. That write happens in the **container materializer**'s `reconcile_tier` (`crates/corelink-billing-stripe-materializer/src/handler.rs:740-784`) after the route is mounted (`crates/corelink-container/src/main.rs:838-859`); the same mount also conditionally attaches the Runners-entitlement seed resolver via `build_runners_resolver` (env-gated on `STRIPE_PRICE_ID_RUNNER_*`, dormant otherwise), and the signup-worker carries an equivalent handler — so an audit of "what activates a tenant" must look at BOTH surfaces, not just the worker (`docs/operator/stripe-checkout-e2e-2026-05-29.md:88-90`).
- **Two live activation writers ⇒ a divergent-tier-map risk.** Both the container materializer and the signup-worker resolve `customer.subscription.updated` → tier off the `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` env map; if the two are seeded with different price ids one writer resolves `UnknownPlan` → 422 and Stripe stops retrying. go-live-readiness flags this; keep the two maps identical (`crates/corelink-container/src/main.rs:705-737`, `164-179`; the signup-worker reverse map at `apps/signup-worker/src/webhooks/stripe_contract.ts:59-66`).
- API-created subscriptions that bypass Checkout do NOT auto-populate `tenant_billing` (only `checkout.session.completed` writes the row) — operators must use the checkout flow for self-serve signups (`docs/operator/stripe-checkout-e2e-2026-05-29.md:91-93`).
- `current_period_end_ms` is left NULL by `checkout.session.completed`; it is filled by the subsequent `customer.subscription.updated` event, so don't read it immediately after activation (`docs/operator/stripe-checkout-e2e-2026-05-29.md:47-48`).
- A stale `STRIPE_WEBHOOK_SECRET` silently 400s every webhook (`invalid_signature`) — when recreating a Stripe endpoint, immediately `wrangler secret put STRIPE_WEBHOOK_SECRET` and resync `.env.local` (`docs/operator/stripe-checkout-e2e-2026-05-29.md:70-78`).
- Direct POSTs to the webhook URL from non-Stripe IPs are 403'd by Cloudflare Bot Fight Mode; only Stripe's allowlisted IPs (with the proper `User-Agent`) get through (`docs/operator/stripe-checkout-e2e-2026-05-29.md:115`).
- The blocking Stripe client cannot be driven from a `#[tokio::test]` runtime, so there is deliberately no automated live harness — verify the path from a throwaway `fn main()` against the Stripe TEST API (`crates/corelink-container/src/routes/tier_select_checkout.rs:301-310`).

# Citations

- `crates/corelink-container/src/routes/tier_select_checkout.rs:150-223` — production Stripe Checkout adapter: `RequestedTier`→`TierKind` mapping across the cache tiers AND the five runner SKUs (`:174-188`), empty-email seam, dedicated-thread blocking call, 502-on-failure, redacted Debug.
- `crates/corelink-container/src/routes/billing_ingest.rs:6-12` — runner concurrency-priced (not Stripe-metered) rationale.
- `crates/corelink-container/src/routes/billing_ingest.rs:29-40` — dedicated fail-closed `BILLING_INGEST_AUTH_KEY` gate.
- `crates/corelink-container/src/routes/billing_ingest.rs:257-303` — idempotent probe-then-insert staging.
- `crates/corelink-container/src/routes/billing_ingest.rs:510-591` — handler: auth-before-parse, per-record skip-and-count validation (accepted/deduped/rejected tally), 503 fail-closed on a persist fault.
- `crates/corelink-container/src/routes/billing_ingest.rs:435-477` — `validate_record`: lowercase-canonicalize `region` (case-insensitive colo) + `idem_key` (case-variant dedup-bypass close) + `MAX_SOURCE_LEN` (256) cap → `SourceTooLong`.
- `docs/operator/stripe-checkout-e2e-2026-05-29.md:85-93` — verified webhook → `tenant_billing` activation behavior + the API-bypass gotcha.
- `crates/corelink-container/src/main.rs:759-764` — the SECOND live activation surface: the `STRIPE_WEBHOOK_SECRET`-gated mount wires the durable billing writer; the materializer's guarded active-tier write is executed in `crates/corelink-billing-stripe-materializer/src/handler.rs:740-784`.
- `worker/src/route_match.ts:412-414` — the worker routes `/v1/billing/stripe-webhook` to the `_system` DO → container as a pure pass-through; the container is the SOLE signature-verifier / routed authority. This route is matched BEFORE the generic `/v1/*` PAT arm so the un-PAT'd webhook is never swallowed into the PAT-required bucket and 401'd.
0. `apps/signup-worker/src/webhooks/stripe.ts:80-80` — declared source anchor.
1. `worker/src/index_common.ts:1` — declared source anchor.

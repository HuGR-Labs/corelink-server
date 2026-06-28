---
type: "CrateCluster"
title: "Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)"
description: "The end-to-end metered-usage path — CloudEvents usage emit, cron aggregation, drift reconciliation, the LIVE container Stripe-webhook state materializer (subscription_state/tier write), and the real HTTPS Stripe transport with HMAC webhook verify."
source_files:
  - "crates/corelink-billing-emit/src/lib.rs"
  - "crates/corelink-billing-emit/src/emitter.rs"
  - "crates/corelink-billing-aggregator/src/lib.rs"
  - "crates/corelink-billing-aggregator/src/aggregator.rs"
  - "crates/corelink-billing-reconcile/src/lib.rs"
  - "crates/corelink-billing-reconcile/src/reconciler.rs"
  - "crates/corelink-billing-stripe-materializer/src/lib.rs"
  - "crates/corelink-billing-stripe-materializer/src/handler.rs"
  - "crates/corelink-billing-stripe-materializer/src/d1.rs"
  - "crates/corelink-billing-stripe-materializer/src/wasm32_binders.rs"
  - "crates/corelink-stripe-real/src/lib.rs"
  - "crates/corelink-stripe-real/src/webhook.rs"
  - "crates/corelink-stripe-real/src/webhook_dispatch.rs"
  - "crates/corelink-stripe-real/src/client.rs"
  - "crates/corelink-analytics/src/lib.rs"
  - "crates/corelink-analytics/src/validator.rs"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "0c44977b9ee49e2a67556377c1f973aa92d5f5b2"
provenance: "AUTHORED"
tags: ["billing", "stripe", "usage-metering", "reconciliation", "money-path"]
timestamp: "2026-06-28T00:00:00Z"
---

# Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)

This is the metered side of the money path: the chain that turns raw cache/runner usage into aggregated counters, reconciles them against Stripe, and — critically — the LIVE inbound webhook that writes a tenant's `subscription_state`/`tier` into D1. It is distinct from the checkout half. The companion concepts to read alongside it are [the money path](/launch/money-path.md) (checkout-session creation + the runner-usage ingest seam), the `launch/stripe-activation-webhook` concept (the signup-worker activation surface), and the [tenancy $-ceiling](/tenancy/dollar-ceiling.md) (the leased monthly cost cap that is the preventive tripwire, NOT this precise-metering pipeline). The single most load-bearing fact: there are **TWO live activation surfaces** that flip a tenant to paid — the signup-worker (`apps/signup-worker/src/webhooks/stripe.ts`) and the **container's mounted webhook** described here. The container is the routed authority that the production deployment points the live Stripe endpoint at.

# Role

The crates split into a **deferred pure-logic front** and a **live egress + materialize back**. Be honest about which is which:

- `corelink-billing-emit` / `corelink-billing-aggregator` / `corelink-billing-reconcile` are **pure-logic skeletons** per the `trait-abstraction-defer` charter: trait surfaces + an in-memory orchestrator that pins every invariant a future CF R2/Cron/Queue binding will rely on; the real R2 PutObject + Cron Durable Object are explicitly deferred (emit → WI-S10-007, aggregator/reconcile likewise). They are NOT yet wired into a production fetch path.
- `corelink-billing-stripe-materializer` ships the **production** `StateMaterializer`: native CI uses `InMemoryBillingD1`, but the container wires the durable `D1HttpBillingWriter` so the same handler writes real D1 rows over the CF REST API. This is the LIVE container activation surface.
- `corelink-stripe-real` is the **real transport**: inbound HMAC webhook verify + the full dispatch pipeline AND the outbound `reqwest::blocking` HTTPS client (the actual Stripe egress, `Bearer`-authed, idempotency-keyed, retrying).
- `corelink-analytics` is a **pure-logic skeleton** (Analytics Engine binding deferred to WI-S09-007) carrying the cardinality validator that the billing `metric_emitted` counter rides.

# How it works

- **Usage emit (skeleton).** `InMemoryUsageEventEmitter::emit` canonicalizes the `UsageEvent` (JCS, idem-slot zeroed), derives the BLAKE3-256 `idem_key`, runs the per-tenant idempotency tracker, fires the audit envelope, then appends to the R2 sink — duplicates short-circuit to `DuplicateRejected` with no second write (`crates/corelink-billing-emit/src/emitter.rs:213-314`). The crate doc-comment is explicit that the R2 PutObject + D1 staging mirror are deferred to WI-S10-007 (`crates/corelink-billing-emit/src/lib.rs:119-132`).
- **Aggregate (skeleton).** `InMemoryCounterAggregator::run` audits `run_started` BEFORE any state read, deterministically orders events by `(time_ms, idem_key)`, sums `qty`, and — on a watermark replay where the recomputed `data` byte-equals the prior aggregate — returns `SkippedDuplicateRun` so the hash-chain head does NOT advance twice (`crates/corelink-billing-aggregator/src/aggregator.rs:373-381`). The production CF Cron DO is deferred (`crates/corelink-billing-aggregator/src/lib.rs:1-20`).
- **Reconcile (skeleton).** `InMemoryBillingReconciler::reconcile` classifies max drift through a 4-tier ladder; only the `PageSev1AutoPaused` arm calls `StripeSubmissionControl::pause(...)` — and the canonical audit + drift-history row land BEFORE the pause attempt (`crates/corelink-billing-reconcile/src/reconciler.rs:280-285`, `crates/corelink-billing-reconcile/src/reconciler.rs:328-340`). The Cron DO trigger is deferred (`crates/corelink-billing-reconcile/src/reconciler.rs:71-74`).
- **Inbound webhook verify (live).** `verify_webhook_signature` parses `t=…,v1=…`, enforces the ±5-minute replay window, recomputes `HMAC-SHA256("{t}.{payload}", secret)` over the EXACT body bytes, and constant-time-compares each `v1` candidate via `subtle::ConstantTimeEq` (`crates/corelink-stripe-real/src/webhook.rs:41-78`).
- **Dispatch (live).** `WebhookDispatcher::process` runs the strict order: header present → signature verify → parse → idempotency dedup (`try_insert`) → materialize → audit/SLI. An already-processed event returns `200` with NO re-dispatch; a bad signature is `401`; a materializer transient is `500` so Stripe retries (`crates/corelink-stripe-real/src/webhook_dispatch.rs:530-622`, `crates/corelink-stripe-real/src/webhook_dispatch.rs:648-690`).
- **Materialize → D1 write (live).** `D1SubscriptionStateHandler` emits the billing audit BEFORE every D1 mutation; on `customer.subscription.updated` it recomputes the tier and, for a granting status, drives `SQL_UPSERT_TIER` which writes the literal `subscription_state='active'` + the new `tier` row on `ON CONFLICT(tenant_id)` (`crates/corelink-billing-stripe-materializer/src/handler.rs:435-478`, `crates/corelink-billing-stripe-materializer/src/d1.rs:173`).
- **Container wiring (live).** When `STRIPE_WEBHOOK_SECRET` is set, the container mounts the dispatcher on the data-plane listener, wiring the durable `D1HttpBillingWriter` (CF D1 REST API) so webhook state survives restarts; without the D1 env it falls back to the in-memory mirror (`crates/corelink-container/src/main.rs:708-782`, `crates/corelink-container/src/main.rs:831-847`).
- **Outbound egress (live).** `StripeRealClient::post_form` is the real HTTPS POST: `bearer_auth` with the secret-wrapped key, an `Idempotency-Key` header for retry safety, the pinned `Stripe-Version`, and exponential backoff on `5xx`/`429` with no silent cross-mode fallback (`crates/corelink-stripe-real/src/client.rs:452-501`).
- **Analytics validator (skeleton).** `CardinalityValidator::validate_and_register` rejects an emit that would push a metric over its per-metric or global budget BEFORE registering the tuple — the runtime half of INV-OBS-CARDINALITY-BUDGET (`crates/corelink-analytics/src/validator.rs:195-237`).

# Invariants

- **Webhook authenticity is verified BEFORE the body is parsed.** A missing/invalid signature or an out-of-window timestamp short-circuits to `400`/`401` and the envelope is never `serde`-parsed (`crates/corelink-stripe-real/src/webhook_dispatch.rs:536-599`). The compare is constant-time over equal-length candidates (`crates/corelink-stripe-real/src/webhook.rs:73`).
- **The container materializer is GRANT-ONLY on `updated`.** `subscription_status_grants_access` admits only `active`/`trialing`; a recognized plan carrying a non-granting status (`past_due`/`unpaid`/`paused`/…) is NOT downgraded here — the entitlement upsert is simply skipped, because actively flipping the gate OFF is the signup-worker's authority (`crates/corelink-billing-stripe-materializer/src/handler.rs:141-143`, `crates/corelink-billing-stripe-materializer/src/handler.rs:459-476`). Explicit `customer.subscription.deleted` DOES downgrade to Free.
- **Audit fires BEFORE state mutation on every arm (fail-CLOSED).** Across emit, aggregate, reconcile, and materialize, an audit failure aborts the mutation and propagates the typed error (`crates/corelink-billing-stripe-materializer/src/handler.rs:329-355`, `crates/corelink-billing-aggregator/src/aggregator.rs:321-328`).
- **Idempotency is keyed on the Stripe `event_id` and committed BEFORE materialize.** A redelivery hits `AlreadyProcessed` and returns `200` without re-mutating (`crates/corelink-stripe-real/src/webhook_dispatch.rs:601-622`). Because the dedup row commits first, a TRANSIENT materialize failure is quarantined to the DLQ instead of silently lost (`crates/corelink-stripe-real/src/webhook_dispatch.rs:714-738`).
- **Every outbound POST carries an `Idempotency-Key`** so a retry after a transport failure cannot double-charge (`crates/corelink-stripe-real/src/client.rs:465`).
- **`tenant_id` is forbidden as an analytics label** (cardinality + tenant isolation); only the canonical `Tier` enum appears (`crates/corelink-analytics/src/lib.rs:98-103`).

# Gotchas

- **TWO live activation surfaces — do not assume single-writer.** The container's `D1SubscriptionStateHandler` is a SECOND writer of `tier_selections.subscription_state` alongside the signup-worker; the handler's own comment names itself "a SECOND writer of `subscription_state`" and explains the status gate exists precisely to stop it re-granting `active` on an unpaid `updated` (`crates/corelink-billing-stripe-materializer/src/handler.rs:124-140`). Reconcile a change against BOTH writers.
- **The native binary never reaches real D1 via the wasm32 binder.** `CfD1BillingWriter::sync_gate` validates SQL shape + does the tenant ct-eq probe, then returns `Transient("wasm32_async_dispatch_pending: …")` — the actual `worker::D1Database` call is dispatched one frame above in the CF Worker boot layer (`crates/corelink-billing-stripe-materializer/src/wasm32_binders.rs:157-191`). The container instead uses the native `D1HttpBillingWriter`; the wasm32 binder is for the CF Worker target.
- **emit / aggregate / reconcile / analytics are NOT yet on a production fetch path.** They are charter-deferred trait + in-memory-fake skeletons; their R2/Cron/Analytics-Engine bindings ship in the WI-S10-007 / WI-S09-007 PRR gates. Do not cite them as "live metering" — the LIVE metered surfaces today are the materializer webhook + the runner-usage ingest (in the money-path concept).
- **The plan→tier map must key off the real `STRIPE_PRICE_ID_{TIER}` env values**, not the `plan_{tier}` literals — a real `customer.subscription.updated` carries `price_…` ids, so a misconfigured map resolves every event to `UnknownPlan` → `422` and Stripe stops retrying (`crates/corelink-container/src/main.rs:784-800`).
- **`extract_plan_id` tolerates both Stripe shapes** — legacy `data.object.plan.id` and modern `items.data[0].price.id` — so a pinned-API-version bump that drops `plan.id` doesn't 422 every subscription (`crates/corelink-billing-stripe-materializer/src/handler.rs:150-163`).
- **The outbound client is `reqwest::blocking`** and must be driven off the tokio runtime (the container ferries the call to a dedicated thread); it also has a dual auth mode (`direct` default / `wallet-broker`) selected by `STRIPE_AUTH_MODE`, with no silent fallback between them (`crates/corelink-stripe-real/src/client.rs:30-32`, `crates/corelink-stripe-real/src/lib.rs:18-41`).

# Citations

1. `crates/corelink-billing-emit/src/emitter.rs:213-314` — emit pipeline: canonicalize → derive idem_key → idempotency check → audit-before-write → append; duplicate short-circuit.
2. `crates/corelink-billing-emit/src/lib.rs:119-132` — emit R2/D1/Queue production wiring DEFERRED to WI-S10-007 (skeleton disclosure).
3. `crates/corelink-billing-aggregator/src/aggregator.rs:321-381` — audit-before-observe + deterministic ordering + `SkippedDuplicateRun` watermark-replay (no double chain advance).
4. `crates/corelink-billing-aggregator/src/lib.rs:1-20` — aggregator is the in-memory orchestrator; production CF Cron DO deferred.
5. `crates/corelink-billing-reconcile/src/reconciler.rs:280-340` — 4-tier drift ladder; only `PageSev1AutoPaused` calls `StripeSubmissionControl::pause`, audit + history row BEFORE pause.
6. `crates/corelink-billing-reconcile/src/lib.rs:1-12` — reconcile run-pipeline contract (audit-fail-CLOSED, Cron DO deferred).
7. `crates/corelink-stripe-real/src/webhook.rs:41-78` — `verify_webhook_signature`: ±5-min window, HMAC over exact bytes, constant-time multi-`v1` compare.
8. `crates/corelink-stripe-real/src/webhook_dispatch.rs:530-622` — `process`: header→verify→parse→dedup ordering; `AlreadyProcessed`→200, bad sig→401.
9. `crates/corelink-stripe-real/src/webhook_dispatch.rs:648-738` — dispatch routing + transient→500 + DLQ quarantine (dedup row committed before materialize).
10. `crates/corelink-stripe-real/src/client.rs:452-501` — `post_form`: real bearer-authed HTTPS egress + `Idempotency-Key` + retry on 5xx/429.
11. `crates/corelink-stripe-real/src/lib.rs:18-41` — dual auth mode (`direct`/`wallet-broker`), credentials secret-wrapped, no silent fallback.
12. `crates/corelink-billing-stripe-materializer/src/handler.rs:124-143` — `subscription_status_grants_access` + the "SECOND writer of subscription_state" grant-only rationale.
13. `crates/corelink-billing-stripe-materializer/src/handler.rs:329-355` — audit-before-D1-write on the subscription upsert arm.
14. `crates/corelink-billing-stripe-materializer/src/handler.rs:435-478` — `reconcile_tier`: status gate guards the `'active'` upsert.
15. `crates/corelink-billing-stripe-materializer/src/handler.rs:150-163` — `extract_plan_id` tolerant of legacy `plan.id` + modern `items.data[].price.id`.
16. `crates/corelink-billing-stripe-materializer/src/d1.rs:173` — `SQL_UPSERT_TIER`: writes literal `subscription_state='active'` + tier on `ON CONFLICT(tenant_id)`.
17. `crates/corelink-billing-stripe-materializer/src/wasm32_binders.rs:157-191` — wasm32 `sync_gate` validates+ct-eq then stages (`wasm32_async_dispatch_pending`); real call one frame above.
18. `crates/corelink-billing-stripe-materializer/src/lib.rs:1-56` — materializer ships the production `StateMaterializer`; native InMemory vs wasm32/native-HTTP binders.
19. `crates/corelink-container/src/main.rs:708-847` — the LIVE container webhook mount: `D1HttpBillingWriter` durable writer + `WebhookDispatcher` on the data-plane listener.
20. `crates/corelink-container/src/main.rs:784-800` — plan→tier map MUST key off real `STRIPE_PRICE_ID_{TIER}` env values or every event is `UnknownPlan` 422.
21. `crates/corelink-analytics/src/validator.rs:195-237` — `validate_and_register` rejects over-budget emits BEFORE registering (INV-OBS-CARDINALITY-BUDGET runtime half).
22. `crates/corelink-analytics/src/lib.rs:98-103` — `tenant_id` forbidden as a label; only canonical `Tier` appears.
</content>
</invoke>

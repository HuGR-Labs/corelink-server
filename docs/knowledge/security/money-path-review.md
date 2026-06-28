---
type: "SecurityControl"
title: "Money-path security review"
description: "The go-live review of the Stripe webhook → materializer → D1 → quota/$-ceiling chain: which money-integrity invariants hold, and the MED/LOW robustness gaps that remain."
source_files:
  - "docs/security/2026-06-23-review-money-path.md"
  - "crates/corelink-stripe-real/src/webhook_dispatch.rs"
  - "crates/corelink-billing-stripe-materializer/src/handler.rs"
checkpoint_sha: "03c2ae27deb7094fea4009927b90959533dae21e"
provenance: "AUTHORED"
tags: ["security", "billing", "stripe", "money-path", "quota"]
timestamp: "2026-06-26T00:00:00Z"
---

# Money-path security review

> **Dated snapshot — 2026-06-23.** This concept transcribes the verdict and finding list of the
> `docs/security/2026-06-23-review-money-path.md` go-live money-path review **as it stood on
> 2026-06-23**. The core money-integrity invariants are durable, but the individual robustness
> findings are a point-in-time record: **two of them (F-MP-2, F-MP-3) have since been RESOLVED in
> code** (see the Gotchas, below) and predate this concept's `checkpoint_sha`. Read the findings for
> WHAT the review checked and HOW it separates money-loss defects from robustness debt, not as a list
> of still-open gaps.

The money path is the chain from a signed Stripe webhook through the materializer into D1 and the
quota / `$`-ceiling enforcement — the thing that must never serve a paid tier without payment, never
double-grant, and never lose billing state. This concept records the 2026-06-23 go-live review: the
core money-integrity invariants hold (HMAC-verify-before-parse, idempotency-before-materialize,
audit-before-write fail-closed, the `subscription_state='active'` gate enforced in three places,
atomic check-and-accrue for the `$`-ceiling, and clean runners-vs-cache routing), and the residual
findings are MED/LOW correctness and robustness gaps, not money-loss defects. It complements the
[billing/quota check flow](/flows/billing-quota-check.md), the
[per-tenant $-ceiling](/tenancy/dollar-ceiling.md), and [request quota](/tenancy/request-quota.md).

# Role

It is the launch gate for billing integrity: the documented set of invariants that protect revenue
and entitlement, plus the asymmetries between the two tier writers (signup-worker authoritative,
container materializer defense-in-depth) that an operator must understand before trusting either.

# How it works

- The review's verdict: the core money-integrity invariants hold and no CRITICAL money-loss or
  wrong-grant defect was found; the findings are MED/LOW correctness/robustness gaps
  (`docs/security/2026-06-23-review-money-path.md:6-26`).
- HMAC signature verification precedes JSON parse: a bad/replayed/future-dated signature 401s before
  any envelope parse, with a 5-minute replay tolerance and a constant-time compare
  (`docs/security/2026-06-23-review-money-path.md:137-146`).
- Idempotency is ordered correctly: the dedup row commits before materialize, a duplicate
  short-circuits to 200 with no dispatch, and insert-vs-replay is detected atomically — no
  double-dispatch (`docs/security/2026-06-23-review-money-path.md:137-153`).
- The `subscription_state='active'` gate (no paid tier for unpaid) is enforced consistently in three
  places: the Worker read path, the container materializer write path, and the OCI cap resolver
  (`docs/security/2026-06-23-review-money-path.md:154-162`).
- Runners-vs-cache routing never cross-grants: a runners price seeds the runners entitlement and
  suppresses the cache reconcile, a cache price never touches the runners table, and the two env
  price tables are disjoint by construction (`docs/security/2026-06-23-review-money-path.md:163-172`).
- The `$`-ceiling and byte-accounting TOCTOU windows are closed via atomic single-statement D1
  check-and-accrue (steady-state, cycle-roll, and brand-new-tenant first op all covered)
  (`docs/security/2026-06-23-review-money-path.md:173-182`).
- F-018 billing scope gate: every billing/PII customer surface requires `requires_cache_write`, so a
  read-only `cas:r` token gets 403 (`docs/security/2026-06-23-review-money-path.md:183-188`).

# Invariants

- A signature is verified before the body is parsed; an unpaid/non-active subscription never reaches
  the paid-tier upsert on any of the three enforcement points (`docs/security/2026-06-23-review-money-path.md:154-162`).
- The `$`-ceiling and byte accounting use reserve-before-PUT atomic check-and-accrue that fails
  CLOSED on indeterminate/transport errors — never serves for free (`docs/security/2026-06-23-review-money-path.md:173-182`).
- Audit-before-write is fail-closed: every state mutation emits the billing audit before the D1
  write and returns Transient if the audit fails, so there is no orphan state
  (`docs/security/2026-06-23-review-money-path.md:151-153`).

# Gotchas

- The top launch-relevant gap (F-MP-1, MED): the `team` tier price has no container-side tier
  mapping, so a real `team` `customer.subscription.updated` 422s in the container materializer and
  the entitlement reconcile is silently dropped (no DLQ; Stripe stops retrying). NOT a live
  money-loss because the signup-worker is the authoritative tier writer and DOES map `team` — but it
  is the "two writers must agree" intent violated (`docs/security/2026-06-23-review-money-path.md:30-59`).
- **F-MP-2 — RESOLVED in code (fix commit `e423ed23`, predates this checkpoint).** The review noted
  that `InvalidPayload` 422s on the container path were not DLQ'd (only `Transient` was), so a
  config-drift 422 from a real-but-unknown price-id was operationally indistinguishable from a
  malformed event and was permanently, silently dropped (`docs/security/2026-06-23-review-money-path.md:61-76`).
  This is now FIXED: the `InvalidPayload` dispatch arm calls `quarantine_transient(...)` before
  returning 422, so the event lands in the DLQ (operator-replayable, depth/age-alertable) rather than
  being silently lost (`crates/corelink-stripe-real/src/webhook_dispatch.rs:740-765`). The `Err(_)`
  catch-all arm quarantines too (`crates/corelink-stripe-real/src/webhook_dispatch.rs:770-774`).
- **F-MP-3 — RESOLVED in code (fix commit `e423ed23`, predates this checkpoint).** The review noted
  that the container resolvers read the legacy `data.object.plan.id` only, so a pinned Stripe API
  version that nests the price under `items.data[].price.id` would 422 every container reconcile
  (`docs/security/2026-06-23-review-money-path.md:78-97`). This is now FIXED: `extract_plan_id`
  prefers the legacy `plan.id` and **falls back to the modern `items.data[0].price.id`**
  (`crates/corelink-billing-stripe-materializer/src/handler.rs:150-163`), and it is the shared
  extractor for both the cache-tier and Runners reconciles, so an API-version shape change no longer
  drops the event.
- The `$`-ceiling charges a flat op cost on CAS reads that 404/410 (over-charge of the tenant, never
  under-charge) — acceptable-by-design coarse metering, worth a doc note (F-MP-4)
  (`docs/security/2026-06-23-review-money-path.md:99-111`).

# Citations

1. `docs/security/2026-06-23-review-money-path.md:6-26` — verdict: core invariants hold, no CRITICAL defect.
2. `docs/security/2026-06-23-review-money-path.md:30-59` — F-MP-1: `team` price has no container-side mapping → 422 + silent drop.
3. `docs/security/2026-06-23-review-money-path.md:61-76` — F-MP-2 (as reviewed): `InvalidPayload` not DLQ'd → unrecoverable on the container path. **RESOLVED →** `crates/corelink-stripe-real/src/webhook_dispatch.rs:740-765` — the `InvalidPayload` arm now `quarantine_transient(...)`-DLQs the event.
4. `docs/security/2026-06-23-review-money-path.md:78-97` — F-MP-3 (as reviewed): legacy `plan.id` vs modern `items.data[].price.id`. **RESOLVED →** `crates/corelink-billing-stripe-materializer/src/handler.rs:150-163` — `extract_plan_id` reads `plan.id` WITH an `items.data[0].price.id` fallback.
5. `docs/security/2026-06-23-review-money-path.md:99-111` — F-MP-4: `$`-ceiling charges flat op cost on 404/410 reads.
6. `docs/security/2026-06-23-review-money-path.md:137-153` — verified clean: HMAC-before-parse + idempotency ordering + audit-before-write.
7. `docs/security/2026-06-23-review-money-path.md:154-162` — the `subscription_state='active'` triple gate.
8. `docs/security/2026-06-23-review-money-path.md:163-182` — runners-vs-cache routing + `$`-ceiling/byte-accounting TOCTOU closed.
9. `docs/security/2026-06-23-review-money-path.md:183-188` — F-018: billing/PII surfaces require `requires_cache_write`.

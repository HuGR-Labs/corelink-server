---
type: "SecurityControl"
title: "Money-path security review"
description: "The go-live review of the Stripe webhook → materializer → D1 → quota/$-ceiling chain: which money-integrity invariants hold, and the MED/LOW robustness gaps that remain."
source_files:
  - "docs/security/2026-06-23-review-money-path.md"
  - "worker/src/lib/quota.ts"
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-container/src/oci_cap.rs"
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/routes/customer.rs"
checkpoint_sha: "e3ab218a549a161083a52327b34c6a04d524f179"
provenance: "AUTHORED"
tags: ["security", "billing", "stripe", "money-path", "quota"]
timestamp: "2026-06-26T00:00:00Z"
---

# Money-path security review

The money path is the chain from a signed Stripe webhook through the materializer into D1 and the
quota / `$`-ceiling enforcement — the thing that must never serve a paid tier without payment, never
double-grant, and never lose billing state. This concept records the 2026-06-23 go-live review: the
core money-integrity invariants hold (HMAC-verify-before-parse, idempotency-before-materialize,
audit-before-write fail-closed, the `subscription_state='active'` active-subscription gate enforced in three places,
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
  places, each with the same `... AND subscription_state = 'active'` D1 filter: the Worker read path
  (`worker/src/lib/quota.ts:152`), the container materializer / tier resolver
  (`crates/corelink-container/src/customer_d1.rs:799`), and the OCI cap resolver
  (`crates/corelink-container/src/oci_cap.rs:131`) (`docs/security/2026-06-23-review-money-path.md:154-162`).
- Runners-vs-cache routing never cross-grants: a runners price seeds the runners entitlement and
  suppresses the cache reconcile, a cache price never touches the runners table, and the two env
  price tables are disjoint by construction (`docs/security/2026-06-23-review-money-path.md:163-172`).
- The `$`-ceiling and byte-accounting TOCTOU windows are closed via atomic single-statement D1
  check-and-accrue (steady-state, cycle-roll, and brand-new-tenant first op all covered): the
  executed enforcer is `check_and_accrue` (`crates/corelink-container/src/tenant_quota.rs:658`),
  whose `Ok(false)` (over-ceiling) arm rejects with `402 Payment Required`
  (`crates/corelink-container/src/tenant_quota.rs:795-799`) and which fails CLOSED on an
  indeterminate/transport error (`docs/security/2026-06-23-review-money-path.md:173-182`).
- F-018 billing scope gate: every billing/PII customer surface requires `requires_cache_write`, so a
  read-only `cas:r` token gets 403 — the executed gate is `if !crate::scope::requires_cache_write(...)
  { 403 }` repeated on each customer-credential handler
  (`crates/corelink-container/src/routes/customer.rs:675`, and siblings at `:733`/`:784`/`:865`/`:914`)
  (`docs/security/2026-06-23-review-money-path.md:183-188`).

# Invariants

- A signature is verified before the body is parsed; an unpaid/non-active subscription never reaches
  the paid-tier upsert on any of the three enforcement points — the identical `subscription_state =
  'active'` filter at `worker/src/lib/quota.ts:152`, `crates/corelink-container/src/customer_d1.rs:799`,
  and `crates/corelink-container/src/oci_cap.rs:131` (`docs/security/2026-06-23-review-money-path.md:154-162`).
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
- `UnknownPlan`/`InvalidPayload` 422s on the container path are not DLQ'd (only `Transient` is), so a
  config-drift 422 from a real-but-unknown price-id is operationally indistinguishable from a
  malformed event and is permanently, silently dropped (F-MP-2) (`docs/security/2026-06-23-review-money-path.md:61-76`).
- The container resolvers read the legacy `data.object.plan.id` only; if the pinned Stripe API
  version nests the price under `items.data[].price.id`, every container reconcile 422s (F-MP-3) —
  verify the live event shape (`docs/security/2026-06-23-review-money-path.md:78-97`).
- The `$`-ceiling charges a flat op cost on CAS reads that 404/410 (over-charge of the tenant, never
  under-charge) — acceptable-by-design coarse metering, worth a doc note (F-MP-4)
  (`docs/security/2026-06-23-review-money-path.md:99-111`).

# Citations

1. `docs/security/2026-06-23-review-money-path.md:6-26` — verdict: core invariants hold, no CRITICAL defect.
2. `docs/security/2026-06-23-review-money-path.md:30-59` — F-MP-1: `team` price has no container-side mapping → 422 + silent drop.
3. `docs/security/2026-06-23-review-money-path.md:61-76` — F-MP-2: `InvalidPayload` not DLQ'd → unrecoverable on the container path.
4. `docs/security/2026-06-23-review-money-path.md:78-97` — F-MP-3: legacy `plan.id` vs modern `items.data[].price.id`.
5. `docs/security/2026-06-23-review-money-path.md:99-111` — F-MP-4: `$`-ceiling charges flat op cost on 404/410 reads.
6. `docs/security/2026-06-23-review-money-path.md:137-153` — verified clean: HMAC-before-parse + idempotency ordering + audit-before-write.
7. `docs/security/2026-06-23-review-money-path.md:154-162` — the `subscription_state='active'` triple gate.
8. `docs/security/2026-06-23-review-money-path.md:163-182` — runners-vs-cache routing + `$`-ceiling/byte-accounting TOCTOU closed.
9. `docs/security/2026-06-23-review-money-path.md:183-188` — F-018: billing/PII surfaces require `requires_cache_write`.
10. `worker/src/lib/quota.ts:152` — Worker read path: `SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active'` (no paid tier for a `pending_checkout` row).
11. `crates/corelink-container/src/customer_d1.rs:799` — container tier resolver: the same `AND subscription_state = 'active'` filter (mirrors the Worker; defense-in-depth second writer).
12. `crates/corelink-container/src/oci_cap.rs:131` — OCI cap resolver: the same active-only filter so a `pending_checkout` row yields no paid OCI quota.
13. `crates/corelink-container/src/tenant_quota.rs:658` / `:795-799` — the executed atomic `$`-ceiling: `check_and_accrue` returns `Ok(false)` over-ceiling, which the middleware maps to `402 Payment Required` (fail-CLOSED on indeterminate/transport error).
14. `crates/corelink-container/src/routes/customer.rs:675` (and `:733`/`:784`/`:865`/`:914`) — the executed F-018 billing/PII scope gate: `if !requires_cache_write(scope) { 403 }` on each customer-credential handler.

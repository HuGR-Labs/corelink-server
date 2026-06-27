---
type: "CrateCluster"
title: "Billing/commerce crate cluster"
description: "The money path — usage aggregation/emit, the idempotent Stripe adapter + webhook verification, and the DPA-gated tier-selection checkout orchestrator."
source_files:
  - "crates/corelink-billing/src/lib.rs"
  - "crates/corelink-billing-stripe/src/signature.rs"
  - "crates/corelink-billing-stripe/src/idempotency.rs"
  - "crates/corelink-tier-selection/src/lib.rs"
  - "crates/corelink-tier-selection/src/ledger.rs"
  - "crates/corelink-tier-selection/src/dpa.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["crates", "billing", "stripe", "tier", "money-path", "commerce"]
timestamp: "2026-06-26T00:00:00Z"
---

# Billing/commerce crate cluster

This is the cluster CoreLink's revenue runs through, so it is the one where a double-charge, a forged webhook, or a tier granted without payment is a launch-blocking bug rather than a defect. It is grouped around two non-negotiable properties: charges are idempotent (the same usage aggregate yields one Stripe charge no matter the retry storm) and state transitions are audited fail-closed (the audit envelope is written before any state mutation). `corelink-billing` is the aggregator over the 14 billing primitives; `corelink-billing-stripe` owns the idempotency-key derivation and webhook signature verify; `corelink-tier-selection` is the checkout orchestrator that gates every subscription behind DPA acceptance.

# Role

The cluster powers the [money-path checkout + billing ingest](/launch/money-path.md) and the [tier model](/launch/tier-model.md). It converts metered cache usage into Stripe usage records, ingests Stripe webhooks back into tenant subscription state, and is the gate that turns a signup into a paying tenant — only after the DPA is accepted.

# How it works

- `corelink-billing` is an Option-A aggregator re-exporting the 14 billing primitives (aggregator, emit, reconcile, replay, stripe, materializer, tier, quota, rate-headers, ratelimit, abuse) at canonical `corelink_billing::*` submodule paths (`crates/corelink-billing/src/lib.rs:1-20`, `crates/corelink-billing/src/lib.rs:32-63`).
- `corelink-billing-stripe` derives the canonical `Idempotency-Key = BLAKE3-256(JCS(aggregate))` in `derive_idempotency_key` so the same aggregate always maps to one Stripe charge regardless of retries (`crates/corelink-billing-stripe/src/idempotency.rs:80-97`).
- It verifies inbound webhooks in `verify_stripe_signature`: HMAC-SHA256 over the `Stripe-Signature: t=…,v1=…` header, a constant-time `subtle::ConstantTimeEq` compare against every `v1=` candidate (key-rotation tolerant), then a 5-minute replay window (`crates/corelink-billing-stripe/src/signature.rs:198-220`).
- `corelink-tier-selection`'s `select_tier` runs the checkout pipeline in fixed order — audit `TierSelectAttempted` BEFORE any mutation → `DpaAcceptanceGate::is_accepted` (returns `TierError::DpaRequired` before any Stripe call) → Enterprise-route reject → `INSERT OR IGNORE` D1 lock + UNIQUE-active check → Free-activate / Stripe-Checkout dispatch (`crates/corelink-tier-selection/src/ledger.rs:191-260`).

# Invariants

- `INV-BILLING-NO-DUP`: one canonical idempotency key per aggregate → a single Stripe charge under retry storms, pinned by 10k-iter property tests (`crates/corelink-billing-stripe/src/idempotency.rs:80-97`).
- `INV-ONBOARD-DPA-FIRST`: `select_tier` checks `dpa_gate.is_accepted` (fail-CLOSED — any storage transient → deny) and returns `TierError::DpaRequired` BEFORE any Stripe API call, for ALL tiers including Free (`crates/corelink-tier-selection/src/ledger.rs:206-221`).
- `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`: `select_tier` calls `audit_emit_or_abort(TierSelectAttempted)` as step 1, before any state mutation or decision branch; an emit failure `?`-propagates and aborts the call (`crates/corelink-tier-selection/src/ledger.rs:197-204`).
- A per-tenant D1 row lock (`INSERT OR IGNORE` on `tier_selection_locks`, 60s window) plus a UNIQUE-active check inside one critical section prevents concurrent tier switches; a contended caller gets `TierError::LockHeld` (`crates/corelink-tier-selection/src/ledger.rs:236-260`).

# Gotchas

- The tier taxonomy is a 6-arm `#[non_exhaustive]` enum (`Free`/`Solo`/`Starter`/`Pro`/`Max`/`Enterprise`); `Enterprise` routes to the inquiry form (`TierError::UseInquiryForm`), it is not a self-serve checkout.
- These crates ship the pure-logic skeleton + in-memory orchestrators; the production wiring binds the real HTTPS Stripe client and the CF Worker routes (`POST /v1/onboarding/tier-select`, `POST /v1/billing/stripe-webhook`) (`crates/corelink-tier-selection/src/lib.rs:1-21`). A historical launch bug served paid tiers on `pending_checkout` rows — the live fix filters on `subscription_state='active'`, which lives in the worker quota path, not these crates.
- `corelink-stripe-real` carries a wallet-broker dual-mode that production-wires Stripe credentials; the aggregator keeps it in place by reference to avoid regressing the fallback path.

# Citations

1. `crates/corelink-billing/src/lib.rs:1-20` — the single-import aggregator over the 14 billing primitives.
2. `crates/corelink-billing/src/lib.rs:32-63` — the absorbed-crate list (aggregator/emit/reconcile/stripe/tier/quota/ratelimit/abuse).
3. `crates/corelink-billing-stripe/src/idempotency.rs:80-97` — `derive_idempotency_key` = BLAKE3-256(JCS(aggregate)) (`INV-BILLING-NO-DUP`).
4. `crates/corelink-billing-stripe/src/signature.rs:198-220` — `verify_stripe_signature`: webhook HMAC-SHA256, constant-time multi-candidate compare, 5-min replay window.
5. `crates/corelink-tier-selection/src/dpa.rs:14-21` — `DpaAcceptanceGate` trait contract: fail-CLOSED `is_accepted` that `select_tier` consults.
6. `crates/corelink-tier-selection/src/ledger.rs:191-260` — `select_tier`: the full DPA-gated checkout orchestrator (audit-first → DPA → enterprise-route → D1 lock + UNIQUE → Free/Stripe dispatch).
7. `crates/corelink-tier-selection/src/ledger.rs:197-221` — `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (audit step 1) then `INV-ONBOARD-DPA-FIRST` (`is_accepted` → `DpaRequired` before any Stripe call).
8. `crates/corelink-tier-selection/src/ledger.rs:236-260` — the 60s `tier_selection_locks` row lock (`INSERT OR IGNORE`) + UNIQUE-active check inside one critical section.

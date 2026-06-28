---
type: "CrateCluster"
title: "Billing/commerce crate cluster"
description: "The money path — usage aggregation/emit, the idempotent Stripe adapter + webhook verification, and the DPA-gated tier-selection checkout orchestrator."
source_files:
  - "crates/corelink-billing/src/lib.rs"
  - "crates/corelink-billing-stripe/src/lib.rs"
  - "crates/corelink-billing-stripe/src/idempotency.rs"
  - "crates/corelink-billing-stripe/src/signature.rs"
  - "crates/corelink-tier-selection/src/lib.rs"
  - "crates/corelink-tier-selection/src/dpa.rs"
  - "crates/corelink-tier-selection/src/ledger.rs"
checkpoint_sha: "cc51893253fa3a86ae5b02bff56c8022cbeb72b5"
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
- `corelink-billing-stripe` derives the canonical `Idempotency-Key = BLAKE3-256(JCS(aggregate))` so the same aggregate always maps to one Stripe charge regardless of retries — the executed `derive_idempotency_key` (`crates/corelink-billing-stripe/src/idempotency.rs:80-92`).
- It verifies inbound webhooks with HMAC-SHA256 over the `Stripe-Signature: t=…,v1=…` header plus a 5-minute replay window, using a constant-time compare via `subtle::ConstantTimeEq` — the executed `verify_stripe_signature` (`crates/corelink-billing-stripe/src/signature.rs:198-230`), replay window `REPLAY_WINDOW_MS` (`crates/corelink-billing-stripe/src/signature.rs:53`).
- `corelink-tier-selection` runs the checkout pipeline audit → DPA gate → Enterprise route → D1 row-lock, returning `TierError::DpaRequired` before any Stripe call if the DPA is not accepted — the executed `select_tier` (`crates/corelink-tier-selection/src/ledger.rs:191-244`), the DPA gate it consults (`crates/corelink-tier-selection/src/dpa.rs:18-20`).

# Invariants

- `INV-BILLING-NO-DUP`: one canonical idempotency key per aggregate → a single Stripe charge under retry storms, pinned by property tests (`crates/corelink-billing-stripe/src/idempotency.rs:80-92`).
- `INV-ONBOARD-DPA-FIRST`: `select_tier` calls the DPA gate BEFORE any Stripe API call, for ALL tiers including Free; a non-accepted DPA returns `TierError::DpaRequired` fail-closed (`crates/corelink-tier-selection/src/ledger.rs:206-220`).
- `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`: the audit envelope is emitted before state mutation on every decision arm; an audit failure aborts the mutation (`crates/corelink-tier-selection/src/ledger.rs:196-204`).
- A per-tenant D1 row lock (`INSERT OR IGNORE` on `tier_selection_locks`, 60s window) prevents concurrent tier switches; a contended caller gets `TierError::LockHeld` (`crates/corelink-tier-selection/src/ledger.rs:38`, `crates/corelink-tier-selection/src/ledger.rs:236-244`).

# Gotchas

- The tier taxonomy is a 6-arm `#[non_exhaustive]` enum (`Free`/`Solo`/`Starter`/`Pro`/`Max`/`Enterprise`); `Enterprise` routes to the inquiry form (`TierError::UseInquiryForm`), it is not a self-serve checkout.
- These crates ship the pure-logic skeleton + in-memory orchestrators; the production wiring binds the real HTTPS Stripe client and the CF Worker routes (`POST /v1/onboarding/tier-select`, `POST /v1/billing/stripe-webhook`). A historical launch bug served paid tiers on `pending_checkout` rows — the live fix filters on `subscription_state='active'`, which lives in the worker quota path, not these crates.
- `corelink-stripe-real` carries a wallet-broker dual-mode that production-wires Stripe credentials; the aggregator keeps it in place by reference to avoid regressing the fallback path.

# Citations

1. `crates/corelink-billing/src/lib.rs:1-20` — the single-import aggregator over the 14 billing primitives.
2. `crates/corelink-billing/src/lib.rs:32-63` — the absorbed-crate list (aggregator/emit/reconcile/stripe/tier/quota/ratelimit/abuse).
2b. `crates/corelink-billing-stripe/src/lib.rs:166-171` — the `corelink-billing-stripe` crate `pub mod` map (idempotency/signature/ledger/event/adapter).
2c. `crates/corelink-tier-selection/src/lib.rs:95-100` — the `corelink-tier-selection` crate `pub mod` map (dpa/ledger/stripe/tenant/audit).
3. `crates/corelink-billing-stripe/src/idempotency.rs:80-92` — `derive_idempotency_key`: `Idempotency-Key = BLAKE3-256(JCS(aggregate))` derivation (`INV-BILLING-NO-DUP`).
4. `crates/corelink-billing-stripe/src/signature.rs:198-230` — `verify_stripe_signature`: webhook HMAC-SHA256 verify, constant-time `ct_eq`, 5-min replay window.
5. `crates/corelink-billing-stripe/src/signature.rs:53` — `REPLAY_WINDOW_MS = 300_000` (the canonical 5-minute skew window).
6. `crates/corelink-tier-selection/src/ledger.rs:191-244` — `select_tier`: the executed audit → DPA-gate → Enterprise-route → D1 row-lock checkout orchestrator.
7. `crates/corelink-tier-selection/src/ledger.rs:206-220` — `INV-ONBOARD-DPA-FIRST` (DPA gate consulted before Stripe, all tiers → `DpaRequired`).
8. `crates/corelink-tier-selection/src/dpa.rs:18-20` — the `DpaAcceptanceGate::is_accepted` trait the checkout consults.
9. `crates/corelink-tier-selection/src/ledger.rs:38` + `:236-244` — the 60s `tier_selection_locks` window const + `INSERT OR IGNORE` row-lock acquire (`TierError::LockHeld`).

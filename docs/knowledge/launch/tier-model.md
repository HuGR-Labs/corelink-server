---
type: "LaunchControl"
title: "The 6-tier model & DPA-first tier-select checkout"
description: "How a tenant selects a tier, passes the DPA-first gate, and is routed to instant activation, Stripe Checkout, or the Enterprise inquiry form."
source_files:
  - "crates/corelink-container/src/routes/tier_select.rs"
  - "crates/corelink-container/src/routes/tier_select_store.rs"
  - "crates/corelink-container/src/routes/tier_select_audit.rs"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["launch", "tier", "checkout", "stripe", "onboarding", "money-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# The 6-tier model & DPA-first tier-select checkout

The tier model is the launch money-path domain: a self-serve SMB picks one of six tiers, and the
`/v1/onboarding/tier-select` route turns that pick into either an instant free activation, a Stripe
Checkout redirect, or an Enterprise "contact us" route — but never before the tenant has accepted the
current DPA. The taxonomy itself (Free / Solo / Starter / Pro / Max / Enterprise) is governed by an
amendment ADR, not by the sealed spec it supersedes, and the route enforces the INV-ONBOARD-DPA-FIRST
invariant in D1 before any state is mutated or any Stripe object is created. This is the concept that
checkout, billing, and the pricing page all transcribe. Related: [ADR-S19-001](/adr/adr-s19-001-tier-taxonomy-amendment-5-to-6.md),
[the per-tenant dollar ceiling](/tenancy/dollar-ceiling.md), and [the billing/quota check flow](/flows/billing-quota-check.md).

# Role

It is the customer-facing entry point of the launch revenue path: it converts an authenticated tenant
plus a chosen tier into a durable subscription intent and a checkout decision, while guaranteeing the
legal (DPA) precondition and a fail-closed audit trail.

# How it works

- The taxonomy is the 6-tier ladder Free/Solo/Starter/Pro/Max/Enterprise with snake_case wire strings, frozen by the amendment ADR at `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:72-84`.
- The route parses the requested tier into a `RequestedTier`/`ParsedTier` decision (self-serve vs inquiry) via `crates/corelink-container/src/routes/tier_select.rs:192`.
- `handle` is the axum entrypoint that drives the orchestration with the route state and verified tenant header `crates/corelink-container/src/routes/tier_select.rs:477`.
- `orchestrate_tier_select` fixes the ordering audit → lock → DPA-first → active-sub → checkout → persist → release `crates/corelink-container/src/routes/tier_select.rs:717`.
- The DPA-first check is a durable D1-over-HTTP `SELECT 1 FROM dpa_acceptances` for the current version, fail-CLOSED on transport error `crates/corelink-container/src/routes/tier_select_store.rs:173-181`.
- A paid selection persists the subscription intent as `pending_checkout` with the mapped `stripe_customer_id` via an upsert `INSERT INTO tier_selections` `crates/corelink-container/src/routes/tier_select_store.rs:214`.
- The fail-CLOSED audit seam emits a `tracing` audit log before the state mutation via `TierSelectAudit::emit` (`crates/corelink-container/src/routes/tier_select_audit.rs:75`) — today a `tracing::info!` that always returns `Ok` (the durable D1 audit-chain write is the tracked Wave-37 hardening). The emit-Err-aborts-before-mutation logic lives in `crates/corelink-container/src/routes/tier_select.rs:735-738` (vacuous today since the adapter never returns `Err`).
- The decision rules (paid set = Solo/Starter/Pro/Max, Free instant, Enterprise inquiry, DPA-first all tiers) are the ADR's decision section `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127`.

# Invariants

- DPA not accepted ⇒ `403 dpa_required` (INV-ONBOARD-DPA-FIRST) at `crates/corelink-container/src/routes/tier_select.rs:291`.
- A direct paid Checkout for Enterprise is rejected `422 use_inquiry_form` — the UI hint alone is bypassable `crates/corelink-container/src/routes/tier_select.rs:290`.
- The route is only mounted when the Stripe config AND `CORELINK_DPA_VERSION` are present; a missing DPA version means the endpoint is NOT mounted `crates/corelink-container/src/routes/tier_select.rs:551`.
- The DPA gate is fail-CLOSED: a transport error never reads as "accepted" `crates/corelink-container/src/routes/tier_select_store.rs:173-181`.
- Audit emit returning `Err` ABORTS the whole operation before mutation (audit-before-mutation) — the abort logic lives in `crates/corelink-container/src/routes/tier_select.rs:735-738`, vacuous today since the `tracing`-only adapter (`crates/corelink-container/src/routes/tier_select_audit.rs:75`) never returns `Err`.
- Supersession is scoped to tier cardinality/membership only — no invariant, security control, or latency/cost gate changes `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144`.

# Gotchas

- Prices ($15/$35/$50/$149) are deliberately NOT in the `TierKind` enum — pricing is a billing concern and lives in the pricing page + Stripe script env vars `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126`.
- `team` is removed from the visible taxonomy but retained in the persisted CHECK domain for back-compat; new flows never mint it `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177`.

# Citations

1. `crates/corelink-container/src/routes/tier_select.rs:192` — self-serve vs inquiry tier parsing.
2. `crates/corelink-container/src/routes/tier_select.rs:477` — the `handle` axum entrypoint.
3. `crates/corelink-container/src/routes/tier_select.rs:717` — `orchestrate_tier_select` fixed ordering.
4. `crates/corelink-container/src/routes/tier_select.rs:290` — Enterprise direct-paid → 422 `use_inquiry_form`.
5. `crates/corelink-container/src/routes/tier_select.rs:291` — DPA-not-accepted → 403 `dpa_required`.
6. `crates/corelink-container/src/routes/tier_select.rs:551` — mount-only-with-DPA-version build_state guard.
7. `crates/corelink-container/src/routes/tier_select_store.rs:173-181` — the fail-CLOSED DPA-first D1 check.
8. `crates/corelink-container/src/routes/tier_select_store.rs:214` — persist `pending_checkout` upsert.
9. `crates/corelink-container/src/routes/tier_select_audit.rs:75` — `tracing`-only audit emit (always `Ok` today; durable D1 chain is Wave-37).
9b. `crates/corelink-container/src/routes/tier_select.rs:735-738` — emit-Err-aborts-before-mutation orchestration (vacuous today).
10. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:72-84` — the 6-tier ladder + wire strings.
11. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127` — the decision (paid set, DPA-first all tiers, Stripe map).
12. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126` — prices out of the enum.
13. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144` — precise scope of supersession.
14. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177` — `team` retained additively in the persisted domain.

---
type: "LaunchControl"
title: "The tier model (6 cache + 5 runner) & DPA-first tier-select checkout"
description: "How a tenant selects a tier — across the six cache tiers and the five runner SKUs (a separate entitlement axis) — passes the DPA-first gate, and is routed to instant activation, Stripe Checkout, or the Enterprise inquiry form."
source_files:
  - "crates/corelink-container/src/routes/tier_select/part-01.rs"
  - "crates/corelink-container/src/routes/tier_select_store.rs"
  - "crates/corelink-container/src/routes/tier_select_audit.rs"
  - "crates/corelink-container/src/routes/dpa_accept.rs"
  - "crates/corelink-container/src/routes/dpa_accept_store.rs"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
  - "worker/src/durable_object_start.ts"
  - "crates/corelink-container/src/routes/tier_select/part-00-00.rs"
  - "crates/corelink-container/src/routes/tier_select/part-00-01.rs"
  - "worker/src/index_auth_policy.ts"
source_blobs:
  - "crates/corelink-container/src/routes/tier_select/part-01.rs@6d44de1f00b96dd34f0c5f51ae07789782856e60"
  - "crates/corelink-container/src/routes/tier_select_store.rs@45b845bc6af612464d81673a7e1cd310dabc2ec4"
  - "crates/corelink-container/src/routes/tier_select_audit.rs@7b8547fd8a4e43bb15cb0418a18a0a929faeeede"
  - "crates/corelink-container/src/routes/dpa_accept.rs@cdaa9f5fc06070a64d64ddb8e44ed3b80bae6d1e"
  - "crates/corelink-container/src/routes/dpa_accept_store.rs@ece87c2ec8bfb8f37cef2d0458880f2c09bc3e76"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md@c62746b9f202e0a90444455d4f184d858b86c5dc"
  - "worker/src/durable_object_start.ts@fa1a69c19f1a3201120a25de3827e94674070754"
  - "crates/corelink-container/src/routes/tier_select/part-00-00.rs@77ff762104a18f54b8072cb90a27aba07dfbb872"
  - "crates/corelink-container/src/routes/tier_select/part-00-01.rs@db07583505dc86f48fed9a6940d79e44dcb9a21a"
  - "worker/src/index_auth_policy.ts@545af17a21b5b3fe2fd19e0c2516782ae6758a7d"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["launch", "tier", "checkout", "stripe", "onboarding", "money-path"]
timestamp: "2026-09-06T00:00:00Z"

---
# The tier model (6 cache + 5 runner) & DPA-first tier-select checkout

The tier model is the launch money-path domain: a self-serve SMB picks a tier, and the
`/v1/onboarding/tier-select` route turns that pick into either an instant free activation, a Stripe
Checkout redirect, or an Enterprise "contact us" route — but never before the tenant has accepted the
current DPA. The route now accepts **eleven** self-serve tiers across **two independent product axes**:
the six-tier CACHE taxonomy (Free / Solo / Starter / Pro / Max / Enterprise, governed by amendment
ADR-S19-001), plus five RUNNER SKUs (`runner_starter` / `runner_pro` / `runner_team` / `runner_scale`
/ `runner_max`) added by the self-serve-runner campaign. Runner is a SEPARATE entitlement axis: a runner
purchase's subscription↔tenant mapping and entitlement are written by the signup-worker Stripe webhook
into `runner_billing` / `runners_entitlement` (migrations 0087 / 0070), NEVER the cache
`tier_selections` / `stripe_checkout_sessions` tables — whose one-row-per-tenant shape + cache-only
`tier` CHECK a runner write would clobber and violate. A tenant may therefore hold a cache tier AND a
runner tier simultaneously, and the orchestration guards each axis independently (the "at most one
active subscription" rule is per-axis, not global). The route enforces the INV-ONBOARD-DPA-FIRST
invariant in D1 before any state is mutated or any Stripe object is created. This is the concept that
checkout, billing, and the pricing page all transcribe. Related: [ADR-S19-001](/adr/adr-s19-001-tier-taxonomy-amendment-5-to-6.md),
[the per-tenant dollar ceiling](/tenancy/dollar-ceiling.md), and [the billing/quota check flow](/flows/billing-quota-check.md).

# Role

It is the customer-facing entry point of the launch revenue path: it converts an authenticated tenant
plus a chosen tier into a durable subscription intent and a checkout decision, while guaranteeing the
legal (DPA) precondition and a fail-closed audit trail.

# How it works

- The CACHE taxonomy is the 6-tier ladder Free/Solo/Starter/Pro/Max/Enterprise with snake_case wire strings, frozen by the amendment ADR at `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:72-84`. The self-serve-runner campaign adds five RUNNER SKUs on a SEPARATE axis (`runner_starter`/`runner_pro`/`runner_team`/`runner_scale`/`runner_max`) as additional `RequestedTier` variants; they are paid (→ Stripe Checkout) but never rejected as Invalid/Enterprise.
- The route parses the requested tier into a `RequestedTier`/`ParsedTier` decision (self-serve vs inquiry). `parse_self_serve` recognizes the four paid cache tiers, `free`, `enterprise`, AND the five `runner_*` SKUs (case-insensitive + trimmed) via `crates/corelink-container/src/routes/tier_select/part-00-00.rs:206-223`. Whether a parsed tier is on the runner axis is answered by `RequestedTier::is_runner` at `crates/corelink-container/src/routes/tier_select/part-00-00.rs:239-252`.
- `handle` is the axum entrypoint that drives the orchestration with the route state and verified tenant header `crates/corelink-container/src/routes/tier_select/part-00-01.rs:56-99`.
- `orchestrate_tier_select` emits the audit event, acquires the lock, and delegates the DPA, subscription, checkout, and persistence work to its locked helper (`crates/corelink-container/src/routes/tier_select/part-01.rs:29-82`, `crates/corelink-container/src/routes/tier_select/part-01.rs:106-205`).
- The DPA-first check is a durable D1-over-HTTP `SELECT 1 FROM dpa_acceptances` for the current version, fail-CLOSED on transport error `crates/corelink-container/src/routes/tier_select_store.rs:615-626`.
- That gate READS a row written by a SEPARATE writer: the DPA click-through endpoint `POST /v1/onboarding/dpa-accept` (SAME onboarding proxy contract — the Worker resolves the endpoint-specific authority and the DO forwards both dedicated names, then the container verifies `x-corelink-internal-auth` + `x-corelink-tenant-id`). Its `handle` entrypoint runs the fail-CLOSED auth gate then `orchestrate_dpa_accept`, which drives the real `corelink-dpa-acceptance` primitives (RS256 receipt, closed 3-locale enum, `sha256(ip‖salt)` hash) and idempotently (`dpa:{tenant}:{version}`) inserts the `dpa_acceptances` row (`crates/corelink-container/src/routes/dpa_accept.rs:528-528`, `crates/corelink-container/src/routes/dpa_accept.rs:397-397`) via the durable D1 `INSERT OR IGNORE` (`crates/corelink-container/src/routes/dpa_accept_store.rs:104`) — so until this route is mounted (gated on `DPA_RECEIPT_SIGNING_KEY`, fail-CLOSED at `crates/corelink-container/src/routes/dpa_accept.rs:608-608`) every paid checkout 403s `dpa_required`.
- The active-subscription guard is per-axis: runner requests query `has_active_runner_subscription`, cache requests query `has_active_subscription`, and an existing subscription returns `AlreadyActive` (`crates/corelink-container/src/routes/tier_select/part-01.rs:124-145`, `crates/corelink-container/src/routes/tier_select_store.rs:628-643`).
- The cache query excludes the free seed row, while the runner query scopes its status to paid subscription states; source code explains this prevents a free activation from blocking a first paid selection (`crates/corelink-container/src/routes/tier_select/part-01.rs:124-136`, `crates/corelink-container/src/routes/tier_select_store.rs:107-116`).
- A paid cache selection persists `pending_checkout`; a runner selection skips the cache reservation and persistence path (`crates/corelink-container/src/routes/tier_select/part-01.rs:147-205`, `crates/corelink-container/src/routes/tier_select_store.rs:809-863`).
- The orchestration emits its attempt audit record before the lock and maps an emit failure to an internal error (`crates/corelink-container/src/routes/tier_select/part-01.rs:46-50`).
- Both money-path routes resolve their internal-auth secret through the common helper at `crates/corelink-container/src/routes/tier_select/part-00-01.rs:123-132` and `crates/corelink-container/src/routes/dpa_accept.rs:406-406`; the Worker selects the matching dedicated key and the DO forwards it (`worker/src/index_auth_policy.ts:50-78`, `worker/src/durable_object_start.ts:96-97`). A valid dedicated key wins, while the resolver preserves the documented shared fallback and enforces the 32-character floor.
- The decision rules (cache paid set = Solo/Starter/Pro/Max, Free instant, Enterprise inquiry, DPA-first all tiers) are the ADR's decision section `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127`; the five runner SKUs are an additive campaign axis on top of that decision.

# Invariants

- DPA not accepted ⇒ `403 dpa_required`: the locked helper returns `DpaRequired` after the fail-closed store check, and the error mapping returns 403 (`crates/corelink-container/src/routes/tier_select/part-01.rs:106-116`, `crates/corelink-container/src/routes/tier_select/part-00-00.rs:339-340`).
- A direct paid Checkout for Enterprise is rejected `422 use_inquiry_form` — the UI hint alone is bypassable: the enforcement is `ParsedTier::Enterprise => return Err(...UseInquiryForm)` at `crates/corelink-container/src/routes/tier_select/part-00-01.rs:16-18` (the `422`/`use_inquiry_form` status mapping is the `parts()` arm at `crates/corelink-container/src/routes/tier_select/part-00-00.rs:339-340`).
- Runner requests skip the cache reservation and pending-checkout persistence branches (`crates/corelink-container/src/routes/tier_select/part-01.rs:147-205`).
- The per-tenant active-subscription guard chooses the runner or cache query by tier axis (`crates/corelink-container/src/routes/tier_select/part-01.rs:124-145`).
- The route is only mounted when the internal-auth resolver yields a key, the Stripe config, AND `CORELINK_DPA_VERSION` are present; a missing DPA version means the endpoint is NOT mounted — the actual `CORELINK_DPA_VERSION` unset-⇒-`None` guard is `crates/corelink-container/src/routes/tier_select/part-00-01.rs:240-244` (inside `build_state_from_env` at `:237`).
- Both routes fail CLOSED when neither the dedicated nor shared resolver input reaches the 32-character floor; valid dedicated values are distinct rotation authorities, and a valid shared key remains the documented migration fallback (`crates/corelink-container/src/routes/tier_select/part-00-01.rs:123-132`, `crates/corelink-container/src/routes/dpa_accept.rs:583-583`, Worker selection at `worker/src/index_auth_policy.ts:50-78`).
- The DPA gate is fail-CLOSED: a transport error never reads as "accepted" `crates/corelink-container/src/routes/tier_select_store.rs:615-626`.
- An audit emit failure maps to an internal error before the lock and subsequent state changes (`crates/corelink-container/src/routes/tier_select/part-01.rs:46-50`).
- Supersession is scoped to tier cardinality/membership only — no invariant, security control, or latency/cost gate changes `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144`.

# Gotchas

- Prices ($15/$35/$50/$149) are deliberately NOT in the `TierKind` enum — pricing is a billing concern and lives in the pricing page + Stripe script env vars `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126`.
- `team` is removed from the visible taxonomy but retained in the persisted CHECK domain for back-compat; new flows never mint it `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177`.

# Citations

1. `crates/corelink-container/src/routes/tier_select/part-00-00.rs:206-223` — `parse_self_serve`: self-serve vs inquiry tier parsing (cache tiers + `free`/`enterprise` + the five `runner_*` SKUs).
1b. `crates/corelink-container/src/routes/tier_select/part-00-00.rs:239-252` — `RequestedTier::is_runner`: the runner-axis discriminator + its clobber/CHECK rationale.
2. `crates/corelink-container/src/routes/tier_select/part-00-01.rs:56-99` — the `handle` axum entrypoint.
3. `crates/corelink-container/src/routes/tier_select/part-01.rs:29-90` — `orchestrate_tier_select` fixed ordering (per-axis active-sub guard + runner-skips-persist).
3b. `crates/corelink-container/src/routes/tier_select/part-01.rs:94` — per-axis single-active-subscription guard (`is_runner` selects `has_active_runner_subscription` vs `has_active_subscription`).
3c. `crates/corelink-container/src/routes/tier_select/part-01.rs:145` — runner checkout SKIPS the cache `persist_pending_checkout` (`!tier.is_runner()` guard).
4. `crates/corelink-container/src/routes/tier_select/part-00-01.rs:16-18` — Enterprise direct-paid → `return Err(...UseInquiryForm)` (the enforcement).
4b. `crates/corelink-container/src/routes/tier_select/part-00-00.rs:339-340` — `parts()` status-mapping arm: `UseInquiryForm` → 422 `use_inquiry_form`.
5. `crates/corelink-container/src/routes/tier_select/part-01.rs:78` — DPA-not-accepted → `return Err(TierSelectHttpError::DpaRequired)` (the enforcement, gated by the fail-closed store check).
5b. `crates/corelink-container/src/routes/tier_select/part-00-00.rs:339-340` — `parts()` status-mapping arm: `DpaRequired` → 403 `dpa_required`.
6. `crates/corelink-container/src/routes/tier_select/part-00-01.rs:240-244` — the `CORELINK_DPA_VERSION` unset-⇒-NOT-mounted guard (in `build_state_from_env` at `:237`).
7. `crates/corelink-container/src/routes/tier_select_store.rs:615-626` — the fail-CLOSED DPA-first D1 check.
7b. `crates/corelink-container/src/routes/tier_select_store.rs:628-643` — `has_active_runner_subscription`: the runner axis `runner_billing` active-status query (migration 0087), separate from the cache `has_active_subscription`.
8. `crates/corelink-container/src/routes/tier_select_store.rs:652-652` — persist `pending_checkout` upsert (cache axis only).
8b. `crates/corelink-container/src/routes/tier_select_store.rs:69-96` — `tier_column`: exhaustive over `RequestedTier` incl. the runner arms, but the runner arms are runtime-UNREACHABLE (orchestration never persists a runner tier here), so no CHECK-widening migration is needed.
9. `crates/corelink-container/src/routes/tier_select_audit.rs:110` — durable audit emit (D1 INSERT into `tier_select_audit_events`, migration 0092; `Err` on failure).
9b. `crates/corelink-container/src/routes/tier_select/part-01.rs:14-18` — emit-Err-aborts-before-mutation orchestration (now REAL: the adapter returns `Err` on durable-D1 failure).
10. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:72-84` — the 6-tier ladder + wire strings.
11. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127` — the decision (paid set, DPA-first all tiers, Stripe map).
12. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126` — prices out of the enum.
13. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144` — precise scope of supersession.
14. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177` — `team` retained additively in the persisted domain.
15. `crates/corelink-container/src/routes/dpa_accept.rs:516` — `handle`: the `POST /v1/onboarding/dpa-accept` writer entrypoint (fail-CLOSED auth gate → `orchestrate_dpa_accept` at `:385`; mount fail-CLOSES on the RS256 key at `:598-600`).
16. `crates/corelink-container/src/routes/dpa_accept_store.rs:104` — `insert_acceptance`: the durable D1 `INSERT OR IGNORE INTO dpa_acceptances` that the tier-select gate then reads.
17. `crates/corelink-container/src/routes/tier_select/part-00-01.rs:123-132` — tier-select resolves through the common internal-auth helper with `CORELINK_TIER_SELECT_AUTH_KEY`.
18. `crates/corelink-container/src/routes/dpa_accept.rs:571` — DPA acceptance resolves through the common internal-auth helper with `CORELINK_DPA_ACCEPT_AUTH_KEY`.
19. `worker/src/index_auth_policy.ts:50-78` — the Worker selects the endpoint-specific money authority and enforces the 32-character floor.
20. `worker/src/durable_object_start.ts:96-97` — the DO forwards both dedicated money authorities to the container.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.

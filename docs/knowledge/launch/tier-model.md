---
type: "LaunchControl"
title: "The tier model (6 cache + 5 runner) & DPA-first tier-select checkout"
description: "How a tenant selects a tier — across the six cache tiers and the five runner SKUs (a separate entitlement axis) — passes the DPA-first gate, and is routed to instant activation, Stripe Checkout, or the Enterprise inquiry form."
source_files:
  - "crates/corelink-container/src/routes/tier_select/part-00.rs"
  - "crates/corelink-container/src/routes/tier_select/part-01.rs"
  - "crates/corelink-container/src/routes/tier_select_store.rs"
  - "crates/corelink-container/src/routes/tier_select_audit.rs"
  - "crates/corelink-container/src/routes/dpa_accept.rs"
  - "crates/corelink-container/src/routes/dpa_accept_store.rs"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
  - "worker/src/durable_object_start.ts"
  - "worker/src/index_auth.ts"
source_blobs:
  - "crates/corelink-container/src/routes/tier_select/part-00.rs@d9e7d097853ac67fa4daf92d46a5a81c541abdf5"
  - "crates/corelink-container/src/routes/tier_select/part-01.rs@db275de662f70184877b3d5b99afa7d2f247a81b"
  - "crates/corelink-container/src/routes/tier_select_store.rs@7d9f19ade75280de6bad893fbc780fd50d54c2a5"
  - "crates/corelink-container/src/routes/tier_select_audit.rs@7b8547fd8a4e43bb15cb0418a18a0a929faeeede"
  - "crates/corelink-container/src/routes/dpa_accept.rs@1f30dd2b0e4406de28c2d5817b748add9c0598b3"
  - "crates/corelink-container/src/routes/dpa_accept_store.rs@ece87c2ec8bfb8f37cef2d0458880f2c09bc3e76"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md@c62746b9f202e0a90444455d4f184d858b86c5dc"
  - "worker/src/durable_object_start.ts@fb3e310dcba3169791ecfc0e6b8bd0c3a7f3c679"
  - "worker/src/index_auth.ts@8cb197bf3200722ceed11e71bcfb82c3a853529e"
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
- The route parses the requested tier into a `RequestedTier`/`ParsedTier` decision (self-serve vs inquiry). `parse_self_serve` recognizes the four paid cache tiers, `free`, `enterprise`, AND the five `runner_*` SKUs (case-insensitive + trimmed) via `crates/corelink-container/src/routes/tier_select/part-00.rs:203`. Whether a parsed tier is on the runner axis is answered by `RequestedTier::is_runner` at `crates/corelink-container/src/routes/tier_select/part-00.rs:236`.
- `handle` is the axum entrypoint that drives the orchestration with the route state and verified tenant header `crates/corelink-container/src/routes/tier_select/part-00.rs:526`.
- `orchestrate_tier_select` fixes the ordering audit → lock → DPA-first → active-sub → checkout → persist → release `crates/corelink-container/src/routes/tier_select/part-00.rs:783`.
- The DPA-first check is a durable D1-over-HTTP `SELECT 1 FROM dpa_acceptances` for the current version, fail-CLOSED on transport error `crates/corelink-container/src/routes/tier_select_store.rs:237-248`.
- That gate READS a row written by a SEPARATE writer: the DPA click-through endpoint `POST /v1/onboarding/dpa-accept` (SAME onboarding proxy contract — the Worker resolves the endpoint-specific authority and the DO forwards both dedicated names, then the container verifies `x-corelink-internal-auth` + `x-corelink-tenant-id`). Its `handle` entrypoint runs the fail-CLOSED auth gate then `orchestrate_dpa_accept`, which drives the real `corelink-dpa-acceptance` primitives (RS256 receipt, closed 3-locale enum, `sha256(ip‖salt)` hash) and idempotently (`dpa:{tenant}:{version}`) inserts the `dpa_acceptances` row (`crates/corelink-container/src/routes/dpa_accept.rs:528-528`, `crates/corelink-container/src/routes/dpa_accept.rs:397-397`) via the durable D1 `INSERT OR IGNORE` (`crates/corelink-container/src/routes/dpa_accept_store.rs:104`) — so until this route is mounted (gated on `DPA_RECEIPT_SIGNING_KEY`, fail-CLOSED at `crates/corelink-container/src/routes/dpa_accept.rs:608-608`) every paid checkout 403s `dpa_required`.
- The "at most one active subscription per tenant" guard is now **per-axis**: a runner tier checks `has_active_runner_subscription` (a `SELECT 1 FROM runner_billing … status IN ('active','trialing')`, migration 0087), a cache tier checks `has_active_subscription` — so a cache-active tenant is never blocked from buying runner (and vice-versa); only a *second* subscription of the SAME axis is refused `AlreadyActive` (`crates/corelink-container/src/routes/tier_select/part-01.rs:94`, `crates/corelink-container/src/routes/tier_select_store.rs:250-265`).
- "Active" in that guard means an active **PAID** subscription, and the free row is explicitly excluded (`AND tier != 'free'`). This is load-bearing rather than cosmetic: signup seeds EVERY tenant with `tier_selections('free','active')` so billing reads `active` instead of `inactive`, so a tier-agnostic read makes the guard reject the first purchase of every account that exists — which is precisely what shipped, and 117 prod tenants sat un-upgradable behind `409 already_active` until 2026-08-02. Free is an instant *activation*, not a subscription (see the decision rules below); the runner axis is already scoped to a real paid subscription by its `status IN ('active','trialing')` predicate. The cache-axis predicate is the named statement at `crates/corelink-container/src/routes/tier_select_store.rs:94-103`, consumed by the guard at `crates/corelink-container/src/routes/tier_select/part-01.rs:94`. The same asymmetry governs the signup-worker's activation UPSERT on the other half of the money path — see [the Stripe activation webhook](/launch/stripe-activation-webhook.md).
- A paid CACHE selection persists the subscription intent as `pending_checkout` with the mapped `stripe_customer_id` via an upsert `INSERT INTO tier_selections` `crates/corelink-container/src/routes/tier_select_store.rs:274`. A RUNNER selection creates the Stripe Checkout Session but DELIBERATELY skips this cache persist (guarded on `!tier.is_runner()`), because the runner axis is written by the webhook into `runner_billing`/`runners_entitlement`, never the cache tables — so the container never clobbers the tenant's cache tier or violates the cache-only `tier` CHECK (`crates/corelink-container/src/routes/tier_select/part-01.rs:145`).
- The fail-CLOSED audit seam emits a **durable** audit record before the state mutation via `TierSelectAudit::emit` (`crates/corelink-container/src/routes/tier_select_audit.rs:110`) — a parameterized D1 INSERT into the dedicated `tier_select_audit_events` table (migration 0092, `crates/corelink-container/src/routes/tier_select_audit.rs:129`) that returns `Err` on any D1 failure (L2 — the Wave-37 durable write, landed). The emit-Err-aborts-before-mutation logic lives at `crates/corelink-container/src/routes/tier_select/part-01.rs:14-18` and now **genuinely fires**: an audit-persist failure aborts before any lock/DPA/active-sub/persist step.
- Both money-path routes resolve their internal-auth secret through the common helper at `crates/corelink-container/src/routes/tier_select/part-00.rs:599` and `crates/corelink-container/src/routes/dpa_accept.rs:406-406`; the Worker selects the matching dedicated key and the DO forwards it (`worker/src/index_auth.ts:17-40`, `worker/src/durable_object_start.ts:96-97`). A valid dedicated key wins, while the resolver preserves the documented shared fallback and enforces the 32-character floor.
- The decision rules (cache paid set = Solo/Starter/Pro/Max, Free instant, Enterprise inquiry, DPA-first all tiers) are the ADR's decision section `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127`; the five runner SKUs are an additive campaign axis on top of that decision.

# Invariants

- DPA not accepted ⇒ `403 dpa_required` (INV-ONBOARD-DPA-FIRST): the enforcement is `return Err(TierSelectHttpError::DpaRequired)` after the fail-closed store check at `crates/corelink-container/src/routes/tier_select/part-01.rs:78` (the `403`/`dpa_required` status mapping itself is the `parts()` arm at `crates/corelink-container/src/routes/tier_select/part-00.rs:333`).
- A direct paid Checkout for Enterprise is rejected `422 use_inquiry_form` — the UI hint alone is bypassable: the enforcement is `ParsedTier::Enterprise => return Err(...UseInquiryForm)` at `crates/corelink-container/src/routes/tier_select/part-00.rs:486` (the `422`/`use_inquiry_form` status mapping is the `parts()` arm at `crates/corelink-container/src/routes/tier_select/part-00.rs:332`).
- Runner is a SEPARATE entitlement axis and MUST NOT persist to the cache tables: for a runner tier the orchestration skips `persist_pending_checkout` entirely (`!tier.is_runner()` guard) — a runner write would clobber the one-row-per-tenant cache `tier_selections` row and violate its cache-only `tier` CHECK. The runner entitlement is instead written by the signup-worker webhook into `runner_billing`/`runners_entitlement` (`crates/corelink-container/src/routes/tier_select/part-01.rs:145`; the exhaustive-but-runtime-unreachable runner arms of `tier_column` are documented at `crates/corelink-container/src/routes/tier_select_store.rs:56-83`).
- The per-tenant single-active-subscription guard is per-axis: a cache subscription never blocks a runner purchase and a runner subscription never blocks a cache purchase; only a second subscription of the SAME axis is refused `AlreadyActive` (`crates/corelink-container/src/routes/tier_select/part-01.rs:94`).
- The route is only mounted when the internal-auth resolver yields a key, the Stripe config, AND `CORELINK_DPA_VERSION` are present; a missing DPA version means the endpoint is NOT mounted — the actual `CORELINK_DPA_VERSION` unset-⇒-`None` guard is `crates/corelink-container/src/routes/tier_select/part-00.rs:614-620` (inside `build_state_from_env` at `:612`).
- Both routes fail CLOSED when neither the dedicated nor shared resolver input reaches the 32-character floor; valid dedicated values are distinct rotation authorities, and a valid shared key remains the documented migration fallback (`crates/corelink-container/src/routes/tier_select/part-00.rs:598-599`, `crates/corelink-container/src/routes/dpa_accept.rs:583-583`, Worker selection at `worker/src/index_auth.ts:17-40`).
- The DPA gate is fail-CLOSED: a transport error never reads as "accepted" `crates/corelink-container/src/routes/tier_select_store.rs:237-248`.
- Audit emit returning `Err` ABORTS the whole operation before mutation (audit-before-mutation) — the abort logic lives at `crates/corelink-container/src/routes/tier_select/part-01.rs:14-18`, and the production adapter (`crates/corelink-container/src/routes/tier_select_audit.rs:110`) now returns `Err` on a durable-D1 INSERT failure, so the abort is REAL (no longer vacuous).
- Supersession is scoped to tier cardinality/membership only — no invariant, security control, or latency/cost gate changes `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144`.

# Gotchas

- Prices ($15/$35/$50/$149) are deliberately NOT in the `TierKind` enum — pricing is a billing concern and lives in the pricing page + Stripe script env vars `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126`.
- `team` is removed from the visible taxonomy but retained in the persisted CHECK domain for back-compat; new flows never mint it `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177`.

# Citations

1. `crates/corelink-container/src/routes/tier_select/part-00.rs:203` — `parse_self_serve`: self-serve vs inquiry tier parsing (cache tiers + `free`/`enterprise` + the five `runner_*` SKUs).
1b. `crates/corelink-container/src/routes/tier_select/part-00.rs:236` — `RequestedTier::is_runner`: the runner-axis discriminator + its clobber/CHECK rationale.
2. `crates/corelink-container/src/routes/tier_select/part-00.rs:526` — the `handle` axum entrypoint.
3. `crates/corelink-container/src/routes/tier_select/part-00.rs:783` — `orchestrate_tier_select` fixed ordering (per-axis active-sub guard + runner-skips-persist).
3b. `crates/corelink-container/src/routes/tier_select/part-01.rs:94` — per-axis single-active-subscription guard (`is_runner` selects `has_active_runner_subscription` vs `has_active_subscription`).
3c. `crates/corelink-container/src/routes/tier_select/part-01.rs:145` — runner checkout SKIPS the cache `persist_pending_checkout` (`!tier.is_runner()` guard).
4. `crates/corelink-container/src/routes/tier_select/part-00.rs:486` — Enterprise direct-paid → `return Err(...UseInquiryForm)` (the enforcement).
4b. `crates/corelink-container/src/routes/tier_select/part-00.rs:332` — `parts()` status-mapping arm: `UseInquiryForm` → 422 `use_inquiry_form`.
5. `crates/corelink-container/src/routes/tier_select/part-01.rs:78` — DPA-not-accepted → `return Err(TierSelectHttpError::DpaRequired)` (the enforcement, gated by the fail-closed store check).
5b. `crates/corelink-container/src/routes/tier_select/part-00.rs:333` — `parts()` status-mapping arm: `DpaRequired` → 403 `dpa_required`.
6. `crates/corelink-container/src/routes/tier_select/part-00.rs:614-620` — the `CORELINK_DPA_VERSION` unset-⇒-NOT-mounted guard (in `build_state_from_env` at `:612`).
7. `crates/corelink-container/src/routes/tier_select_store.rs:237-248` — the fail-CLOSED DPA-first D1 check.
7b. `crates/corelink-container/src/routes/tier_select_store.rs:250-265` — `has_active_runner_subscription`: the runner axis `runner_billing` active-status query (migration 0087), separate from the cache `has_active_subscription`.
8. `crates/corelink-container/src/routes/tier_select_store.rs:274` — persist `pending_checkout` upsert (cache axis only).
8b. `crates/corelink-container/src/routes/tier_select_store.rs:56-83` — `tier_column`: exhaustive over `RequestedTier` incl. the runner arms, but the runner arms are runtime-UNREACHABLE (orchestration never persists a runner tier here), so no CHECK-widening migration is needed.
9. `crates/corelink-container/src/routes/tier_select_audit.rs:110` — durable audit emit (D1 INSERT into `tier_select_audit_events`, migration 0092; `Err` on failure).
9b. `crates/corelink-container/src/routes/tier_select/part-01.rs:14-18` — emit-Err-aborts-before-mutation orchestration (now REAL: the adapter returns `Err` on durable-D1 failure).
10. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:72-84` — the 6-tier ladder + wire strings.
11. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:88-127` — the decision (paid set, DPA-first all tiers, Stripe map).
12. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:117-126` — prices out of the enum.
13. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:128-144` — precise scope of supersession.
14. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:161-177` — `team` retained additively in the persisted domain.
15. `crates/corelink-container/src/routes/dpa_accept.rs:516` — `handle`: the `POST /v1/onboarding/dpa-accept` writer entrypoint (fail-CLOSED auth gate → `orchestrate_dpa_accept` at `:385`; mount fail-CLOSES on the RS256 key at `:598-600`).
16. `crates/corelink-container/src/routes/dpa_accept_store.rs:104` — `insert_acceptance`: the durable D1 `INSERT OR IGNORE INTO dpa_acceptances` that the tier-select gate then reads.
17. `crates/corelink-container/src/routes/tier_select/part-00.rs:598-599` — tier-select resolves through the common internal-auth helper with `CORELINK_TIER_SELECT_AUTH_KEY`.
18. `crates/corelink-container/src/routes/dpa_accept.rs:571` — DPA acceptance resolves through the common internal-auth helper with `CORELINK_DPA_ACCEPT_AUTH_KEY`.
19. `worker/src/index_auth.ts:17-40` — the Worker selects the endpoint-specific money authority and enforces the 32-character floor.
20. `worker/src/durable_object_start.ts:96-97` — the DO forwards both dedicated money authorities to the container.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.

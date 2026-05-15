# Domain — Billing (Stripe + tier-selection + reconciliation)

> **Estimated effort:** ~25 hours over Week 2.
> **Prerequisites:** Day 1–5 complete; have used Stripe in any prior
> project (any role). Comfortable with idempotency keys.
> **Mentors (rotated quarterly):**
> - Primary: TBD (billing pipeline owner)
> - Secondary: TBD (revenue-ops engineering liaison)
> - Backup: TBD (Stripe integration on-call)

## Why this domain matters

Billing is meter-based: customers are billed on bytes-cached,
bytes-served, and per-tier base fees. Idempotency must hold under
network retries, webhook duplicates, and Stripe-side replay. Daily
reconciliation compares emitted usage against Stripe's recorded usage;
a drift > 0.5% is a hard incident.

## Must-read (in order)

1. `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md` — the
   pricing model in plain English.
2. `specs/03_architecture/data_model.md` §Billing — event shape,
   meter names, idempotency-key derivation.
3. `crates/corelink-billing-emit/src/lib.rs` — the emission path.
4. `crates/corelink-billing-stripe/src/lib.rs` — Stripe API wrapper.
5. `crates/corelink-billing-aggregator/src/lib.rs` — usage rollup
   from raw events to meter records.
6. `crates/corelink-billing-reconcile/src/lib.rs` — daily drift
   detection.
7. `crates/corelink-billing-replay/src/lib.rs` — replay tooling for
   when emission fails and we need to backfill.
8. `crates/corelink-tier-selector/src/lib.rs` — tier resolution at
   request time (free / pro / business / enterprise).
9. `specs/tla/billing_atomicity.tla` — atomic emit invariant.
10. `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md` — webhook DLQ handling
    (Stripe webhooks land here).

## Hands-on exercises

1. **Trace an emission round-trip.** Start from a synthetic
   CAS-write in staging. Follow the event into
   `corelink-billing-emit`, into the aggregator, into Stripe (test
   mode). Confirm the meter record appears in Stripe's dashboard.
   Note every queue / table the event passes through.
2. **Idempotency-key collision.** Construct a test (or extend an
   existing one) that re-emits the same event twice. Confirm the
   second emission is a no-op at the aggregator level. Confirm Stripe
   does not double-count. The idempotency key derivation lives in
   `corelink-billing-emit` — make sure you understand the inputs.
3. **Reconcile drift drill.** Pair with mentor on
   `corelink-billing-reconcile` against staging. Inject a synthetic
   1% drift. Watch the alert fire. Resolve via replay. Document the
   wall-clock duration.

## What "comfortable in this domain" looks like by Day 30

- You can sketch the emit-aggregate-reconcile loop on a whiteboard.
- You know which meters exist (`bytes_stored`, `bytes_served`,
  `cache_writes`, plus tier-base) and how each is computed.
- You can read a Stripe webhook payload and predict which crate
  consumes it.
- You have shipped at least one PR touching billing emission, tier
  selection, or reconcile.

## Common pitfalls

- **Idempotency-key inputs.** Adding a field to the key derivation is
  a wire-format break — old retries will produce *new* keys. Always
  version-gate.
- **Stripe test mode vs live.** Never run reconciliation against live
  from a workstation. The runbook gates this with a confirm prompt;
  don't bypass.
- **Float drift in usage aggregation.** Use integer cents / bytes
  everywhere; the codebase enforces this with newtypes
  (`Bytes`, `MilliCents`). Don't introduce `f64` accumulators.

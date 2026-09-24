# Ownership wave 003 — billing pipeline

This wave uses the frozen four-artifact contract in
[WAVE_001_PLAN.md](WAVE_001_PLAN.md#frozen-artifact-contract). Authors may
use source/static evidence only: no Cargo compilation, tests, network,
runtime operation, publication, shared registry/index edits, or self-review.

## Conflict-free slots

| WP | Package | Manifest | Profile | Owned paths |
|---|---|---|---|---|
| W003-AGG | `corelink-billing-aggregator` | `crates/corelink-billing-aggregator/Cargo.toml` | S | `own-corelink-billing-aggregator`, `crates/corelink-billing-aggregator/` |
| W003-EMIT | `corelink-billing-emit` | `crates/corelink-billing-emit/Cargo.toml` | S | `own-corelink-billing-emit`, `crates/corelink-billing-emit/` |
| W003-RECON | `corelink-billing-reconcile` | `crates/corelink-billing-reconcile/Cargo.toml` | S | `own-corelink-billing-reconcile`, `crates/corelink-billing-reconcile/` |
| W003-STRIPE | `corelink-billing-stripe` | `crates/corelink-billing-stripe/Cargo.toml` | S | `own-corelink-billing-stripe`, `crates/corelink-billing-stripe/` |
| W003-MAT | `corelink-billing-stripe-materializer` | `crates/corelink-billing-stripe-materializer/Cargo.toml` | H | `own-corelink-billing-stripe-materializer`, `crates/corelink-billing-stripe-materializer/` |
| W003-TRAITS | `corelink-billing-stripe-traits` | `crates/corelink-billing-stripe-traits/Cargo.toml` | S | `own-corelink-billing-stripe-traits`, `crates/corelink-billing-stripe-traits/` |

Each author writes exactly its skill and three package documents. A package
name—not a directory basename—is authoritative. The lead owns checks,
integration, status, registry/index, cross-package reconciliation, and
publication gates.

## Static anchors

- **Aggregator:** `aggregator`, `audit`, `chain`, `event`, and `store` own
  deterministic `(time_ms, idem_key)` ordering, aggregate chain linkage and
  append-only storage seams. `corelink-billing-emit` is a direct dependency;
  production Cron/D1/R2 remains deferred, not runtime evidence.
- **Emitter:** `event`, `idempotency`, `sink`, and `emitter` define
  canonical usage events, append-only NDJSON and duplicate handling. Its
  analytics dependency is not proof of an export, queue, R2 object or Cron.
- **Reconciler:** `drift`, `reconciler`, `history`, `stripe_pause`, `run`,
  and the `billing-reconcile-run` binary are one package territory. Its three
  billing package dependencies are static edges; a real Stripe pause or CF
  schedule is explicitly unverified.
- **Stripe adapter:** `adapter`, `signature`, `webhook`, `ledger`, and
  idempotency compose a pure/static boundary over emit and aggregator. Do not
  claim real Stripe endpoint, request, signature observation, route mount, or
  secret availability.
- **Materializer:** twelve source modules, optional `cf-billing-real`, wasm
  target dependency, `corelink-billing-stripe-traits`, `corelink-stripe-real`,
  tier selection and optional CF/audit adapters require H capacity. Distinguish
  port ownership from provider implementation and source wiring from real D1,
  Worker or audit operation.
- **Traits:** a deliberate zero-`corelink-*`-dependency port/identity crate.
  Preserve that falsifiable leaf invariant and distinguish compatibility
  re-exports by `corelink-stripe-real` from implementation ownership.

## Acceptance and review

Every slot must pass the four declared structural checks and a scope-only
diff before review. A fresh reviewer returns four separate verdicts (skill,
reference, blast radius, maintenance), samples source/manifests, and rejects
unsupported consumer, runtime, security, compatibility, or package-identity
claims. Any artifact-byte change requires an appropriate fresh re-review.
Integration is not global approval, standard freeze, or issue-publication
authorization.

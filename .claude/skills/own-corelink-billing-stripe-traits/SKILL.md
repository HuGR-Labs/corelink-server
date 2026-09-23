---
name: own-corelink-billing-stripe-traits
description: Ownership routing for corelink-billing-stripe-traits; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-billing-stripe-traits
  manifest: crates/corelink-billing-stripe-traits/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-billing-stripe-traits-structural-normalization-20260921
---

# Ownership — corelink-billing-stripe-traits

This guide is limited to the source-defined port and identity surface in `src/lib.rs` and its manifest. It records no execution, Stripe, D1, wire-protocol, FFI, deployment, or whole-graph compatibility result.

[Baseline](#s01) · [Leaf boundary](#s02) · [Contracts](#s03) · [Classifications](#s04) · [Consumers](#s05) · [Change control](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Establish the static baseline

**Condition:** an ownership or public-surface change is proposed. **Action:** record the revision and inspect `crates/corelink-billing-stripe-traits/src/lib.rs` and `Cargo.toml` before changing a contract. **Evidence:** the package declaration, source file, and [R01](../../../docs/ownership/crates/corelink-billing-stripe-traits/REFERENCE.md#r01). **Stop:** the requested behavior is outside the one-file trait surface or needs execution evidence.

<a id="s02"></a>
## S02 — Preserve the leaf boundary

**Condition:** a dependency or shared implementation is proposed. **Action:** keep the manifest free of dependency names beginning `corelink-`; keep provider implementations outside this package. **Evidence:** `Cargo.toml` `[dependencies]`; [INV-001](../../../docs/ownership/crates/corelink-billing-stripe-traits/REFERENCE.md#inv-001). **Stop:** adding a first-party dependency or treating a re-export as implementation ownership requires an architecture decision.

<a id="s03"></a>
## S03 — Change a port as a shared contract

**Condition:** `AuditEmitter`, `IdempotencyStore`, `StateMaterializer`, or `SliRecorder` changes. **Action:** preserve each supertrait and exact parameter/result shape, then identify the relevant relation. **Evidence:** `src/lib.rs`; [R03](../../../docs/ownership/crates/corelink-billing-stripe-traits/REFERENCE.md#r03); [B03](../../../docs/ownership/crates/corelink-billing-stripe-traits/BLAST_RADIUS.md#b03). **Stop:** an implementor or consumer compatibility claim needs a fresh selected build or source review outside this package.

<a id="s04"></a>
## S04 — Treat identities and outcomes as representation contracts

**Condition:** event taxonomy, envelope, token, record, outcome, or response changes. **Action:** enumerate changed variants, fields, labels, constructors, or status mapping and retain each applicable non-exhaustive boundary. **Evidence:** [R04](../../../docs/ownership/crates/corelink-billing-stripe-traits/REFERENCE.md#r04) and [INV-002](../../../docs/ownership/crates/corelink-billing-stripe-traits/REFERENCE.md#inv-002). **Stop:** do not infer an external wire, Stripe, persistence, or HTTP compatibility guarantee from these Rust APIs.

<a id="s05"></a>
## S05 — Separate aliases from implementations

**Condition:** a caller uses `corelink_stripe_real::webhook_dispatch::*`, `corelink_billing::stripe::traits::*`, or a direct import. **Action:** distinguish the source-defined type owner from static re-export and manifest edges. **Evidence:** `crates/corelink-stripe-real/src/{lib.rs,webhook_dispatch.rs}`, `crates/corelink-billing/src/stripe.rs`, and [B04](../../../docs/ownership/crates/corelink-billing-stripe-traits/BLAST_RADIUS.md#b04). **Stop:** a path alias does not prove all consumers have migrated or that its provider behavior belongs here.

<a id="s06"></a>
## S06 — Keep maintenance source-static

**Condition:** validation, recovery, or compatibility work is requested. **Action:** use the procedures in [M02](../../../docs/ownership/crates/corelink-billing-stripe-traits/MAINTENANCE.md#m02), keeping source inspection and documentary checks separate from Cargo or runtime work. **Evidence:** [M03](../../../docs/ownership/crates/corelink-billing-stripe-traits/MAINTENANCE.md#m03). **Stop:** a request needs a selected target, D1 state, webhook delivery, external service, secret, or deployment operation.

<a id="s07"></a>
## S07 — Hand off bounded evidence

**Condition:** scoped documentation or source review is ready for another owner. **Action:** report baseline, affected symbols, manifest invariant, static relations, changed paths, checker results, and explicit unknowns. **Evidence:** [M06](../../../docs/ownership/crates/corelink-billing-stripe-traits/MAINTENANCE.md#m06). **Stop:** do not call author checks a build, runtime result, semantic approval, or cold review.

---
name: own-corelink-billing-stripe-materializer
description: >-
  Use when changing the static webhook-materialization ports, handler,
  in-memory seams, target-gated binders, or tests of
  corelink-billing-stripe-materializer. It routes source evidence without
  assigning provider implementations or external operation to this package.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-billing-stripe-materializer
  manifest: crates/corelink-billing-stripe-materializer/Cargo.toml
  source-commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
  evidence-set: billing-materializer-static-graph-20260920
---

# Ownership — corelink-billing-stripe-materializer

Candidate ownership based on source and static manifest evidence only. It does
not establish feature selection, a D1 or Worker operation, audit-chain
delivery, Stripe event delivery, migration state, deployment, or review.

[Entry](#s01) · [Boundary](#s02) · [Read](#s03) · [Change](#s04) · [Tests](#s05) · [Stops](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Entry

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A change names materialization, billing D1 ports, materializer audit, idempotency, clock, tier bridge, runners, or event matrix | Enter this skill and locate the module and public reexport | `src/lib.rs` and [R03](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r03) | The request is for provider implementation or server composition |
| A request claims a received Stripe webhook, applied D1 row, Worker execution, audit archive, or migration | Separate static contracts from the requested operational claim | [R08](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r08) | No independent operational evidence is supplied |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `BillingD1Writer`, `BillingAuditEmitter`, handler, clock, idempotency, tier/runners bridge, constants, or in-memory fake | Keep the package-local contract here and trace ports and callers | [R02](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r02), [B01](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b01) | It requires a `CfD1DatabaseReal`, `ArchiveProducer`, Stripe dispatcher, or tier implementation change |
| Changing CF binding, audit-chain, Stripe-real, tier-selection, route, credentials, deployment, or D1 schema | Hand off to the corresponding owner | [B02](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b02)–[B05](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b05) | Do not represent the handoff as implementation or operation proof |

<a id="s03"></a>
## S03 — Read route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Event handling, audit ordering, or materialized rows change | Read `handler.rs`, `audit.rs`, `d1.rs`, and the event matrix | [R04](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r04) | A provider-side or migration semantic must be decided |
| Port identity, dedup, tier, runners, or clock changes | Read `lib.rs`, `idempotency.rs`, `tier.rs`, `runners.rs`, and `clock.rs` | [R05](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r05) | Callers or historical compatibility are not statically classified |
| Feature, target, or binder changes | Read the manifest, `lib.rs`, `wasm32_binders.rs`, and `tests/wasm32_binders.rs` | [R06](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r06) | Feature selection or target execution must be proven |

<a id="s04"></a>
## S04 — Change decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A mutation path changes | Trace the local audit call before the local `BillingD1Writer` call on every affected arm | [R04](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r04), [B03](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b03) | A path cannot establish that ordering from code |
| Public port, reexport, error, SQL literal, or event-matrix item changes | Trace static direct dependency and flow relations; retain unknown consumer compatibility | [B01](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b01), [B06](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b06) | N/N-1 or stored-data compatibility requires a decision |
| `cf-billing-real` or wasm32 code changes | Preserve the manifest/module `cfg` distinction and inspect native seams | [R06](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r06) | Feature activation, a wasm build, or Worker execution is inferred |

<a id="s05"></a>
## S05 — Test routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Handler, port, fake, or clock behavior changes | Select module tests, `tests/materializers_e2e.rs`, or the split test modules by touched surface | [M03](../../../docs/ownership/crates/corelink-billing-stripe-materializer/MAINTENANCE.md#m03) | Commands, target, or environment are not authorized |
| Binder or feature-boundary source changes | Select `tests/wasm32_binders.rs` and inspect its feature gate | [M04](../../../docs/ownership/crates/corelink-billing-stripe-materializer/MAINTENANCE.md#m04) | A native test declaration is treated as wasm execution |

<a id="s06"></a>
## S06 — Stops and escalation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Package identity, manifest, or baseline differs | Preserve the observation and request scope correction | manifest and checkout diff | Make no ownership conclusion |
| A claim needs D1 persistence, Worker runtime, audit archive, Stripe delivery, migration state, or configuration/secret evidence | Escalate to the relevant runtime/provider owner | [B05](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b05) | This crate’s source cannot close the claim |
| Compatibility, complete consumers, feature choice, or target behavior is unknown | Request the missing contract or environment evidence | [R08](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r08) | Do not infer it from comments, fakes, or declarations |

<a id="s07"></a>
## S07 — Record

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work is ready to report | State baseline, paths read, affected ports, static relations, unrun checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-billing-stripe-materializer/MAINTENANCE.md#m06) | Do not label author validation as runtime proof or cold review |
| An unknown remains | State its boundary and escalation route explicitly | [R08](../../../docs/ownership/crates/corelink-billing-stripe-materializer/REFERENCE.md#r08), [B06](../../../docs/ownership/crates/corelink-billing-stripe-materializer/BLAST_RADIUS.md#b06) | Do not convert absence of evidence into approval |

[Back to entry](#s01)

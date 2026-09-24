---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-stripe-materializer
manifest: crates/corelink-billing-stripe-materializer/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: H
state: draft
evidence_set: billing-materializer-static-graph-20260920
---

# corelink-billing-stripe-materializer — maintenance

Source-guided maintenance modes. No Cargo command, test, network call, runtime
operation, deployment, or publication was run or is implied here.

[Baseline](#m01) · [Contracts](#m02) · [Tests](#m03) · [Features](#m04) · [External effects](#m05) · [Record](#m06).

<a id="m01"></a>
## M01 — Static baseline mode

**Mode:** source inspection. **Prerequisite:** a package, module, manifest, or
dependency question. **Expected predicate:** the selected baseline and package
identity match the manifest. **Action:** record the commit and paths read.
**Stop/recovery:** stop on a baseline/manifest mismatch; refresh the static map
against the approved baseline. **Evidence:** checkout, manifest, and source
paths only; no execution result is implied.

<a id="m02"></a>
## M02 — Contract and ordering mode

**Mode:** compatibility/flow analysis. **Prerequisite:** a public port, reexport, event matrix, SQL literal, error, mutation helper, or local fake is changing. **Expected predicate:** the affected local contract and its audit-before-writer relation are traced. **Action:** inspect `lib.rs`, `handler.rs`, `audit.rs`, `d1.rs`, and the applicable clock/idempotency/tier/ runners module; trace direct provider edges without assigning their ownership. **Stop/recovery:** stop if cross-release compatibility, stored data, migration, or provider behavior needs a decision; obtain that owner’s contract.

**Evidence:** symbol map, call order, static relation list, and explicit gaps.

<a id="m03"></a>
## M03 — Test-selection mode

**Mode:** test planning. **Prerequisite:** a source change has a matching
package test seam. **Expected predicate:** relevant test files are named, not
assumed to have run. **Action:** select module tests, `src/tests.rs`, split
test parts, `tests/materializers_e2e.rs`, or `tests/wasm32_binders.rs` by the
touched surface. **Stop/recovery:** stop if a command, target, or environment
is not separately authorized; record tests as unrun. **Evidence:** named files
and feature/target gates; exit status only if separately obtained.

<a id="m04"></a>
## M04 — Feature and target-boundary mode

**Mode:** static cfg analysis. **Prerequisite:** `cf-billing-real`, optional
provider dependencies, `wasm32_binders.rs`, `js-sys`, or clock target code is
changing. **Expected predicate:** feature gating and target gating are
distinguished. **Action:** inspect the manifest, `lib.rs`, `clock.rs`,
`wasm32_binders.rs`, and the feature-gated binder test. **Stop/recovery:** stop
if actual feature selection, wasm compilation, or Worker execution is needed;
obtain separately recorded environment evidence. **Evidence:** feature list,
`cfg` paths, and target dependency declarations only.

<a id="m05"></a>
## M05 — External-effect recovery mode

**Mode:** boundary escalation. **Prerequisite:** the requested result concerns D1 persistence, CF Worker dispatch, audit-chain archive, Stripe event delivery/retry, price configuration, migration state, route wiring, or secrets. **Expected predicate:** the request is separated from local port/source ownership. **Action:** route it to the CF-binding, audit-chain, Stripe-real, tier-selection, composition, migration, or runtime owner as applicable. **Stop/recovery:** stop when an external effect must be established; use the responsible owner’s approved procedure and evidence. **Evidence:**

an operator/provider record is required and is absent from this document.

<a id="m06"></a>
## M06 — Handoff record mode

**Mode:** reporting. **Prerequisite:** a static analysis or scoped change is
ready to hand off. **Expected predicate:** baseline, paths, symbols, source
relations, selected/unrun checks, stop condition, recovery route, and unknowns
are present. **Action:** report those fields with [Reference](REFERENCE.md#r01)
and [Blast radius](BLAST_RADIUS.md#b01). **Stop/recovery:** stop if the report
would present static evidence as operational proof, compatibility certification,
or cold review; restate the boundary and request the missing evidence.
**Evidence:** repository paths and commit only; author validation is not an
independent review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)

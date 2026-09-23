---
name: own-corelink-dsr-statuspage-scheduler
description: Maintain SOURCE-only ownership records for corelink-dsr-statuspage-scheduler's declared composition, target gates, traits, and in-memory fakes; do not use it to assert scheduling, Statuspage API activity, or runtime behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-dsr-statuspage-scheduler"
  manifest: "crates/corelink-dsr-statuspage-scheduler/Cargo.toml"
  source-commit: "3feae2baed63061354533ffdfe2d94acfd24aa9a"
  evidence-set: "w012-dsr-statuspage-scheduler-static-20260920"
---

# Ownership — corelink-dsr-statuspage-scheduler

Use for static package contracts only. Source text does not establish a schedule,
Statuspage API activity, D1 result, audit delivery, target selection, or runtime behavior.

[Scope](#s01) · [Surface](#s02) · [Axioms](#s03) · [Relations](#s04) · [Modes](#s05) · [Unknowns](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope

Read only the stated manifest and `src/{lib,scheduler,audit,cron_log,row_source,wasm32_row_source}.rs`. Stop if a request needs an invocation, schedule, D1 state, audit sink, credential, API request, target build, or runtime outcome.

<a id="s02"></a>
## S02 — Source surface

The local surface declares `DsrStatuspagePublishScheduler`, row-source, ledger,
and audit traits; typed errors/outcomes; a wasm-only row-source type; constants;
and in-memory fakes. `corelink-privacy-erasure-worker`, `corelink-statuspage-real`,
`corelink-cf-bindings`, and target-gated `corelink-ops` are manifest declarations,
not observed integrations.

<a id="s03"></a>
## S03 — Five source axioms

Preserve the five falsifiable axioms in [R05](../../../docs/ownership/crates/corelink-dsr-statuspage-scheduler/REFERENCE.md#r05): opposed target imports; audit order for explicitly logged branches; date-plus-metric fake dedupe; half-open fake filtering; and JSON helper symmetry. Every changed axiom needs a textual falsifier; none proves an external effect.

<a id="s04"></a>
## S04 — Atomic relations

Map each changed local declaration to exactly one relation in [B01–B06](../../../docs/ownership/crates/corelink-dsr-statuspage-scheduler/BLAST_RADIUS.md#b01), state its source endpoints and falsifier, then retain its non-inference boundary. Do not infer a reverse-consumer graph or compatibility result.

<a id="s05"></a>
## S05 — Evidence modes

Use SOURCE for manifest/source text, STATIC_RELATION for one named local arrow,
DOCUMENTARY for artifact form, and UNKNOWN for all unselected external facts.
The verified [SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md) is an OKF route only: do not copy or revalidate it.

<a id="s06"></a>
## S06 — Stop conditions

Stop and route outward for scheduling, Statuspage API, D1, credentials, audit
persistence, compilation/linking, deployment, network activity, or runtime
observation. Traits, comments, SQL strings, and fakes remain source declarations.

<a id="s07"></a>
## S07 — Handoff

Report the baseline, four artifact paths, affected R/B/M identifiers, five axioms,
five unknowns, four profile-S structural checker results, and whitespace-diff result.
Structural checks are documentary only, never Cargo/test, API, schedule, or runtime evidence.

Success is bounded, falsifiable SOURCE documentation. Completeness requires S01–S07,
[R01–R08](../../../docs/ownership/crates/corelink-dsr-statuspage-scheduler/REFERENCE.md#r01),
[B01–B06](../../../docs/ownership/crates/corelink-dsr-statuspage-scheduler/BLAST_RADIUS.md#b01),
and [M01–M06](../../../docs/ownership/crates/corelink-dsr-statuspage-scheduler/MAINTENANCE.md#m01).

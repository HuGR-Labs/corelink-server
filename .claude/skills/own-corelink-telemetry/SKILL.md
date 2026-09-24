---
name: own-corelink-telemetry
description: >-
  Govern source-grounded changes to the corelink-telemetry hybrid umbrella,
  distinguishing local implementations from tracing and SLO facade routes.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-telemetry"
  manifest: "crates/corelink-telemetry/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "telemetry-static-source-20260920"
  profile: "H"
---

# Ownership — corelink-telemetry

This H-profile guide is bounded to checked-in SOURCE at the recorded revision.
It separates local implementation ownership, public paths, composition, runtime
operation, and review authority; it establishes none of the latter three.

[Baseline](#s01) · [Authority](#s02) · [Local](#s03) · [Facades](#s04) · [Impact](#s05) · [Stops](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the static baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Beginning a change or review | Record revision; inspect manifest, `src/lib.rs`, and changed family | `crates/corelink-telemetry/{Cargo.toml,src/lib.rs}` | Revision, manifest, or changed paths differ from the record |

Activate for an edit to an absorbed local family, root route, facade, dependency,
or source test relation. Do not activate for a generic telemetry question or a
change confined to a defining external package unless this facade/import changes.
Read Reference → matching Blast Radius REL → Maintenance PROC. Stop for an
untraced consumer, adapter selection, secrets, endpoint, deployment, delivery,
or runtime assertion; keep it unknown and route it to its owner.

<a id="s02"></a>
## S02 — Split ownership authorities

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A root path or dependency changes | Identify local implementation owner, defining implementation owner for each facade route, public-contract owner, composition relation, runtime-operator unknown, and external review authority | [R02](../../../docs/ownership/crates/corelink-telemetry/REFERENCE.md#r02) | A `pub use`, static importer, or author check is treated as implementation, runtime, or review ownership |

<a id="s03"></a>
## S03 — Review absorbed local modules

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `canary`, `lighthouse`, `logpush`, `otel`, or `synthetic_pager` changes | Trace root, local submodules, public re-exports, source axiom, and B01/B02 relation | [R03](../../../docs/ownership/crates/corelink-telemetry/REFERENCE.md#r03); [R05](../../../docs/ownership/crates/corelink-telemetry/REFERENCE.md#r05) | An adapter, endpoint, scheduled trigger, persistence, runtime, or delivery conclusion is required |

<a id="s04"></a>
## S04 — Review tracing and SLO facades

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `tracing` or `slo` changes | Verify root path, `pub use`, manifest edge, and named defining package | [R04](../../../docs/ownership/crates/corelink-telemetry/REFERENCE.md#r04); [B04](../../../docs/ownership/crates/corelink-telemetry/BLAST_RADIUS.md#b04) | The umbrella is treated as owner of tracing or SLO implementation or operation |

<a id="s05"></a>
## S05 — Bound contract and composition impact

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Public path, local export, or dependency changes | Trace the applicable B01–B06 relation and locate direct static references only | [B01](../../../docs/ownership/crates/corelink-telemetry/BLAST_RADIUS.md#b01)–[B06](../../../docs/ownership/crates/corelink-telemetry/BLAST_RADIUS.md#b06) | A complete consumer graph, selected composition, runtime operator, or delivery result is needed |
| Cross-cutting observability guidance is needed | Route to the verified OKF observability-plane concept without reproducing it | `docs/knowledge/ops/observability-plane.md` | The requested conclusion exceeds this source/static boundary |

<a id="s06"></a>
## S06 — Mandatory stops

Stop for Cargo, build, test, network, deployment, credentials, remote endpoint,
telemetry delivery, runtime operation, or independent-review claims. A local
trait/fake and any re-export remain source evidence only. Recover by recording
the exact gap and routing it to the identified external authority.

<a id="s07"></a>
## S07 — Static handoff

Report baseline, paths read, authority split, affected R/B/M identifiers, static
relations, documentary-check results, and explicit unknowns. A checker or
`git diff --check` is not Cargo execution, test evidence, delivery evidence,
runtime proof, or cold review.

[Reference](../../../docs/ownership/crates/corelink-telemetry/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-telemetry/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-telemetry/MAINTENANCE.md#m01)
